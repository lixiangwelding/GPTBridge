use coding_tools_mcp_desktop_lib::tools::{call_tool, ToolContext};
use serde_json::{json, Value};

fn context(root: &std::path::Path) -> ToolContext {
    let workspace = root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let mut ctx = ToolContext::for_test(workspace, root.join("state")).unwrap();
    ctx.policy.permission_mode = "dangerous".into();
    ctx.permission_mode = "dangerous".into();
    ctx
}

fn request(ctx: &ToolContext, arguments: Value) -> Value {
    call_tool(ctx, "request_permissions", &json!({
        "tool_name":"exec_command", "permission":"privileged_executable",
        "arguments":arguments, "scope":"once", "reason":"isolated preflight regression"
    }))
}

#[test]
fn arbitrary_external_executable_is_denied_before_grant() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(tmp.path());
    let external = tmp.path().join(if cfg!(windows) { "python.exe" } else { "python3" });
    std::fs::write(&external, "MUST NOT EXECUTE").unwrap();
    let arguments = json!({"cmd":format!("\"{}\" --version", external.display())});
    let permission = request(&ctx, arguments.clone());
    let actual = call_tool(&ctx, "exec_command", &arguments);
    assert_eq!(permission["ok"], false, "{permission}");
    assert_eq!(permission["status"], "denied", "{permission}");
    assert_eq!(permission["grant_id"], Value::Null);
    assert_eq!(permission["error"]["code"], "EXECUTABLE_OUTSIDE_WORKSPACE");
    assert_eq!(permission["error"]["code"], actual["error"]["code"]);
    assert!(actual.get("job_id").is_none());
    assert_eq!(permission["command_executed"], false);
}

#[test]
fn missing_program_is_not_reported_granted() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(tmp.path());
    let result = request(&ctx, json!({"cmd":"missing-directory/python3 --version"}));
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["status"], "denied");
    assert_eq!(result["grant_id"], Value::Null);
}

#[test]
fn external_workdir_remains_forbidden() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(tmp.path());
    let result = request(&ctx, json!({"cmd":"python3 --version", "workdir":tmp.path()}));
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["status"], "denied");
    assert_eq!(result["command_executed"], false);
}

#[test]
fn allowlisted_path_program_can_be_acknowledged_without_execution() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(tmp.path());
    let program = if cfg!(windows) { "python" } else { "python3" };
    let result = request(&ctx, json!({"cmd":format!("{program} --version")}));
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["status"], "granted");
    assert!(result.get("job_id").is_none());
    assert!(result.get("command_ok").is_none());
}

#[test]
fn trusted_mode_still_requires_supported_elicitation() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ctx = context(tmp.path());
    ctx.policy.permission_mode = "trusted".into();
    let result = request(&ctx, json!({"cmd":"python3 --version"}));
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["status"], "unsupported");
}
