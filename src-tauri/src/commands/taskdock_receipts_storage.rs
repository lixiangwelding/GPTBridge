//! Bounded readers of existing evidence, without store initialization or reconciliation.
use super::super::taskdock::failure;
use super::{ReceiptCursor, DETAIL_BYTES, HASH_ROWS, SCAN_ROWS};
use crate::error::AppResult;
use coding_tools_personal_runtime::{digest, now_ms};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path},
    time::Duration,
};

// READ_ONLY respects the live WAL. Immutable mode would silently miss committed evidence.
// Do not call Store::open/conn or AuditStore::open_default/query: those may write or clean up.
pub(super) fn existing_connection(path: &Path) -> AppResult<Option<Connection>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(failure("既有回执数据库不可读取")),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || path
            .parent()
            .is_some_and(|p| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()))
    {
        return Err(failure("UNSAFE_STATE_PATH：回执存储路径无效"));
    }
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| failure("既有回执数据库不可读取"))?;
    c.busy_timeout(Duration::from_millis(150))
        .map_err(|_| failure("回执读取暂不可用"))?;
    Ok(Some(c))
}

pub(super) struct AuditRow {
    pub(super) id: String,
    pub(super) started: i64,
    pub(super) finished: i64,
    pub(super) workspace_path: Option<String>,
    pub(super) tool: Option<String>,
    pub(super) status: String,
    pub(super) protocol_request: Option<String>,
    pub(super) input: Option<String>,
    pub(super) output: Option<String>,
    pub(super) input_truncated: bool,
    pub(super) output_truncated: bool,
    pub(super) exit: Option<i32>,
}

pub(super) fn audit_rows(
    c: &Connection,
    workspace: &str,
    cursor: Option<&ReceiptCursor>,
) -> AppResult<Vec<AuditRow>> {
    let (before, id) = cursor
        .map(|c| (c.started_at_ms, c.id.as_str()))
        .unwrap_or((i64::MAX, ""));
    // An indexed workspace/time window bounds both record count and captured JSON memory.
    let mut q = c
        .prepare(
            "SELECT id,started_at_ms,finished_at_ms,workspace_path,tool_name,status,request_id,
        CASE WHEN input_truncated=0 AND length(CAST(input_json AS BLOB))<=?4 THEN input_json END,
        CASE WHEN output_truncated=0 AND length(CAST(output_json AS BLOB))<=?4 THEN output_json END,
        input_truncated OR input_bytes>?4,output_truncated OR output_bytes>?4,exit_code
        FROM audit_records WHERE record_type='tool' AND workspace_id=?1
        AND (started_at_ms<?2 OR (started_at_ms=?2 AND id<?3))
        ORDER BY started_at_ms DESC,id DESC LIMIT ?5",
        )
        .map_err(|_| failure("审计结构不可读取"))?;
    let rows = q
        .query_map(
            params![
                workspace,
                before,
                id,
                DETAIL_BYTES as i64,
                (SCAN_ROWS + 1) as i64
            ],
            |r| {
                Ok(AuditRow {
                    id: r.get(0)?,
                    started: r.get(1)?,
                    finished: r.get(2)?,
                    workspace_path: r.get(3)?,
                    tool: r.get(4)?,
                    status: r.get(5)?,
                    protocol_request: r.get(6)?,
                    input: r.get(7)?,
                    output: r.get(8)?,
                    input_truncated: r.get(9)?,
                    output_truncated: r.get(10)?,
                    exit: r.get(11)?,
                })
            },
        )
        .map_err(|_| failure("审计记录不可读取"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| failure("审计记录不可读取"))
}

pub(super) fn parse_detail(text: Option<&str>) -> Option<Value> {
    let text = text.filter(|t| t.len() <= DETAIL_BYTES)?;
    serde_json::from_str::<Value>(text)
        .ok()
        .filter(Value::is_object)
}

pub(super) fn bounded_string(value: Option<&Value>, max: usize) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty() && v.len() <= max)
        .map(str::to_string)
}

pub(super) fn job_id(input: Option<&Value>, output: Option<&Value>) -> Option<String> {
    let direct = output
        .and_then(|v| v.get("job_id"))
        .or_else(|| input.and_then(|v| v.get("job_id")))
        .and_then(Value::as_str);
    let session = input
        .and_then(|v| v.get("session_id"))
        .and_then(Value::as_str)
        .and_then(|s| s.strip_prefix("job-"));
    direct
        .or(session)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .map(str::to_string)
}

pub(super) fn belongs(
    c: &Connection,
    task: &str,
    input: Option<&Value>,
    output: Option<&Value>,
    job: Option<&str>,
) -> AppResult<bool> {
    let input_task = input.and_then(|v| v.get("task_id")).and_then(Value::as_str);
    let output_task = output
        .and_then(|v| v.get("task_id").or_else(|| v.get("task_scope")))
        .and_then(Value::as_str);
    if input_task.is_some_and(|t| t != task) || output_task.is_some_and(|t| t != task) {
        return Ok(false);
    }
    if let Some(job) = job {
        let owner: Option<Option<String>> = c
            .query_row("SELECT task_id FROM jobs WHERE id=?1", [job], |r| r.get(0))
            .optional()
            .map_err(|_| failure("作业归属不可读取"))?;
        return Ok(owner.flatten().as_deref() == Some(task));
    }
    Ok(input_task == Some(task) || output_task == Some(task))
}

