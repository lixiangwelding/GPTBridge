use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use crate::{Store,Error,Result,digest,now_ms};
use crate::store::{id,text};

impl Store {
    pub fn bind_task(&self, owner: &str, task: &str) -> Result<()> {
        self.task_status(task)?;
        self.conn()?.execute("INSERT INTO bindings(owner,task_id) VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET task_id=excluded.task_id", (digest(owner), task))?;
        Ok(())
    }
    pub fn task_open(&self, goal: &str, owner: Option<&str>, new_task: bool) -> Result<Value> {
        if goal.trim().is_empty() || goal.len()>16_384 {return Err(Error::contract("INVALID_GOAL","goal is required and must be <=16KiB"))}
        let owner=owner.map(digest);
        let mut c=self.conn()?; let tx=c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(owner)=owner.as_ref() {
            let old:Option<(String,String)>=tx.query_row("SELECT t.id,t.goal FROM tasks t JOIN bindings b ON b.task_id=t.id WHERE b.owner=?1",[owner],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((id,old_goal))=old {if !new_task {
                if old_goal!=goal {return Err(Error::contract("GOAL_CONFLICT","conversation is bound to a different goal; use explicit new_task=true or task_id"))}
                drop(tx); return self.task_status(&id);
            }}
        }
        let id=uuid::Uuid::new_v4().to_string(); let now=now_ms();
        tx.execute("INSERT INTO tasks VALUES(?1,?2,'active',0,'{}',?3,?3)",(&id,goal,now))?;
        if let Some(owner)=owner {tx.execute("INSERT INTO bindings(owner,task_id) VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET task_id=excluded.task_id",(owner,&id))?;}
        tx.commit()?; self.task_status(&id)
    }
    pub fn bound_task(&self, owner: &str) -> Result<Option<String>> {
        Ok(self.conn()?.query_row("SELECT task_id FROM bindings WHERE owner=?1",[digest(owner)],|r|r.get(0)).optional()?)
    }
    pub fn task_status(&self, task: &str) -> Result<Value> {
        id(task)?;
        let c=self.conn()?;
        let row:Option<(String,String,i64,String,i64,i64)>=c.query_row("SELECT goal,state,revision,checkpoint,created,updated FROM tasks WHERE id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        let Some((goal,state,revision,checkpoint,created,updated))=row else {return Err(Error::contract("TASK_NOT_FOUND","task is not in this workspace"))};
        Ok(json!({"task_id":task,"goal":goal,"state":state,"revision":revision,"checkpoint":serde_json::from_str::<Value>(&checkpoint)?,"created":created,"updated":updated,"transcript_capture":"only_explicitly_supplied_text","recovery":"read_current_files_and_resume_next_step; never restore the whole workspace"}))
    }
    pub fn task_list(&self, after: Option<&str>, limit: usize) -> Result<Value> {
        let c=self.conn()?; let mut q=c.prepare("SELECT id,goal,state,revision FROM tasks WHERE id>?1 ORDER BY id LIMIT ?2")?;
        let limit=limit.clamp(1,100);
        let rows=q.query_map((after.unwrap_or(""),(limit+1) as i64),|r|Ok(json!({"task_id":r.get::<_,String>(0)?,"goal":r.get::<_,String>(1)?,"state":r.get::<_,String>(2)?,"revision":r.get::<_,i64>(3)?})))?;
        let mut out=rows.collect::<std::result::Result<Vec<_>,_>>()?;
        let more=out.len()>limit; if more {out.pop();}
        let next=if more {out.last().map(|v|v["task_id"].clone())} else {None};
        Ok(json!({"tasks":out,"next_cursor":next}))
    }
    pub fn task_checkpoint(&self, args: &Value) -> Result<Value> {
        let task=id(text(args,"task_id",64)?)?;
        let request=text(args,"request_id",160)?;
        let revision=args.get("expected_revision").and_then(Value::as_i64).ok_or_else(||Error::contract("REVISION_REQUIRED","checkpoint needs expected_revision"))?;
        let update=args.get("checkpoint").filter(|v|v.is_object()).ok_or_else(||Error::contract("INVALID_CHECKPOINT","checkpoint must be an object"))?;
        if update.to_string().len()>262_144 {return Err(Error::contract("CHECKPOINT_TOO_LARGE","send only step deltas, at most 256KiB"))}
        let hash=digest(args.to_string()); let scope=format!("task:{task}");
        let mut c=self.conn()?; let tx=c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old_receipt:Option<(String,String)>=tx.query_row("SELECT input_hash,result FROM receipts WHERE scope=?1 AND request_id=?2",(&scope,request),|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((h,r))=old_receipt {if h!=hash {return Err(Error::contract("IDEMPOTENCY_CONFLICT","checkpoint request_id has different content"))} return Ok(serde_json::from_str(&r)?)}
        let row:Option<(i64,String,String)>=tx.query_row("SELECT revision,checkpoint,state FROM tasks WHERE id=?1",[task],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((current,old,state))=row else {return Err(Error::contract("TASK_NOT_FOUND","unknown task"))};
        if current!=revision {return Err(Error::contract("STALE_CHECKPOINT",format!("expected revision {revision}, current {current}; re-read and merge")))}
        if state=="completed" {return Err(Error::contract("TASK_COMPLETED","completed task is immutable; open a follow-up task"))}
        let mut checkpoint:Value=serde_json::from_str(&old)?;
        for (key,value) in update.as_object().unwrap() {
            if key=="steps" {
                let steps=value.as_object().ok_or_else(||Error::contract("INVALID_STEPS","steps must map stable IDs to records"))?;
                if checkpoint.get("steps").is_none() {checkpoint["steps"]=json!({})}
                for (step,record) in steps {
                    if step.is_empty() || step.len()>160 || !record.is_object() || !matches!(record["state"].as_str(),Some("pending"|"running"|"passed"|"failed"|"blocked")) {return Err(Error::contract("INVALID_STEP","step requires ID, object and explicit state"))}
                    checkpoint["steps"][step]=record.clone();
                }
            } else {checkpoint[key]=value.clone();}
        }
        if checkpoint.to_string().len()>2_097_152 {return Err(Error::contract("CHECKPOINT_TOO_LARGE","checkpoint limit is 2MiB; store detailed evidence in files"))}
        let next=args.get("state").and_then(Value::as_str).unwrap_or("active");
        if !matches!(next,"active"|"paused"|"blocked"|"completed"|"completed_unverified") {return Err(Error::contract("INVALID_STATE","unsupported task state"))}
        if next=="completed" {
            let steps=checkpoint["steps"].as_object().ok_or_else(||Error::contract("ACCEPTANCE_MISSING","no acceptance steps"))?;
            if steps.is_empty() || steps.values().any(|v| v["state"]!="passed" || v["evidence"].as_array().is_none_or(|a|a.is_empty())) {return Err(Error::contract("ACCEPTANCE_MISSING","every acceptance step must pass with evidence"))}
            let running:i64=tx.query_row("SELECT count(*) FROM jobs WHERE task_id=?1 AND state IN ('queued','running','unknown')",[task],|r|r.get(0))?;
            if running!=0 {return Err(Error::contract("JOBS_UNRESOLVED","task has running or unknown jobs"))}
        }
        let result=json!({"task_id":task,"revision":revision+1,"state":next,"saved":true});
        tx.execute("UPDATE tasks SET revision=revision+1,checkpoint=?2,state=?3,updated=?4 WHERE id=?1",(task,checkpoint.to_string(),next,now_ms()))?;
        tx.execute("INSERT INTO receipts VALUES(?1,?2,?3,'complete',?4)",(&scope,request,hash,result.to_string()))?;
        tx.execute("INSERT INTO events(task_id,kind,body,created) VALUES(?1,'checkpoint',?2,?3)",(task,result.to_string(),now_ms()))?;
        if let Some(raw) = args.get("raw_user_input").and_then(Value::as_str) {
            if raw.len() > 30_000 { return Err(Error::contract("INPUT_TOO_LARGE", "raw_user_input must be <=30000 bytes")); }
            tx.execute("INSERT INTO events(task_id,kind,body,created) VALUES(?1,'user_input',?2,?3)", (task,json!({"text":raw}).to_string(),now_ms()))?;
        }
        tx.commit()?; Ok(result)
    }
}
