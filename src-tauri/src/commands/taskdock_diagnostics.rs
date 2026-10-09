//! Read-only protocol observations of the saved local MCP endpoint.
//! Credentials stay in this process; this command never starts or reconfigures a listener.
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tauri::State;

use crate::{app_state::AppState, error::AppResult, workspace::WorkspaceProfile};

const BODY_LIMIT: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(12);

enum Credentials {
    None,
    Bearer(Option<String>),
    OAuth {
        client_id: String,
        password: Option<String>,
    },
    Unsupported,
}

impl Credentials {
    fn mode(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bearer(_) => "bearer",
            Self::OAuth { .. } => "oauth",
            Self::Unsupported => "unknown",
        }
    }

    fn sensitive_values(&self) -> Vec<String> {
        match self {
            Self::Bearer(Some(token)) => vec![token.clone()],
            Self::OAuth {
                password: Some(password),
                ..
            } => vec![password.clone()],
            _ => Vec::new(),
        }
    }
}

#[tauri::command]
pub async fn taskdock_protocol_diagnostics(
    state: State<'_, AppState>,
    workspace_id: String,
) -> AppResult<Value> {
    let profile = super::taskdock::profile(&state, &workspace_id)?;
    let tracked = state.with_runtime(|runtime| {
        Ok(runtime.is_running(&profile.id, crate::runtime::ServiceKind::Mcp))
    })?;
    probe_if_owned(&profile, tracked, || credentials_for(&state, &profile)).await
}

fn credentials_for(state: &AppState, profile: &WorkspaceProfile) -> AppResult<Credentials> {
    // DataStore getters only clone already-loaded values. No initialization, writes, or disk locks.
    state.with_data(|store| {
        let secret = |key: &str| -> AppResult<Option<String>> {
            if profile.auth.use_shared_secrets {
                Ok(store.get_shared_secret(key))
            } else {
                store.get_workspace_secret(&profile.id, key)
            }
        };
        Ok(if profile.auth.bearer_enabled() {
            Credentials::Bearer(secret("bearer_token")?)
        } else if profile.auth.oauth_enabled() {
            let client_id = if profile.auth.use_shared_secrets {
                store
                    .get_shared_secret("oauth_client_id")
                    .unwrap_or_else(|| profile.auth.oauth_client_id.clone())
            } else {
                profile.auth.oauth_client_id.clone()
            };
            Credentials::OAuth {
                client_id,
                password: secret("oauth_password")?,
            }
        } else if matches!(profile.auth.auth_type.as_str(), "noauth" | "none") {
            Credentials::None
        } else {
            Credentials::Unsupported
        })
    })
}

fn configured_mode(profile: &WorkspaceProfile) -> &'static str {
    if profile.auth.bearer_enabled() {
        "bearer"
    } else if profile.auth.oauth_enabled() {
        "oauth"
    } else if matches!(profile.auth.auth_type.as_str(), "noauth" | "none") {
        "none"
    } else {
        "unknown"
    }
}

fn confirmed_listener_pid(profile: &WorkspaceProfile, tracked: bool) -> Option<u32> {
    let actual = crate::platform::platform()
        .find_pid_listening_on_port(profile.runtime.local_port)
        .ok()??;
    if tracked && crate::runtime::is_own_process(actual) {
        return Some(actual);
    }
    crate::runtime::managed_personal_mcp_pid(&profile.id, profile.runtime.local_port)
        .filter(|expected| *expected == actual)
}

async fn probe_if_owned(
    profile: &WorkspaceProfile,
    tracked: bool,
    read_credentials: impl FnOnce() -> AppResult<Credentials>,
) -> AppResult<Value> {
    let began = Instant::now();
    let profile_to_check = profile.clone();
    // Inspect only the OS listener/process identity before even reading saved credentials.
    // The existing managed matcher checks the exact executable, launchd job, profile and port.
    let owner = tokio::time::timeout(
        REQUEST_TIMEOUT,
        tokio::task::spawn_blocking(move || confirmed_listener_pid(&profile_to_check, tracked)),
    )
    .await;
    if !matches!(owner, Ok(Ok(Some(_)))) {
        let mut report = initial_report(profile, configured_mode(profile));
        report["auth"]["detail"] = json!("未确认选中项目的受管监听器身份，未读取或发送鉴权材料");
        report["observed_at"] = json!(coding_tools_personal_runtime::now_ms());
        return Ok(report);
    }
    let credentials = read_credentials()?;
    Ok(probe_with_budget(
        profile,
        credentials,
        TOTAL_TIMEOUT.saturating_sub(began.elapsed()),
    )
    .await)
}

