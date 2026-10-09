//! A bounded projection of existing evidence. Never initialise, reconcile or clean a store.
use super::taskdock::{failure, profile};
use crate::{app_state::AppState, error::AppResult};
use coding_tools_personal_runtime::{digest, now_ms};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use tauri::State;

#[path = "taskdock_receipts_storage.rs"]
mod storage;
use storage::*;

const DETAIL_BYTES: usize = 65_536;
const SCAN_ROWS: usize = 100;
const HASH_ROWS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptCursor {
    pub started_at_ms: i64,
    pub id: String,
}

impl ReceiptCursor {
    fn validate(&self) -> AppResult<()> {
        if self.started_at_ms < 0 || self.id.is_empty() || self.id.len() > 200 {
            return Err(failure("INVALID_CURSOR：操作游标无效"));
        }
        if self.started_at_ms == 0 && !self.id.starts_with("receipt:") {
            return Err(failure("INVALID_CURSOR：持久回执游标无效"));
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn taskdock_operation_receipts(
    state: State<'_, AppState>,
    workspace_id: String,
    task_id: String,
    cursor: Option<ReceiptCursor>,
    limit: Option<usize>,
) -> AppResult<Value> {
    let p = profile(&state, &workspace_id)?;
    let limit = validate_query(&task_id, cursor.as_ref(), limit)?;
    let root = PathBuf::from(&p.path);
    if !root.is_absolute() || !root.is_dir() {
        return Err(failure("项目目录不可用"));
    }
    let root = root.canonicalize().map_err(|_| failure("项目目录不可用"))?;
    let runtime_dir = crate::harness::Harness::default_root()
        .map_err(|_| failure("任务存储不可用"))?
        .join("personal-runtime")
        .join(digest(root.to_string_lossy().as_bytes()));
    let audit_path = crate::platform::platform()
        .app_config_dir()
        .map_err(|_| failure("审计存储不可用"))?
        .join("audit/audit.sqlite");
    tauri::async_runtime::spawn_blocking(move || {
        project(
            &root,
            &runtime_dir,
            &audit_path,
            &workspace_id,
            &task_id,
            cursor.as_ref(),
            limit,
        )
    })
    .await
    .map_err(|_| failure("操作回执读取未完成"))?
}

fn validate_query(
    task: &str,
    cursor: Option<&ReceiptCursor>,
    limit: Option<usize>,
) -> AppResult<usize> {
    coding_tools_personal_runtime::store::id(task)
        .map_err(|_| failure("INVALID_ID：任务标识无效"))?;
    if let Some(cursor) = cursor {
        cursor.validate()?;
    }
    let limit = limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(failure("INVALID_LIMIT：每页须为 1 至 50 条"));
    }
    Ok(limit)
}

fn project(
    root: &Path,
    dir: &Path,
    audit_path: &Path,
    workspace: &str,
    task: &str,
    cursor: Option<&ReceiptCursor>,
    limit: usize,
) -> AppResult<Value> {
    let mut items = Vec::new();
    let mut warnings = BTreeSet::<String>::new();
    let mut next = None;
    let Some(c) = existing_connection(&dir.join("runtime.sqlite3"))? else {
        return Ok(
            json!({"workspace_id":workspace,"task_id":task,"observed_at":now_ms(),"items":[],
            "next_cursor":null,"partial":true,"warnings":["尚无持久任务库；没有创建空库"]}),
        );
    };
    let exists: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [task],
            |r| r.get(0),
        )
        .map_err(|_| failure("任务结构不可读取"))?;
    if !exists {
        return Err(failure("TASK_NOT_FOUND：任务不属于选中工作区"));
    }
    let durable_phase = cursor.is_some_and(|v| v.started_at_ms == 0);
    let mut audited_requests = BTreeSet::new();
    if !durable_phase {
        match existing_connection(audit_path) {
            Ok(Some(audit)) => match audit_rows(&audit, workspace, cursor) {
                Ok(rows) => {
                    for (index, row) in rows.iter().take(SCAN_ROWS).enumerate() {
                        let position = ReceiptCursor {
                            started_at_ms: row.started,
                            id: row.id.clone(),
                        };
                        let input = parse_detail(row.input.as_deref());
                        let output = parse_detail(row.output.as_deref());
                        if input.is_none() || output.is_none() {
                            warnings.insert("部分审计正文未记录、被截断或超过读取上限；不能推断任务归属或文件哈希".into());
                        }
                        let path_matches = row
                            .workspace_path
                            .as_deref()
                            .is_some_and(|p| Path::new(p) == root);
                        let job = job_id(input.as_ref(), output.as_ref());
                        if path_matches
                            && belongs(&c, task, input.as_ref(), output.as_ref(), job.as_deref())?
                        {
                            if let Some(tool) = row.tool.as_ref().filter(|s| {
                                s.len() <= 128
                                    && s.bytes().all(|b| {
                                        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')
                                    })
                            }) {
                                let request = bounded_string(
                                    input.as_ref().and_then(|v| v.get("request_id")).or_else(
                                        || output.as_ref().and_then(|v| v.get("request_id")),
                                    ),
                                    160,
                                );
                                let saved = if tool == "apply_patch" && row.status == "success" {
                                    request
                                        .as_deref()
                                        .zip(input.as_ref())
                                        .and_then(|(r, input)| {
                                            durable_result(&c, task, r, input, output.as_ref())
                                        })
                                } else {
                                    None
                                };
                                if saved.is_some() {
                                    if let Some(r) = &request {
                                        audited_requests.insert(r.clone());
                                    }
                                }
                                if hash_rows_limited(saved.as_ref().or(output.as_ref())) {
                                    warnings.insert(
                                        "补丁文件哈希超过本页上限；仅展示前 100 个路径".into(),
                                    );
                                }
                                let hashes = patch_hashes(saved.as_ref().or(output.as_ref()));
                                let (job_start, job_finish, job_exit) =
                                    job_timing(&c, dir, task, job.as_deref())?;
                                let details = if row.input_truncated || row.output_truncated {
                                    "truncated"
                                } else if input.is_some() && output.is_some() {
                                    "complete"
                                } else {
                                    "not_recorded"
                                };
                                items.push(json!({"id":row.id,"source":"audit","tool_name":tool,"workspace_id":workspace,
                                    "task_id":task,"request_id":request,"protocol_request_id":row.protocol_request.as_ref().filter(|s|s.len()<=160),
                                    "job_id":job,"status":row.status,"started_at_ms":row.started,"finished_at_ms":row.finished,
                                    "timing_source":"dispatcher_audit","job_started_at_ms":job_start,"job_finished_at_ms":job_finish,
                                    "exit_code":job_exit.or(row.exit),"patch_hashes":hashes,"details_state":details}));
                            }
                        }
                        if index + 1 < rows.len()
                            && (items.len() >= limit || index + 1 == SCAN_ROWS)
                        {
                            next = Some(position);
                            break;
                        }
                    }
                }
                Err(_) => {
                    warnings.insert("既有审计结构不可读取；仅返回可验证的持久回执".into());
                }
            },
            Ok(None) => {
                warnings.insert("尚无审计库；持久补丁回执没有原调用档位与开始时间".into());
            }
            Err(_) => {
                warnings.insert("既有审计库不可读取；未初始化或清理数据库".into());
            }
        }
    }
    if next.is_none() && items.len() < limit {
        let after = cursor
            .filter(|_| durable_phase)
            .map(|v| v.id.strip_prefix("receipt:").unwrap_or(""))
            .unwrap_or("");
        let mut q=c.prepare("SELECT request_id,state,substr(result,1,?3) FROM receipts WHERE scope=?1 AND request_id>?2 ORDER BY request_id LIMIT ?4")
            .map_err(|_|failure("持久回执结构不可读取"))?;
        let rows = q
            .query_map(
                params![
                    format!("patch:{task}"),
                    after,
                    (DETAIL_BYTES + 1) as i64,
                    (SCAN_ROWS + 1) as i64
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .map_err(|_| failure("持久回执不可读取"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| failure("持久回执不可读取"))?;
        for (index, (request, state, text)) in rows.iter().take(SCAN_ROWS).enumerate() {
            if !audited_requests.contains(request) && request.len() <= 160 {
                let output = parse_detail(text.as_deref())
                    .filter(|v| v["task_id"] == task && v["request_id"] == request.as_str());
                if hash_rows_limited(output.as_ref()) {
                    warnings.insert("补丁文件哈希超过本页上限；仅展示前 100 个路径".into());
                }
                let end:Option<i64>=c.query_row("SELECT created FROM events WHERE task_id=?1 AND kind='patch_finished'
                    AND length(CAST(body AS BLOB))<=?3 AND CASE WHEN json_valid(body) THEN json_extract(body,'$.request_id') END=?2
                    ORDER BY seq DESC LIMIT 1",params![task,request,DETAIL_BYTES as i64],|r|r.get(0)).optional().unwrap_or(None);
                let status = if state != "complete" {
                    "unknown"
                } else if output.as_ref().is_some_and(|v| v["ok"] == false) {
                    "rejected"
                } else if output.as_ref().is_some_and(|v| v["ok"] == true) {
                    "success"
                } else {
                    "unknown"
                };
                items.push(json!({"id":format!("receipt:{request}"),"source":"durable_receipt","tool_name":"apply_patch",
                    "workspace_id":null,"task_id":task,"request_id":request,"protocol_request_id":null,"job_id":null,
                    "status":status,"started_at_ms":null,"finished_at_ms":end,
                    "timing_source":if end.is_some(){"task_event_end_only"}else{"not_recorded"},
                    "job_started_at_ms":null,"job_finished_at_ms":null,"exit_code":null,
                    "patch_hashes":patch_hashes(output.as_ref()),"details_state":if output.is_some(){"complete"}else{"not_recorded"}}));
                warnings.insert("持久补丁摘要属于该实际目录，原调用 workspace_id 与开始时间未记录；可能与较早审计描述同一操作".into());
            }
            if index + 1 < rows.len() && (items.len() >= limit || index + 1 == SCAN_ROWS) {
                next = Some(ReceiptCursor {
                    started_at_ms: 0,
                    id: format!("receipt:{request}"),
                });
                break;
            }
        }
    }
    // A full last audit page must still offer the separate durable summary phase.
    if next.is_none() && !durable_phase && items.len() >= limit {
        let count: i64 = c
            .query_row(
                "SELECT count(*) FROM receipts WHERE scope=?1",
                [format!("patch:{task}")],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if count > audited_requests.len() as i64 {
            next = Some(ReceiptCursor {
                started_at_ms: 0,
                id: "receipt:".into(),
            });
        }
    }
    Ok(
        json!({"workspace_id":workspace,"task_id":task,"observed_at":now_ms(),"items":items,"next_cursor":next,
        "partial":!warnings.is_empty(),"warnings":warnings}),
    )
}

#[cfg(test)]
#[path = "taskdock_receipts_tests.rs"]
mod tests;
