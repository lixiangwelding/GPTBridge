//! Regression tests for the log-audit follow-up. All state and subprocesses are isolated.
use std::fs;
use serde_json::json;
use super::{call_tool, ToolContext};

fn fixture() -> (tempfile::TempDir, ToolContext) {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let ctx = ToolContext::for_test(workspace, root.path().join("state")).unwrap();
    (root, ctx)
}

#[test]
fn session_handoff_output_pages_preserve_persisted_identity() {
    let (_root, ctx) = fixture();
    let task = call_tool(&ctx, "task_open", &json!({
        "goal":"isolated output ownership", "request_id":"output-owner-task"
    }));
    assert_eq!(task["ok"], true, "{task}");
    let python = if cfg!(windows) { "python" } else { "python3" };
    for bound in [true, false] {
        let request = if bound { "bound-output" } else { "unbound-output" };
        let mut args = json!({
            "request_id":request, "mode":"read", "yield_time_ms":5000,
            "cmd":format!("{python} -c \"import sys; sys.stdout.buffer.write('中文🙂🚀\\n'.encode('utf-8'))\"")
        });
        let owner = if bound { task["task_id"].clone() } else { serde_json::Value::Null };
        if bound { args["task_id"] = owner.clone(); }
        let job = call_tool(&ctx, "exec_command", &args);
        assert_eq!(job["command_ok"], true, "{job}");
        let mut offset = 0;
        let mut assembled = String::new();
        for _ in 0..16 {
            // The public schema excludes task_id on read_output. This internal
            // probe additionally proves a caller cannot relabel a persisted job.
            let page = call_tool(&ctx, "read_output", &json!({
                "output_ref":job["output_refs"]["stdout"], "offset":offset,
                "limit":5, "task_id":"not-the-output-owner"
            }));
            assert_eq!(page["ok"], true, "{page}");
            assert_eq!(page["task_id"], owner, "{page}");
            assert_eq!(page["task_scope"], owner, "{page}");
            assert_eq!(page["job_id"], job["job_id"]);
            assert_eq!(page["request_id"], request);
            assert_eq!(page["content_lossy"], false);
            assembled.push_str(page["content"].as_str().unwrap());
            if page["next_offset"].is_null() { break; }
            let next = page["next_offset"].as_u64().unwrap();
            assert!(next > offset);
            offset = next;
        }
        assert_eq!(assembled, "中文🙂🚀\n");
    }
}

