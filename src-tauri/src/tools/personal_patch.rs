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
    let _gate = locks::gate(&ctx.personal.dir, "source", true, Duration::from_secs(5)).map_err(error)?;
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
    let request = args.get("request_id").and_then(Value::as_str)
        .filter(|v| !v.is_empty() && v.len() <= 160)
        .ok_or_else(|| error(Error::contract("REQUEST_ID_REQUIRED", "apply_patch needs a stable request_id; this also prevents duplicate application after a lost response")))?;
    let scope = format!("patch:{}", task.unwrap_or("workspace"));
    let mut input = args.clone();
    if let Some(object) = input.as_object_mut() { object.remove("reason"); object.remove("_host_session_key"); }
    if let Some(prior) = ctx.personal.begin_receipt(&scope, request, &digest(input.to_string())).map_err(error)? {
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
