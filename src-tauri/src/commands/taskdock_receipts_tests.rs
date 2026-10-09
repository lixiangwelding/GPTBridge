use super::*;
use crate::audit::{AuditRequestContext, AuditStore};
use coding_tools_personal_runtime::Store;
use std::fs;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    store: Store,
    audit: AuditStore,
    audit_path: PathBuf,
    task: String,
    other: String,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let store = Store::open(&temp.path().join("runtime"), &root).unwrap();
    let task = store.task_open("receipt projection", None, true).unwrap()["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let other = store.task_open("other task", None, true).unwrap()["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let audit_path = temp.path().join("audit/audit.sqlite");
    let audit = AuditStore::open(audit_path.clone()).unwrap();
    let mut config = audit.config().unwrap();
    config.tool_audit_retention_days = 0;
    audit.set_config(config).unwrap();
    Fixture {
        _temp: temp,
        root,
        store,
        audit,
        audit_path,
        task,
        other,
    }
}
fn audit(f: &Fixture, workspace: &str, tool: &str, args: Value, out: Value, start: i64) -> String {
    f.audit
        .record_tool_call(
            &AuditRequestContext {
                transport: "mcp".into(),
                request_id: Some("rpc-id".into()),
                ..Default::default()
            },
            workspace,
            f.root.to_str().unwrap(),
            tool,
            &args,
            &out,
            start,
            start + 9,
        )
        .unwrap()
        .unwrap()
}
fn page(
    f: &Fixture,
    workspace: &str,
    task: &str,
    cursor: Option<&ReceiptCursor>,
    limit: usize,
) -> Value {
    project(
        &f.root,
        &f.store.dir,
        &f.audit_path,
        workspace,
        task,
        cursor,
        limit,
    )
    .unwrap()
}
#[test]
fn filters_original_workspace_and_task_and_redacts_raw_details() {
    let f = fixture();
    let hash = "a".repeat(64);
    let after = "b".repeat(64);
    audit(
        &f,
        "selected",
        "apply_patch",
        json!({"task_id":f.task,"request_id":"p1","cmd":"PRIVATE_COMMAND","patch":"PRIVATE_PATCH"}),
        json!({"ok":true,"task_id":f.task,"before_hashes":{"a.txt":hash,"new.txt":null},"after_hashes":{"a.txt":after,"new.txt":"c".repeat(64)}}),
        100,
    );
    audit(
        &f,
        "foreign",
        "apply_patch",
        json!({"task_id":f.task}),
        json!({"ok":true,"task_id":f.task}),
        200,
    );
    audit(
        &f,
        "selected",
        "apply_patch",
        json!({"task_id":f.other}),
        json!({"ok":true,"task_id":f.other}),
        300,
    );
    let view = page(&f, "selected", &f.task, None, 20);
    let rows = view["items"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["workspace_id"], "selected");
    assert_eq!(rows[0]["started_at_ms"], 100);
    assert_eq!(rows[0]["finished_at_ms"], 109);
    assert_eq!(rows[0]["patch_hashes"][0]["before_sha256"], "a".repeat(64));
    assert_eq!(rows[0]["patch_hashes"][1]["before_exists"], false);
    assert!(!view.to_string().contains("PRIVATE"));
    assert!(rows[0].get("input_json").is_none());
}
#[test]
fn cursor_pages_equal_timestamps_without_duplicates() {
    let f = fixture();
    let mut ids = BTreeSet::new();
    for _ in 0..3 {
        ids.insert(audit(
            &f,
            "selected",
            "task_status",
            json!({"task_id":f.task}),
            json!({"ok":true,"task_id":f.task}),
            100,
        ));
    }
    let mut cursor = None;
    let mut actual = BTreeSet::new();
    for _ in 0..3 {
        let p = page(&f, "selected", &f.task, cursor.as_ref(), 1);
        assert_eq!(p["items"].as_array().unwrap().len(), 1);
        assert!(actual.insert(p["items"][0]["id"].as_str().unwrap().to_string()));
        cursor = serde_json::from_value(p["next_cursor"].clone()).ok();
    }
    assert_eq!(actual, ids);
    assert!(cursor.is_none());
}
#[test]
fn scan_limit_advances_even_when_latest_records_belong_to_another_task() {
    let f = fixture();
    audit(
        &f,
        "selected",
        "task_status",
        json!({"task_id":f.task}),
        json!({"ok":true,"task_id":f.task}),
        10,
    );
    for start in 100..=200 {
        audit(
            &f,
            "selected",
            "task_status",
            json!({"task_id":f.other}),
            json!({"ok":true,"task_id":f.other}),
            start,
        );
    }
    let first = page(&f, "selected", &f.task, None, 20);
    assert!(first["items"].as_array().unwrap().is_empty());
    let cursor: ReceiptCursor = serde_json::from_value(first["next_cursor"].clone()).unwrap();
    let second = page(&f, "selected", &f.task, Some(&cursor), 20);
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["started_at_ms"], 10);
    assert!(second["next_cursor"].is_null());
}
#[test]
fn durable_legacy_receipt_has_hashes_but_never_invents_profile_or_start() {
    let f = fixture();
    let scope = format!("patch:{}", f.task);
    f.store
        .begin_receipt(&scope, "saved", "input-hash")
        .unwrap();
    let result = json!({"ok":true,"task_id":f.task,"request_id":"saved","before_hashes":{"old.txt":"a".repeat(64)},"after_hashes":{"old.txt":null}});
    f.store.finish_receipt(&scope, "saved", &result).unwrap();
    f.store.event(&f.task, "patch_finished", &result).unwrap();
    let p = page(&f, "selected", &f.task, None, 20);
    let r = &p["items"][0];
    assert_eq!(r["source"], "durable_receipt");
    assert!(r["workspace_id"].is_null());
    assert!(r["started_at_ms"].is_null());
    assert!(r["finished_at_ms"].is_number());
    assert_eq!(r["patch_hashes"][0]["after_exists"], false);
    assert_eq!(r["timing_source"], "task_event_end_only");
}
#[test]
fn truncated_output_uses_authoritative_receipt_and_metadata_only_stays_unbound() {
    let f = fixture();
    let scope = format!("patch:{}", f.task);
    let args = json!({"task_id":f.task,"request_id":"saved","patch":"PRIVATE_PATCH"});
    f.store
        .begin_receipt(&scope, "saved", &digest(args.to_string()))
        .unwrap();
    let result = json!({"ok":true,"task_id":f.task,"request_id":"saved","before_hashes":{"a.txt":null},"after_hashes":{"a.txt":"a".repeat(64)}});
    f.store.finish_receipt(&scope, "saved", &result).unwrap();
    let mut config = f.audit.config().unwrap();
    config.detail_limit_bytes = 200;
    f.audit.set_config(config.clone()).unwrap();
    audit(
        &f,
        "selected",
        "apply_patch",
        args,
        json!({"ok":true,"task_id":f.task,"secret":"PRIVATE".repeat(100)}),
        100,
    );
    config.detail_limit_bytes = -1;
    f.audit.set_config(config).unwrap();
    audit(
        &f,
        "selected",
        "read_file",
        json!({"task_id":f.task}),
        json!({"ok":true}),
        200,
    );
    let p = page(&f, "selected", &f.task, None, 20);
    assert_eq!(p["items"].as_array().unwrap().len(), 1);
    assert_eq!(p["items"][0]["details_state"], "truncated");
    assert_eq!(
        p["items"][0]["patch_hashes"][0]["after_sha256"],
        "a".repeat(64)
    );
    assert_eq!(p["partial"], true);
    assert!(!p.to_string().contains("PRIVATE"));
}
#[test]
fn rejected_reuse_of_request_id_never_borrows_previous_patch_hashes() {
    let f = fixture();
    let scope = format!("patch:{}", f.task);
    let original = json!({"task_id":f.task,"request_id":"reused","patch":"first patch"});
    let receipt = json!({"ok":true,"task_id":f.task,"request_id":"reused","change_id":"original-change",
        "before_hashes":{"a.txt":"a".repeat(64)},"after_hashes":{"a.txt":"b".repeat(64)}});
    f.store
        .begin_receipt(&scope, "reused", &digest(original.to_string()))
        .unwrap();
    f.store.finish_receipt(&scope, "reused", &receipt).unwrap();
    audit(
        &f,
        "selected",
        "apply_patch",
        json!({"task_id":f.task,"request_id":"reused","patch":"different patch"}),
        json!({"ok":false,"error":{"code":"IDEMPOTENCY_CONFLICT","category":"conflict"}}),
        100,
    );
    let page = page(&f, "selected", &f.task, None, 20);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let rejected = items.iter().find(|v| v["source"] == "audit").unwrap();
    assert_eq!(rejected["status"], "failure");
    assert!(rejected["patch_hashes"].as_array().unwrap().is_empty());
    let durable = items
        .iter()
        .find(|v| v["source"] == "durable_receipt")
        .unwrap();
    assert_eq!(durable["patch_hashes"][0]["after_sha256"], "b".repeat(64));
    let mismatched = json!({"ok":true,"change_id":"another-change"});
    assert!(durable_result(
        &f.store.conn().unwrap(),
        &f.task,
        "reused",
        &original,
        Some(&mismatched)
    )
    .is_none());
}
#[test]
fn successful_preview_with_reused_request_does_not_hide_applied_receipt() {
    let f = fixture();
    let scope = format!("patch:{}", f.task);
    let original = json!({"task_id":f.task,"request_id":"preview-reused","patch":"applied patch"});
    let receipt = json!({"ok":true,"task_id":f.task,"request_id":"preview-reused",
        "before_hashes":{"a.txt":null},"after_hashes":{"a.txt":"a".repeat(64)}});
    f.store
        .begin_receipt(&scope, "preview-reused", &digest(original.to_string()))
        .unwrap();
    f.store
        .finish_receipt(&scope, "preview-reused", &receipt)
        .unwrap();
    audit(
        &f,
        "selected",
        "apply_patch",
        json!({"task_id":f.task,"request_id":"preview-reused","patch":"new preview","dry_run":true}),
        json!({"ok":true,"dry_run":true,"expected_hashes":{"a.txt":"a".repeat(64)}}),
        100,
    );
    let p = page(&f, "selected", &f.task, None, 20);
    let rows = p["items"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter().find(|v| v["source"] == "audit").unwrap()["patch_hashes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        rows.iter()
            .find(|v| v["source"] == "durable_receipt")
            .unwrap()["patch_hashes"][0]["after_sha256"],
        "a".repeat(64)
    );
}
#[test]
fn poll_is_bound_by_persisted_job_and_reading_never_reconciles_running_state() {
    let f = fixture();
    let job = uuid::Uuid::new_v4().to_string();
    let foreign = uuid::Uuid::new_v4().to_string();
    let conn = f.store.conn().unwrap();
    for (id, task) in [(&job, &f.task), (&foreign, &f.other)] {
        conn.execute("INSERT INTO jobs(id,task_id,scope,request_id,input_hash,spec,state,created,updated,detail) VALUES(?1,?2,'exec',?1,'hash','{}','running',1,2,'old heartbeat')",params![id,task]).unwrap();
    }
    let child = f.store.dir.join("jobs").join(&job);
    fs::create_dir(&child).unwrap();
    fs::write(
        child.join("child.json"),
        json!({"job_id":job,"started":3}).to_string(),
    )
    .unwrap();
    audit(
        &f,
        "selected",
        "write_stdin",
        json!({"session_id":format!("job-{job}")}),
        json!({"ok":true,"job_id":job,"task_id":f.task}),
        100,
    );
    audit(
        &f,
        "selected",
        "write_stdin",
        json!({"session_id":format!("job-{foreign}"),"task_id":f.task}),
        json!({"ok":true,"job_id":foreign}),
        200,
    );
    let p = page(&f, "selected", &f.task, None, 20);
    assert_eq!(p["items"].as_array().unwrap().len(), 1);
    assert_eq!(p["items"][0]["job_started_at_ms"], 3);
    assert!(p["items"][0]["job_finished_at_ms"].is_null());
    let state: String = conn
        .query_row("SELECT state FROM jobs WHERE id=?1", [&job], |r| r.get(0))
        .unwrap();
    assert_eq!(state, "running");
}
#[test]
fn terminal_job_without_child_never_claims_command_process_finished() {
    let f = fixture();
    let conn = f.store.conn().unwrap();
    for state in ["queue_timeout", "spawn_failed", "cancelled", "exited"] {
        let job = uuid::Uuid::new_v4().to_string();
        conn.execute("INSERT INTO jobs(id,task_id,scope,request_id,input_hash,spec,state,created,updated,detail) VALUES(?1,?2,'exec',?1,'hash','{}',?3,1,10,'terminal')",params![job,f.task,state]).unwrap();
        let (started, finished, _) = job_timing(&conn, &f.store.dir, &f.task, Some(&job)).unwrap();
        assert!(started.is_none());
        assert!(finished.is_none());
        if state == "exited" {
            let child = f.store.dir.join("jobs").join(&job);
            fs::create_dir(&child).unwrap();
            fs::write(
                child.join("child.json"),
                json!({"job_id":job,"started":3}).to_string(),
            )
            .unwrap();
            let (started, finished, _) =
                job_timing(&conn, &f.store.dir, &f.task, Some(&job)).unwrap();
            assert_eq!(started, Some(3));
            assert_eq!(finished, Some(10));
        }
    }
}
#[test]
fn missing_store_is_not_created_and_wrong_task_and_unsafe_paths_fail_closed() {
    let f = fixture();
    let missing = f._temp.path().join("never-created");
    let p = project(
        &f.root,
        &missing,
        &f.audit_path,
        "selected",
        &f.task,
        None,
        20,
    )
    .unwrap();
    assert!(p["items"].as_array().unwrap().is_empty());
    assert!(!missing.exists());
    let wrong = uuid::Uuid::new_v4().to_string();
    assert!(project(
        &f.root,
        &f.store.dir,
        &f.audit_path,
        "selected",
        &wrong,
        None,
        20
    )
    .unwrap_err()
    .to_string()
    .contains("TASK_NOT_FOUND"));
    assert!(validate_query(&f.task, None, Some(51)).is_err());
    assert!(validate_query("invalid", None, Some(1)).is_err());
    assert!(patch_hashes(Some(
        &json!({"before_hashes":{"../private.txt":"a".repeat(64),"/private.txt":null}})
    ))
    .is_empty());
    #[cfg(unix)]
    {
        let link = f._temp.path().join("linked.sqlite");
        std::os::unix::fs::symlink(f.store.dir.join("runtime.sqlite3"), &link).unwrap();
        assert!(existing_connection(&link).is_err());
    }
    let readonly = existing_connection(&f.store.dir.join("runtime.sqlite3"))
        .unwrap()
        .unwrap();
    assert!(readonly.execute("DELETE FROM tasks", []).is_err());
}

#[test]
fn canonical_parent_alias_retains_metadata_and_rejects_escape() {
    let temp = tempfile::tempdir().unwrap();
    let allowed = temp.path().join("allowed");
    fs::create_dir(&allowed).unwrap();
    let alias = allowed.join("..").join("allowed");
    let child = alias.join("child.json");
    fs::write(&child, json!({"started":3}).to_string()).unwrap();
    assert_eq!(bounded_file(&child, &alias).unwrap()["started"], 3);
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let external = outside.join("child.json");
    fs::write(&external, json!({"started":9}).to_string()).unwrap();
    assert!(bounded_file(&external, &alias).is_none());
    #[cfg(unix)]
    {
        let direct_link = alias.join("linked-child.json");
        std::os::unix::fs::symlink(&external, &direct_link).unwrap();
        assert!(bounded_file(&direct_link, &alias).is_none());
        let directory_link = alias.join("escaped-directory");
        std::os::unix::fs::symlink(&outside, &directory_link).unwrap();
        assert!(bounded_file(&directory_link.join("child.json"), &alias).is_none());
    }
}
