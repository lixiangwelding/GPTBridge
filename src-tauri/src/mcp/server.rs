use std::sync::Arc;

use serde_json::Value;

use crate::audit::{AuditRequestContext, AuditStore};
use crate::mcp::upstream::UpstreamMcpManager;
use crate::tools::{
    call_tool_with_audit, list_tools_for_profile, record_tool_rejection_with_audit,
    wrap_mcp_tool_result, SharedToolContext, ToolContext, Workspace,
};
use crate::workspace::AuthConfig;

pub struct McpState {
    pub tools: SharedToolContext,
    pub upstream: Arc<UpstreamMcpManager>,
}

impl McpState {
    pub fn audit_store(&self) -> Option<AuditStore> {
        self.tools.audit_store()
    }
}

pub type SharedState = Arc<McpState>;

// 生产入口接收监听层采集的 headers 上下文；原 handle_request 仅为无 HTTP 环境的测试生成
// 最小上下文。协议分发保持与 Axum 解耦，且只让 tools/call 进入工具审计。
#[cfg(test)]
pub fn handle_request(state: &SharedState, body: &Value) -> Value {
    let request = AuditRequestContext {
        transport: "mcp".into(),
        method: body
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string),
        request_id: body.get("id").and_then(crate::audit::request_id_from_value),
        route: Some("/mcp".into()),
        ..AuditRequestContext::default()
    };
    handle_request_with_context(state, body, &request)
}

pub fn handle_request_with_context(
    state: &SharedState,
    body: &Value,
    request: &AuditRequestContext,
) -> Value {
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let params = body.get("params").cloned().unwrap_or(Value::Null);

    if id.is_null() && method.starts_with("notifications/") {
        return Value::Null;
    }

    let result = match method {
        "initialize" => {
            let mut initialized = initialize_result();
            crate::tools::skill_discovery::append_to_instructions(&state.tools, &mut initialized);
            Ok(initialized)
        },
        "ping" => Ok(serde_json::json!({})),
        "tools/list" => {
            let mut tools = list_tools_for_profile(&state.tools.tool_profile);
            crate::tools::skill_discovery::decorate_tools(&state.tools, &mut tools);
            tools.extend(state.upstream.public_tools().iter().cloned());
            Ok(serde_json::json!({ "tools": tools }))
        }
        "tools/call" => handle_tools_call(state, &params, request),
        _ => Err(serde_json::json!({
            "code": -32601,
            "message": format!("Method not found: {method}")
        })),
    };

    match result {
        Ok(result) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    }
}

pub(crate) fn initialize_result() -> Value {
    serde_json::json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "tools": { "listChanged": false },
            "logging": {}
        },
        "serverInfo": {
            "name": "coding-tools-mcp",
            "title": "Coding Tools MCP Personal",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": format!("{} {}", crate::tools::personal::INSTRUCTIONS, crate::tools::skills::INSTRUCTIONS)
    })
}

fn handle_tools_call(
    state: &SharedState,
    params: &Value,
    request: &AuditRequestContext,
) -> Result<Value, Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| serde_json::json!({ "code": -32602, "message": "Missing tool name" }))?;
    let args = tool_arguments(name, params);

    if state.upstream.owns_tool(name) {
        let result = tauri::async_runtime::block_on(state.upstream.call_tool(name, args));
        return Ok(normalize_upstream_result(result));
    }

    let canonical_name = crate::tools::registry::canonical_tool_name(name);
    let known = crate::tools::registry::exposed_tool_names(&state.tools.tool_profile);
    // 未知/未暴露工具在 dispatcher 前提前返回，拒绝记录必须放在这里；通过校验的路径改用
    // audited wrapper，审计成功与否都不改变原 MCP 结果。
    if !known.iter().any(|n| n == &canonical_name) {
        let message = format!("Unknown tool: {name}");
        record_tool_rejection_with_audit(
            state.tools.as_ref(),
            request,
            name,
            &args,
            "UNKNOWN_TOOL",
            &message,
        );
        return Err(serde_json::json!({
            "code": -32602,
            "message": message,
            "data": { "reason": "unknown_tool" }
        }));
    }

    let structured = call_tool_with_audit(state.tools.as_ref(), canonical_name, &args, request);
    Ok(wrap_mcp_tool_result(canonical_name, &args, structured))
}

fn normalize_upstream_result(result: Result<Value, String>) -> Value {
    match result {
        Ok(result) if result.get("content").is_some() => result,
        Ok(result) => serde_json::json!({
            "content": [{ "type": "text", "text": result.to_string() }],
            "structuredContent": result,
            "isError": false
        }),
        Err(message) => serde_json::json!({
            "content": [{ "type": "text", "text": message }],
            "structuredContent": {
                "ok": false,
                "status": "error",
                "error": {
                    "category": "upstream_mcp",
                    "message": "本地 MCP 工具调用失败"
                }
            },
            "isError": true
        }),
    }
}

