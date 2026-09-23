//! Small task API. Task identity is explicit; a workspace never has one global active task.
use std::time::Duration;
use coding_tools_personal_runtime::{Error, Store};
use serde_json::{json, Value};
use super::{context::ToolContext, workspace::{tool_ok, WorkspaceError}};

pub const TOOLS: &[&str] = &["task_open", "task_status", "task_checkpoint"];
pub const INSTRUCTIONS: &str = "Use task_open with the user's goal (or task_id to resume); no pasted startup prompt is required. Keep task_id on every write/command. Use a stable request_id for each logical mutation and expected_revision for task_checkpoint. Read file_sha256 and pass expected_hashes to apply_patch; on conflict re-read and merge, never restore an old workspace snapshot. Independent work may run concurrently in the same directory; no worktree is required. Non-interactive exec_command uses durable jobs; query the returned job/session before retrying. Save step deltas and evidence using task_checkpoint before replying. Only explicitly supplied goal/raw_user_input is recorded; do not claim automatic access to unprovided chat text. Legacy history_session tools remain available for old archives. Unknown results require inspection, never blind replay. For filesystem reads, prefer the exposed native file/search tools rather than exec_command, whose default write mode takes the source lock. Only declare read/build modes when exposed and accurate; never label writes as reads. For long commands, use short initial yield_time_ms and bounded polling of the original job instead of repeatedly holding 30-second HTTP waits.";

pub fn error(e: Error) -> WorkspaceError {
    WorkspaceError::ToolDetails {
        code: e.code(), message: e.to_string(), category: "personal_runtime",
        retryable: matches!(e.code(), "RESOURCE_BUSY" | "QUEUE_FULL" | "STALE_CHECKPOINT"),
        details: json!({"recovery": "query the original task/job; re-read changed inputs before a new attempt", "safe_to_replay": false}),
    }
}

pub fn call(ctx: &ToolContext, name: &str, args: &Value) -> Result<Value, WorkspaceError> {
    let result = match name {
        "task_open" => open(ctx, args),
        "task_checkpoint" => ctx.personal.task_checkpoint(args),
        "task_status" => status(ctx, args),
        _ => Err(Error::contract("UNKNOWN_TOOL", "unknown personal tool")),
    };
    result.map(tool_ok).map_err(error)
}

fn open(ctx: &ToolContext, args: &Value) -> coding_tools_personal_runtime::Result<Value> {
    let receipt = ctx.personal.open_task_request(args)?;
    let task = receipt["task_id"].as_str().ok_or_else(||Error::contract("STATE_CORRUPT","task receipt has no ID"))?;
    let mut view = task_view(&ctx.personal,task,&json!({}))?;
    view["instructions"] = json!(INSTRUCTIONS);
    view["raw_input_captured"] = receipt["raw_input_captured"].clone();
    view["deduplicated"] = receipt["deduplicated"].clone();
    Ok(view)
}

fn status(ctx: &ToolContext, args: &Value) -> coding_tools_personal_runtime::Result<Value> {
    if let Some(job) = args.get("job_id").and_then(Value::as_str) {
        let view=ctx.personal.job_status(job, 4096)?;
        if let Some(task)=args.get("task_id").and_then(Value::as_str) {
            if view["task_id"].as_str()!=Some(task) {
                return Err(Error::contract("JOB_TASK_MISMATCH","job does not belong to the explicitly requested task; inspect the original receipt"));
            }
        }
        return Ok(view);
    }
    let bound = args.get("_host_session_key").and_then(Value::as_str)
        .map(|owner| ctx.personal.bound_task(owner)).transpose()?.flatten();
    let task = args.get("task_id").and_then(Value::as_str).or(bound.as_deref());
    let Some(task) = task else {
        return ctx.personal.task_list(args.get("cursor").and_then(Value::as_str), limit(args, 20));
    };
    if let Some(after) = args.get("events_after").and_then(Value::as_i64) {
        return ctx.personal.events(task, after.max(0), limit(args, 2));
    }
    task_view(&ctx.personal, task, args)
}

