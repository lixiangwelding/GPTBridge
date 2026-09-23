use super::*;
use crate::tools::{dispatch::call_tool, registry};
use std::{fs, path::Path, process::Command};

fn fixture() -> (tempfile::TempDir, ToolContext) {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("workspace")).unwrap();
    let ctx = ToolContext::for_test(temp.path().join("workspace"), temp.path().join("runtime")).unwrap();
    (temp, ctx)
}

fn observed(ctx: &ToolContext) -> Value {
    let contract = registry::catalog_contract(&ctx.tool_profile);
    let tools: Vec<_> = contract["input_fields"].as_object().unwrap().iter()
        .map(|(name, fields)| json!({"name":name,"input_fields":fields})).collect();
    json!({"client_tools":tools,"complete":true,"client_schema_sha256":contract["schema_sha256"]})
}

#[test]
fn toolbox_catalogs_expose_identical_read_only_schemas() {
    for profile in ["core", "advanced", "read-only", "compat-readonly-all"] {
        let tools = registry::list_tools_for_profile(profile);
        for name in TOOLS {
            let tool = tools.iter().find(|tool| tool["name"] == *name).unwrap();
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_eq!(tool["annotations"]["destructiveHint"], false);
            assert_eq!(tool["inputSchema"], registry::input_schema(name));
            assert!(registry::is_allowed_tool(name));
        }
    }
    assert_eq!(registry::input_schema("check_command")["properties"]["arguments"], registry::input_schema("exec_command"));
    assert!(registry::input_schema("git_diff")["properties"]["repo_path"].is_object());
}

#[test]
fn toolbox_complete_catalog_and_fingerprint_are_required_for_sync() {
    let (_temp, ctx) = fixture();
    let empty = call_tool(&ctx, "tool_catalog_check", &json!({}));
    assert_eq!(empty["client_catalog_synchronized"], Value::Null);
    assert_eq!(empty["missing_tools_definitive"], false);
    let mut args = observed(&ctx);
    assert_eq!(call_tool(&ctx, "tool_catalog_check", &args)["client_catalog_synchronized"], true);
    args.as_object_mut().unwrap().remove("client_schema_sha256");
    let names = call_tool(&ctx, "tool_catalog_check", &args);
    assert_eq!(names["status"], "names_and_fields_match");
    assert_eq!(names["client_catalog_synchronized"], Value::Null);
}

#[test]
fn toolbox_catalog_reports_missing_tool_and_patch_preconditions() {
    let (_temp, ctx) = fixture();
    let mut args = observed(&ctx);
    let tools = args["client_tools"].as_array_mut().unwrap();
    tools.retain(|tool| tool["name"] != "task_open");
    tools.iter_mut().find(|tool| tool["name"] == "apply_patch").unwrap()["input_fields"] = json!(["patch"]);
    let out = call_tool(&ctx, "tool_catalog_check", &args);
    assert_eq!(out["status"], "drift_detected");
    assert_eq!(out["client_catalog_synchronized"], false);
    assert!(out["missing_tools"].as_array().unwrap().contains(&json!("task_open")));
    assert!(out["missing_input_fields"]["apply_patch"].as_array().unwrap().contains(&json!("expected_hashes")));
}

#[test]
fn toolbox_catalog_detects_duplicates_unknowns_and_schema_drift() {
    let (_temp, ctx) = fixture();
    let duplicate = call_tool(&ctx, "tool_catalog_check", &json!({"client_tools":[{"name":"read_file"},{"name":"read_file"},{"name":"unknown_tool"}]}));
    assert_eq!(duplicate["duplicate_tools"], json!(["read_file"]));
    assert_eq!(duplicate["unexpected_tools"], json!(["unknown_tool"]));
    let mut args = observed(&ctx);
    args["client_schema_sha256"] = json!("0".repeat(64));
    assert_eq!(call_tool(&ctx, "tool_catalog_check", &args)["client_catalog_synchronized"], false);
    assert_eq!(call_tool(&ctx, "tool_catalog_check", &json!({"client_schema_sha256":"invalid"}))["ok"], false);
    assert_eq!(call_tool(&ctx, "tool_catalog_check", &json!({"client_tools":vec![json!({"name":"x"});257]}))["ok"], false);
}

