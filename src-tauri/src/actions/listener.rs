use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Form, Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Extension, Router,
};
use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex, RwLock, Semaphore};
use tower_http::cors::CorsLayer;

use crate::auth::{
    authorization_server_metadata, authorize_get, authorize_post, external_base_url,
    token_exchange, AuthorizeForm, AuthorizeParams, OAuthRuntime, TokenForm,
};
use crate::audit::request_context_from_headers;
use crate::tools::{self, is_allowed_tool, policy::PolicySettings, wrap_tool_result, ToolContext};
use crate::tunnel::append_profile_log;

use super::auth::{require_actions_auth, AuthConfig};
use super::openapi;

pub type ShutdownSender = oneshot::Sender<()>;

#[derive(Clone)]
struct AppState {
    ctx: Arc<ToolContext>,
    openapi: Arc<RwLock<Value>>,
    auth: Arc<AuthConfig>,
    workspace_path: String,
    bind_port: u16,
    configured_public_url: String,
    oauth: Option<Arc<OAuthRuntime>>,
    oauth_client_secret: Option<String>,
    write_lock: Arc<Mutex<()>>,
    request_slots: Arc<Semaphore>,
    waiting_slots: Arc<Semaphore>,
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_listener(
    workspace_id: &str,
    actions_port: u16,
    workspace_path: PathBuf,
    public_base_url: String,
    auth_type: String,
    api_key: Option<String>,
    oauth_client_id: String,
    oauth_client_secret: Option<String>,
    oauth_password: Option<String>,
    oauth_token_secret: Option<String>,
    policy: PolicySettings,
) -> Result<(ShutdownSender, tauri::async_runtime::JoinHandle<()>), String> {
    if auth_type == "api_key" && api_key.as_ref().is_none_or(String::is_empty) {
        return Err("Actions API key is not configured".into());
    }
    if auth_type == "oauth" {
        if oauth_password.as_ref().is_none_or(String::is_empty) {
            return Err("Actions OAuth password is not configured".into());
        }
        if oauth_token_secret.as_ref().is_none_or(String::is_empty) {
            return Err("Actions OAuth token secret is not configured".into());
        }
    }

    let configured_public_url = public_base_url.trim().to_string();
    let oauth = if auth_type == "oauth" {
        let oauth_base = external_base_url(
            &HeaderMap::new(),
            actions_port,
            &configured_public_url,
        );
        Some(Arc::new(OAuthRuntime::new(
            oauth_base,
            oauth_client_id,
            oauth_client_secret.clone(),
            oauth_password.unwrap_or_default(),
            oauth_token_secret.unwrap_or_default(),
        )))
    } else {
        None
    };

    // 在返回 Running 之前完成 bind，避免后台任务里的端口冲突被伪装成启动成功。
    let listener = bind_listener(actions_port)?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let profile_id = workspace_id.to_string();
    let handle = tauri::async_runtime::spawn(async move {
        let result = serve(
            listener,
            actions_port,
            &profile_id,
            workspace_path,
            configured_public_url,
            auth_type,
            api_key,
            oauth,
            oauth_client_secret,
            policy,
            shutdown_rx,
        )
        .await;
        if let Err(err) = &result {
            append_profile_log(
                &profile_id,
                "actions-stderr.log",
                &format!("[actions] listener stopped: {err}"),
            );
            eprintln!("actions listener stopped: {err}");
        } else {
            append_profile_log(
                &profile_id,
                "actions-stderr.log",
                "[actions] listener stopped",
            );
        }
    });
    Ok((shutdown_tx, handle))
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    listener: tokio::net::TcpListener,
    actions_port: u16,
    profile_id: &str,
    workspace_path: PathBuf,
    configured_public_url: String,
    auth_type: String,
    api_key: Option<String>,
    oauth: Option<Arc<OAuthRuntime>>,
    oauth_client_secret: Option<String>,
    policy: PolicySettings,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let workspace = tools::Workspace::new(workspace_path.clone()).map_err(|e| e.message())?;
    // 在 ToolContext 进入 Arc 前绑定审计库与 profile_id，使所有 Actions 路由共享同一工作区身份；
    // with_audit 初始化失败时仅关闭审计，不阻断服务启动。
    let ctx = Arc::new(ToolContext::from_workspace(
        workspace,
        crate::workspace::AuthConfig {
            auth_type: auth_type.clone(),
            ..crate::workspace::AuthConfig::default()
        },
        policy.clone(),
        "full".into(),
        policy.permission_mode.clone(),
    ).with_audit(profile_id.to_string()));
    let tools: Vec<Value> = tools::list_tools()
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(is_allowed_tool)
                .unwrap_or(false)
        })
        .collect();
    let public_base_url = if configured_public_url.is_empty() {
        format!("http://127.0.0.1:{actions_port}")
    } else {
        configured_public_url.clone()
    };
    let openapi_doc = openapi::build_openapi(&tools, &public_base_url, &auth_type);

    let auth = Arc::new(AuthConfig::new(
        auth_type,
        api_key,
        oauth.clone(),
        actions_port,
        configured_public_url.clone(),
    ));

    let state = AppState {
        workspace_path: ctx.workspace_path(),
        ctx,
        openapi: Arc::new(RwLock::new(openapi_doc)),
        auth: auth.clone(),
        bind_port: actions_port,
        configured_public_url,
        oauth,
        oauth_client_secret,
        write_lock: Arc::new(Mutex::new(())),
        request_slots: Arc::new(Semaphore::new(32)),
        waiting_slots: Arc::new(Semaphore::new(24)),
    };
    // access 中间件包在完整 Router 外层，以覆盖工具、OAuth、discovery 和 404；"actions"
    // 分区使同一工作区内的 MCP 与 Actions 日志可独立查询。
    let access_log_state = crate::access_log::AccessLogState::new(
        profile_id.to_string(),
        "actions",
        state.ctx.audit_store(),
    );

    let protected = Router::new()
        .route("/actions/{tool_name}", post(execute_action))
        .layer(middleware::from_fn(require_actions_auth))
        .layer(Extension(auth));

    let app = Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(openapi_json))
        .route("/privacy", get(privacy))
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_authorization_server_metadata),
        )
        .route("/oauth/authorize", get(oauth_authorize_get).post(oauth_authorize_post))
        .route("/oauth/token", post(oauth_token_post))
        .merge(protected)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn_with_state(
            access_log_state,
            crate::access_log::middleware,
        ));

    append_profile_log(
        profile_id,
        "actions-stdout.log",
        &format!(
            "[actions] listening on http://127.0.0.1:{actions_port} (public: {public_base_url})"
        ),
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await?;
    Ok(())
}

