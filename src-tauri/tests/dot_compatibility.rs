//! Native-workspace compatibility: use the real dispatcher and durable worker.
mod common;

use std::{
    fs,
    time::{Duration, Instant},
};

use coding_tools_mcp_desktop_lib::tools::{list_tools_for_profile, ToolContext};
use coding_tools_personal_runtime::{digest, locks};
use common::*;
use serde_json::{json, Value};

#[cfg(windows)]
const PYTHON: &str = "python";
#[cfg(not(windows))]
const PYTHON: &str = "python3";

fn isolated_context(fx: &FixtureWorkspace, state: &tempfile::TempDir) -> ToolContext {
    ToolContext::for_test(fx.root.clone(), state.path().join("harness")).unwrap()
}

fn open_task(ctx: &ToolContext, request: &str) -> Value {
    let result = invoke(
        ctx,
        "task_open",
        json!({
            "goal": format!("native compatibility {request}"), "request_id": request
        }),
    );
    assert_ok(&result);
    result
}

fn wait_original(ctx: &ToolContext, session: &Value) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let result = invoke(
            ctx,
            "write_stdin",
            json!({
                "session_id": session, "chars": "", "yield_time_ms": 100,
                "max_output_bytes": 1024
            }),
        );
        assert_ok(&result);
        if !matches!(result["status"].as_str(), Some("queued" | "running")) {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "original job did not finish: {result}"
        );
    }
}

#[test]
fn optional_metadata_preserves_old_fields_and_reports_actual_read_scope() {
    let fx = tiny_js_fixture();
    let state = tempfile::tempdir().unwrap();
    let ctx = isolated_context(&fx, &state);
    let info = invoke(&ctx, "server_info", json!({}));
    assert_ok(&info);
    // Older clients can continue reading their original fields and ignore the addition.
    for key in [
        "server",
        "title",
        "version",
        "protocol_version",
        "workspace",
        "permission_mode",
        "default_cwd",
        "network_allowed",
        "tool_profile",
        "auth_enabled",
        "auth_type",
        "endpoint_path",
        "tools",
        "tool_count",
        "tool_contract",
        "delivery",
        "runtime_pressure",
        "concurrency",
        "skill_bridge",
        "personal_runtime",
    ] {
        assert!(info.get(key).is_some(), "legacy field removed: {key}");
    }
    assert_eq!(info["server"], "coding-tools-mcp");
    assert_eq!(info["endpoint_path"], "/mcp");
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    let direct = &info["direct_workspace"];
    assert_eq!(direct["version"], 1);
    assert_eq!(direct["execution_path"], "native_tools");
    assert_eq!(direct["external_agent_required"], false);
    assert_eq!(
        direct["durable_bookkeeping"],
        "existing_task_and_request_lifecycle"
    );
    assert_eq!(
        direct["catalog_refresh"],
        "client_dependent_no_list_changed_notification"
    );
    assert_eq!(direct["security"]["execution_isolation"], "policy_only");
    assert_eq!(direct["security"]["sandbox_enforced"], false);
    assert_eq!(
        direct["security"]["read_scope"],
        "explicit_external_paths_allowed"
    );
    assert!(direct.get("dot_supported").is_none());
    assert!(!ctx.workspace.reads_restricted_to_root());

    let outside = fx.outside_secret.to_string_lossy().to_string();
    assert_ok(&invoke(&ctx, "read_file", json!({"path": outside})));
    assert_ok(&invoke(
        &ctx,
        "read_file",
        json!({"path": "../outside-secret.txt"}),
    ));
    // The gateway uses this exact flag. Metadata must observe it, including clones.
    ctx.workspace.clone().restrict_reads_to_root();
    let strict = invoke(&ctx, "server_info", json!({}));
    assert_eq!(
        strict["direct_workspace"]["security"]["read_scope"],
        "workspace_root"
    );
    assert!(ctx.workspace.reads_restricted_to_root());
    assert_err(&invoke(&ctx, "read_file", json!({"path": outside})));
    assert_err(&invoke(
        &ctx,
        "read_file",
        json!({"path": "../outside-secret.txt"}),
    ));
    assert_ok(&invoke(&ctx, "read_file", json!({"path": "src/math.js"})));
}

