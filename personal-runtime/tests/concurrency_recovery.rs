#![cfg(unix)]
//! Crash-state fixtures and real workers, confined to a private temporary workspace.
use std::{fs, path::{Path, PathBuf}, time::Duration};
use coding_tools_personal_runtime::{Store, digest, now_ms, jobs::{JobSpec, MAX_QUEUED}, locks};
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Store, JobSpec) {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let store = Store::open(&root.path().join("state"), &workspace).unwrap();
    let spec = JobSpec { program: PathBuf::from("/bin/sh"), args: vec!["-c".into(), "printf x >> counter; printf done".into()],
        cwd: workspace.clone(), workspace, stdin: String::new(), mode: "write".into(), resources: vec![], timeout_ms: 5000 };
    (root, store, spec)
}
fn worker() -> &'static Path { Path::new(env!("CARGO_BIN_EXE_coding-tools-personal-worker")) }

fn seed(store: &Store, spec: &JobSpec, n: usize, state: &str, updated: i64) -> Vec<String> {
    let c = store.conn().unwrap();
    (0..n).map(|i| {
        let id = uuid::Uuid::new_v4().to_string();
        c.execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace',?2,?3,?4,?5,?6,?6)",
            (&id, format!("old-{i}"), digest(serde_json::to_vec(spec).unwrap()), serde_json::to_string(spec).unwrap(), state, updated)).unwrap();
        id
    }).collect()
}

#[test]
fn dead_workers_do_not_permanently_fill_admission_queue() {
    let (_root, store, spec) = fixture();
    let stale = seed(&store, &spec, MAX_QUEUED, "queued", now_ms()-60_000);
    let result = store.launch_job(worker(), &spec, None, "fresh-after-crash");
    assert!(result.is_ok(), "stale workers must be reconciled before rejecting admission: {result:?}");
    let launched = result.unwrap();
    let finished = store.wait_job(launched["job_id"].as_str().unwrap(), Duration::from_secs(10), 100).unwrap();
    assert_eq!(finished["command_ok"], true);
    for id in stale {
        let state = store.job_status(&id, 1).unwrap();
        assert_eq!(state["status"], "unknown");
        assert_eq!(state["command_ok"], serde_json::Value::Null);
    }
    assert_eq!(fs::read_to_string(store.workspace.join("counter")).unwrap(), "x", "crashed intents must never replay");
}

#[test]
fn completed_task_can_retrieve_original_receipt_but_cannot_submit_new_work() {
    let (_root, store, spec) = fixture();
    let task = store.open_task_request(&json!({"goal":"completed receipt fixture", "request_id":"open"})).unwrap();
    let id = task["task_id"].as_str().unwrap();
    let launched = store.launch_job(worker(), &spec, Some(id), "once").unwrap();
    assert_eq!(store.wait_job(launched["job_id"].as_str().unwrap(), Duration::from_secs(10), 100).unwrap()["command_ok"], true);
    store.task_checkpoint(&json!({"task_id":id,"expected_revision":0,"request_id":"complete","state":"completed",
        "checkpoint":{"steps":{"execution":{"state":"passed","evidence":["private worker exit and counter"]}}}})).unwrap();
    let recovered = store.launch_job(worker(), &spec, Some(id), "once");
    assert!(recovered.is_ok(), "completion must not block fetching a prior idempotent receipt: {recovered:?}");
    let recovered = recovered.unwrap();
    assert_eq!(recovered["job_id"], launched["job_id"]);
    assert_eq!(recovered["deduplicated"], true);
    assert_eq!(store.launch_job(worker(), &spec, Some(id), "new-work").unwrap_err().code(), "TASK_COMPLETED");
    assert_eq!(fs::read_to_string(store.workspace.join("counter")).unwrap(), "x");
}

#[test]
fn living_workers_keep_the_queue_budget_even_with_old_heartbeats() {
    let (_root, store, spec) = fixture();
    let ids = seed(&store, &spec, MAX_QUEUED, "running", now_ms()-60_000);
    let _alive: Vec<_> = ids.iter().map(|id| locks::try_gate(&store.dir, &format!("alive:{id}"), true).unwrap().unwrap()).collect();
    assert_eq!(store.launch_job(worker(), &spec, None, "over-capacity").unwrap_err().code(), "QUEUE_FULL");
    assert_eq!(store.conn().unwrap().query_row("SELECT count(*) FROM jobs WHERE state='running'", [], |r|r.get::<_,i64>(0)).unwrap(), MAX_QUEUED as i64);
    assert!(!store.workspace.join("counter").exists());
}

#[test]
fn newly_queued_workers_get_their_startup_grace_period() {
    let (_root, store, spec) = fixture();
    seed(&store, &spec, MAX_QUEUED, "queued", now_ms());
    assert_eq!(store.launch_job(worker(), &spec, None, "startup-race").unwrap_err().code(), "QUEUE_FULL");
    assert!(!store.workspace.join("counter").exists());
}

#[test]
fn pressure_is_bounded_private_and_does_not_reconcile_a_snapshot() {
    let (_root, store, mut spec) = fixture();
    spec.stdin = "PRIVATE-INPUT-MUST-NOT-APPEAR".into();
    seed(&store, &spec, MAX_QUEUED, "queued", now_ms()-60_000);
    let pressure = store.runtime_pressure().unwrap();
    assert_eq!(pressure["queued"], MAX_QUEUED);
    assert_eq!(pressure["admission_remaining"], 0);
    assert_eq!(pressure["sample"].as_array().unwrap().len(), 8);
    assert_eq!(pressure["sample_truncated"], true);
    assert_eq!(pressure["counts_are_lower_bounds"], false);
    assert_eq!(pressure["state_modified"], false);
    assert!(!pressure.to_string().contains("PRIVATE-INPUT"));
    assert!(!pressure.to_string().contains("counter"));
    assert_eq!(store.conn().unwrap().query_row("SELECT count(*) FROM jobs WHERE state='queued'", [], |r|r.get::<_,i64>(0)).unwrap(), MAX_QUEUED as i64);
    let (_other_root, other, _) = fixture();
    assert_eq!(other.runtime_pressure().unwrap()["active"], 0);
}