fn limit(args: &Value, default: usize) -> usize {
    args.get("limit").and_then(Value::as_u64).unwrap_or(default as u64).clamp(1, 100) as usize
}

/// Bounded recovery context, regardless of the number of already finished steps.
pub fn task_view(store: &Store, task: &str, args: &Value) -> coding_tools_personal_runtime::Result<Value> {
    let mut state = store.task_status(task)?;
    let checkpoint = state.as_object_mut().unwrap().remove("checkpoint").unwrap_or(json!({}));
    let cursor = args.get("cursor").and_then(Value::as_str).unwrap_or("");
    let include_passed = args.get("include_passed").and_then(Value::as_bool).unwrap_or(false);
    let mut counts = serde_json::Map::new();
    let mut page = serde_json::Map::new();
    let mut bytes = 0;
    let mut next = Value::Null;
    if let Some(steps) = checkpoint.get("steps").and_then(Value::as_object) {
        for record in steps.values() {
            let key = record["state"].as_str().unwrap_or("unknown");
            let count = counts.get(key).and_then(Value::as_u64).unwrap_or(0);
            counts.insert(key.into(), json!(count + 1));
        }
        let mut ordered: Vec<_> = steps.iter().collect();
        ordered.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (id, record) in ordered {
            if id.as_str() <= cursor || (!include_passed && record["state"] == "passed") { continue; }
            let encoded = record.to_string();
            let entry = if encoded.len() > 8192 {
                json!({"state": record["state"], "record_bytes": encoded.len(), "detail_omitted": true,
                    "hint": "store detailed evidence in files and keep task records small"})
            } else { record.clone() };
            if page.len() >= limit(args, 20) || bytes + entry.to_string().len() > 48_000 {
                next = page.keys().next_back().map(|key| json!(key)).unwrap_or(Value::Null);
                break;
            }
            bytes += entry.to_string().len();
            page.insert(id.clone(), entry);
        }
    }
    let mut summary = serde_json::Map::new();
    for key in ["summary", "next_step", "remaining_issues", "source_revision"] {
        if let Some(value) = checkpoint.get(key) {
            if value.to_string().len() <= 4096 { summary.insert(key.into(), value.clone()); }
        }
    }
    state["checkpoint"] = Value::Object(summary);
    state["steps"] = Value::Object(page);
    state["step_counts"] = Value::Object(counts);
    state["next_cursor"] = next;
    state["include_passed"] = json!(include_passed);
    let mut jobs = store.job_list(Some(task))?;
    // The task recovery limit also bounds its job summary. Keep the store's
    // existing 50-job cap and truncation evidence; never mutate/replay jobs.
    let job_limit = limit(args, 20).min(50);
    if let Some(items) = jobs.get_mut("jobs").and_then(Value::as_array_mut) {
        let shortened = items.len() > job_limit;
        items.truncate(job_limit);
        if shortened { jobs["truncated"] = json!(true); }
    }
    jobs["limit"] = json!(job_limit);
    state["jobs"] = jobs;
    state["acceptance_source"] = json!("step outcomes are caller-declared; inspect referenced evidence before claiming business acceptance");
    Ok(state)
}

pub fn job_poll(ctx: &ToolContext, args: &Value, cancel: bool) -> Result<Value, WorkspaceError> {
    let session = args.get("session_id").and_then(Value::as_str).unwrap_or("");
    let job = session.strip_prefix("job-").ok_or_else(|| WorkspaceError::invalid_argument("expected durable job session"))?;
    if !args.get("chars").and_then(Value::as_str).unwrap_or("").is_empty() {
        return Err(WorkspaceError::invalid_argument("durable jobs close stdin at launch; use tty=true for interactive commands"));
    }
    if cancel { ctx.personal.cancel_job(job).map_err(error)?; }
    let wait = args.get(if cancel { "wait_ms" } else { "yield_time_ms" }).and_then(Value::as_u64).unwrap_or(1000).min(30_000);
    let max = args.get("max_output_bytes").and_then(Value::as_u64).unwrap_or(65_536).clamp(1, 1_048_576) as usize;
    let mut result=ctx.personal.wait_job(job,Duration::from_millis(wait),max).map_err(error)?;
    result["transport_ok"]=json!(true);
    Ok(tool_ok(result))
}

