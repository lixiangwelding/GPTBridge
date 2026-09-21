//! Synthetic isolated load fixture; timings are observations, not CI gates.
use coding_tools_personal_runtime::Store;
use serde_json::json;
use std::{fs, sync::{Arc, Barrier}, time::Instant};
#[test]
fn benchmark_multi_conversation_job_list() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");fs::create_dir(&workspace).unwrap();
    let store = Store::open(&temp.path().join("state"), &workspace).unwrap();
    let mut connection = store.conn().unwrap();let tx = connection.transaction().unwrap();
    for n in 0..50 {
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,exit_code,created,updated) VALUES(?1,'workspace',?2,'fixture','{}','exited',0,?3,?3)",
            (&id, format!("fixture-{n}"), n)).unwrap();
        let dir = store.dir.join("jobs").join(id);fs::create_dir(&dir).unwrap();
        for name in ["stdout", "stderr"] {fs::write(dir.join(format!("{name}.log")), "x".repeat(65536)).unwrap();}
    }
    tx.commit().unwrap();
    let barrier = Arc::new(Barrier::new(8));let start = Instant::now();
    let threads: Vec<_> = (0..8).map(|_| {
        let store = store.clone();let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            (0..10).map(|_| {
                let start = Instant::now();let value = store.job_list(None).unwrap();
                assert_eq!(value["jobs"].as_array().unwrap().len(), 50);
                start.elapsed().as_secs_f64() * 1000.0
            }).collect::<Vec<_>>()
        })
    }).collect();
    let mut latency: Vec<_> = threads.into_iter().flat_map(|t| t.join().unwrap()).collect();latency.sort_by(f64::total_cmp);
    println!("PERF_JOB_LIST {}", json!({"conversations":8,"requests":latency.len(),"jobs_per_request":50,
        "wall_ms":start.elapsed().as_secs_f64()*1000.0,"p50_ms":latency[40],"p95_ms":latency[76]}));
}


fn running_fixture() -> (tempfile::TempDir,Store,String) {
    let temp=tempfile::tempdir().unwrap();
    let workspace=temp.path().join("workspace");fs::create_dir(&workspace).unwrap();
    let store=Store::open(&temp.path().join("state"),&workspace).unwrap();
    let job=uuid::Uuid::new_v4().to_string();
    store.conn().unwrap().execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace','fixture','fixture','{}','running',0,?2)",
        (&job,coding_tools_personal_runtime::now_ms())).unwrap();
    let dir=store.dir.join("jobs").join(&job);fs::create_dir(&dir).unwrap();
    fs::write(dir.join("stdout.log"),"real-output-tail").unwrap();fs::write(dir.join("stderr.log"),"").unwrap();
    (temp,store,job)
}

#[test]
fn metadata_list_does_not_claim_it_read_output() {
    let (_temp,store,job)=running_fixture();
    store.finish_job(&job,"exited",Some(0),"fixture completed").unwrap();
    let list=store.job_list(None).unwrap();let row=&list["jobs"][0];
    assert_eq!(row["job_id"],job);assert_eq!(row["command_ok"],true);
    assert_eq!(row["output_loaded"],false);
    assert!(row.get("stdout").is_none());assert!(row.get("stderr").is_none());
    assert!(row["stdout_truncated"].is_null());assert!(row["stderr_truncated"].is_null());
    let output=store.job_status(&job,4).unwrap();
    assert_eq!(output["output_loaded"],true);assert_eq!(output["stdout"],"tail");
    assert_eq!(output["stdout_truncated"],true);assert_eq!(output["stderr_truncated"],false);
}

#[test]
fn waiting_connection_observes_external_commit_without_holding_a_snapshot() {
    let (_temp,store,job)=running_fixture();
    let _alive=coding_tools_personal_runtime::locks::try_gate(&store.dir,&format!("alive:{job}"),true).unwrap().unwrap();
    let other=store.clone();let id=job.clone();
    let writer=std::thread::spawn(move|| {
        std::thread::sleep(std::time::Duration::from_millis(60));
        other.finish_job(&id,"exited",Some(7),"real nonzero outcome").unwrap();
    });
    let output=store.wait_job(&job,std::time::Duration::from_secs(2),4).unwrap();writer.join().unwrap();
    assert_eq!(output["status"],"exited");assert_eq!(output["exit_code"],7);
    assert_eq!(output["command_ok"],false);assert_eq!(output["stdout"],"tail");
    assert_eq!(store.conn().unwrap().query_row("PRAGMA synchronous",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    assert_eq!(store.conn().unwrap().query_row("PRAGMA integrity_check",[],|r|r.get::<_,String>(0)).unwrap(),"ok");
}

#[test]
fn zero_wait_and_missing_job_keep_the_existing_contract() {
    let (_temp,store,job)=running_fixture();
    let _alive=coding_tools_personal_runtime::locks::try_gate(&store.dir,&format!("alive:{job}"),true).unwrap().unwrap();
    let output=store.wait_job(&job,std::time::Duration::ZERO,32).unwrap();
    assert_eq!(output["status"],"running");assert_eq!(output["command_ok"],serde_json::Value::Null);
    assert_eq!(output["stdout"],"real-output-tail");
    assert_eq!(store.wait_job(&uuid::Uuid::new_v4().to_string(),std::time::Duration::ZERO,32).unwrap_err().code(),"JOB_NOT_FOUND");
}

#[test]
fn lost_worker_metadata_has_the_persisted_unknown_timestamp() {
    let (_temp,store,job)=running_fixture();
    let value=store.wait_job(&job,std::time::Duration::ZERO,32).unwrap();
    assert_eq!(value["status"],"unknown");assert_eq!(value["safe_to_replay"],false);
    let persisted=store.conn().unwrap().query_row("SELECT updated FROM jobs WHERE id=?1",[&job],|r|r.get::<_,i64>(0)).unwrap();
    assert_eq!(value["updated"],persisted);
}
