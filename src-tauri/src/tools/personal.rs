//! Small task API. Task identity is explicit; a workspace never has one global active task.
use std::time::Duration;
use coding_tools_personal_runtime::{digest, Error, Store};
use serde_json::{json, Value};
use super::{context::ToolContext, workspace::{tool_ok, WorkspaceError}};

pub const TOOLS: &[&str] = &["task_open", "task_status", "task_checkpoint"];
pub const INSTRUCTIONS: &str = "Use task_open with the user's goal (or task_id to resume); no pasted startup prompt is required. Keep task_id on every write/command. Use a stable request_id for each logical mutation and expected_revision for task_checkpoint. Read file_sha256 and pass expected_hashes to apply_patch; on conflict re-read and merge, never restore an old workspace snapshot. Independent work may run concurrently in the same directory; no worktree is required. Non-interactive exec_command uses durable jobs; query the returned job/session before retrying. Save step deltas and evidence using task_checkpoint before replying. Only explicitly supplied goal/raw_user_input is recorded; do not claim automatic access to unprovided chat text. Legacy history_session tools remain available for old archives. Unknown results require inspection, never blind replay.";

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
    let store = &ctx.personal;
    let owner = args.get("_host_session_key").and_then(Value::as_str);
    let request = args.get("request_id").and_then(Value::as_str);
    let explicit = args.get("task_id").and_then(Value::as_str);
    let new_task = args.get("new_task").and_then(Value::as_bool).unwrap_or(false);
    if explicit.is_none() && (owner.is_none() || new_task) && request.is_none() {
        return Err(Error::contract("REQUEST_ID_REQUIRED", "send a stable request_id to create a task without conversation metadata, or when requesting a new task"));
    }
    let _gate = coding_tools_personal_runtime::locks::gate(&store.dir, "task-open", true, Duration::from_secs(5))?;
    let mut canonical = args.clone();
    if let Some(obj) = canonical.as_object_mut() { obj.remove("_host_session_key"); }
    let scope = format!("task-open:{}", digest(owner.unwrap_or("explicit-request")));
    if let Some(key) = request {
        if let Some(cached) = store.begin_receipt(&scope, key, &digest(canonical.to_string()))? {
            if let Some(task) = cached["task_id"].as_str() {
                let mut view = task_view(store, task, &json!({}))?;
                view["deduplicated"] = json!(true);
                return Ok(view);
            }
            return Ok(cached);
        }
    }
    let outcome = (|| {
        let state = if let Some(task) = explicit {
            let state = store.task_status(task)?;
            if let Some(goal) = args.get("goal").and_then(Value::as_str) {
                if state["goal"] != goal { return Err(Error::contract("GOAL_CONFLICT", "task_id refers to a different goal")); }
            }
            if let Some(owner) = owner { store.bind_task(owner, task)?; }
            state
        } else {
            let goal = coding_tools_personal_runtime::store::text(args, "goal", 16_384)?;
            store.task_open(goal, owner, new_task)?
        };
        let task = state["task_id"].as_str().ok_or_else(|| Error::contract("STATE_CORRUPT", "task has no ID"))?;
        // Raw text is optional, private, and never synthesized from unavailable conversations.
        if let Some(raw) = args.get("raw_user_input").and_then(Value::as_str) {
            store.event(task, "user_input", &json!({"text": raw}))?;
        }
        let mut view = task_view(store, task, &json!({}))?;
        view["instructions"] = json!(INSTRUCTIONS);
        view["raw_input_captured"] = json!(args.get("raw_user_input").is_some());
        Ok(view)
    })();
    if let Some(key) = request {
        match &outcome {
            Ok(v) => store.finish_receipt(&scope, key, v)?,
            // The intent stays indeterminate on error; it cannot silently create a duplicate task.
            Err(_) => {}
        }
    }
    outcome
}

fn status(ctx: &ToolContext, args: &Value) -> coding_tools_personal_runtime::Result<Value> {
    if let Some(job) = args.get("job_id").and_then(Value::as_str) {
        return ctx.personal.job_status(job, 4096);
    }
    let bound = args.get("_host_session_key").and_then(Value::as_str)
        .map(|owner| ctx.personal.bound_task(owner)).transpose()?.flatten();
    let task = args.get("task_id").and_then(Value::as_str).or(bound.as_deref());
    let Some(task) = task else {
        return ctx.personal.task_list(args.get("cursor").and_then(Value::as_str), limit(args, 20));
    };
    if let Some(after) = args.get("events_after").and_then(Value::as_i64) {
        return ctx.personal.events(task, after.max(0), limit(args, 2).min(3));
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
    state["jobs"] = store.job_list(Some(task))?;
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
    let start = std::time::Instant::now();
    loop {
        let mut result = ctx.personal.job_status(job, max).map_err(error)?;
        if !matches!(result["status"].as_str(), Some("queued" | "running")) || start.elapsed() >= Duration::from_millis(wait) {
            result["transport_ok"] = json!(true);
            return Ok(tool_ok(result));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
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
