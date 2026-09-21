use serde_json::{json, Value};

pub fn definition(name: &str) -> Value {
    let description = match name {
        "task_open" => "Start or resume a durable task. Send goal and a stable request_id, or task_id to resume. No user-pasted startup prompt is needed. Only supplied text is captured; preserve returned task_id.",
        "task_status" => "Read bounded task progress, list tasks when no identity is available, page steps/events, or inspect a durable job_id. Never selects an unrelated workspace-global task. May reconcile a dead worker to unknown.",
        _ => "Persist step deltas and optional explicit raw_user_input. Preserve task_id, expected_revision and stable request_id. Include next_step and evidence; completed requires all declared acceptance steps to pass and no unresolved jobs.",
    };
    json!({"name":name,"title":name,"description":description,"inputSchema":schema(name).unwrap(),
        "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
}

pub fn schema(name: &str) -> Option<Value> {
    let properties = match name {
        "task_open" => json!({
            "goal":{"type":"string","maxLength":16384},
            "task_id":{"type":"string"},
            "request_id":{"type":"string","minLength":1,"maxLength":160},
            "new_task":{"type":"boolean","default":false},
            "raw_user_input":{"type":"string","maxLength":30000}
        }),
        "task_status" => json!({
            "task_id":{"type":"string"},"job_id":{"type":"string"},
            "cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100,"default":20},
            "include_passed":{"type":"boolean","default":false},
            "events_after":{"type":"integer","minimum":0}
        }),
        "task_checkpoint" => json!({
            "task_id":{"type":"string","minLength":1},
            "request_id":{"type":"string","minLength":1,"maxLength":160},
            "expected_revision":{"type":"integer","minimum":0},
            "state":{"type":"string","enum":["active","paused","blocked","completed","completed_unverified"],"default":"active"},
            "checkpoint":{"type":"object","description":"Small delta: summary, next_step, source_revision, remaining_issues, and steps keyed by stable IDs. Each step has state pending/running/passed/failed/blocked and evidence references.","additionalProperties":true},
            "raw_user_input":{"type":"string","maxLength":30000}
        }),
        _ => return None,
    };
    let mut result = json!({"type":"object","properties":properties,"additionalProperties":false});
    if name == "task_checkpoint" { result["required"] = json!(["task_id","request_id","expected_revision","checkpoint"]); }
    Some(result)
}

pub fn extend(name: &str, schema: &mut Value) {
    if matches!(name,"apply_patch"|"patch_check"|"exec_command") {
        schema["properties"]["task_id"] = json!({"type":"string","description":"Explicit durable task ID; unrelated tasks are never selected implicitly."});
        schema["properties"]["request_id"] = json!({"type":"string","minLength":1,"maxLength":160,"description":"Stable ID per logical operation. Reuse only to retrieve the same attempt; changed content needs a new ID."});
    }
    if matches!(name,"apply_patch"|"patch_check") {
        schema["properties"]["expected_hashes"] = json!({"type":"object","description":"Every touched path maps to read_file.file_sha256; use null only for a new, absent file. Required for actual writes, optional for dry_run/patch_check which returns current hashes.","additionalProperties":{"type":["string","null"]}});
    }
    if name == "exec_command" {
        schema["properties"]["durable"] = json!({"type":"boolean","default":true,"description":"Non-interactive commands run in a detached worker; interactive tty sessions remain transport-bound."});
        schema["properties"]["mode"] = json!({"type":"string","enum":["read","build","write"],"default":"write","description":"Cooperative scheduling declaration, NOT a security sandbox. read may overlap source writes; build freezes sources; write is exclusive."});
        schema["properties"]["resources"] = json!({"type":"array","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":256},"description":"Named exclusive outputs/resources shared with other jobs, e.g. target:module. Builds without explicit resources use a shared build-output lock."});
        schema["properties"]["timeout_ms"]["maximum"] = json!(86400000);
    }
}