#[test]
fn toolbox_batch_read_keeps_hashes_and_individual_errors() {
    let (_temp, ctx) = fixture();
    fs::write(ctx.workspace.root().join("a.txt"), "第一行\nsecond\n").unwrap();
    fs::write(ctx.workspace.root().join("b.txt"), "third\n").unwrap();
    let out = call_tool(&ctx, "read_files", &json!({"paths":["a.txt","missing.txt","b.txt"],"end_line":1}));
    assert_eq!(out["ok"], true);
    assert_eq!(out["failed_count"], 1);
    assert_eq!(out["results"][0]["content"], "第一行\n");
    assert_eq!(out["results"][0]["file_sha256"], json!(coding_tools_personal_runtime::digest("第一行\nsecond\n")));
    assert_eq!(out["results"][1]["error"]["code"], "NOT_FOUND");
    assert_eq!(out["results"][2]["content"], "third\n");
    assert_eq!(out["snapshot_atomic"], false);
}

#[test]
fn toolbox_batch_budget_is_unicode_safe_and_resumable() {
    let (_temp, ctx) = fixture();
    fs::write(ctx.workspace.root().join("a.txt"), "😀😀😀").unwrap();
    fs::write(ctx.workspace.root().join("b.txt"), "next").unwrap();
    let out = call_tool(&ctx, "read_files", &json!({"paths":["a.txt","b.txt"],"max_total_bytes":5}));
    assert!(out["content_bytes"].as_u64().unwrap() <= 5);
    assert_eq!(out["results"][0]["content"], "😀");
    assert_eq!(out["next_index"], 1);
    let next = call_tool(&ctx, "read_files", &json!({"paths":["a.txt","b.txt"],"start_index":1}));
    assert_eq!(next["results"][0]["requested_path"], "b.txt");
    assert_eq!(next["next_index"], Value::Null);
}

#[test]
fn toolbox_batch_rejects_invalid_bounds_and_external_paths() {
    let (temp, ctx) = fixture();
    fs::write(temp.path().join("secret.txt"), "not exposed").unwrap();
    for args in [json!({"paths":[]}), json!({"paths":vec!["x";21]}),
        json!({"paths":["x"],"max_total_bytes":0}), json!({"paths":["x"],"max_bytes_per_file":65537}),
        json!({"paths":["x"],"start_index":1}), json!({"paths":["x"],"start_line":5,"end_line":2})] {
        assert_eq!(call_tool(&ctx, "read_files", &args)["ok"], false, "{args}");
    }
    let out = call_tool(&ctx, "read_files", &json!({"paths":["../secret.txt", temp.path().join("secret.txt").to_str().unwrap()]}));
    assert_eq!(out["failed_count"], 2);
    assert!(!out.to_string().contains("not exposed"));
}

#[test]
fn toolbox_metadata_does_not_claim_write_permission() {
    let (_temp, ctx) = fixture();
    fs::write(ctx.workspace.root().join("data"), "abc").unwrap();
    let out = call_tool(&ctx, "stat_path", &json!({"path":"data"}));
    assert_eq!(out["size_bytes"], 3);
    assert_eq!(out["type"], "file");
    assert_eq!(out["write_authorized"], Value::Null);
    assert_eq!(out["content_read"], false);
    assert_eq!(call_tool(&ctx, "stat_path", &json!({"path":"."}))["type"], "directory");
    assert_eq!(call_tool(&ctx, "stat_path", &json!({"path":"../"}))["ok"], false);
}

#[cfg(unix)]
#[test]
fn toolbox_read_tools_reject_escaping_symlinks_and_directories() {
    let (temp, ctx) = fixture();
    fs::write(temp.path().join("secret"), "private payload").unwrap();
    std::os::unix::fs::symlink(temp.path().join("secret"), ctx.workspace.root().join("link")).unwrap();
    assert_eq!(call_tool(&ctx, "stat_path", &json!({"path":"link"}))["error"]["code"], "SYMLINK_ESCAPE");
    let out = call_tool(&ctx, "read_files", &json!({"paths":["link","."]}));
    assert_eq!(out["failed_count"], 2);
    assert!(!out.to_string().contains("private payload"));
}

