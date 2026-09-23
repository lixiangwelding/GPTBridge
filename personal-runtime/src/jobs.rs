use std::{fs, io::{Read,Seek,SeekFrom}, path::{Path,PathBuf}, process::{Command,Stdio}};
use rusqlite::{Connection, OptionalExtension};
use std::time::{Duration, Instant};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use crate::{Store,Error,Result,digest,now_ms};
use crate::store::{id,private_dir};

pub const MAX_RUNNING:usize=8;
pub const MAX_HEAVY:usize=2;
pub const MAX_QUEUED:usize=32;
pub const MAX_STREAM_BYTES:usize=8*1024*1024;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpec {
    pub program:PathBuf,
    pub args:Vec<String>,
    pub cwd:PathBuf,
    pub workspace:PathBuf,
    #[serde(default)] pub stdin:String,
    #[serde(default="write_mode")] pub mode:String,
    #[serde(default)] pub resources:Vec<String>,
    pub timeout_ms:u64,
}
fn write_mode()->String {"write".into()}
impl JobSpec {
    pub fn validate(&self,workspace:&Path)->Result<()> {
        if self.workspace.canonicalize()?!=workspace.canonicalize()? || !self.cwd.canonicalize()?.starts_with(workspace.canonicalize()?) {return Err(Error::contract("OUTSIDE_WORKSPACE","job cwd/workspace mismatch"))}
        if !self.program.is_absolute() || !self.program.is_file() {return Err(Error::contract("INVALID_EXECUTABLE","worker requires the policy-resolved executable"))}
        if !matches!(self.mode.as_str(),"read"|"build"|"write") || self.resources.len()>16 || self.resources.iter().any(|s|s.is_empty()||s.len()>256) {return Err(Error::contract("INVALID_RESOURCES","invalid execution mode/resources"))}
        if self.timeout_ms==0 || self.timeout_ms>86_400_000 || self.stdin.len()>1_048_576 || self.args.iter().map(|a|a.len()).sum::<usize>()>1_048_576 {return Err(Error::contract("JOB_LIMIT","job time/input limit exceeded"))}
        Ok(())
    }
}
impl Store {
    pub fn launch_job(&self,worker:&Path,spec:&JobSpec,task:Option<&str>,request:&str)->Result<Value> {
        spec.validate(&self.workspace)?;
        if request.is_empty()||request.len()>160 {return Err(Error::contract("REQUEST_ID_REQUIRED","durable commands need a stable request_id per logical step"))}
        if let Some(t)=task {id(t)?;}
        let hash=digest(serde_json::to_vec(spec)?); let scope=task.unwrap_or("workspace");
        let mut c=self.conn()?;
        let mut reconciled=0usize;
        let mut retried_admission=false;
        let job=loop {
        let tx=c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old:Option<(String,String)>=tx.query_row("SELECT id,input_hash FROM jobs WHERE scope=?1 AND request_id=?2",(scope,request),|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((id,old_hash))=old {
            if hash!=old_hash {return Err(Error::contract("IDEMPOTENCY_CONFLICT","command request_id already has different input"))}
            drop(tx); let mut value=self.job_status(&id,4096)?;value["deduplicated"]=json!(true);return Ok(value)
        }
        // Configuration changes must not produce inconsistent shared slot pools.
        // Old receipts above remain readable even when new work requires restart.
        self.limits.require_unchanged(&self.dir)?;
        // Receipt recovery is not new work. Check completion only after dedup,
        // in the same transaction as admission so completion cannot race an insert.
        if let Some(t)=task {
            let state:Option<String>=tx.query_row("SELECT state FROM tasks WHERE id=?1",[t],|r|r.get(0)).optional()?;
            let state=state.ok_or_else(||Error::contract("TASK_NOT_FOUND","unknown task in this workspace"))?;
            if state=="completed" {return Err(Error::contract("TASK_COMPLETED","open a follow-up task before launching new work"))}
        }
        let count:i64=tx.query_row("SELECT count(*) FROM jobs WHERE state IN ('queued','running')",[],|r|r.get(0))?;
        if count>=self.limits.queued_and_running as i64 {
            drop(tx);
            if retried_admission {return Err(Error::contract("QUEUE_FULL","bounded worker queue is full; query existing jobs before submitting more"))}
            // Bounded recovery, outside the write transaction. Alive locks and
            // startup grace remain authoritative; stale intents become unknown,
            // never successful or replayable, and no command is relaunched.
            reconciled=self.reconcile_active_jobs_on(&c)?;
            retried_admission=true;
            continue;
        }
        let job=uuid::Uuid::new_v4().to_string();let now=now_ms();
        tx.execute("INSERT INTO jobs(id,task_id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,?2,?3,?4,?5,?6,'queued',?7,?7)",(&job,task,scope,request,hash,serde_json::to_string(spec)?,now))?;
        tx.commit()?;
        break job;
        };
        drop(c);
        if let Err(error)=private_dir(&self.dir.join("jobs").join(&job)) {
            self.finish_job(&job,"spawn_failed",None,&format!("worker preparation failed: {}; command not launched",error.code()))?;
            return self.job_status(&job,4096);
        }
        let mut cmd=Command::new(worker);
        cmd.args(["--personal-job-worker",self.dir.to_str().ok_or_else(||Error::contract("PATH_ENCODING","state path must be UTF-8"))?,&job]);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(unix)] {use std::os::unix::process::CommandExt; unsafe {cmd.pre_exec(||{if libc::setsid()<0 {Err(std::io::Error::last_os_error())}else{Ok(())}});} }
        match cmd.spawn() {
            Ok(mut child)=>{std::thread::spawn(move||{let _=child.wait();});},
            Err(e)=>{self.finish_job(&job,"spawn_failed",None,&format!("worker launch failed: {}",e.kind()))?;}
        }
        let mut value=self.job_status(&job,4096)?;
        value["admission_reconciled"]=json!(reconciled);
        Ok(value)
    }
    fn reconcile_active_jobs_on(&self,c:&Connection)->Result<usize> {
        let ids={
            let mut q=c.prepare_cached("SELECT id FROM jobs WHERE state IN ('queued','running') ORDER BY updated,id LIMIT ?1")?;
            let rows=q.query_map([self.limits.queued_and_running as i64],|r|r.get::<_,String>(0))?;
            rows.collect::<std::result::Result<Vec<_>,_>>()?
        };
        let mut reconciled=0;
        for job in ids {
            if self.job_status_on(c,&job,None)?["status"]=="unknown" {reconciled+=1;}
        }
        Ok(reconciled)
    }
    pub fn job_spec(&self,job:&str)->Result<JobSpec> {
        id(job)?;
        let raw:Option<String>=self.conn()?.query_row("SELECT spec FROM jobs WHERE id=?1",[job],|r|r.get(0)).optional()?;
        Ok(serde_json::from_str(&raw.ok_or_else(||Error::contract("JOB_NOT_FOUND","unknown job in this workspace"))?)?)
    }
    /// Read-only active-queue snapshot; excludes commands, stdin and output.
    pub fn runtime_pressure(&self)->Result<Value> {
        let observed=now_ms();
        let c=self.conn()?;
        let mut q=c.prepare_cached("SELECT id,task_id,state,created,updated,json_extract(spec,'$.mode'),detail FROM jobs WHERE state IN ('queued','running') ORDER BY created,id LIMIT ?1")?;
        let rows=q.query_map([(self.limits.queued_and_running+1) as i64],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,String>(6)?)))?
            .collect::<std::result::Result<Vec<_>,_>>()?;
        let (mut queued,mut running,mut heavy)=(0usize,0usize,0usize);
        let mut sample=Vec::new();
        for (job,task,state,created,updated,mode,detail) in rows.iter().take(self.limits.queued_and_running) {
            if state=="queued" {queued+=1;} else {running+=1;if mode.as_deref()==Some("build") {heavy+=1;}}
            if sample.len()<8 {
                sample.push(json!({"job_id":job,"task_id":task,"state":state,"mode":mode,
                    "age_ms":observed.saturating_sub(*created).max(0),"heartbeat_age_ms":observed.saturating_sub(*updated).max(0),
                    "waiting_for":if state=="queued" {waiting_reason(detail)} else {None}}));
            }
        }
        Ok(json!({"available":true,"scope":"workspace","observed_at_ms":observed,"queued":queued,"running":running,"running_builds":heavy,
            "active":queued+running,"admission_remaining":self.limits.queued_and_running.saturating_sub(queued+running),
            "counts_are_lower_bounds":rows.len()>self.limits.queued_and_running,"sample":sample,"sample_truncated":rows.len()>8,
            "limits":self.limits.jobs_json(),
            "state_modified":false,"includes_commands":false,
            "interpretation":"database snapshot, not held-permit measurement; old heartbeat alone does not prove worker loss"}))
    }
    pub fn finish_job(&self,job:&str,state:&str,exit:Option<i32>,detail:&str)->Result<()> {
        id(job)?;
        if !matches!(state,"exited"|"cancelled"|"timeout"|"queue_timeout"|"spawn_failed"|"unknown") {return Err(Error::contract("INVALID_STATE","invalid worker terminal state"))}
        // A late worker/error handler must never overwrite an already persisted terminal result.
        self.conn()?.execute("UPDATE jobs SET state=?2,exit_code=?3,updated=?4,detail=?5 WHERE id=?1 AND state IN ('queued','running')",(job,state,exit,now_ms(),detail))?;
        Ok(())
    }
    pub fn uncertain_writes(&self)->Result<Vec<String>> {
        let c=self.conn()?;
        let mut q=c.prepare("SELECT id FROM jobs WHERE state IN ('running','unknown') AND json_extract(spec,'$.mode') <> 'read' LIMIT 64")?;
        let ids=q.query_map([],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let mut result=Vec::new();
        for id in ids {if self.job_status_on(&c,&id,None)?["status"]=="unknown" {result.push(id);}}
        Ok(result)
    }
    pub fn job_cancelled(&self,job:&str)->Result<bool> {self.job_cancelled_on(&self.conn()?,job)}
    pub(crate) fn job_cancelled_on(&self,c:&Connection,job:&str)->Result<bool> {
        Ok(c.prepare_cached("SELECT cancel FROM jobs WHERE id=?1")?
            .query_row([id(job)?],|r|r.get::<_,i64>(0))?!=0)
    }
    pub fn cancel_job(&self,job:&str)->Result<Value> {
        id(job)?;self.conn()?.execute("UPDATE jobs SET cancel=1 WHERE id=?1 AND state IN ('queued','running')",[job])?;
        self.job_status(job,4096)
    }
    pub fn job_status(&self,job:&str,max_output:usize)->Result<Value> {
        self.job_status_on(&self.conn()?,job,Some(max_output))
    }

    // A connection belongs to this request, never a global mutex. No read
    // transaction spans a sleep, so worker commits/cancellation remain visible.
    fn job_status_on(&self,c:&Connection,job:&str,max_output:Option<usize>)->Result<Value> {
        id(job)?;
        let mut row=read_job_row(c,job)?;
        if row.2=="running" || (row.2=="queued" && now_ms()-row.5>5000) {
            if let Some(_guard)=crate::locks::try_gate(&self.dir,&format!("alive:{job}"),true)? {
                c.execute("UPDATE jobs SET state='unknown',detail='worker unavailable; verify side effects before retry',updated=?2 WHERE id=?1 AND updated=?3 AND state IN ('queued','running')",(job,now_ms(),row.5))?;
                // A late terminal commit or another observer may win the CAS.
                // Always return its real state and timestamp, not our old row.
                row=read_job_row(c,job)?;
            }
        }
        let (task,request,state,exit,created,updated,cancel,detail)=row;
        let command_ok=match state.as_str(){"exited"=>Some(exit==Some(0)),"queued"|"running"|"unknown"|"resolved"=>None,_=>Some(false)};
        let mut value=json!({"job_id":job,"session_id":format!("job-{job}"),"task_id":task,"request_id":request,"status":state,"termination_reason":state,"exit_code":exit,"command_ok":command_ok,"created":created,"updated":updated,"cancel_requested":cancel!=0,"detail":detail,"output_loaded":false,"stdout_truncated":null,"stderr_truncated":null,"output_refs":{"stdout":format!("job:{job}:stdout"),"stderr":format!("job:{job}:stderr")},"durable":true,"safe_to_replay":false,"limits":{"running":MAX_RUNNING,"heavy":MAX_HEAVY,"queued_and_running":MAX_QUEUED,"stream_bytes":MAX_STREAM_BYTES}});
        let observed=now_ms();
        value["limits"]=self.limits.jobs_json();
        value["limits"]["stream_bytes"]=json!(MAX_STREAM_BYTES);
        value["waiting_for"]=json!(if state=="queued" {waiting_reason(&detail)} else {None});
        value["queued_for_ms"]=json!(if state=="queued" {Some(observed.saturating_sub(created).max(0))} else {None});
        value["heartbeat_age_ms"]=json!(if matches!(state.as_str(),"queued"|"running") {Some(observed.saturating_sub(updated).max(0))} else {None});
        if let Some(max)=max_output {attach_output(&mut value,&self.dir.join("jobs").join(job),max);}
        Ok(value)
    }

    /// Poll metadata with one connection and bounded backoff; load output once.
    /// This waits on the original job and never resubmits an uncertain command.
    pub fn wait_job(&self,job:&str,wait:Duration,max_output:usize)->Result<Value> {
        id(job)?;
        let c=self.conn()?;
        let start=Instant::now();
        let wait=wait.min(Duration::from_secs(30));
        let mut delay=Duration::from_millis(20);
        loop {
            let mut value=self.job_status_on(&c,job,None)?;
            let remaining=wait.saturating_sub(start.elapsed());
            if !matches!(value["status"].as_str(),Some("queued"|"running")) || remaining.is_zero() {
                attach_output(&mut value,&self.dir.join("jobs").join(job),max_output);
                return Ok(value);
            }
            std::thread::sleep(delay.min(remaining));
            delay=(delay*2).min(Duration::from_millis(200));
        }
    }

    pub fn job_list(&self,task:Option<&str>)->Result<Value> {
        if let Some(t)=task {id(t)?;}
        let c=self.conn()?;
        let ids={
            let sql=if task.is_some(){"SELECT id FROM jobs WHERE task_id=?1 ORDER BY created DESC,id LIMIT 51"}
                else{"SELECT id FROM jobs ORDER BY created DESC,id LIMIT 51"};
            let mut q=c.prepare_cached(sql)?;
            let params:Vec<&dyn rusqlite::ToSql>=task.as_ref().map(|t|vec![t as &dyn rusqlite::ToSql]).unwrap_or_default();
            let rows=q.query_map(params.as_slice(),|r|r.get::<_,String>(0))?;
            rows.collect::<std::result::Result<Vec<_>,_>>()?
        };
        let more=ids.len()>50;
        let jobs=ids.iter().take(50).map(|id|self.job_status_on(&c,id,None)).collect::<Result<Vec<_>>>()?;
        // Null truncation flags mean output was not inspected, not an empty log.
        Ok(json!({"jobs":jobs,"truncated":more,"limit":50}))
    }
    pub fn job_output(&self,job:&str,stream:&str,offset:u64,limit:usize)->Result<Value> {
        id(job)?;
        if !matches!(stream,"stdout"|"stderr"){return Err(Error::contract("INVALID_STREAM","stream must be stdout or stderr"))}
        let status=self.job_status_on(&self.conn()?,job,None)?;
        let path=self.dir.join("jobs").join(job).join(format!("{stream}.log"));
        let budget = limit.clamp(1, 1_048_576);
        let mut data = Vec::new();
        let (mut total, mut actual) = (0, 0);
        if path.exists() {
            let mut file = fs::File::open(path)?;
            total = file.metadata()?.len();
            actual = offset.min(total);
            file.seek(SeekFrom::Start(actual))?;
            // At most three lookahead bytes complete a UTF-8 codepoint. Bind the
            // read to this file-size snapshot even if the worker is appending.
            file.take((budget as u64 + 3).min(total - actual)).read_to_end(&mut data)?;
        }
        let mut consumed = 0;
        let mut pending_utf8_bytes = 0;
        while consumed < budget && consumed < data.len() {
            let remaining = &data[consumed..];
            let (valid_bytes, invalid_len) = match std::str::from_utf8(remaining) {
                Ok(_) => (remaining.len(), None),
                Err(error) => (error.valid_up_to(), error.error_len()),
            };
            if valid_bytes > 0 {
                let mut take = (budget - consumed).min(valid_bytes);
                while take < valid_bytes && remaining[take] & 0xc0 == 0x80 { take -= 1; }
                if take == 0 && consumed == 0 {
                    // Tiny limits must still advance by one complete codepoint.
                    take = 1;
                    while take < valid_bytes && remaining[take] & 0xc0 == 0x80 { take += 1; }
                }
                consumed += take;
                if take < valid_bytes { break; }
            } else if invalid_len.is_none()
                && matches!(status["status"].as_str(), Some("queued" | "running" | "unknown")) {
                // Defer a live partial codepoint without advancing its cursor.
                if consumed == 0 { pending_utf8_bytes = remaining.len(); }
                break;
            } else {
                // Treat each malformed sequence as a unit, then continue at the
                // next boundary; don't split normal text following bad bytes.
                let take = invalid_len.unwrap_or(remaining.len());
                if consumed > 0 && consumed + take > budget { break; }
                consumed += take;
            }
        }
        let content = String::from_utf8_lossy(&data[..consumed]);
        let content_lossy = matches!(&content, std::borrow::Cow::Owned(_));
        let next = actual + consumed as u64;
        Ok(json!({"output_ref":format!("job:{job}:{stream}"),"content":content,"offset":actual,
            "next_offset":if pending_utf8_bytes == 0 && next < total {Some(next)}else{None},
            "poll_offset":next,"bytes_read":consumed,"content_lossy":content_lossy,
            "pending_utf8_bytes":pending_utf8_bytes,"retained_bytes":total,
            "stream_cap_bytes":MAX_STREAM_BYTES,"may_be_truncated":total>=MAX_STREAM_BYTES as u64,
            // Reuse the authoritative row already read above, including an
            // explicitly unbound null owner; never infer ownership from callers.
            "job_id":status["job_id"],"task_id":status["task_id"],"request_id":status["request_id"],
            "job_status":status["status"],
            "offset_encoding":"bytes; use returned offsets; a tiny limit may expand by up to 3 bytes for one UTF-8 codepoint"}))
    }
}
fn waiting_reason(detail:&str)->Option<&str> {
    detail.strip_prefix("waiting:").filter(|reason|matches!(*reason,"source"|"resource"|"heavy_capacity"|"command_capacity"))
}
type JobRow=(Option<String>,String,String,Option<i32>,i64,i64,i64,String);
fn read_job_row(c:&Connection,job:&str)->Result<JobRow> {
    c.prepare_cached("SELECT task_id,request_id,state,exit_code,created,updated,cancel,detail FROM jobs WHERE id=?1")?
        .query_row([job],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?)))
        .optional()?.ok_or_else(||Error::contract("JOB_NOT_FOUND","unknown job in this workspace"))
}
fn bounded_metadata(path:&Path)->Option<Value> {
    let file=fs::File::open(path).ok()?;
    let mut bytes=Vec::new();
    file.take(16_385).read_to_end(&mut bytes).ok()?;
    if bytes.len()>16_384 {return None;}
    serde_json::from_slice(&bytes).ok()
}
fn attach_output(value:&mut Value,dir:&Path,max:usize) {
    // Share a bounded metadata read across streams; polling loops still skip output.
    let totals=bounded_metadata(&dir.join("output.json"));
    let (stdout,out_truncated)=capture_view_with_totals(dir,"stdout",max,totals.as_ref());
    let (stderr,err_truncated)=capture_view_with_totals(dir,"stderr",max,totals.as_ref());
    value["stdout"]=json!(stdout);value["stderr"]=json!(stderr);
    value["stdout_truncated"]=json!(out_truncated);value["stderr_truncated"]=json!(err_truncated);
    value["output_loaded"]=json!(true);
    let child=bounded_metadata(&dir.join("child.json"));
    let started=child.as_ref().and_then(|v|v["started"].as_i64());
    let created=value["created"].as_i64();
    value["timing"]=json!({
        "queue_wait_ms":started.zip(created).filter(|(start,created)|start>=created).map(|(start,created)|start-created),
        "worker_duration_ms":totals.as_ref().and_then(|v|v["duration_ms"].as_u64()),
        "running_for_ms":if value["status"]=="running" {started.map(|start|now_ms().saturating_sub(start).max(0))} else {None},
        "missing_values_are_unknown":true});
}
fn tail_file(path: &Path, limit: usize) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    let take = size.min(limit.clamp(1, 1_048_576) as u64);
    let lookbehind = (size - take).min(3) as usize;
    file.seek(SeekFrom::Start(size - take - lookbehind as u64))?;
    let mut bytes = Vec::new();
    file.take(take + lookbehind as u64).read_to_end(&mut bytes)?;
    // A bounded tail may start inside a valid codepoint. Omit that partial
    // prefix; complete recovery remains available through read_output.
    let mut start = lookbehind.min(bytes.len());
    if start < bytes.len() && bytes[start] & 0xc0 == 0x80 {
        let mut lead = start;
        while lead > 0 && bytes[lead] & 0xc0 == 0x80 { lead -= 1; }
        let width = match bytes[lead] {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 0,
        };
        let end = lead + width;
        // Skip only a validated codepoint crossing the preview boundary;
        // orphan continuation bytes must remain visible as invalid data.
        if lead < start && end > start && end <= bytes.len()
            && std::str::from_utf8(&bytes[lead..end]).is_ok() {
            start = end;
        }
    }
    Ok(String::from_utf8_lossy(&bytes[start..]).into_owned())
}