fn initial_report(profile: &WorkspaceProfile, mode: &str) -> Value {
    json!({
        "workspace_id": profile.id, "endpoint": profile.local_endpoint(),
        "observed_at": coding_tools_personal_runtime::now_ms(),
        "auth": {"status":"unavailable", "mode":mode, "http_status":null, "detail":"尚未验证入口鉴权"},
        "handshake": {"status":"unavailable", "protocol_version":null, "server_name":null,
            "server_version":null, "detail":"尚未完成协议握手"},
        "catalog": {"status":"unavailable", "count":null, "sha256":null,
            "list_changed_supported":null, "detail":"尚未读取运行中的工具目录"},
        "runtime_policy": {"status":"unavailable", "workspace":null, "tool_profile":null,
            "permission_mode":null, "direct_workspace":null, "capabilities":null,
            "auth_enabled":null, "auth_type":null, "detail":"尚未读取运行中的工作区策略"},
        "client_cache": {"status":"not_observable"}, "dot_route": {"status":"not_observable"}
    })
}

async fn probe_with_budget(
    profile: &WorkspaceProfile,
    credentials: Credentials,
    budget: Duration,
) -> Value {
    let mut report = initial_report(profile, credentials.mode());
    let expected = match PathBuf::from(&profile.path).canonicalize() {
        Ok(path) if Path::new(&profile.path).is_absolute() && path.is_dir() => path,
        _ => {
            report["runtime_policy"]["detail"] = json!("项目目录不可用，未发送诊断请求");
            return report;
        }
    };
    let client = match Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            report["auth"]["detail"] = json!("无法建立本地诊断客户端");
            return report;
        }
    };
    let outcome = tokio::time::timeout(
        budget,
        probe_stages(&client, profile, &expected, credentials, &mut report),
    )
    .await;
    if outcome.is_err() {
        for stage in ["auth", "handshake", "catalog", "runtime_policy"] {
            if report[stage]["status"] == "unavailable" {
                report[stage]["status"] = json!("failed");
                report[stage]["detail"] = json!("本地协议诊断超过总时间限制");
                break;
            }
        }
    }
    report["observed_at"] = json!(coding_tools_personal_runtime::now_ms());
    report
}

// HTTP fixtures supply a trusted isolated listener explicitly. They exercise the protocol
// stages, not the production OS-ownership gate. A separate negative test uses the real gate.
#[cfg(test)]
async fn probe(profile: &WorkspaceProfile, credentials: Credentials) -> Value {
    probe_with_budget(profile, credentials, TOTAL_TIMEOUT).await
}

struct ProbeFailure {
    status: Option<u16>,
    detail: &'static str,
}

impl ProbeFailure {
    fn plain(detail: &'static str) -> Self {
        Self {
            status: None,
            detail,
        }
    }
    fn http(status: StatusCode, detail: &'static str) -> Self {
        Self {
            status: Some(status.as_u16()),
            detail,
        }
    }
}

fn fail(report: &mut Value, stage: &str, failure: ProbeFailure) {
    report[stage]["status"] = json!("failed");
    report[stage]["detail"] = json!(failure.detail);
    if stage == "auth" {
        report["auth"]["http_status"] = json!(failure.status);
    } else if matches!(failure.status, Some(401 | 403)) {
        report["auth"]["status"] = json!("failed");
        report["auth"]["http_status"] = json!(failure.status);
        report["auth"]["detail"] = json!("运行中的入口拒绝了保存的鉴权材料");
    }
}

#[path = "taskdock_diagnostics_runtime.rs"]
mod diagnostics_runtime;
#[path = "taskdock_diagnostics_transport.rs"]
mod transport;
use diagnostics_runtime::probe_stages;

#[cfg(test)]
#[path = "taskdock_diagnostics_tests.rs"]
mod tests;
