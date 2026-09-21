#![cfg(unix)]
//! Real subprocess tests; all files, workers and signals belong to a fresh temp workspace.
use std::{fs, path::{Path, PathBuf}, process::Command, time::{Duration, Instant}};
use coding_tools_personal_runtime::{Store, digest, jobs::{JobSpec, MAX_STREAM_BYTES}, locks};
use serde_json::{json, Value};

fn fixture() -> (tempfile::TempDir, Store) {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace"); fs::create_dir(&workspace).unwrap();
    let store = Store::open(&root.path().join("state"), &workspace).unwrap();
    (root, store)
}
fn worker() -> &'static Path { Path::new(env!("CARGO_BIN_EXE_coding-tools-personal-worker")) }
fn spec(store: &Store, script: &str, mode: &str) -> JobSpec {
    JobSpec { program: PathBuf::from("/bin/sh"), args:vec!["-c".into(),script.into()],
        workspace:store.workspace.clone(),cwd:store.workspace.clone(),stdin:String::new(),
        mode:mode.into(),resources:vec![],timeout_ms:10000 }
}
fn launch(store: &Store, spec: &JobSpec, request: &str) -> String {
    store.launch_job(worker(),spec,None,request).unwrap()["job_id"].as_str().unwrap().into()
}
fn await_path(path: &Path) {
    let start=Instant::now();
    while !path.exists() { assert!(start.elapsed()<Duration::from_secs(8),"missing {}",path.display()); std::thread::sleep(Duration::from_millis(20)); }
}
fn finish(store: &Store, job: &str) -> Value {
    let start=Instant::now();
    loop {
        let value=store.job_status(job,65536).unwrap();
        if !matches!(value["status"].as_str(),Some("queued"|"running")) { return value; }
        assert!(start.elapsed()<Duration::from_secs(15),"job did not finish: {value}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn bundled_sqlite_contains_wal_reset_fix_and_database_is_sound() {
    let (_root,store)=fixture();
    assert!(rusqlite::version_number()>=3_051_003,"{}",rusqlite::version());
    assert_eq!(store.conn().unwrap().query_row("PRAGMA integrity_check",[],|r|r.get::<_,String>(0)).unwrap(),"ok");
    println!("bundled SQLite {}",rusqlite::version());
}

#[test]
fn duplicate_submission_executes_once_and_changed_request_conflicts() {
    let (_root,store)=fixture();
    let command=spec(&store,"printf x >> counter; sleep 0.1; printf finished","write");
    let first=launch(&store,&command,"once");
    let again=store.launch_job(worker(),&command,None,"once").unwrap();
    assert_eq!(again["job_id"],first); assert_eq!(again["deduplicated"],true);
    assert_eq!(finish(&store,&first)["command_ok"],true);
    assert_eq!(fs::read_to_string(store.workspace.join("counter")).unwrap(),"x");
    assert_eq!(store.launch_job(worker(),&spec(&store,"printf changed","write"),None,"once").unwrap_err().code(),"IDEMPOTENCY_CONFLICT");
}

#[test]
fn empty_stdin_is_closed_and_nonzero_exit_is_not_success() {
    let (_root,store)=fixture();
    let id=launch(&store,&spec(&store,"cat; printf eof; printf failure >&2; exit 7","read"),"stdin");
    let out=finish(&store,&id);
    assert_eq!(out["stdout"],"eof"); assert_eq!(out["stderr"],"failure");
    assert_eq!(out["exit_code"],7); assert_eq!(out["command_ok"],false);
}

#[test]
fn reopening_store_keeps_task_revision_job_identity_and_output() {
    let (root,store)=fixture();
    let task=store.task_open("recover me",Some("conversation"),false).unwrap();
    let checkpoint=json!({"task_id":task["task_id"],"request_id":"step-1","expected_revision":0,
        "checkpoint":{"next_step":"step-2","steps":{"step-1":{"state":"passed","evidence":["test receipt"]}}}});
    store.task_checkpoint(&checkpoint).unwrap();
    let id=launch(&store,&spec(&store,"sleep 0.15; printf persistent","read"),"persist");
    let reopened=Store::open(&root.path().join("state"),&store.workspace).unwrap();
    assert_eq!(finish(&reopened,&id)["stdout"],"persistent");
    let state=reopened.task_status(task["task_id"].as_str().unwrap()).unwrap();
    assert_eq!(state["revision"],1); assert_eq!(state["checkpoint"]["next_step"],"step-2");
    assert_eq!(reopened.job_output(&id,"stdout",0,100).unwrap()["content"],"persistent");
}

#[test]
fn producer_helper() {
    let Ok(root)=std::env::var("PERSONAL_TEST_PRODUCER_ROOT") else { return; };
    let root=PathBuf::from(root);
    let store=Store::open(&root.join("state"),&root.join("workspace")).unwrap();
    let id=launch(&store,&spec(&store,"printf ready > started; while [ ! -f release ]; do sleep 0.05; done; printf survived","write"),"producer");
    fs::write(root.join("job-id"),id).unwrap();
    // Returning exits this producer process without waiting for the detached worker.
}

#[test]
fn command_survives_its_submitting_process_exiting() {
    let (root,store)=fixture();
    let status=Command::new(std::env::current_exe().unwrap())
        .args(["--exact","producer_helper","--nocapture"])
        .env("PERSONAL_TEST_PRODUCER_ROOT",root.path()).status().unwrap();
    assert!(status.success());
    let id=fs::read_to_string(root.path().join("job-id")).unwrap();
    await_path(&store.workspace.join("started"));
    fs::write(store.workspace.join("release"),"go").unwrap();
    assert_eq!(finish(&store,&id)["stdout"],"survived");
}

#[test]
fn four_independent_jobs_overlap_and_source_writes_are_not_directory_owned() {
    let (_root,store)=fixture();
    let mut jobs=Vec::new();
    for n in 0..4 {
        // These are isolated fixture signal files, not edits to shared application source.
        jobs.push(launch(&store,&spec(&store,&format!("printf ready > started-{n}; while [ ! -f release ]; do sleep 0.05; done"),"read"),&format!("parallel-{n}")));
    }
    for n in 0..4 { await_path(&store.workspace.join(format!("started-{n}"))); }
    let source=locks::try_gate(&store.dir,"source",true).unwrap();
    assert!(source.is_some(),"read jobs must not occupy the source gate"); drop(source);
    fs::write(store.workspace.join("release"),"go").unwrap();
    for id in jobs { assert_eq!(finish(&store,&id)["command_ok"],true); }
}

#[test]
fn same_named_output_is_serialized_without_worktrees() {
    let (_root,store)=fixture();
    let mut first=spec(&store,"printf ready > first; while [ ! -f release ]; do sleep 0.05; done","read");
    first.resources=vec!["shared-output".into()];
    let id1=launch(&store,&first,"resource-1"); await_path(&store.workspace.join("first"));
    let mut second=spec(&store,"printf ready > second","read"); second.resources=first.resources.clone();
    let id2=launch(&store,&second,"resource-2");
    std::thread::sleep(Duration::from_millis(180));
    assert!(!store.workspace.join("second").exists());
    fs::write(store.workspace.join("release"),"go").unwrap();
    assert_eq!(finish(&store,&id1)["command_ok"],true);
    assert_eq!(finish(&store,&id2)["command_ok"],true);
}

#[test]
fn cancellation_and_timeout_stop_only_the_owned_command() {
    let (_root,store)=fixture();
    let mut slow=spec(&store,"printf ready > ready; sleep 20","write"); slow.timeout_ms=5000;
    let id=launch(&store,&slow,"cancel"); await_path(&store.workspace.join("ready"));
    store.cancel_job(&id).unwrap(); assert_eq!(finish(&store,&id)["status"],"cancelled");
    slow.stdin="x".repeat(900000); slow.args=vec!["-c".into(),"sleep 20".into()]; slow.timeout_ms=150;
    let id2=launch(&store,&slow,"timeout"); assert_eq!(finish(&store,&id2)["status"],"timeout");
    assert!(locks::try_gate(&store.dir,"source",true).unwrap().is_some());
}

#[test]
fn output_is_bounded_and_still_drained_to_completion() {
    let (_root,store)=fixture();
    let id=launch(&store,&spec(&store,"head -c 9000000 /dev/zero; printf tail-marker","read"),"output");
    assert_eq!(finish(&store,&id)["command_ok"],true);
    let log=store.dir.join("jobs").join(&id).join("stdout.log");
    assert_eq!(log.metadata().unwrap().len(),MAX_STREAM_BYTES as u64);
    assert!(store.job_status(&id,100).unwrap()["stdout"].as_str().unwrap().ends_with("tail-marker"));
    let meta:Value=serde_json::from_slice(&fs::read(log.parent().unwrap().join("output.json")).unwrap()).unwrap();
    assert_eq!(meta["stdout"]["truncated"],true);
}

#[test]
fn foreign_workspace_cannot_resolve_a_job_identifier() {
    let (_a,store)=fixture(); let (_b,other)=fixture();
    let id=launch(&store,&spec(&store,"printf owned","read"),"scope");
    assert_eq!(other.job_status(&id,20).unwrap_err().code(),"JOB_NOT_FOUND");
    assert_eq!(finish(&store,&id)["command_ok"],true);
    assert_ne!(digest(store.workspace.to_string_lossy().as_bytes()),digest(other.workspace.to_string_lossy().as_bytes()));
}

#[test]
fn historical_unknown_result_does_not_deadlock_independent_tasks_or_replay() {
    let (_root,store)=fixture();
    let old=uuid::Uuid::new_v4().to_string();
    let command=spec(&store,"printf old > must-not-replay","write");
    let hash=digest(serde_json::to_vec(&command).unwrap());
    // A deliberately inserted crash-state fixture; not evidence of a physical reboot.
    store.conn().unwrap().execute(
        "INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace','uncertain',?2,?3,'unknown',0,0)",
        (&old,hash,serde_json::to_string(&command).unwrap())).unwrap();
    let replay=store.launch_job(worker(),&command,None,"uncertain").unwrap();
    assert_eq!(replay["job_id"],old); assert_eq!(replay["status"],"unknown");
    assert!(!store.workspace.join("must-not-replay").exists());
    let fresh=launch(&store,&spec(&store,"printf independent","write"),"independent");
    assert_eq!(finish(&store,&fresh)["command_ok"],true);
    assert_eq!(store.job_status(&old,10).unwrap()["status"],"unknown");
}

#[test]
fn killed_test_worker_is_unknown_and_its_foreground_child_keeps_resource_lock() {
    struct Release(PathBuf);
    impl Drop for Release { fn drop(&mut self) { let _=fs::write(&self.0,"release"); } }
    let (_root,store)=fixture();
    let release=Release(store.workspace.join("release"));
    let command=spec(&store,"printf ready > started; while [ ! -f release ]; do sleep 0.05; done","write");
    let id=launch(&store,&command,"owned-crash-fixture");
    await_path(&store.workspace.join("started"));
    let metadata:Value=serde_json::from_slice(&fs::read(store.dir.join("jobs").join(&id).join("worker.json")).unwrap()).unwrap();
    assert_eq!(metadata["job_id"],id);
    let pid=metadata["worker_pid"].as_u64().unwrap() as i32;
    assert!(pid>1 && pid!=std::process::id() as i32);
    // This PID was created by this test's private Store and is bound to its UUID.
    assert_eq!(unsafe { libc::kill(pid,libc::SIGKILL) },0);
    let start=Instant::now();
    while store.job_status(&id,10).unwrap()["status"]!="unknown" {
        assert!(start.elapsed()<Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20));
    }
    assert!(locks::try_gate(&store.dir,"source",true).unwrap().is_none());
    drop(release);
    let start=Instant::now();
    loop {
        if locks::try_gate(&store.dir,"source",true).unwrap().is_some() { break; }
        assert!(start.elapsed()<Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(store.job_status(&id,10).unwrap()["command_ok"],Value::Null);
}


#[test]
fn published_terminal_results_have_already_released_source_and_resources() {
    let (_root,store)=fixture();
    for n in 0..16 {
        let mut command=spec(&store,"printf complete","write");
        command.resources=vec!["terminal-receipt-fixture".into()];
        let id=launch(&store,&command,&format!("terminal-{n}"));
        let result=store.wait_job(&id,Duration::from_secs(5),100).unwrap();
        assert_eq!(result["status"],"exited");assert_eq!(result["command_ok"],true);
        assert!(locks::try_gate(&store.dir,"source",true).unwrap().is_some(),"terminal source gate {n}");
        assert!(locks::try_gate(&store.dir,"resource:terminal-receipt-fixture",true).unwrap().is_some(),"terminal resource gate {n}");
    }
}