/// Preserve the legacy execution contract without hiding lost or truncated errors.
#[cfg(test)]
fn capture_view(dir:&Path,stream:&str,limit:usize)->(String,bool){
    capture_view_with_totals(dir,stream,limit,bounded_metadata(&dir.join("output.json")).as_ref())
}
fn capture_view_with_totals(dir:&Path,stream:&str,limit:usize,totals:Option<&Value>)->(String,bool){
    let log=dir.join(format!("{stream}.log"));
    let text=tail_file(&dir.join(format!("{stream}.tail")),limit).or_else(|_|tail_file(&log,limit));
    let Ok(text)=text else {return (String::new(),true)};
    let retained=fs::metadata(&log).map(|m|m.len()).unwrap_or(0);
    let total=totals.and_then(|v|v[stream]["total_bytes"].as_u64());
    let truncated=total.unwrap_or(retained)>text.len() as u64
        || (total.is_none() && retained>=MAX_STREAM_BYTES as u64);
    (text,truncated)
}

#[cfg(test)]
mod output_contract_tests {
    use super::*;
    #[test]
    fn absent_output_is_not_reported_as_a_verified_empty_stream(){
        let dir=tempfile::tempdir().unwrap();
        assert_eq!(capture_view(dir.path(),"stderr",32),(String::new(),true));
    }
    #[test]
    fn empty_stderr_and_truncated_tail_are_distinguished(){
        let dir=tempfile::tempdir().unwrap();
        fs::write(dir.path().join("stderr.log"),b"").unwrap();
        assert_eq!(capture_view(dir.path(),"stderr",32),(String::new(),false));
        fs::write(dir.path().join("stderr.log"),b"error line\n").unwrap();
        fs::write(dir.path().join("stderr.tail"),b"line\n").unwrap();
        fs::write(dir.path().join("output.json"),br#"{"stderr":{"total_bytes":1000}}"#).unwrap();
        assert_eq!(capture_view(dir.path(),"stderr",32),("line\n".to_string(),true));
    }
}

#[cfg(test)]
mod audit_tail_preview_tests {
    use super::*;
    #[test]
    fn audit_tail_preview_never_splits_valid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unicode.log");
        let text = "log 中文🙂 emoji終";
        fs::write(&path, text).unwrap();
        for limit in 1..=text.len()+1 {
            let tail = tail_file(&path, limit).unwrap();
            assert!(!tail.contains('\u{fffd}'), "limit={limit}, tail={tail:?}");
            assert!(text.ends_with(&tail));
            assert!(tail.len() <= limit);
        }
    }
    #[test]
    fn audit_tail_preview_keeps_real_invalid_byte_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.log");
        fs::write(&path, [b'A', 0xff, b'B']).unwrap();
        assert!(tail_file(&path, 10).unwrap().contains('\u{fffd}'));
    }
    #[test]
    fn audit_tail_preview_preserves_orphan_continuation_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orphan.log");
        fs::write(&path, [b'A', 0x80, b'B']).unwrap();
        assert_eq!(tail_file(&path, 2).unwrap(), "\u{fffd}B");
    }
}
