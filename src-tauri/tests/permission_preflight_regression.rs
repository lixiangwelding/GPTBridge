use coding_tools_mcp_desktop_lib::tools::{call_tool, ToolContext};
use serde_json::{json, Value};

fn dangerous(root: &std::path::Path) -> ToolContext {
    // Keep the task database and audit fixtures under this test temporary root.
    let mut ctx = ToolContext::for_test(root.to_path_buf(), root.join(".permission-state")).unwrap();
    ctx.policy.permission_mode = "dangerous".into();
    ctx.permission_mode = "dangerous".into();
    ctx
}

fn request(ctx: &ToolContext, arguments: Value) -> Value {
    call_tool(ctx, "request_permissions", &json!({
        "tool_name": "exec_command", "permission": "privileged_executable",
        "arguments": arguments, "reason": "isolated executable preflight regression"
    }))
}

#[test]
fn permission_does_not_grant_an_outside_impersonator() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let fake = root.path().join(if cfg!(windows) { "python.exe" } else { "python3" });
    std::fs::write(&fake, "not a trusted executable").unwrap();
    let result = request(&dangerous(&project), json!({"cmd": format!("\"{}\" --version", fake.display())}));
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "EXECUTABLE_OUTSIDE_WORKSPACE", "{result}");
    assert!(result.get("job_id").is_none());
}

#[test]
fn permission_reports_missing_program_before_granting() {
    let root = tempfile::tempdir().unwrap();
    // Allowlisted basename reaches the resolver; the path deliberately does not exist.
    let result = request(&dangerous(root.path()), json!({"cmd": "./missing/node --version"}));
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "COMMAND_REJECTED", "{result}");
}

#[test]
fn permission_does_not_skip_command_allowlist() {
    let root = tempfile::tempdir().unwrap();
    let result = request(&dangerous(root.path()), json!({"cmd": "./missing-node --version"}));
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "POLICY_REJECTED", "{result}");
    assert_eq!(result["status"], "denied");
    assert_eq!(result["command_executed"], false);
}

#[test]
fn permission_keeps_workdir_boundary() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let result = request(&dangerous(&project), json!({"cmd": "echo safe", "workdir": root.path()}));
    assert_eq!(result["ok"], false, "{result}");
}

#[cfg(unix)]
#[test]
fn permission_preflight_does_not_execute_the_command() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let entry = root.path().join("probe");
    std::fs::write(&entry, "#!/bin/sh\nprintf executed > marker.txt\n").unwrap();
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = request(&dangerous(root.path()), json!({"cmd": "./probe"}));
    assert_eq!(result["status"], "granted", "{result}");
    assert!(!root.path().join("marker.txt").exists());
    assert!(result.get("job_id").is_none());
}