#[test]
fn toolbox_preflight_does_not_execute_or_grant_authority() {
    let (_temp, ctx) = fixture();
    let out = call_tool(&ctx, "check_command", &json!({"arguments":{"cmd":"git init preflight-only-repo"}}));
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["status"], "allowed_locally");
    assert_eq!(out["command_executed"], false);
    assert_eq!(out["grant_issued"], false);
    assert_eq!(out["platform_authorization"], "not_observable");
    assert!(!ctx.workspace.root().join("preflight-only-repo").exists());
}

#[test]
fn toolbox_preflight_matches_execution_denial_and_default_cwd() {
    let (_temp, ctx) = fixture();
    for args in [json!({"cmd":"git --version","workdir":"../outside"}), json!({"cmd":"git --version; echo bad"})] {
        let check = call_tool(&ctx, "check_command", &json!({"arguments":args}));
        let execution = call_tool(&ctx, "exec_command", &args);
        assert_eq!(check["ok"], false);
        assert_eq!(check["error"], execution["error"]);
    }
    fs::create_dir(ctx.workspace.root().join("cwd")).unwrap();
    ctx.set_default_cwd(ctx.workspace.root().join("cwd"));
    fs::remove_dir(ctx.workspace.root().join("cwd")).unwrap();
    assert_eq!(call_tool(&ctx, "check_command", &json!({"arguments":{"cmd":"git --version"}}))["ok"], false);
}

#[test]
fn toolbox_all_exec_permission_kinds_reject_hard_boundary_failures() {
    let (_temp, mut ctx) = fixture();
    ctx.policy.permission_mode = "dangerous".into();
    for permission in ["network","long_timeout","inline_script","privileged_executable","write_generated_or_ignored","destructive_command","shell_expansion","sensitive_env"] {
        let out = call_tool(&ctx, "request_permissions", &json!({"tool_name":"exec_command","permission":permission,
            "reason":"test hard boundary","arguments":{"cmd":"git --version","workdir":"../outside"}}));
        assert_eq!(out["ok"], false, "{permission}: {out}");
        assert_eq!(out["status"], "denied");
        assert_eq!(out["grant_id"], Value::Null);
        assert_eq!(out["command_executed"], false);
    }
}

#[test]
fn toolbox_preflight_checks_task_execution_shape() {
    let (_temp, ctx) = fixture();
    let out = call_tool(&ctx, "check_command", &json!({"arguments":{"cmd":"git --version","task_id":"test-task"}}));
    assert_eq!(out["ok"], false);
    assert_eq!(out["error"]["code"], "INVALID_ARGUMENT");
    assert_eq!(call_tool(&ctx, "check_command", &json!({"arguments":[]}))["ok"], false);
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git").args(args).current_dir(cwd).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn toolbox_git_diff_selects_nested_repository_without_changing_default_cwd() {
    let (_temp, ctx) = fixture();
    let nested = ctx.workspace.root().join("nested");
    fs::create_dir(&nested).unwrap();
    git(ctx.workspace.root(), &["init"]);
    git(&nested, &["init"]);
    fs::write(nested.join("text.txt"), "before\n").unwrap();
    git(&nested, &["add","--","text.txt"]);
    git(&nested, &["-c","user.name=Test","-c","user.email=test@example.invalid","commit","-m","fixture"]);
    fs::write(nested.join("text.txt"), "after 汉字\n").unwrap();
    ctx.set_default_cwd(nested.clone());
    let out = call_tool(&ctx, "git_diff", &json!({"repo_path":"nested","paths":["text.txt"]}));
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["repository"], "nested");
    assert!(out["diff"].as_str().unwrap().contains("after 汉字"));
    assert_eq!(ctx.default_cwd_path(), nested);
    let cap = out["diff"].as_str().unwrap().find('汉').unwrap() + 1;
    let cut = call_tool(&ctx, "git_diff", &json!({"repo_path":"nested","max_bytes":cap}));
    assert!(cut["diff"].as_str().unwrap().len() <= cap);
    assert!(!cut["diff"].as_str().unwrap().contains('�'));
    assert_eq!(call_tool(&ctx, "git_diff", &json!({"repo_path":"../outside"}))["ok"], false);
}