pub fn job_output(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let reference = args.get("output_ref").and_then(Value::as_str).unwrap_or("");
    let parts: Vec<&str> = reference.split(':').collect();
    if parts.len() != 3 || parts[0] != "job" { return Err(WorkspaceError::invalid_argument("invalid job output reference")); }
    if let Some(stream) = args.get("stream").and_then(Value::as_str) {
        if stream != parts[2] { return Err(WorkspaceError::invalid_argument("stream conflicts with output_ref")); }
    }
    ctx.personal.job_output(parts[1], parts[2], args.get("offset").and_then(Value::as_u64).unwrap_or(0),
        args.get("limit").and_then(Value::as_u64).unwrap_or(4096).clamp(1, 1_048_576) as usize)
        .map(tool_ok).map_err(error)
}


#[cfg(test)]
mod status_contract_regressions {
    use super::*;

    fn fixture() -> (tempfile::TempDir, ToolContext, String) {
        let dir=tempfile::tempdir().unwrap();
        let workspace=dir.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let ctx=ToolContext::for_test(workspace,dir.path().join("harness")).unwrap();
        let task=ctx.personal.open_task_request(&json!({"goal":"status contract fixture","request_id":"open"})).unwrap()["task_id"].as_str().unwrap().to_string();
        (dir,ctx,task)
    }

    #[test]
    fn explicit_event_page_size_is_honored_without_changing_default() {
        let (_dir,ctx,task)=fixture();
        for n in 0..12 {ctx.personal.event(&task,"fixture",&json!({"number":n})).unwrap();}
        let first=status(&ctx,&json!({"task_id":task,"events_after":0,"limit":10})).unwrap();
        assert_eq!(first["events"].as_array().unwrap().len(),10);
        assert!(first["next_cursor"].is_number());
        let next=status(&ctx,&json!({"task_id":task,"events_after":first["next_cursor"],"limit":10})).unwrap();
        assert!(!next["events"].as_array().unwrap().is_empty());
        let default=status(&ctx,&json!({"task_id":task,"events_after":0})).unwrap();
        assert_eq!(default["events"].as_array().unwrap().len(),2);
    }

    #[test]
    fn contradictory_explicit_task_and_job_ids_are_rejected() {
        let (_dir,ctx,task)=fixture();
        let other=ctx.personal.open_task_request(&json!({"goal":"other fixture","request_id":"other"})).unwrap()["task_id"].as_str().unwrap().to_string();
        let job=uuid::Uuid::new_v4().to_string();
        ctx.personal.conn().unwrap().execute("INSERT INTO jobs(id,task_id,scope,request_id,input_hash,spec,state,exit_code,created,updated) VALUES(?1,?2,?2,'fixture','hash','{}','exited',0,0,0)",(&job,&task)).unwrap();
        assert_eq!(status(&ctx,&json!({"task_id":other,"job_id":job})).unwrap_err().code(),"JOB_TASK_MISMATCH");
        assert_eq!(status(&ctx,&json!({"task_id":task,"job_id":job})).unwrap()["task_id"],task);
        assert_eq!(status(&ctx,&json!({"job_id":job})).unwrap()["task_id"],task);
    }

    #[test]
    fn server_health_contains_bounded_runtime_pressure() {
        let (_dir,ctx,_task)=fixture();
        let result=super::super::dispatch::server_info(&ctx).unwrap();
        assert_eq!(result["runtime_pressure"]["active"],0);
        assert_eq!(result["runtime_pressure"]["includes_commands"],false);
        assert_eq!(result["runtime_pressure"]["state_modified"],false);
    }
}