#[test]
fn descriptive_metadata_never_changes_catalog_or_readonly_exposure() {
    let fx = tiny_js_fixture();
    let state = tempfile::tempdir().unwrap();
    let mut ctx = isolated_context(&fx, &state);
    for profile in ["core", "advanced", "read-only", "compat-readonly-all"] {
        ctx.tool_profile = profile.into();
        let catalog = list_tools_for_profile(profile);
        let info = invoke(&ctx, "server_info", json!({}));
        assert_ok(&info);
        assert_eq!(info["tool_count"].as_u64().unwrap(), catalog.len() as u64);
        assert_eq!(catalog, list_tools_for_profile(profile));
        assert_eq!(
            info["direct_workspace"]["security"]["read_scope"], "explicit_external_paths_allowed",
            "read-only exposure is not strict_reads"
        );
        if profile == "read-only" {
            for name in [
                "apply_patch",
                "exec_command",
                "task_open",
                "task_checkpoint",
            ] {
                assert!(catalog.iter().all(|tool| tool["name"] != name));
            }
        }
        if profile == "core" || profile == "advanced" {
            for name in ["apply_patch", "exec_command"] {
                let tool = catalog.iter().find(|tool| tool["name"] == name).unwrap();
                assert_eq!(tool["annotations"]["readOnlyHint"], false);
                assert_eq!(tool["inputSchema"]["additionalProperties"], false);
            }
        }
    }
}

#[test]
fn native_read_hash_patch_durable_exec_reopens_original_output_and_checkpoint() {
    let fx = tiny_js_fixture();
    let state = tempfile::tempdir().unwrap();
    let ctx = isolated_context(&fx, &state);
    let task = open_task(&ctx, "direct-flow");
    assert!(task["instructions"]
        .as_str()
        .unwrap()
        .contains("no external Agent is required"));
    let path = "dot-owned.txt";
    fs::write(fx.root.join(path), "before\n").unwrap();
    let file = invoke(&ctx, "read_file", json!({"path": path}));
    assert_ok(&file);
    let patch = json!({"task_id": task["task_id"], "request_id": "protected-edit",
        "expected_hashes": {path: file["file_sha256"]},
        "patch": "*** Begin Patch\n*** Update File: dot-owned.txt\n@@\n-before\n+verified\n*** End Patch"});
    let applied = invoke(&ctx, "apply_patch", patch.clone());
    assert_ok(&applied);
    assert_eq!(applied["after_hashes"][path], digest("verified\n"));
    let replay = invoke(&ctx, "apply_patch", patch);
    assert_ok(&replay);
    assert_eq!(replay["deduplicated"], true);
    assert_eq!(replay["change_id"], applied["change_id"]);

    // The append is a real side effect: response recovery must never repeat it.
    let command = json!({"task_id": task["task_id"], "request_id": "verify-once",
        "cmd": format!("{PYTHON} -c \"import time; time.sleep(0.2); print(open('dot-owned.txt').read(), end=''); open('run-count.txt', 'a').write('once\\n')\""),
        "mode": "write", "yield_time_ms": 0, "max_output_bytes": 1024});
    let started = invoke(&ctx, "exec_command", command.clone());
    assert_ok(&started);
    assert_eq!(started["execution_mode"], "durable_worker");
    assert_eq!(started["execution_boundary"], "policy_only");
    assert_eq!(started["sandbox_enforced"], false);
    assert_eq!(started["task_id"], task["task_id"]);
    let session = started["session_id"].clone();
    let job = started["job_id"].clone();
    let output_ref = started["output_refs"]["stdout"].clone();
    drop(ctx);

    // A fresh server context inspects the original durable identity, without an Agent.
    let resumed = isolated_context(&fx, &state);
    let done = wait_original(&resumed, &session);
    assert_eq!(done["status"], "exited");
    assert_eq!(done["exit_code"], 0);
    assert_eq!(done["command_ok"], true);
    assert_eq!(done["job_id"], job);
    assert_eq!(done["task_scope"], task["task_id"]);
    let first = invoke(
        &resumed,
        "read_output",
        json!({"output_ref": output_ref, "limit": 4}),
    );
    assert_ok(&first);
    assert_eq!(first["content"], "veri");
    assert_eq!(first["next_offset"], 4);
    let tail = invoke(
        &resumed,
        "read_output",
        json!({
        "output_ref": output_ref, "offset": first["next_offset"], "limit": 1024}),
    );
    assert_ok(&tail);
    assert_eq!(tail["content"], "fied\n");
    assert_eq!(tail["task_id"], task["task_id"]);
    assert!(tail["next_offset"].is_null());
    let recovered = invoke(&resumed, "exec_command", command);
    assert_ok(&recovered);
    assert_eq!(recovered["deduplicated"], true);
    assert_eq!(recovered["job_id"], job);
    assert_eq!(
        fs::read_to_string(fx.root.join("run-count.txt")).unwrap(),
        "once\n"
    );

    let checkpoint = json!({"task_id": task["task_id"], "request_id": "verified-checkpoint",
        "expected_revision": task["revision"], "state": "completed",
        "checkpoint": {"summary": "read, patched and verified using native tools",
            "steps": {"native-flow": {"state": "passed", "evidence": [output_ref]}}}});
    let saved = invoke(&resumed, "task_checkpoint", checkpoint.clone());
    assert_ok(&saved);
    assert_eq!(saved["state"], "completed");
    assert_eq!(
        invoke(&resumed, "task_checkpoint", checkpoint)["revision"],
        saved["revision"]
    );
    let restored = invoke(&resumed, "task_open", json!({"task_id": task["task_id"]}));
    assert_ok(&restored);
    assert_eq!(restored["state"], "completed");
    assert_eq!(restored["step_counts"]["passed"], 1);
}