pub(super) fn bounded_file(path: &Path, parent: &Path) -> Option<Value> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16_384 {
        return None;
    }
    let actual = path.canonicalize().ok()?;
    let parent = parent.canonicalize().ok()?;
    if !actual.starts_with(&parent) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(actual)
        .ok()?
        .take(16_385)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 16_384 {
        return None;
    }
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
}

pub(super) fn job_timing(
    c: &Connection,
    dir: &Path,
    task: &str,
    job: Option<&str>,
) -> AppResult<(Option<i64>, Option<i64>, Option<i32>)> {
    let Some(job) = job else {
        return Ok((None, None, None));
    };
    let row: Option<(String, Option<i32>, i64, i64)> = c
        .query_row(
            "SELECT state,exit_code,created,updated FROM jobs WHERE id=?1 AND task_id=?2",
            params![job, task],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|_| failure("作业时间不可读取"))?;
    let Some((status, exit, created, updated)) = row else {
        return Ok((None, None, None));
    };
    let child = bounded_file(&dir.join("jobs").join(job).join("child.json"), dir);
    let started = child
        .filter(|v| v["job_id"] == job)
        .and_then(|v| v["started"].as_i64())
        .filter(|t| *t >= created && *t <= now_ms());
    let finished = if matches!(status.as_str(), "exited" | "cancelled" | "timeout") {
        started.and_then(|start| Some(updated).filter(|t| *t >= created && *t >= start))
    } else {
        None
    };
    Ok((started, finished, exit))
}

pub(super) fn patch_hashes(output: Option<&Value>) -> Vec<Value> {
    let before = output
        .and_then(|v| v.get("before_hashes"))
        .and_then(Value::as_object);
    let after = output
        .and_then(|v| v.get("after_hashes"))
        .and_then(Value::as_object);
    let paths: BTreeSet<&String> = before
        .into_iter()
        .flat_map(|m| m.keys())
        .chain(after.into_iter().flat_map(|m| m.keys()))
        .collect();
    paths
        .into_iter()
        .filter(|s| {
            let p = Path::new(s);
            s.len() <= 4096
                && !p.is_absolute()
                && p.components().all(|c| matches!(c, Component::Normal(_)))
        })
        .take(HASH_ROWS)
        .map(|path| {
            fn cell(value: Option<&Value>) -> (Option<String>, Option<bool>) {
                match value {
                    Some(Value::Null) => (None, Some(false)),
                    Some(Value::String(s))
                        if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) =>
                    {
                        (Some(s.clone()), Some(true))
                    }
                    _ => (None, None),
                }
            }
            let (before_sha, before_exists) = cell(before.and_then(|m| m.get(path)));
            let (after_sha, after_exists) = cell(after.and_then(|m| m.get(path)));
            json!({"path":path,"before_sha256":before_sha,"after_sha256":after_sha,
            "before_exists":before_exists,"after_exists":after_exists})
        })
        .collect()
}

pub(super) fn hash_rows_limited(output: Option<&Value>) -> bool {
    let before = output
        .and_then(|v| v.get("before_hashes"))
        .and_then(Value::as_object);
    let after = output
        .and_then(|v| v.get("after_hashes"))
        .and_then(Value::as_object);
    before
        .into_iter()
        .flat_map(|m| m.keys())
        .chain(after.into_iter().flat_map(|m| m.keys()))
        .collect::<BTreeSet<_>>()
        .len()
        > HASH_ROWS
}

pub(super) fn durable_result(
    c: &Connection,
    task: &str,
    request: &str,
    input: &Value,
    audited: Option<&Value>,
) -> Option<Value> {
    if !input["patch"].is_string()
        || input["dry_run"] == true
        || audited.is_some_and(|v| v["ok"] == false)
    {
        return None;
    }
    // Match personal_patch's original intent identity. Reusing only task/request would
    // attach an old successful patch to a later IDEMPOTENCY_CONFLICT rejection.
    let mut normalized = input.clone();
    let object = normalized.as_object_mut()?;
    object.remove("reason");
    object.remove("_host_session_key");
    let hash = digest(normalized.to_string());
    let text: Option<String>=c.query_row("SELECT substr(result,1,?3) FROM receipts WHERE scope=?1 AND request_id=?2 AND state='complete' AND input_hash=?4",
        params![format!("patch:{task}"),request,(DETAIL_BYTES+1) as i64,hash],|r|r.get(0)).optional().ok().flatten();
    parse_detail(text.as_deref()).filter(|v| {
        v["ok"] == true
            && v["task_id"] == task
            && v["request_id"] == request
            && audited
                .and_then(|a| a.get("change_id"))
                .is_none_or(|change| v.get("change_id") == Some(change))
    })
}