fn bind_listener(port: u16) -> Result<tokio::net::TcpListener, String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = std::net::TcpListener::bind(addr)
        .map_err(|err| format!("Actions 本地端口 {port} 绑定失败: {err}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("Actions 本地端口 {port} 设置非阻塞失败: {err}"))?;
    tokio::net::TcpListener::from_std(listener)
        .map_err(|err| format!("Actions 本地监听器初始化失败: {err}"))
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    let tools_loaded = state
        .openapi
        .read()
        .await
        .get("paths")
        .and_then(Value::as_object)
        .map(|paths| paths.len())
        .unwrap_or(0);

    Json(json!({
        "ok": true,
        "service": "coding-tools-actions",
        "workspace": state.workspace_path,
        "auth_type": state.auth.auth_type,
        "tools_loaded": tools_loaded
    }))
}

async fn openapi_json(State(state): State<AppState>) -> Json<Value> {
    Json(state.openapi.read().await.clone())
}

async fn privacy() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8">
    <title>Coding Tools Actions Privacy</title>
  </head>
  <body>
    <h1>隐私政策</h1>
    <p>本服务仅供仓库所有者本人使用。</p>
    <p>请求内容只用于执行用户主动发起的代码操作。</p>
    <p>服务不会出售或共享请求数据。</p>
    <p>API 密钥、GitHub 令牌和环境变量不会返回给模型。</p>
  </body>
</html>"#,
    )
}