#[test]
fn hash_conflicts_and_reused_request_identity_preserve_other_writes() {
    let fx = tiny_js_fixture();
    let state = tempfile::tempdir().unwrap();
    let ctx = isolated_context(&fx, &state);
    let a = open_task(&ctx, "writer-a");
    let b = open_task(&ctx, "writer-b");
    fs::write(fx.root.join("shared.txt"), "first\nsecond\n").unwrap();
    let original = invoke(&ctx, "read_file", json!({"path": "shared.txt"}));
    let first = json!({"task_id": a["task_id"], "request_id": "first-change",
        "expected_hashes": {"shared.txt": original["file_sha256"]},
        "patch": "*** Begin Patch\n*** Update File: shared.txt\n@@\n-first\n+FIRST\n*** End Patch"});
    assert_ok(&invoke(&ctx, "apply_patch", first.clone()));
    let mut second = json!({"task_id": b["task_id"], "request_id": "stale-change",
        "expected_hashes": {"shared.txt": original["file_sha256"]},
        "patch": "*** Begin Patch\n*** Update File: shared.txt\n@@\n-second\n+SECOND\n*** End Patch"});
    let stale = invoke(&ctx, "apply_patch", second.clone());
    assert_eq!(stale["error"]["code"], "STALE_FILE");
    assert_eq!(stale["error"]["details"]["patch_applied"], false);
    assert_eq!(
        fs::read_to_string(fx.root.join("shared.txt")).unwrap(),
        "FIRST\nsecond\n"
    );
    let current = invoke(&ctx, "read_file", json!({"path": "shared.txt"}));
    second["request_id"] = json!("merged-change");
    second["expected_hashes"]["shared.txt"] = current["file_sha256"].clone();
    assert_ok(&invoke(&ctx, "apply_patch", second));
    let mut conflicting_identity = first;
    conflicting_identity["patch"] =
        json!("*** Begin Patch\n*** Update File: shared.txt\n@@\n-FIRST\n+clobber\n*** End Patch");
    assert_eq!(
        invoke(&ctx, "apply_patch", conflicting_identity)["error"]["code"],
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(
        fs::read_to_string(fx.root.join("shared.txt")).unwrap(),
        "FIRST\nSECOND\n"
    );
}

#[test]
fn source_lock_rejection_creates_no_mutation_receipt_and_preserves_files() {
    let fx = tiny_js_fixture();
    let state = tempfile::tempdir().unwrap();
    let ctx = isolated_context(&fx, &state);
    let task = open_task(&ctx, "locked-write");
    let args = json!({"task_id": task["task_id"], "request_id": "locked-patch",
        "expected_hashes": {"locked.txt": null},
        "patch": "*** Begin Patch\n*** Add File: locked.txt\n+after-unlock\n*** End Patch"});
    let guard = locks::try_gate(&ctx.personal.dir, "source", false)
        .unwrap()
        .unwrap();
    let denied = invoke(&ctx, "apply_patch", args.clone());
    assert_eq!(denied["error"]["code"], "RESOURCE_BUSY");
    assert_eq!(denied["error"]["details"]["patch_applied"], false);
    assert_eq!(denied["error"]["details"]["receipt_persisted"], false);
    assert!(!fx.root.join("locked.txt").exists());
    drop(guard);
    assert_ok(&invoke(&ctx, "apply_patch", args));
    assert_eq!(
        fs::read_to_string(fx.root.join("locked.txt")).unwrap(),
        "after-unlock\n"
    );
}

#[test]
fn original_job_identity_cannot_be_reassigned_to_another_task_or_workspace() {
    let fx = tiny_js_fixture();
    let other_fx = tiny_js_fixture();
    let state = tempfile::tempdir().unwrap();
    let ctx = isolated_context(&fx, &state);
    let other_ctx = isolated_context(&other_fx, &state);
    let owner = open_task(&ctx, "job-owner");
    let stranger = open_task(&ctx, "other-task");
    let args = json!({"task_id": owner["task_id"], "request_id": "owned-command",
        "cmd": format!("{PYTHON} -c \"print('owned-output')\""),
        "mode": "read", "yield_time_ms": 0});
    let started = invoke(&ctx, "exec_command", args.clone());
    assert_ok(&started);
    let done = wait_original(&ctx, &started["session_id"]);
    assert_eq!(done["command_ok"], true);
    let contradictory = invoke(
        &ctx,
        "task_status",
        json!({
        "task_id": stranger["task_id"], "job_id": started["job_id"]}),
    );
    assert_eq!(contradictory["error"]["code"], "JOB_TASK_MISMATCH");
    assert_eq!(
        invoke(
            &ctx,
            "task_status",
            json!({
        "task_id": owner["task_id"], "job_id": started["job_id"]})
        )["task_id"],
        owner["task_id"]
    );
    assert_eq!(
        invoke(
            &other_ctx,
            "task_status",
            json!({
        "job_id": started["job_id"]})
        )["error"]["code"],
        "JOB_NOT_FOUND"
    );
    assert_eq!(
        invoke(
            &other_ctx,
            "read_output",
            json!({
        "output_ref": started["output_refs"]["stdout"]})
        )["error"]["code"],
        "JOB_NOT_FOUND"
    );
    assert_eq!(
        invoke(
            &other_ctx,
            "write_stdin",
            json!({
        "session_id": started["session_id"], "yield_time_ms": 0})
        )["error"]["code"],
        "JOB_NOT_FOUND"
    );
    let mut changed = args;
    changed["cmd"] = json!(format!("{PYTHON} -c \"print('different-output')\""));
    assert_eq!(
        invoke(&ctx, "exec_command", changed)["error"]["code"],
        "IDEMPOTENCY_CONFLICT"
    );
}
