use super::super::{probe, Credentials, BODY_LIMIT};
use crate::workspace::{AuthConfig, WorkspaceProfile};
use axum::{
    extract::State as HttpState,
    http::HeaderMap,
    response::{IntoResponse, Response as HttpResponse},
    routing::{get, post},
    Form, Json, Router,
};
use reqwest::header::LOCATION;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

pub(super) const BEARER: &str = "isolated-diagnostic-bearer-credential";
pub(super) const PASSWORD: &str = "isolated-diagnostic-password";
pub(super) const TOKEN_SECRET: &str = "isolated-diagnostic-server-signing-secret";

#[derive(Clone, Copy)]
pub(super) enum Auth {
    None,
    NoneAlias,
    Bearer,
    OAuth,
}
#[derive(Clone, Copy)]
pub(super) enum Mode {
    Healthy,
    Redirect,
    InvalidJson,
    Oversized,
    WrongTarget,
    Gateway,
    OmitServerInfo,
    WrongId,
    OAuthRedirect,
    Leaky,
}

#[derive(Default)]
pub(super) struct Observations {
    pub(super) calls: Mutex<Vec<Value>>,
    pub(super) callback_requests: AtomicUsize,
    pub(super) token_requests: AtomicUsize,
    pub(super) authorize_requests: AtomicUsize,
}
struct FixtureState {
    auth: Auth,
    auth_config: AuthConfig,
    mode: Mode,
    path: String,
    id: String,
    base: String,
    oauth: crate::auth::OAuthRuntime,
    observations: Arc<Observations>,
}
pub(super) struct Fixture {
    pub(super) profile: WorkspaceProfile,
    pub(super) observations: Arc<Observations>,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
    _directory: tempfile::TempDir,
    auth: Auth,
}

impl Fixture {
    pub(super) async fn new(auth: Auth, mode: Mode) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let mut profile = WorkspaceProfile::new(
            directory.path().to_string_lossy().into_owned(),
            Some("fixture".into()),
        );
        profile.runtime.local_port = port;
        profile.runtime.tool_profile = "read-only".into(); // The report must use the live policy, not this setting.
        profile.auth.auth_type = match auth {
            Auth::None => "noauth",
            Auth::NoneAlias => "none",
            Auth::Bearer => "bearer",
            Auth::OAuth => "oauth",
        }
        .into();
        profile.auth.oauth_client_id = "fixture-client".into();
        let observations = Arc::new(Observations::default());
        let state = Arc::new(FixtureState {
            auth,
            auth_config: profile.auth.clone(),
            mode,
            path: directory
                .path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            id: profile.id.clone(),
            base: base.clone(),
            oauth: crate::auth::OAuthRuntime::new(
                base,
                "fixture-client".into(),
                None,
                PASSWORD.into(),
                TOKEN_SECRET.into(),
            ),
            observations: observations.clone(),
        });
        let app = Router::new()
            .route("/mcp", post(mcp))
            .route("/oauth/authorize", post(authorize))
            .route("/oauth/token", post(token))
            .route("/trap", get(trap).post(trap))
            .route("/__taskdock_diagnostic_callback", get(trap).post(trap))
            .with_state(state);
        let (shutdown, receiver) = oneshot::channel();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = receiver.await;
                })
                .await
                .unwrap();
        });
        Self {
            profile,
            observations,
            shutdown: Some(shutdown),
            handle: Some(handle),
            _directory: directory,
            auth,
        }
    }
    pub(super) fn credentials(&self) -> Credentials {
        match self.auth {
            Auth::None | Auth::NoneAlias => Credentials::None,
            Auth::Bearer => Credentials::Bearer(Some(BEARER.into())),
            Auth::OAuth => Credentials::OAuth {
                client_id: "fixture-client".into(),
                password: Some(PASSWORD.into()),
            },
        }
    }
    pub(super) async fn report(&self) -> Value {
        probe(&self.profile, self.credentials()).await
    }
    pub(super) async fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.await.unwrap();
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