#[test]
fn session87_large_skill_preview_is_compact_without_losing_manual_access() {
    let (_root, ctx) = fixture();
    for i in 0..12 {
        let dir = ctx.workspace.root().join(format!(".agents/skills/test-{i:02}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("---\nname: test-{i:02}\ndescription: 隔离的代码审查技能\n---\nPRIVATE_BODY\n")).unwrap();
    }
    let result = super::skill_discovery::snapshot(&ctx);
    assert_eq!(result["available_count"], 12);
    assert!(result["shown_count"].as_u64().unwrap() <= 6, "{result}");
    assert!(result.to_string().len() < 4 * 1024);
    assert_eq!(result["partial"], true);
    let all = call_tool(&ctx, "list_skills", &json!({"limit":20}));
    assert_eq!(all["skills"].as_array().unwrap().len(), 12);
    let info = call_tool(&ctx, "server_info", &json!({}));
    assert_eq!(info["skill_discovery"]["available_count"], 12);
    assert_eq!(info["skill_discovery"]["shown_count"], 0);
    assert_eq!(info["skill_discovery"]["summary_only"], true);
    assert!(!info.to_string().contains("PRIVATE_BODY"));
}

#[test]
fn session87_durable_poll_preserves_original_task_scope() {
    let (_root, ctx) = fixture();
    let task = call_tool(&ctx, "task_open", &json!({"goal":"isolated job", "request_id":"session87-task"}));
    assert_eq!(task["ok"], true, "{task}");
    let args = json!({"task_id":task["task_id"],"request_id":"session87-job", "mode":"read", "cmd":"python3 -c \"print('session87')\"", "yield_time_ms":5000});
    let job = call_tool(&ctx, "exec_command", &args);
    assert_eq!(job["command_ok"], true, "{job}");
    let poll = call_tool(&ctx, "write_stdin", &json!({"session_id":job["session_id"],"yield_time_ms":0}));
    assert_eq!(poll["task_id"], task["task_id"]);
    assert_eq!(poll["task_scope"], task["task_id"], "{poll}");
    let replay = call_tool(&ctx, "exec_command", &args);
    assert_eq!(replay["job_id"], job["job_id"]);
    assert_eq!(replay["deduplicated"], true);
}

#[test]
fn session87_history_search_reports_phases_and_builds_only_page_snippets() {
    let (_root, ctx) = fixture();
    for i in 0..3 {
        let boot = call_tool(&ctx, "history_session_bootstrap", &json!({"session_key":format!("s87-{i}"),"initial_user_input":format!("检测中文检索 {i}")}));
        assert_eq!(boot["ok"], true, "{boot}");
    }
    let args = json!({"query":"检测", "limit":1});
    let first = call_tool(&ctx, "history_session_search", &args);
    assert_eq!(first["total_matches"], 3);
    assert_eq!(first["results"].as_array().unwrap().len(), 1);
    assert_eq!(first["diagnostics"]["manifest_builds"], 1);
    assert_eq!(first["diagnostics"]["snippets_built"], 1);
    for key in ["scan_ms","manifest_ms","ranking_ms","page_ms","total_ms"] {
        assert!(first["diagnostics"][key].as_u64().is_some(), "{key}: {first}");
    }
    assert!(first["results"][0]["snippet"].as_str().unwrap().contains("检测"));
    // Derived files never override fresh Markdown truth, including invalid caches.
    fs::write(ctx.workspace.root().join("docs/history-session/memory/manifest.json"), "broken").unwrap();
    let second = call_tool(&ctx, "history_session_search", &args);
    assert_eq!(first["results"], second["results"]);
    let page2 = call_tool(&ctx, "history_session_search", &json!({"query":"检测","limit":1,"cursor":first["next_cursor"]}));
    assert_ne!(page2["results"][0]["number"], first["results"][0]["number"]);
}

#[test]
fn session87_audit_timing_includes_post_dispatch_phase_without_changing_outcome() {
    let (_root, ctx) = fixture();
    let request = crate::audit::AuditRequestContext::default();
    let result = super::call_tool_with_audit(&ctx, "get_default_cwd", &json!({}), &request);
    assert_eq!(result["ok"], true);
    let timing = &result["dispatcher_timing"];
    let dispatch = timing["dispatch_ms"].as_u64().unwrap();
    let audit = timing["audit_ms"].as_u64().unwrap();
    let total = timing["total_ms"].as_u64().unwrap();
    assert!(total >= dispatch + audit);
    assert_eq!(timing["includes_transport"], false);
    assert_eq!(timing["includes_worker_lifetime"], false);
}

#[test]
fn session87_git_status_accepts_only_inside_absolute_paths() {
    let (_root, ctx) = fixture();
    fs::create_dir(ctx.workspace.root().join("child")).unwrap();
    for path in [ctx.workspace.root().to_path_buf(), ctx.workspace.root().join("child")] {
        let result = call_tool(&ctx, "git_status", &json!({"path":path.to_string_lossy()}));
        assert_eq!(result["ok"], true, "{result}");
    }
    assert_eq!(call_tool(&ctx, "git_status", &json!({"path":"child"}))["ok"], true);
}

#[test]
fn session87_git_status_still_rejects_outside_parent_and_prefix_siblings() {
    let (root, ctx) = fixture();
    let sibling = root.path().join("workspace-other");
    fs::create_dir(&sibling).unwrap();
    for path in [sibling, ctx.workspace.root().join("../workspace"), root.path().to_path_buf()] {
        let result = call_tool(&ctx, "git_status", &json!({"path":path.to_string_lossy()}));
        assert_eq!(result["ok"], false, "{result}");
    }
}

#[cfg(unix)]
#[test]
fn session87_git_status_still_rejects_symlink_escape() {
    let (root, ctx) = fixture();
    std::os::unix::fs::symlink(root.path(), ctx.workspace.root().join("escape")).unwrap();
    let result = call_tool(&ctx, "git_status", &json!({"path":ctx.workspace.root().join("escape").to_string_lossy()}));
    assert_eq!(result["ok"], false, "{result}");
}


#[test]
fn session87_poll_cannot_relabel_an_unbound_job() {
    let (_root, ctx) = fixture();
    let job = call_tool(&ctx, "exec_command", &json!({
        "request_id":"unbound-scope", "cmd":"python3 -c \"print('unbound')\"",
        "mode":"read", "yield_time_ms":5000
    }));
    assert_eq!(job["command_ok"], true, "{job}");
    assert_eq!(job["task_id"], serde_json::Value::Null);
    // Public schema disallows task_id on polls. Defense in depth keeps the
    // persisted owner authoritative even at the internal dispatcher boundary.
    let poll = call_tool(&ctx, "write_stdin", &json!({
        "session_id":job["session_id"], "yield_time_ms":0, "task_id":"not-this-job"
    }));
    assert_eq!(poll["ok"], true, "{poll}");
    assert_eq!(poll["task_id"], serde_json::Value::Null);
    assert_eq!(poll["task_scope"], serde_json::Value::Null);
}
