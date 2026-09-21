//! Task creation, binding, supplied input and idempotency receipt share one transaction.
use rusqlite::{OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use crate::{digest, now_ms, Error, Result, Store};
use crate::store::{id, text};

impl Store {
    pub fn open_task_request(&self, args: &Value) -> Result<Value> {
        let owner = args.get("_host_session_key").and_then(Value::as_str);
        let explicit = args.get("task_id").and_then(Value::as_str);
        if let Some(task) = explicit { id(task)?; }
        let new_task = args.get("new_task").and_then(Value::as_bool).unwrap_or(false);
        let request = if args.get("request_id").is_some() { Some(text(args,"request_id",160)?) } else { None };
        if explicit.is_none() && (owner.is_none() || new_task) && request.is_none() {
            return Err(Error::contract("REQUEST_ID_REQUIRED","a stable request_id is required without conversation metadata or for a new task"));
        }
        let goal = if explicit.is_none() || args.get("goal").is_some() { Some(text(args,"goal",16_384)?) } else { None };
        let raw = if args.get("raw_user_input").is_some() {
            Some(args["raw_user_input"].as_str().filter(|s|s.len()<=30_000)
                .ok_or_else(||Error::contract("INPUT_TOO_LARGE","raw_user_input must be a string <=30000 bytes"))?)
        } else { None };
        let mut canonical = args.clone();
        if let Some(obj) = canonical.as_object_mut() {obj.remove("_host_session_key");}
        let hash = digest(canonical.to_string());
        let scope = format!("task-open:{}",digest(owner.unwrap_or("explicit-request")));
        let mut c = self.conn()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = request {
            let prior:Option<(String,String,Option<String>)> = tx.query_row(
                "SELECT input_hash,state,result FROM receipts WHERE scope=?1 AND request_id=?2",
                (&scope,key),|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
            if let Some((old_hash,state,result)) = prior {
                if old_hash != hash {return Err(Error::contract("IDEMPOTENCY_CONFLICT","request_id already has different task input"));}
                if state != "complete" {return Err(Error::contract("OPERATION_INDETERMINATE","old non-atomic attempt needs inspection; do not create a duplicate"));}
                let mut value:Value = serde_json::from_str(result.as_deref().unwrap_or("null"))?;
                value["deduplicated"] = json!(true);
                return Ok(value);
            }
        }
        let mut created = false;
        let task = if let Some(task) = explicit {
            let existing:Option<String> = tx.query_row("SELECT goal FROM tasks WHERE id=?1",[task],|r|r.get(0)).optional()?;
            let existing = existing.ok_or_else(||Error::contract("TASK_NOT_FOUND","task is not in this workspace"))?;
            if goal.is_some_and(|g|g!=existing) {return Err(Error::contract("GOAL_CONFLICT","task refers to a different goal"));}
            task.to_string()
        } else {
            let prior:Option<(String,String)> = if !new_task {
                if let Some(owner) = owner {
                    tx.query_row("SELECT t.id,t.goal FROM tasks t JOIN bindings b ON b.task_id=t.id WHERE b.owner=?1",[digest(owner)],|r|Ok((r.get(0)?,r.get(1)?))).optional()?
                } else {None}
            } else {None};
            if let Some((task,old_goal)) = prior {
                if Some(old_goal.as_str()) != goal {return Err(Error::contract("GOAL_CONFLICT","conversation is bound to another goal; resume by task_id or explicitly open a new task"));}
                task
            } else {
                let task = uuid::Uuid::new_v4().to_string();
                let now = now_ms();
                tx.execute("INSERT INTO tasks VALUES(?1,?2,'active',0,'{}',?3,?3)",(&task,goal.unwrap(),now))?;
                created = true;
                task
            }
        };
        if let Some(owner) = owner {
            tx.execute("INSERT INTO bindings(owner,task_id) VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET task_id=excluded.task_id",(digest(owner),&task))?;
        }
        if let Some(raw) = raw {
            // A supplied resume message must actually be saved before reporting
            // raw_input_captured=true. Request-ID replays returned above already.
            tx.execute("INSERT INTO events(task_id,kind,body,created) VALUES(?1,'user_input',?2,?3)",(&task,json!({"text":raw}).to_string(),now_ms()))?;
        }
        let result = json!({"task_id":task,"created":created,"deduplicated":false,"raw_input_captured":raw.is_some()});
        if let Some(key) = request {
            tx.execute("INSERT INTO receipts VALUES(?1,?2,?3,'complete',?4)",(&scope,key,hash,result.to_string()))?;
        }
        tx.commit()?;
        Ok(result)
    }
}
