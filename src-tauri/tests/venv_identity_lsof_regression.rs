mod common;

use coding_tools_mcp_desktop_lib::tools::{call_tool, policy::{PolicySettings, validate_tool_arguments}};
use serde_json::json;

#[test]
fn bounded_listener_diagnostic_is_allowed_without_a_shell() {
    let policy = PolicySettings::default();
    let args = json!({"cmd":"lsof -nP -iTCP:13000 -sTCP:LISTEN", "timeout_ms":5000});
    assert!(validate_tool_arguments("exec_command", &args, &policy).is_ok());
    assert!(!policy.allowed_commands.contains("kill"));
}

#[cfg(target_os = "macos")]
#[test]
fn installed_versioned_python_preserves_workspace_venv_identity() {
    use std::os::unix::fs::symlink;
    let python = which::which("python3.12").expect("macOS regression needs installed Python 3.12");
    let root = tempfile::tempdir().unwrap();
    let venv = root.path().join("isolated venv");
    std::fs::create_dir_all(venv.join("bin")).unwrap();
    std::fs::write(venv.join("pyvenv.cfg"), "include-system-site-packages = false\n").unwrap();
    symlink(&python, venv.join("bin/python")).unwrap();
    let script = root.path().join("prefix.py");
    std::fs::write(&script, "import sys\nprint(sys.prefix)\n").unwrap();
    let ctx = common::ctx_for(root.path());
    let result = call_tool(&ctx, "exec_command", &json!({
        "cmd":format!("\"{}\" -B \"{}\"", venv.join("bin/python").display(), script.display()),
        "durable":false, "yield_time_ms":1000
    }));
    assert_eq!(result["command_ok"], true, "{result}");
    let prefix = result["stdout"].as_str().unwrap().trim();
    assert_eq!(std::path::Path::new(prefix).canonicalize().unwrap(), venv.canonicalize().unwrap());
}

#[cfg(target_os = "macos")]
#[test]
fn absolute_versioned_python_and_permission_preflight_agree() {
    let python = which::which("python3.12").expect("installed Python 3.12");
    let root = tempfile::tempdir().unwrap();
    let mut ctx = common::ctx_for(root.path());
    ctx.policy.permission_mode = "dangerous".into();
    let args = json!({"cmd":format!("\"{}\" --version", python.display()), "durable":false});
    let permission = call_tool(&ctx, "request_permissions", &json!({
        "tool_name":"exec_command", "permission":"privileged_executable",
        "arguments":args, "reason":"isolated trusted interpreter regression"
    }));
    assert_eq!(permission["ok"], true, "{permission}");
    let result = call_tool(&ctx, "exec_command", &args);
    assert_eq!(result["command_ok"], true, "{result}");
}

#[cfg(unix)]
#[test]
fn external_versioned_impostor_is_rejected_even_through_a_venv_link() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let outer = tempfile::tempdir().unwrap();
    let workspace = outer.path().join("workspace");
    std::fs::create_dir_all(workspace.join("venv/bin")).unwrap();
    let fake = outer.path().join("python3.12");
    std::fs::write(&fake, "#!/bin/sh\necho SHOULD_NOT_EXECUTE\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let link = workspace.join("venv/bin/python");
    symlink(&fake, &link).unwrap();
    let ctx = common::ctx_for(&workspace);
    for entry in [&fake, &link] {
        let result = call_tool(&ctx, "exec_command", &json!({"cmd":format!("\"{}\" --version", entry.display())}));
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(result["error"]["code"], "EXECUTABLE_OUTSIDE_WORKSPACE", "{result}");
        assert!(result.get("job_id").is_none());
    }
}
