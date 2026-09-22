//! Short, process-shared patch critical section with full-file optimistic preconditions.
use std::{collections::BTreeMap, fs, time::Duration};
use coding_tools_personal_runtime::{digest, locks, Error};
use serde_json::{json, Value};
use super::{context::ToolContext, patch, personal::error, workspace::{tool_err, tool_ok, WorkspaceError}};

pub fn apply(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let text = args.get("patch").and_then(Value::as_str).ok_or_else(|| WorkspaceError::invalid_argument("patch is required"))?;
    let paths = patch::touched_paths(text)?;
    if paths.is_empty() { return Err(WorkspaceError::invalid_argument("patch has no files")); }
    let dry = args.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
    let task = args.get("task_id").and_then(Value::as_str);
    if let Some(task) = task { ctx.personal.task_status(task).map_err(error)?; }
    // Preserve path-denial precedence even for incomplete client arguments.
    // Repeat these guards under the source lock before any new write (TOCTOU).
    for path in &paths {
        ctx.workspace.reject_protected_write_path(path)?;
        ctx.workspace.reject_write_symlink(path)?;
        ctx.workspace.resolve_for_write(path)?;
    }
    let mut input = args.clone();
    if let Some(object) = input.as_object_mut() { object.remove("reason"); object.remove("_host_session_key"); }
    let input_hash = digest(input.to_string());
    let scope = format!("patch:{}", task.unwrap_or("workspace"));
    let request = if dry { None } else {
        Some(args.get("request_id").and_then(Value::as_str)
            .filter(|v| !v.is_empty() && v.len() <= 160)
            .ok_or_else(|| error(Error::contract("REQUEST_ID_REQUIRED", "apply_patch needs a stable request_id; this also prevents duplicate application after a lost response")))?)
    };
    if let Some(request) = request {
        if let Some(mut prior) = ctx.personal.peek_receipt(&scope, request, &input_hash).map_err(error)? {
            prior["deduplicated"] = json!(true);
            return Ok(prior);
        }
    }
    // Preflight can overlap builds; actual writes retain the exclusive gate.
    let _gate = locks::gate(&ctx.personal.dir, "source", !dry, Duration::from_secs(5))
        .map_err(|e| if e.code() == "RESOURCE_BUSY" {
            WorkspaceError::ToolDetails {
                code: "RESOURCE_BUSY", category: "conflict", retryable: true,
                message: "source lock is busy; no patch was applied or receipt started".into(),
                details: json!({"resource":"source","requested_mode":if dry {"shared"} else {"exclusive"},
                    "waited_ms":5000,"patch_applied":false,"receipt_persisted":false,
                    "request_id":request,"task_id":task,
                    "next_action":"wait for the owning operation to finish, then retry the same unchanged request; do not force-unlock or replay unknown jobs"}),
            }
        } else { error(e) })?;
    let mut current = BTreeMap::new();
    for path in &paths {
        ctx.workspace.reject_protected_write_path(path)?;
        ctx.workspace.reject_write_symlink(path)?;
        let resolved = ctx.workspace.resolve_for_write(path)?;
        current.insert(resolved.display, fingerprint(&resolved.path)?);
    }
    if dry {
        let mut result = patch::patch_check(ctx, args)?;
        result["expected_hashes"] = json!(current);
        result["cooperative_lock"] = json!("source");
        return Ok(result);
    }
    let request = request.expect("non-dry patch has validated request_id");
    if let Some(prior) = ctx.personal.begin_receipt(&scope, request, &input_hash).map_err(error)? {
        let mut prior = prior; prior["deduplicated"] = json!(true); return Ok(prior);
    }
    let outcome = (|| {
        if let Some(task) = task {
            if ctx.personal.task_status(task).map_err(error)?["state"] == "completed" {
                return Err(error(Error::contract("TASK_COMPLETED","open a follow-up task before applying new patches")));
            }
        }
        let expected = args.get("expected_hashes").and_then(Value::as_object)
            .ok_or_else(|| error(Error::contract("PRECONDITION_REQUIRED", "pass hashes from read_file or patch_check for every touched file")))?;
        let mut normalized = BTreeMap::new();
        for (path, hash) in expected {
            let resolved = ctx.workspace.resolve_for_write(path)?;
            if normalized.insert(resolved.display, hash.clone()).is_some() {
                return Err(error(Error::contract("DUPLICATE_PATH", "expected_hashes contains aliases for the same file")));
            }
        }
        for (path, hash) in &current {
            if normalized.get(path) != Some(hash) {
                return Err(WorkspaceError::ToolDetails { code:"STALE_FILE", message:format!("file changed or its precondition is missing: {path}"),category:"conflict",retryable:true,
                    details:json!({"path":path,"expected":normalized.get(path),"current":hash,"patch_applied":false,"next_action":"read the current file, preserve others' changes, and submit a revised patch with a new request_id"}) });
            }
        }
        let mut result = patch::apply_patch(ctx, args)?;
        let mut after = BTreeMap::new();
        for path in current.keys() {
            let resolved = ctx.workspace.resolve_for_write(path)?;
            after.insert(path.clone(), fingerprint(&resolved.path)?);
        }
        result["before_hashes"] = json!(current);
        result["after_hashes"] = json!(after);
        result["request_id"] = json!(request);
        result["task_id"] = json!(task);
        result["cooperative_lock"] = json!("source");
        result["recovery"] = json!("receipt-and-current-files; never restore the whole workspace");
        Ok(result)
    })();
    let mut output = match outcome { Ok(v) => v, Err(e) => tool_err(e) };
    output["request_id"] = json!(request);
    output["task_id"] = json!(task);
    output["receipt_persisted"] = json!(true);
    ctx.personal.finish_receipt(&scope, request, &output).map_err(error)?;
    if let Some(task) = task {
        // Receipt is authoritative even if the optional task event cannot be appended.
        let _ = ctx.personal.event(task, "patch_finished", &json!({"request_id":request,"ok":output["ok"],"before_hashes":output["before_hashes"],"after_hashes":output["after_hashes"]}));
    }
    Ok(tool_ok(output))
}

