#![cfg(unix)]
use std::{fs,path::{Path,PathBuf},time::{Duration,Instant}};
use coding_tools_personal_runtime::{Store,jobs::JobSpec,limits::{ConcurrencyLimits,CONFIG_FILE},locks,now_ms};
use serde_json::json;
fn fixture()->(tempfile::TempDir,Store,JobSpec) {
    let root=tempfile::tempdir().unwrap();let workspace=root.path().join("workspace");fs::create_dir(&workspace).unwrap();
    let initial=Store::open(&root.path().join("state"),&workspace).unwrap();
    fs::write(initial.dir.join(CONFIG_FILE),serde_json::to_vec(&ConcurrencyLimits::performance()).unwrap()).unwrap();
    let store=Store::at(initial.dir,workspace.clone()).unwrap();
    let spec=JobSpec{program:PathBuf::from("/bin/sh"),args:vec![],cwd:workspace.clone(),workspace,
        stdin:String::new(),mode:"read".into(),resources:vec![],timeout_ms:30_000};
    (root,store,spec)
}
fn worker()->&'static Path {Path::new(env!("CARGO_BIN_EXE_coding-tools-personal-worker"))}
struct Release(PathBuf);
impl Drop for Release {fn drop(&mut self){let _=fs::write(&self.0,b"go");}}
fn contend(count:usize,mode:&str,expected:usize) {
    let (_root,store,base)=fixture();let release=Release(store.workspace.join("release"));
    let start=Instant::now();let mut ids=Vec::new();
    for n in 0..count {
        let mut spec=base.clone();spec.mode=mode.into();spec.resources=vec![format!("isolated-output:{n}")];
        spec.args=vec!["-c".into(),format!("printf x >> once-{n}; while [ ! -f release ]; do sleep 0.2; done; printf done")];
        ids.push(store.launch_job(worker(),&spec,None,&format!("{mode}-{n}")).unwrap()["job_id"].as_str().unwrap().to_string());
    }
    loop {
        let pressure=store.runtime_pressure().unwrap();let running=pressure["running"].as_u64().unwrap() as usize;
        assert!(running<=expected,"over-admission: {pressure}");
        if running==expected {break;}
        assert!(start.elapsed()<Duration::from_secs(20),"{expected} workers must start: {pressure}");
        std::thread::sleep(Duration::from_millis(20));
    }
    let reached_ms=start.elapsed().as_millis();drop(release);
    for (n,id) in ids.iter().enumerate() {
        let result=store.wait_job(id,Duration::from_secs(20),100).unwrap();
        assert_eq!(result["command_ok"],true,"{result}");assert_eq!(result["limits"]["running"],64);
        assert_eq!(fs::read_to_string(store.workspace.join(format!("once-{n}"))).unwrap(),"x");
    }
    assert_eq!(store.runtime_pressure().unwrap()["active"],0);
    println!("{}",json!({"test":"real-worker-contention","mode":mode,"submitted":count,
        "observed_simultaneously_running":expected,"reached_ms":reached_ms,"total_ms":start.elapsed().as_millis(),"duplicates":0}));
}
#[test]
fn ninety_six_jobs_reach_sixty_four_simultaneous_commands_without_replay(){contend(96,"read",64);}
#[test]
fn eight_independent_build_jobs_run_four_at_a_time(){contend(8,"build",4);}
#[test]
fn two_hundred_fifty_six_live_records_exhaust_admission_without_being_reaped() {
    coding_tools_personal_runtime::limits::raise_file_capacity(4096);
    let (_root,store,mut spec)=fixture();spec.args=vec!["-c".into(),"printf unexpected > new-command".into()];
    let mut c=store.conn().unwrap();let tx=c.transaction().unwrap();let mut guards=Vec::new();
    for n in 0..256 {
        let id=uuid::Uuid::new_v4().to_string();
        guards.push(locks::try_gate(&store.dir,&format!("alive:{id}"),true).unwrap().unwrap());
        tx.execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace',?2,'fixture','{}','queued',?3,?3)",
            (&id,format!("seed-{n}"),now_ms()-60_000)).unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(store.launch_job(worker(),&spec,None,"overflow").unwrap_err().code(),"QUEUE_FULL");
    let pressure=store.runtime_pressure().unwrap();assert_eq!(pressure["active"],256);assert_eq!(pressure["admission_remaining"],0);
    assert_eq!(pressure["sample"].as_array().unwrap().len(),8);assert_eq!(pressure["counts_are_lower_bounds"],false);
    assert!(!store.workspace.join("new-command").exists());
    assert_eq!(c.query_row("SELECT count(*) FROM jobs WHERE state='unknown'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
#[test]
fn config_drift_blocks_new_work_but_preserves_original_receipts() {
    let (_root,store,mut spec)=fixture();spec.args=vec!["-c".into(),"printf x >> counter".into()];
    let first=store.launch_job(worker(),&spec,None,"once").unwrap();
    assert_eq!(store.wait_job(first["job_id"].as_str().unwrap(),Duration::from_secs(10),100).unwrap()["command_ok"],true);
    fs::write(store.dir.join(CONFIG_FILE),serde_json::to_vec(&ConcurrencyLimits::default()).unwrap()).unwrap();
    let old=store.launch_job(worker(),&spec,None,"once").unwrap();assert_eq!(old["job_id"],first["job_id"]);assert_eq!(old["deduplicated"],true);
    assert_eq!(store.launch_job(worker(),&spec,None,"new").unwrap_err().code(),"CONCURRENCY_RESTART_REQUIRED");
    assert_eq!(fs::read_to_string(store.workspace.join("counter")).unwrap(),"x");
}
