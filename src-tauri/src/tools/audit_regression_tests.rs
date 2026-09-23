//! Regressions from the 2026-09-22 personal runtime audit. All data is temporary.
use super::{context::ToolContext, file, registry, workspace::Workspace, exec};
use serde_json::json;
use std::fs;

#[test]
fn audit_only_the_mcp_skill_writer_is_excluded_from_actions() {
    for profile in ["core", "read-only", "advanced"] {
        for tool in registry::list_tools_for_profile(profile) {
            let name = tool["name"].as_str().unwrap();
            assert_eq!(
                registry::is_allowed_tool(name),
                name != "apply_skill_patch",
                "{profile}: unexpected Actions exposure for {name}"
            );
        }
    }
    assert!(!registry::is_allowed_tool("apply_skill_patch"));
    assert!(!registry::is_allowed_tool("arbitrary-command"));
}


#[test]
fn audit_patch_and_exec_contracts_keep_concurrency_guards() {
    let patch = registry::input_schema("apply_patch");
    for key in ["request_id", "expected_hashes", "task_id"] {
        assert!(patch["properties"][key].is_object(), "{key}");
    }
    let exec = registry::input_schema("exec_command");
    for key in ["request_id", "task_id", "mode", "durable", "resources"] {
        assert!(exec["properties"][key].is_object(), "{key}");
    }
    let skill_patch = registry::input_schema("apply_skill_patch");
    for key in ["root_id", "patch", "request_id", "expected_hashes", "task_id"] {
        assert!(skill_patch["properties"][key].is_object(), "{key}");
    }
    let readonly = registry::list_tools_for_profile("read-only");
    assert!(!readonly.iter().any(|tool| tool["name"] == "apply_skill_patch"));
}

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::new(dir.path().into()).unwrap();
    (dir, ws)
}

#[test]
fn audit_lowercase_case_sensitive_search_does_not_match_uppercase() {
    let (_dir, ws) = workspace();
    fs::write(ws.root().join("data.txt"), "needle\nNEEDLE\nNeedle\n").unwrap();
    let found = file::search_text(&ws, &json!({"query":"needle", "case_sensitive":true})).unwrap();
    assert_eq!(found["matches"].as_array().unwrap().len(), 1);
    let all = file::search_text(&ws, &json!({"query":"needle", "case_sensitive":false})).unwrap();
    assert_eq!(all["matches"].as_array().unwrap().len(), 3);
}

#[test]
fn audit_subdirectory_globs_accept_search_root_and_workspace_relative_paths() {
    let (_dir, ws) = workspace();
    fs::create_dir_all(ws.root().join("nested/src")).unwrap();
    fs::write(ws.root().join("nested/src/main.rs"), "needle").unwrap();
    for pattern in ["src/*.rs", "nested/src/*.rs"] {
        let found = file::list_files(&ws, &json!({"path":"nested", "patterns":[pattern]})).unwrap();
        assert_eq!(found["files"].as_array().unwrap().len(), 1, "{pattern}");
        let matches = file::search_text(&ws, &json!({"path":"nested", "query":"needle", "include_globs":[pattern]})).unwrap();
        assert_eq!(matches["matches"].as_array().unwrap().len(), 1, "{pattern}");
    }
}

#[test]
fn audit_explicit_subtree_exclusion_prunes_traversal() {
    let (_dir, ws) = workspace();
    fs::create_dir_all(ws.root().join("nested/cache")).unwrap();
    for i in 0..150 {
        fs::write(ws.root().join(format!("nested/cache/{i}.txt")), "needle").unwrap();
    }
    fs::write(ws.root().join("nested/keep.txt"), "needle").unwrap();
    let found = file::list_files(&ws, &json!({"path":"nested", "exclude_patterns":["cache/**"], "max_visited_entries":20})).unwrap();
    assert_eq!(found["truncated"], false);
    assert_eq!(found["files"].as_array().unwrap().len(), 1);
    assert!(found["visited_entries"].as_u64().unwrap() < 20);
    let searched = file::search_text(&ws, &json!({"path":"nested", "query":"needle", "exclude_globs":["cache/**"], "max_visited_entries":20})).unwrap();
    assert_eq!(searched["truncated"], false);
    assert_eq!(searched["matches"].as_array().unwrap().len(), 1);
}

#[test]
fn audit_search_context_respects_the_preview_budget() {
    let (_dir, ws) = workspace();
    let long = "中文".repeat(10000);
    fs::write(ws.root().join("huge.txt"), format!("{long}\nneedle\n{long}\n")).unwrap();
    let out = file::search_text(&ws, &json!({"query":"needle", "context_lines":1, "max_preview_bytes":64})).unwrap();
    let item = &out["matches"][0];
    for key in ["before", "after"] {
        assert!(item[key][0].as_str().unwrap().len() <= 67, "{key} unbounded");
    }
}

#[test]
fn audit_native_ls_resolves_relative_to_requested_workdir() {
    let (dir, _ws) = workspace();
    let state = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("nested/child")).unwrap();
    fs::write(dir.path().join("nested/child/expected.txt"), "ok").unwrap();
    let ctx = ToolContext::for_test(dir.path().into(), state.path().into()).unwrap();
    let out = exec::exec_command(&ctx, &json!({"cmd":"ls child", "workdir":"nested"})).unwrap();
    assert!(out["stdout"].as_str().unwrap().contains("expected.txt"));
}

#[test]
fn audit_timeout_policy_matches_effective_durable_mode() {
    let policy = super::policy::PolicySettings::default();
    assert!(super::policy::validate_command(&json!({"cmd":"echo ok", "timeout_ms":700000}), &policy).is_ok());
    for args in [json!({"cmd":"echo ok","timeout_ms":700000,"durable":false}),
                 json!({"cmd":"echo ok","timeout_ms":700000,"durable":true,"tty":true})] {
        assert!(super::policy::validate_command(&args, &policy).is_err());
    }
}

#[test]
fn audit_contract_fingerprint_and_client_drift_diagnostics_are_deterministic() {
    let core = registry::catalog_contract("core");
    assert_eq!(core, registry::catalog_contract("core"));
    assert_eq!(core["schema_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(core["tool_count"], registry::list_tools_for_profile("core").len());
    assert!(core["client_catalog_synchronized"].is_null());
    assert!(core["input_fields"]["apply_patch"].as_array().unwrap().contains(&json!("expected_hashes")));
    let readonly = registry::catalog_contract("read-only");
    assert_ne!(core["schema_sha256"], readonly["schema_sha256"]);
    assert!(readonly["input_fields"].get("apply_patch").is_none());
}

#[test]
fn audit_directory_exclusion_does_not_prune_file_only_patterns() {
    let (_dir, ws) = workspace();
    fs::create_dir_all(ws.root().join("nested/cache")).unwrap();
    fs::write(ws.root().join("nested/cache/drop.rs"), "needle").unwrap();
    fs::write(ws.root().join("nested/cache/keep.txt"), "needle").unwrap();
    let found = file::list_files(&ws, &json!({"path":"nested", "exclude_patterns":["cache/*.rs"]})).unwrap();
    let files = found["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "nested/cache/keep.txt");
}

#[test]
fn audit_traversal_budget_is_exposed_in_both_mcp_and_actions_contracts() {
    for name in ["list_files", "search_text"] {
        let schema = registry::input_schema(name);
        assert_eq!(schema["properties"]["max_visited_entries"]["maximum"], 1_000_000);
    }
}
