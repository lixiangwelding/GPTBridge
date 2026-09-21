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
        if let Some(t)=task {if self.task_status(t)?["state"] == "completed" {return Err(Error::contract("TASK_COMPLETED","open a follow-up task before launching new work"))}}
        let hash=digest(serde_json::to_vec(spec)?); let scope=task.unwrap_or("workspace");
        let mut c=self.conn()?; let tx=c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old:Option<(String,String)>=tx.query_row("SELECT id,input_hash FROM jobs WHERE scope=?1 AND request_id=?2",(scope,request),|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((id,old_hash))=old {
            if hash!=old_hash {return Err(Error::contract("IDEMPOTENCY_CONFLICT","command request_id already has different input"))}
            drop(tx); let mut value=self.job_status(&id,4096)?;value["deduplicated"]=json!(true);return Ok(value)
        }
        let count:i64=tx.query_row("SELECT count(*) FROM jobs WHERE state IN ('queued','running')",[],|r|r.get(0))?;
        if count>=MAX_QUEUED as i64 {return Err(Error::contract("QUEUE_FULL","bounded worker queue is full; query existing jobs before submitting more"))}
        let job=uuid::Uuid::new_v4().to_string();let now=now_ms();
        tx.execute("INSERT INTO jobs(id,task_id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,?2,?3,?4,?5,?6,'queued',?7,?7)",(&job,task,scope,request,hash,serde_json::to_string(spec)?,now))?;
        tx.commit()?;
        private_dir(&self.dir.join("jobs").join(&job))?;
        let mut cmd=Command::new(worker);
        cmd.args(["--personal-job-worker",self.dir.to_str().ok_or_else(||Error::contract("PATH_ENCODING","state path must be UTF-8"))?,&job]);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(unix)] {use std::os::unix::process::CommandExt; unsafe {cmd.pre_exec(||{if libc::setsid()<0 {Err(std::io::Error::last_os_error())}else{Ok(())}});} }
        match cmd.spawn() {
            Ok(mut child)=>{std::thread::spawn(move||{let _=child.wait();});},
            Err(e)=>{self.finish_job(&job,"spawn_failed",None,&format!("worker launch failed: {}",e.kind()))?;}
        }
        self.job_status(&job,4096)
    }
    pub fn job_spec(&self,job:&str)->Result<JobSpec> {
        id(job)?;
        let raw:Option<String>=self.conn()?.query_row("SELECT spec FROM jobs WHERE id=?1",[job],|r|r.get(0)).optional()?;
        Ok(serde_json::from_str(&raw.ok_or_else(||Error::contract("JOB_NOT_FOUND","unknown job in this workspace"))?)?)
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
        let mut data=Vec::new();let mut total=0;let mut actual=0;
        if path.exists(){let mut f=fs::File::open(path)?;total=f.metadata()?.len();actual=offset.min(total);f.seek(SeekFrom::Start(actual))?;f.take(limit.clamp(1,1_048_576) as u64).read_to_end(&mut data)?;}
        let next=actual+data.len() as u64;
        Ok(json!({"output_ref":format!("job:{job}:{stream}"),"content":String::from_utf8_lossy(&data),"offset":actual,"next_offset":if next<total {Some(next)}else{None},"poll_offset":next,"retained_bytes":total,"stream_cap_bytes":MAX_STREAM_BYTES,"may_be_truncated":total>=MAX_STREAM_BYTES as u64,"job_status":status["status"],"offset_encoding":"bytes; use returned offsets"}))
    }
}
type JobRow=(Option<String>,String,String,Option<i32>,i64,i64,i64,String);
fn read_job_row(c:&Connection,job:&str)->Result<JobRow> {
    c.prepare_cached("SELECT task_id,request_id,state,exit_code,created,updated,cancel,detail FROM jobs WHERE id=?1")?
        .query_row([job],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?)))
        .optional()?.ok_or_else(||Error::contract("JOB_NOT_FOUND","unknown job in this workspace"))
}
fn attach_output(value:&mut Value,dir:&Path,max:usize) {
    let (stdout,out_truncated)=capture_view(dir,"stdout",max);
    let (stderr,err_truncated)=capture_view(dir,"stderr",max);
    value["stdout"]=json!(stdout);value["stderr"]=json!(stderr);
    value["stdout_truncated"]=json!(out_truncated);value["stderr_truncated"]=json!(err_truncated);
    value["output_loaded"]=json!(true);
}
fn tail_file(path:&Path,limit:usize)->Result<String>{let mut f=fs::File::open(path)?;let size=f.metadata()?.len();let take=size.min(limit.clamp(1,1_048_576) as u64);f.seek(SeekFrom::End(-(take as i64)))?;let mut b=Vec::new();f.take(take).read_to_end(&mut b)?;Ok(String::from_utf8_lossy(&b).into_owned())}

/// Preserve the legacy execution contract without hiding lost or truncated errors.
fn capture_view(dir:&Path,stream:&str,limit:usize)->(String,bool){
    let log=dir.join(format!("{stream}.log"));
    let text=tail_file(&dir.join(format!("{stream}.tail")),limit).or_else(|_|tail_file(&log,limit));
    let Ok(text)=text else {return (String::new(),true)};
    let retained=fs::metadata(&log).map(|m|m.len()).unwrap_or(0);
    let totals=fs::read(dir.join("output.json")).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
    let total=totals.as_ref().and_then(|v|v[stream]["total_bytes"].as_u64());
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