fn fingerprint(path: &std::path::Path) -> Result<Value, WorkspaceError> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() <= 4 * 1024 * 1024 => fs::read(path).map(|data|json!(digest(data))).map_err(|e|WorkspaceError::invalid_argument(e.to_string())),
        Ok(meta) if !meta.is_file() => Err(WorkspaceError::invalid_argument("patch target must be a regular file")),
        Ok(_) => Err(WorkspaceError::invalid_argument("managed text patches are limited to 4MiB per file; use a reviewed resource-specific operation for larger assets")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(e) => Err(WorkspaceError::invalid_argument(e.to_string())),
    }
}

#[cfg(test)]
mod lock_replay_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, ToolContext) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let ctx = ToolContext::for_test(workspace, dir.path().join("state")).unwrap();
        (dir, ctx)
    }
    fn args() -> Value {
        json!({"patch":"*** Begin Patch\n*** Add File: owned.txt\n+once\n*** End Patch",
            "request_id":"patch-once","expected_hashes":{"owned.txt":null}})
    }
    #[test]
    fn completed_receipt_replays_while_source_is_exclusively_locked() {
        let (_dir,ctx) = fixture();
        let first = apply(&ctx,&args()).unwrap();
        let _guard = locks::try_gate(&ctx.personal.dir,"source",true).unwrap().unwrap();
        let second = apply(&ctx,&args()).unwrap();
        assert_eq!(second["deduplicated"],true);
        assert_eq!(first["change_id"],second["change_id"]);
        assert_eq!(fs::read_to_string(ctx.workspace.root().join("owned.txt")).unwrap(),"once\n");
        let mut changed = args(); changed["patch"] = json!("*** Begin Patch\n*** Add File: owned.txt\n+different\n*** End Patch");
        assert!(apply(&ctx,&changed).unwrap_err().to_string().contains("IDEMPOTENCY_CONFLICT"));
    }
    #[test]
    fn dry_run_can_overlap_a_build_without_writing_or_starting_receipt() {
        let (_dir,ctx) = fixture();
        let _guard = locks::try_gate(&ctx.personal.dir,"source",false).unwrap().unwrap();
        let mut input = args(); input["dry_run"] = json!(true);
        let result = apply(&ctx,&input).unwrap();
        assert_eq!(result["ok"],true);
        assert_eq!(result["expected_hashes"]["owned.txt"],Value::Null);
        assert!(!ctx.workspace.root().join("owned.txt").exists());
        let count: i64 = ctx.personal.conn().unwrap().query_row("SELECT count(*) FROM receipts",[],|r|r.get(0)).unwrap();
        assert_eq!(count,0);
    }
    #[test]
    fn busy_write_starts_no_receipt_and_same_request_works_after_unlock() {
        let (_dir,ctx) = fixture();
        let guard = locks::try_gate(&ctx.personal.dir,"source",false).unwrap().unwrap();
        let err = apply(&ctx,&args()).unwrap_err();
        match err {
            WorkspaceError::ToolDetails { code, retryable, details, .. } => {
                assert_eq!(code,"RESOURCE_BUSY"); assert!(retryable);
                assert_eq!(details["patch_applied"],false);
                assert_eq!(details["receipt_persisted"],false);
            },
            other => panic!("unexpected error: {other}"),
        }
        assert!(!ctx.workspace.root().join("owned.txt").exists());
        drop(guard);
        assert_eq!(apply(&ctx,&args()).unwrap()["ok"],true);
    }
    #[test]
    fn unfinished_receipt_never_replays_even_when_source_is_free() {
        let (_dir,ctx) = fixture();
        let input = args();
        ctx.personal.begin_receipt("patch:workspace","patch-once",&digest(input.to_string())).unwrap();
        assert!(apply(&ctx,&input).unwrap_err().to_string().contains("OPERATION_INDETERMINATE"));
        assert!(!ctx.workspace.root().join("owned.txt").exists());
    }
}