#[test]
fn queued_source_wait_is_visible_and_cancellation_never_starts_command() {
    let (_root, store, spec) = fixture();
    let source = locks::try_gate(&store.dir, "source", true).unwrap().unwrap();
    let result = store.launch_job(worker(), &spec, None, "blocked-source").unwrap();
    let id = result["job_id"].as_str().unwrap();
    let start = std::time::Instant::now();
    loop {
        let view = store.job_status(id, 100).unwrap();
        if view["waiting_for"] == "source" {
            assert_eq!(view["status"], "queued");
            assert!(view["queued_for_ms"].is_number());
            assert_eq!(view["timing"]["queue_wait_ms"], serde_json::Value::Null);
            break;
        }
        assert!(start.elapsed()<Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    store.cancel_job(id).unwrap();
    assert_eq!(store.wait_job(id, Duration::from_secs(3), 100).unwrap()["status"], "cancelled");
    assert!(!store.workspace.join("counter").exists());
    drop(source);
}

#[test]
fn command_and_heavy_capacity_waits_release_partial_source_reservations() {
    for (name, count, mode, reason) in [("commands",8,"write","command_capacity"),("heavy",2,"build","heavy_capacity")] {
        let (_root, store, mut spec) = fixture();
        spec.mode = mode.into();
        let _slots: Vec<_> = (0..count).map(|_| locks::slot(&store.dir,name,count).unwrap().unwrap()).collect();
        let result = store.launch_job(worker(), &spec, None, "capacity-wait").unwrap();
        let id = result["job_id"].as_str().unwrap();
        let start = std::time::Instant::now();
        loop {
            if store.job_status(id, 100).unwrap()["waiting_for"] == reason {break;}
            assert!(start.elapsed()<Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(20));
        }
        // The worker may briefly re-acquire source while retrying capacity.
        // Assert bounded release rather than racing a single observation.
        let release_start=std::time::Instant::now();
        while locks::try_gate(&store.dir,"source",true).unwrap().is_none() {
            assert!(release_start.elapsed()<Duration::from_secs(2),"partial reservation stayed held");
            std::thread::sleep(Duration::from_millis(5));
        }
        store.cancel_job(id).unwrap();
        assert_eq!(store.wait_job(id, Duration::from_secs(3), 100).unwrap()["status"], "cancelled");
        assert!(locks::try_gate(&store.dir,"source",true).unwrap().is_some());
        assert!(!store.workspace.join("counter").exists());
    }
}

#[test]
fn terminal_timings_and_oversized_metadata_are_not_fabricated() {
    let (_root, store, spec) = fixture();
    let launched = store.launch_job(worker(), &spec, None, "timing").unwrap();
    let id = launched["job_id"].as_str().unwrap();
    let finished = store.wait_job(id, Duration::from_secs(10), 100).unwrap();
    assert!(finished["timing"]["queue_wait_ms"].is_number());
    assert!(finished["timing"]["worker_duration_ms"].is_number());
    assert_eq!(finished["heartbeat_age_ms"], serde_json::Value::Null);
    assert_eq!(finished["stdout"], "done");
    fs::write(store.dir.join("jobs").join(id).join("output.json"), " ".repeat(20_000)).unwrap();
    fs::write(store.dir.join("jobs").join(id).join("child.json"), "broken").unwrap();
    let degraded = store.job_status(id, 100).unwrap();
    assert_eq!(degraded["command_ok"], true);
    assert_eq!(degraded["stdout"], "done");
    assert_eq!(degraded["timing"]["queue_wait_ms"], serde_json::Value::Null);
    assert_eq!(degraded["timing"]["worker_duration_ms"], serde_json::Value::Null);
}

#[test]
fn twenty_four_contenders_finish_once_with_eight_execution_slots() {
    let (_root, store, base) = fixture();
    let mut ids = Vec::new();
    for n in 0..24 {
        let mut spec = base.clone();
        // Read mode writes only test-owned signal files, never application source.
        spec.mode = "read".into();
        spec.args = vec!["-c".into(),format!("printf x >> once-{n}; while [ ! -f release ]; do sleep 0.05; done; printf done")];
        ids.push(store.launch_job(worker(), &spec, None, &format!("contender-{n}")).unwrap()["job_id"].as_str().unwrap().to_string());
    }
    let start = std::time::Instant::now();
    loop {
        let pressure = store.runtime_pressure().unwrap();
        let running = pressure["running"].as_u64().unwrap();
        assert!(running<=8);
        if running==8 {break;}
        assert!(start.elapsed()<Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    fs::write(store.workspace.join("release"), "go").unwrap();
    for (n,id) in ids.iter().enumerate() {
        assert_eq!(store.wait_job(id, Duration::from_secs(10), 100).unwrap()["command_ok"], true);
        assert_eq!(fs::read_to_string(store.workspace.join(format!("once-{n}"))).unwrap(), "x");
    }
    assert_eq!(store.runtime_pressure().unwrap()["active"], 0);
}