fn resolve_oauth_base(state: &AppState, headers: &HeaderMap) -> String {
    external_base_url(headers, state.bind_port, &state.configured_public_url)
}

async fn oauth_authorization_server_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if !state.auth.oauth_enabled() {
        return oauth_not_configured();
    }
    Json(authorization_server_metadata(
        &resolve_oauth_base(&state, &headers),
        state.oauth_client_secret.as_deref(),
    ))
    .into_response()
}

async fn oauth_authorize_get(
    State(state): State<AppState>,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_get(oauth, params, Some(state.workspace_path.as_str()))
}

async fn oauth_authorize_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<AuthorizeForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return oauth_not_configured();
    };
    authorize_post(oauth, form, &resolve_oauth_base(&state, &headers))
}

async fn oauth_token_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Response {
    let Some(oauth) = state.oauth.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "unsupported_grant_type" })),
        )
            .into_response();
    };
    token_exchange(
        oauth,
        &headers,
        form,
        &resolve_oauth_base(&state, &headers),
    )
}

fn oauth_not_configured() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "OAuth not configured" })),
    )
        .into_response()
}

async fn execute_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tool_name): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let arguments = match body {
        Some(Json(value)) if value.is_object() || value.is_null() => {
            if value.is_null() {
                json!({})
            } else {
                value
            }
        }
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "detail": "Request body must be a JSON object" })),
            )
                .into_response();
        }
        None => json!({}),
    };

    // 参数归一化后、暴露策略检查前构造上下文：提前拒绝与正常执行由此共享同一组请求元数据。
    let request = request_context_from_headers(
        &headers,
        "actions",
        Some("POST".into()),
        None,
        Some(format!("/actions/{tool_name}")),
    );

    if let Err(err) = tools::policy::validate_actions_exposure(&tool_name) {
        let message = err.to_string();
        tools::record_tool_rejection_with_audit(
            state.ctx.as_ref(),
            &request,
            &tool_name,
            &arguments,
            "ACTION_NOT_EXPOSED",
            &message,
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": message })),
        )
            .into_response();
    }

    let waiting = matches!(tool_name.as_str(), "exec_command" | "exec_health_check" | "write_stdin");
    let ctx = state.ctx.clone();
    let name = tool_name.clone();
    let write_lock = state.write_lock.clone();
    let structured = match run_bounded_action(
        state.request_slots.clone(), state.waiting_slots.clone(), waiting, move || {
            // Personal mutations already coordinate through durable SQLite/file
            // gates. Only the old workspace-global task API needs this lock.
            let legacy_write = tools::registry::MUTATING_TOOLS.contains(&name.as_str())
                && !matches!(name.as_str(), "apply_patch" | "exec_command" | "write_stdin" | "kill_session"
                    | "history_session_bootstrap" | "history_session_checkpoint" | "history_session_validate"
                    | "task_open" | "task_checkpoint" | "task_status");
            let _guard = if legacy_write { Some(write_lock.blocking_lock()) } else { None };
            tools::call_tool_with_audit(ctx.as_ref(), &name, &arguments, &request)
        },
    ).await {
        Ok(value) => value,
        Err(status) => return (status, Json(json!({
            "ok": false,
            "error": if status == StatusCode::TOO_MANY_REQUESTS { "ACTION_CAPACITY" } else { "ACTION_WORKER_FAILED" },
            "safe_to_replay": false,
            "recovery": "query the existing task/job before retrying an uncertain mutation"
        }))).into_response(),
    };
    let result = wrap_tool_result(structured);
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = if is_error {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::OK
    };
    (
        status,
        Json(json!({
            "ok": !is_error,
            "tool": tool_name,
            "structured_content": result.get("structuredContent").cloned().unwrap_or(Value::Null),
            "content": result.get("content").cloned().unwrap_or_else(|| json!([])),
            "is_error": is_error
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::bind_listener;

    #[test]
    fn bind_listener_reports_port_conflict_synchronously() {
        let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("占用测试端口");
        let port = occupied.local_addr().expect("读取测试端口").port();

        assert!(bind_listener(port).is_err());
    }
}

#[cfg(test)]
mod audit_async_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, AppState) {
        let workspace = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let ctx = Arc::new(ToolContext::for_test(workspace.path().into(), runtime.path().into()).unwrap());
        let state = AppState {
            workspace_path: workspace.path().display().to_string(), ctx,
            openapi: Arc::new(RwLock::new(json!({}))),
            auth: Arc::new(AuthConfig::new("none".into(), None, None, 0, String::new())),
            bind_port: 0, configured_public_url: String::new(), oauth: None, oauth_client_secret: None,
            write_lock: Arc::new(Mutex::new(())),
            request_slots: Arc::new(Semaphore::new(32)),
            waiting_slots: Arc::new(Semaphore::new(24)),
        };
        (workspace, runtime, state)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn audit_actions_command_wait_does_not_block_the_async_executor() {
        let (_workspace, _runtime, state) = fixture();
        let started = Instant::now();
        let request = execute_action(State(state), HeaderMap::new(), Path("exec_command".into()),
            Some(Json(json!({"cmd":"python3 -c \"import time; time.sleep(0.35)\"", "yield_time_ms":1000, "timeout_ms":3000}))));
        let (response, timer_elapsed) = tokio::join!(request, async {
            tokio::time::sleep(Duration::from_millis(25)).await;
            started.elapsed()
        });
        assert_eq!(response.status(), StatusCode::OK);
        assert!(timer_elapsed < Duration::from_millis(180), "executor blocked for {timer_elapsed:?}");
    }
}

// Both permits belong to the blocking execution, not the HTTP future. Client
// disconnects cannot release capacity while a command is still using it.
async fn run_bounded_action<F>(
    slots: Arc<Semaphore>, waiting_slots: Arc<Semaphore>, waiting: bool, operation: F,
) -> Result<Value, StatusCode>
where F: FnOnce() -> Value + Send + 'static {
    let waiting_permit = if waiting {
        Some(waiting_slots.try_acquire_owned().map_err(|_| StatusCode::TOO_MANY_REQUESTS)?)
    } else { None };
    let permit = slots.try_acquire_owned().map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    tokio::task::spawn_blocking(move || {
        let _permits = (permit, waiting_permit);
        operation()
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(test)]
mod audit_capacity_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn audit_actions_waiters_leave_capacity_for_short_queries() {
        let slots = Arc::new(Semaphore::new(3));
        let waits = Arc::new(Semaphore::new(2));
        let _busy = waits.clone().acquire_many_owned(2).await.unwrap();
        let rejected = run_bounded_action(slots.clone(), waits.clone(), true, || panic!("must not execute")).await;
        assert_eq!(rejected.unwrap_err(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(slots.available_permits(), 3);
        assert_eq!(run_bounded_action(slots.clone(), waits, false, || json!({"ok":true})).await.unwrap()["ok"], true);
        assert_eq!(slots.available_permits(), 3);
    }

    #[tokio::test]
    async fn audit_actions_disconnect_keeps_permits_until_work_finishes() {
        let slots = Arc::new(Semaphore::new(1));
        let waits = Arc::new(Semaphore::new(1));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let (slot_copy, wait_copy) = (slots.clone(), waits.clone());
        let request = tokio::spawn(async move {
            run_bounded_action(slot_copy, wait_copy, true, move || {
                let _ = started_tx.send(());
                let _ = finish_rx.recv_timeout(Duration::from_secs(2));
                json!({"ok":true})
            }).await
        });
        started_rx.await.unwrap();
        request.abort();
        let _ = request.await;
        assert_eq!(slots.available_permits(), 0);
        assert_eq!(waits.available_permits(), 0);
        finish_tx.send(()).unwrap();
        for _ in 0..100 {
            if slots.available_permits() == 1 { break; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(slots.available_permits(), 1);
        assert_eq!(waits.available_permits(), 1);
    }
}
