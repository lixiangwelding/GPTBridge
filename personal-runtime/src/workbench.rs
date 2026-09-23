//! Read-only, bounded workbench views of the existing task/job store.
//! An active task without a running job is READY, never a simulated executor.
use std::collections::BTreeMap;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use crate::{Error, Result, Store, now_ms};

pub const DISPLAY_STATES: &[&str] = &["running", "attention", "queued", "ready", "paused", "done"];

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct WorkbenchCursor {
    pub updated: i64,
    pub task_id: String,
    #[serde(default)]
    pub workspace_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct WorkbenchQuery {
    #[serde(default)] pub query: String,
    #[serde(default)] pub filter: String,
    pub cursor: Option<WorkbenchCursor>,
    pub limit: Option<usize>,
}

impl WorkbenchQuery {
    pub fn validate(&self) -> Result<usize> {
        if self.query.len() > 1024 { return Err(Error::contract("INVALID_QUERY", "query must be <=1024 bytes")); }
        if !self.filter.is_empty() && self.filter != "all" && !DISPLAY_STATES.contains(&self.filter.as_str()) {
            return Err(Error::contract("INVALID_FILTER", "unsupported task filter"));
        }
        let limit = self.limit.unwrap_or(30);
        if !(1..=50).contains(&limit) { return Err(Error::contract("INVALID_LIMIT", "page size must be 1..50")); }
        if let Some(cursor) = &self.cursor {
            crate::store::id(&cursor.task_id)?;
            if cursor.updated < 0 || cursor.workspace_id.len() > 160 {
                return Err(Error::contract("INVALID_CURSOR", "invalid task cursor"));
            }
        }
        Ok(limit)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskRow {
    pub task_id: String,
    pub goal: String,
    pub state: String,
    pub display_state: String,
    pub revision: i64,
    pub created: i64,
    pub updated: i64,
    pub summary: String,
    pub next_step: String,
    pub running_jobs: u64,
    pub queued_jobs: u64,
    pub unknown_jobs: u64,
    #[serde(default)] pub workspace_id: String,
    #[serde(default)] pub workspace_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkbenchPage {
    pub tasks: Vec<TaskRow>,
    pub counts: BTreeMap<String, u64>,
    pub matched_total: u64,
    pub next_cursor: Option<WorkbenchCursor>,
    pub observed_at: i64,
}

// Only fixed SQL fragments are composed. User input is always bound, and
// instr() deliberately treats %, _ and quotes as literal search text.
const ROWS: &str = r#"
WITH jobs_by_task AS (
 SELECT task_id,
 SUM(state='running') AS running_jobs, SUM(state='queued') AS queued_jobs,
 SUM(state='unknown') AS unknown_jobs, MAX(updated) AS job_updated,
 MAX(CASE WHEN state='exited' AND exit_code=0 THEN updated ELSE 0 END) AS success_at,
 MAX(CASE WHEN (state='exited' AND exit_code<>0) OR state='failed' THEN updated ELSE 0 END) AS failure_at
 FROM jobs WHERE task_id IS NOT NULL GROUP BY task_id
), rows AS (
 SELECT t.id AS task_id, substr(t.goal,1,600) AS goal, t.state, t.revision, t.created,
 MAX(t.updated,COALESCE(j.job_updated,0)) AS updated,
 substr(COALESCE(json_extract(t.checkpoint,'$.summary'),''),1,600) AS summary,
 substr(COALESCE(json_extract(t.checkpoint,'$.next_step'),''),1,300) AS next_step,
 COALESCE(j.running_jobs,0) AS running_jobs, COALESCE(j.queued_jobs,0) AS queued_jobs,
 COALESCE(j.unknown_jobs,0) AS unknown_jobs,
 CASE
  WHEN COALESCE(j.running_jobs,0)>0 THEN 'running'
  WHEN COALESCE(j.queued_jobs,0)>0 THEN 'queued'
  WHEN COALESCE(j.unknown_jobs,0)>0 THEN 'attention'
  WHEN t.state='completed' THEN 'done'
  WHEN t.state IN ('blocked','completed_unverified') THEN 'attention'
  WHEN t.state='paused' THEN 'paused'
  WHEN COALESCE(j.failure_at,0)>MAX(t.updated,COALESCE(j.success_at,0)) THEN 'attention'
  ELSE 'ready' END AS display_state,
 t.goal AS search_goal
 FROM tasks t LEFT JOIN jobs_by_task j ON j.task_id=t.id
)
"#;
const MATCHES: &str = "WHERE (?1='' OR instr(lower(search_goal),lower(?1))>0 OR instr(lower(task_id),lower(?1))>0) AND (?2='' OR ?2='all' OR display_state=?2)";

impl Store {
    /// A single transaction binds counts and rows to one SQLite read snapshot.
    /// No jobs are reconciled, cancelled, replayed or created by this view.
    pub fn workbench_page(&self, query: &WorkbenchQuery, workspace_id: &str) -> Result<WorkbenchPage> {
        let limit = query.validate()?;
        let mut connection = self.conn()?;
        let tx = connection.transaction()?;
        let counts = {
            let mut counts: BTreeMap<String,u64> = DISPLAY_STATES.iter().map(|s|(s.to_string(),0)).collect();
            let mut statement = tx.prepare(&format!("{ROWS} SELECT display_state,count(*) FROM rows GROUP BY display_state"))?;
            for entry in statement.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))? {
                let (key,value) = entry?; counts.insert(key,value.max(0) as u64);
            }
            counts
        };
        let matched_total = tx.query_row(&format!("{ROWS} SELECT count(*) FROM rows {MATCHES}"),
            params![query.query.trim(), query.filter], |r|r.get::<_,i64>(0))?.max(0) as u64;
        let (before, id, equal) = query.cursor.as_ref()
            .map(|c|(c.updated,c.task_id.as_str(),i32::from(workspace_id < c.workspace_id.as_str())))
            .unwrap_or((i64::MAX,"",0));
        let mut tasks = {
            let sql = format!("{ROWS} SELECT task_id,goal,state,display_state,revision,created,updated,summary,next_step,running_jobs,queued_jobs,unknown_jobs FROM rows {MATCHES} AND (updated<?3 OR (updated=?3 AND (task_id<?4 OR (?5=1 AND task_id=?4)))) ORDER BY updated DESC,task_id DESC LIMIT ?6");
            let mut statement = tx.prepare(&sql)?;
            let result = statement.query_map(params![query.query.trim(),query.filter,before,id,equal,(limit+1) as i64], |r| {
                Ok(TaskRow {task_id:r.get(0)?,goal:r.get(1)?,state:r.get(2)?,display_state:r.get(3)?,revision:r.get(4)?,created:r.get(5)?,updated:r.get(6)?,summary:r.get(7)?,next_step:r.get(8)?,running_jobs:r.get::<_,i64>(9)?.max(0) as u64,queued_jobs:r.get::<_,i64>(10)?.max(0) as u64,unknown_jobs:r.get::<_,i64>(11)?.max(0) as u64,workspace_id:workspace_id.to_string(),workspace_name:String::new()})
            })?.collect::<std::result::Result<Vec<_>,_>>()?;
            result
        };
        let more = tasks.len() > limit;
        tasks.truncate(limit);
        let next_cursor = if more {tasks.last().map(|t|WorkbenchCursor{updated:t.updated,task_id:t.task_id.clone(),workspace_id:workspace_id.to_string()})} else {None};
        tx.commit()?;
        Ok(WorkbenchPage {tasks,counts,matched_total,next_cursor,observed_at:now_ms()})
    }
}