#[cfg(test)]
mod task_view_job_limit_tests {
    use super::*;

    fn fixture(count: usize) -> (tempfile::TempDir, Store, String) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let store = Store::at(dir.path().join("state"), workspace).unwrap();
        let task = store.open_task_request(&json!({"goal":"bounded recovery fixture", "request_id":"bounded-task"})).unwrap()["task_id"]
            .as_str().unwrap().to_owned();
        let conn = store.conn().unwrap();
        for index in 0..count {
            conn.execute("INSERT INTO jobs(id,task_id,scope,request_id,input_hash,spec,state,exit_code,created,updated) VALUES(?1,?2,?2,?3,'fixture','{}','exited',0,?4,?4)",
                (uuid::Uuid::new_v4().to_string(), &task, format!("job-{index}"), index as i64)).unwrap();
        }
        (dir, store, task)
    }

    #[test]
    fn explicit_small_limit_bounds_jobs_and_reports_truncation() {
        let (_dir, store, task) = fixture(3);
        let view = task_view(&store, &task, &json!({"limit":1})).unwrap();
        assert_eq!(view["jobs"]["jobs"].as_array().unwrap().len(), 1);
        assert_eq!(view["jobs"]["limit"], 1);
        assert_eq!(view["jobs"]["truncated"], true);
        assert_eq!(view["jobs"]["jobs"][0]["request_id"], "job-2");
        assert_eq!(view["jobs"]["jobs"][0]["task_id"], task);
    }

    #[test]
    fn exact_limit_does_not_claim_missing_jobs() {
        let (_dir, store, task) = fixture(2);
        let view = task_view(&store, &task, &json!({"limit":2})).unwrap();
        assert_eq!(view["jobs"]["jobs"].as_array().unwrap().len(), 2);
        assert_eq!(view["jobs"]["limit"], 2);
        assert_eq!(view["jobs"]["truncated"], false);
    }

    #[test]
    fn default_recovery_is_bounded_to_twenty_jobs() {
        let (_dir, store, task) = fixture(25);
        let view = task_view(&store, &task, &json!({})).unwrap();
        assert_eq!(view["jobs"]["jobs"].as_array().unwrap().len(), 20);
        assert_eq!(view["jobs"]["limit"], 20);
        assert_eq!(view["jobs"]["truncated"], true);
    }

    #[test]
    fn upstream_fifty_job_cap_and_truncation_are_preserved() {
        let (_dir, store, task) = fixture(51);
        let view = task_view(&store, &task, &json!({"limit":100})).unwrap();
        assert_eq!(view["jobs"]["jobs"].as_array().unwrap().len(), 50);
        assert_eq!(view["jobs"]["limit"], 50);
        assert_eq!(view["jobs"]["truncated"], true);
    }

    #[test]
    fn empty_task_does_not_borrow_another_tasks_jobs() {
        let (_dir, store, _task) = fixture(3);
        let other = store.open_task_request(&json!({"goal":"unrelated empty fixture", "request_id":"other-task"})).unwrap()["task_id"]
            .as_str().unwrap().to_owned();
        let view = task_view(&store, &other, &json!({"limit":1})).unwrap();
        assert!(view["jobs"]["jobs"].as_array().unwrap().is_empty());
        assert_eq!(view["jobs"]["truncated"], false);
        assert_eq!(view["task_id"], other);
    }

    #[test]
    fn limiting_a_view_does_not_delete_jobs_or_change_step_cursor() {
        let (_dir, store, task) = fixture(3);
        let before = store.task_status(&task).unwrap();
        let view = task_view(&store, &task, &json!({"limit":1})).unwrap();
        assert_eq!(store.job_list(Some(&task)).unwrap()["jobs"].as_array().unwrap().len(), 3);
        assert_eq!(store.task_status(&task).unwrap(), before);
        assert_eq!(view["next_cursor"], Value::Null);
        assert!(view["steps"].as_object().unwrap().is_empty());
    }
}