async fn mcp(
    HttpState(state): HttpState<Arc<FixtureState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> HttpResponse {
    match state.auth {
        Auth::Bearer => {
            if headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                != Some(format!("Bearer {BEARER}").as_str())
            {
                return (
                    StatusCode::UNAUTHORIZED,
                    format!("private rejection body: {BEARER}"),
                )
                    .into_response();
            }
        }
        Auth::OAuth => {
            if let Some(rejected) =
                crate::auth::verify_oauth_bearer_header(&headers, &state.oauth, &state.base)
            {
                return rejected;
            }
        }
        Auth::None | Auth::NoneAlias => {}
    }
    state.observations.calls.lock().unwrap().push(body.clone());
    match state.mode {
        Mode::Redirect => {
            return (
                StatusCode::FOUND,
                [(LOCATION, format!("{}/trap", state.base))],
            )
                .into_response()
        }
        Mode::InvalidJson => {
            return format!("invalid JSON private body: {PASSWORD}").into_response()
        }
        Mode::Oversized => return "x".repeat(BODY_LIMIT + 1).into_response(),
        _ => {}
    }
    let id = if matches!(state.mode, Mode::WrongId) {
        json!("different-request")
    } else {
        body["id"].clone()
    };
    let result = match body["method"].as_str() {
        Some("initialize") => {
            let mut result = crate::mcp::server::initialize_result();
            result["capabilities"]["private_token"] = json!(TOKEN_SECRET);
            result
        }
        Some("tools/list") => {
            let required = if matches!(state.mode, Mode::Gateway) {
                json!(["workspace_id"])
            } else {
                json!([])
            };
            let tools = if matches!(state.mode, Mode::OmitServerInfo) {
                json!([])
            } else {
                json!([{
                    "name":"server_info", "inputSchema":{"type":"object", "properties":{}, "required":required}
                }])
            };
            json!({"tools":tools})
        }
        Some("tools/call") => {
            if matches!(state.mode, Mode::Gateway)
                && body["params"]["arguments"]["workspace_id"] != state.id
            {
                return Json(json!({"jsonrpc":"2.0", "id":id,
                    "result":{"isError":true,"structuredContent":{"ok":false}}}))
                .into_response();
            }
            let path = if matches!(state.mode, Mode::WrongTarget) {
                "/"
            } else {
                &state.path
            };
            let tool_profile = if matches!(state.mode, Mode::Leaky) {
                BEARER
            } else {
                "core"
            };
            let mut info = json!({"ok":true, "server":"coding-tools-mcp", "workspace":path,
                "tool_profile":tool_profile, "permission_mode":"safe", "auth_enabled":state.auth_config.auth_enabled(),
                "auth_type":state.auth_config.auth_type,
                "private_token":TOKEN_SECRET,
                "direct_workspace":{"version":1, "execution_path":"native_tools", "external_agent_required":false,
                    "durable_bookkeeping":"existing_task_and_request_lifecycle",
                    "catalog_refresh":"client_dependent_no_list_changed_notification", "private_password":PASSWORD,
                    "security":{"execution_isolation":"policy_only","sandbox_enforced":false,"read_scope":"workspace_root"}}
            });
            if matches!(state.mode, Mode::Gateway) {
                info["workspace_id"] = json!(state.id);
            }
            json!({"isError":false, "structuredContent":info})
        }
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    Json(json!({"jsonrpc":"2.0", "id":id, "result":result})).into_response()
}
async fn authorize(
    HttpState(state): HttpState<Arc<FixtureState>>,
    Form(form): Form<crate::auth::AuthorizeForm>,
) -> HttpResponse {
    state
        .observations
        .authorize_requests
        .fetch_add(1, Ordering::SeqCst);
    if matches!(state.mode, Mode::OAuthRedirect) {
        return (
            StatusCode::SEE_OTHER,
            [(
                LOCATION,
                format!("{}/trap?code=private-code&state={}", state.base, form.state),
            )],
        )
            .into_response();
    }
    crate::auth::authorize_post(&state.oauth, form, &state.base)
}
async fn token(
    HttpState(state): HttpState<Arc<FixtureState>>,
    headers: HeaderMap,
    Form(form): Form<crate::auth::TokenForm>,
) -> HttpResponse {
    state
        .observations
        .token_requests
        .fetch_add(1, Ordering::SeqCst);
    crate::auth::token_exchange(&state.oauth, &headers, form, &state.base)
}
async fn trap(HttpState(state): HttpState<Arc<FixtureState>>) -> StatusCode {
    state
        .observations
        .callback_requests
        .fetch_add(1, Ordering::SeqCst);
    StatusCode::OK
}
pub(super) fn assert_no_secrets(report: &Value) {
    let encoded = report.to_string();
    for secret in [
        BEARER,
        PASSWORD,
        TOKEN_SECRET,
        "private-code",
        "access_token",
        "private_password",
        "private_token",
    ] {
        assert!(
            !encoded.contains(secret),
            "diagnostic DTO exposed a sensitive field"
        );
    }
}