fn tool_arguments(name: &str, params: &Value) -> Value {
    let mut args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if name.starts_with("history_session_") || matches!(name, "task_open" | "task_status") {
        if !args.is_object() { args = serde_json::json!({}); }
        args.as_object_mut().unwrap().remove("_host_session_key");
        if let Some(session_key) = params
            .get("_meta")
            .and_then(|meta| meta.get("openai/session"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !args.is_object() {
                args = serde_json::json!({});
            }
            args["_host_session_key"] = Value::String(session_key.to_string());
        }
    }
    args
}

// 审计在 SharedState 创建时绑定：此处同时拥有正式 workspace_id，且尚未被多请求共享；
// 测试构造路径可不调用 with_audit，因此不会写入用户数据库。
pub fn new_state(
    workspace: Workspace,
    workspace_id: String,
    auth: AuthConfig,
    policy: crate::tools::policy::PolicySettings,
    tool_profile: String,
    permission_mode: String,
    upstream: Arc<UpstreamMcpManager>,
) -> SharedState {
    Arc::new(McpState {
        tools: Arc::new(
            ToolContext::from_workspace(workspace, auth, policy, tool_profile, permission_mode)
                .with_audit(workspace_id),
        ),
        upstream,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use serde_json::json;

    use crate::mcp::upstream::UpstreamMcpManager;
    use crate::tools::ToolContext;

    use super::{handle_request, initialize_result, tool_arguments, McpState};

    #[test]
    fn initialize_instructions_define_the_history_persistence_workflow() {
        let initialized = initialize_result();
        let instructions = initialized["instructions"].as_str().expect("instructions");
        assert!(instructions.contains("task_open"));
        assert!(instructions.contains("task_checkpoint"));
        assert!(instructions.contains("no pasted startup prompt"));
        assert!(instructions.contains("raw_user_input"));
        assert!(instructions.contains("Legacy history_session"));
        assert!(instructions.contains("Unknown results"));
        assert!(!instructions.contains("call history_session_bootstrap exactly once"));
    }

    #[test]
    fn initialize_does_not_claim_tool_catalog_notifications_without_a_stream() {
        let initialized = initialize_result();

        assert_eq!(initialized["capabilities"]["tools"]["listChanged"], false);
    }

    #[test]
    fn workspace_prompt_initializes_or_restores_a_chatgpt_session() {
        let component = include_str!("../../../src/lib/components/ChatGptSessionPrompt.svelte");
        assert!(component.contains("无需复制初始化提示词"));
        assert!(component.contains("task_id"));
        assert!(component.contains("history_session"));
        assert!(!component.contains("复制完整提示词"));
    }

    #[test]
    fn chatgpt_session_metadata_is_injected_only_for_history_tools() {
        let params = json!({
            "arguments": {"session_key": "explicit"},
            "_meta": {"openai/session": "chatgpt-conversation"}
        });
        let history = tool_arguments("history_session_bootstrap", &params);
        assert_eq!(history["session_key"], "explicit");
        assert_eq!(history["_host_session_key"], "chatgpt-conversation");

        let existing = tool_arguments("read_file", &params);
        assert_eq!(existing["session_key"], "explicit");
        assert!(existing.get("_host_session_key").is_none());
    }

    #[test]
    fn host_session_key_takes_precedence_over_explicit_session_key() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let harness = tempfile::tempdir().expect("harness tempdir");
        let state = Arc::new(McpState {
            tools: Arc::new(
                ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                    .expect("tool context"),
            ),
            upstream: Arc::new(UpstreamMcpManager::empty()),
        });
        let response = handle_request(
            &state,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "history_session_bootstrap",
                    "arguments": {
                        "session_key": "explicit-session",
                        "initial_user_input": "保存首轮原文"
                    },
                    "_meta": {"openai/session": "chatgpt-session"}
                }
            }),
        );
        let structured = &response["result"]["structuredContent"];
        assert_eq!(structured["ok"], true);
        assert_eq!(structured["session_key_source"], "platform_conversation_id");
        assert_eq!(structured["session_key"], "chatgpt-session");
        assert_eq!(structured["initial_input_captured"], true);
        let content = fs::read_to_string(workspace.path().join("docs/history-session/1.md"))
            .expect("read history file");
        assert!(content.contains("**Session key:** chatgpt-session"));
        assert!(!content.contains("**Session key:** explicit-session"));
    }

    #[test]
    fn legacy_grep_calls_are_mapped_to_the_public_grep_text_tool() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let harness = tempfile::tempdir().expect("harness tempdir");
        fs::write(workspace.path().join("sample.txt"), "catalog needle")
            .expect("write sample file");
        let state = Arc::new(McpState {
            tools: Arc::new(
                ToolContext::for_test(workspace.path().to_path_buf(), harness.path().to_path_buf())
                    .expect("tool context"),
            ),
            upstream: Arc::new(UpstreamMcpManager::empty()),
        });

        let response = handle_request(
            &state,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "grep",
                    "arguments": {"query": "needle", "path": "."}
                }
            }),
        );

        assert!(response.get("error").is_none());
        assert_eq!(response["result"]["structuredContent"]["ok"], true);
    }
}
