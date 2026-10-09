//! Projection of actual initialize, catalog and server_info observations.
use super::transport::{
    oauth_token, rpc, safe_capabilities, safe_direct_workspace, safe_text, tool_payload,
};
use super::{fail, Credentials, ProbeFailure};
use crate::workspace::WorkspaceProfile;
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) async fn probe_stages(
    client: &Client,
    profile: &WorkspaceProfile,
    expected: &Path,
    credentials: Credentials,
    report: &mut Value,
) {
    let requested_mode = credentials.mode();
    let mut sensitive = credentials.sensitive_values();
    let token = match credentials {
        Credentials::None => None,
        Credentials::Bearer(Some(token)) if !token.is_empty() => Some(token),
        Credentials::Bearer(_) => {
            report["auth"]["detail"] = json!("未保存可用的 Bearer 凭据，未发送诊断请求");
            return;
        }
        Credentials::OAuth {
            client_id,
            password: Some(password),
        } if !password.is_empty() => {
            let client_id = if client_id.is_empty() {
                "gptbridge-taskdock-diagnostics".to_string()
            } else {
                client_id
            };
            match oauth_token(client, profile.runtime.local_port, &client_id, &password).await {
                Ok(token) => {
                    sensitive.push(token.clone());
                    Some(token)
                }
                Err(failure) => {
                    fail(report, "auth", failure);
                    return;
                }
            }
        }
        Credentials::OAuth { .. } => {
            report["auth"]["detail"] = json!("未保存可用的 OAuth 口令，未发送诊断请求");
            return;
        }
        Credentials::Unsupported => {
            report["auth"]["detail"] = json!("当前保存的鉴权类型不受诊断接口支持");
            return;
        }
    };
    let endpoint = profile.local_endpoint();
    let initialized = match rpc(client, &endpoint, token.as_deref(), "initialize", json!({
        "protocolVersion":"2024-11-05", "capabilities":{},
        "clientInfo":{"name":"gptbridge-taskdock-diagnostics","version":env!("CARGO_PKG_VERSION")}
    })).await {
        Ok((result, status)) => {
            report["auth"]["http_status"] = json!(status);
            // A successful request alone cannot prove a bearer was checked: noauth ignores it.
            report["auth"]["detail"] = json!("入口接受了协议请求，等待核对实际鉴权配置");
            result
        }
        Err(failure) => {
            if matches!(failure.status, Some(401 | 403)) { fail(report, "auth", failure); }
            else { fail(report, "handshake", failure); }
            return;
        }
    };
    let protocol = initialized["protocolVersion"].as_str();
    if initialized["serverInfo"]["name"] != "coding-tools-mcp"
        || !matches!(protocol, Some("2024-11-05" | "2025-03-26" | "2025-06-18"))
    {
        fail(
            report,
            "handshake",
            ProbeFailure::plain("入口未返回可识别的 GPTBridge 协议握手"),
        );
        return;
    }
    report["handshake"] = json!({"status":"passed", "protocol_version":protocol,
        "server_name":"coding-tools-mcp", "server_version":safe_text(&initialized["serverInfo"]["version"], &sensitive, 64),
        "detail":"已从运行中的入口完成 initialize 请求"});
    let changed = initialized["capabilities"]["tools"]["listChanged"].as_bool();
    report["catalog"]["list_changed_supported"] = json!(changed);
    let capabilities = safe_capabilities(&initialized["capabilities"]);
    let listed = match rpc(client, &endpoint, token.as_deref(), "tools/list", json!({})).await {
        Ok((result, _)) => result,
        Err(failure) => {
            fail(report, "catalog", failure);
            return;
        }
    };
    if listed
        .get("nextCursor")
        .is_some_and(|cursor| !cursor.is_null() && cursor != "")
    {
        fail(
            report,
            "catalog",
            ProbeFailure::plain("工具目录包含分页，当前诊断未确认完整目录"),
        );
        return;
    }
    let tools = match listed["tools"].as_array() {
        Some(tools)
            if tools.len() <= 4096
                && tools.iter().all(|tool| {
                    tool.is_object()
                        && tool["name"]
                            .as_str()
                            .is_some_and(|name| !name.is_empty() && name.len() <= 128)
                        && tool["inputSchema"].is_object()
                }) =>
        {
            tools
        }
        _ => {
            fail(
                report,
                "catalog",
                ProbeFailure::plain("入口返回的工具目录结构无效"),
            );
            return;
        }
    };
    let sha = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(tools).unwrap_or_default())
    );
    report["catalog"]["status"] = json!("passed");
    report["catalog"]["count"] = json!(tools.len());
    report["catalog"]["sha256"] = json!(sha);
    report["catalog"]["detail"] = json!("已读取本次运行中的目录；无法据此判断客户端缓存");
    let Some(info_tool) = tools.iter().find(|tool| tool["name"] == "server_info") else {
        report["runtime_policy"]["detail"] = json!("运行中的工具目录未公开 server_info");
        return;
    };
    let requires_workspace = info_tool["inputSchema"]["required"]
        .as_array()
        .is_some_and(|fields| fields.iter().any(|field| field == "workspace_id"));
    let args = if requires_workspace {
        json!({"workspace_id":profile.id})
    } else {
        json!({})
    };
    let result = match rpc(
        client,
        &endpoint,
        token.as_deref(),
        "tools/call",
        json!({"name":"server_info", "arguments":args}),
    )
    .await
    {
        Ok((result, _)) => result,
        Err(failure) => {
            fail(report, "runtime_policy", failure);
            return;
        }
    };
    let Some(info) = tool_payload(&result) else {
        fail(
            report,
            "runtime_policy",
            ProbeFailure::plain("server_info 未返回可用的结构化策略"),
        );
        return;
    };
    let workspace_matches = info["workspace"]
        .as_str()
        .filter(|path| Path::new(path).is_absolute())
        .and_then(|path| Path::new(path).canonicalize().ok())
        .as_deref()
        == Some(expected);
    let routing_matches = if requires_workspace {
        info["workspace_id"] == profile.id
    } else {
        info.get("workspace_id")
            .is_none_or(|id| id == &json!(profile.id))
    };
    if info["server"] != "coding-tools-mcp" || !workspace_matches || !routing_matches {
        fail(
            report,
            "runtime_policy",
            ProbeFailure::plain("运行中的 server_info 与选中项目不一致"),
        );
        return;
    }
    let actual_enabled = info["auth_enabled"].as_bool();
    let actual_type = info["auth_type"].as_str();
    let auth_matches = match requested_mode {
        "bearer" => actual_enabled == Some(true) && actual_type == Some("bearer"),
        "oauth" => actual_enabled == Some(true) && actual_type == Some("oauth"),
        "none" => actual_enabled == Some(false) && actual_type == Some("noauth"),
        _ => false,
    };
    if auth_matches {
        report["auth"]["status"] = json!("passed");
        report["auth"]["detail"] = json!(if requested_mode == "none" {
            "运行中的入口未启用认证，无需鉴权凭据"
        } else {
            "已核对运行中的鉴权类型，入口接受了保存的鉴权材料"
        });
    } else if actual_enabled.is_some() && actual_type.is_some() {
        report["auth"]["status"] = json!("failed");
        report["auth"]["detail"] = json!("运行中的入口鉴权与保存配置不同，未确认保存凭据有效");
    } else {
        report["auth"]["status"] = json!("unavailable");
        report["auth"]["detail"] = json!("运行中的入口未提供完整鉴权配置，未确认保存凭据有效");
    }
    report["runtime_policy"] = json!({"status":"passed", "workspace":expected.to_string_lossy(),
        "tool_profile":safe_text(&info["tool_profile"], &sensitive, 64),
        "permission_mode":safe_text(&info["permission_mode"], &sensitive, 64),
        "direct_workspace":safe_direct_workspace(&info["direct_workspace"], &sensitive),
        "capabilities":capabilities, "auth_enabled":info["auth_enabled"].as_bool(),
        "auth_type":safe_text(&info["auth_type"], &sensitive, 32),
        "detail":"已核对 server_info 的实际项目与运行策略"});
}
