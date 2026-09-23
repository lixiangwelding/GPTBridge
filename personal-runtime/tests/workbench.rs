use coding_tools_personal_runtime::{Store, workbench::WorkbenchQuery};
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Store) {
    let dir=tempfile::tempdir().unwrap();
    let workspace=dir.path().join("workspace");std::fs::create_dir(&workspace).unwrap();
    let store=Store::at(dir.path().join("state"),workspace).unwrap();(dir,store)
}
fn task(store:&Store,goal:&str)->String {
    store.open_task_request(&json!({"goal":goal,"request_id":uuid::Uuid::new_v4().to_string()})).unwrap()["task_id"].as_str().unwrap().into()
}
fn job(store:&Store,task:&str,state:&str,exit:Option<i64>,updated:i64) {
    let id=uuid::Uuid::new_v4().to_string();
    store.conn().unwrap().execute("INSERT INTO jobs(id,task_id,scope,request_id,input_hash,spec,state,exit_code,created,updated) VALUES(?1,?2,?2,?1,'hash','{}',?3,?4,0,?5)",(&id,task,state,exit,updated)).unwrap();
}
#[test]
fn active_is_ready_and_real_jobs_have_priority() {
    let (_d,s)=fixture();let a=task(&s,"ready");let b=task(&s,"running");let c=task(&s,"queued");let d=task(&s,"unknown");
    job(&s,&b,"running",None,1);job(&s,&c,"queued",None,2);job(&s,&d,"unknown",None,3);
    let page=s.workbench_page(&WorkbenchQuery::default(),"p").unwrap();
    assert_eq!(page.counts["ready"],1);assert_eq!(page.counts["running"],1);assert_eq!(page.counts["queued"],1);assert_eq!(page.counts["attention"],1);
    assert_eq!(page.tasks.iter().find(|t|t.task_id==a).unwrap().display_state,"ready");
}
#[test]
fn completed_unverified_is_not_done_and_paused_is_explicit() {
    let (_d,s)=fixture();let a=task(&s,"unverified");let b=task(&s,"paused");let c=task(&s,"done");
    for (id,state) in [(&a,"completed_unverified"),(&b,"paused"),(&c,"completed")] {
        s.conn().unwrap().execute("UPDATE tasks SET state=?2 WHERE id=?1",(id,state)).unwrap();
    }
    let page=s.workbench_page(&WorkbenchQuery::default(),"p").unwrap();
    assert_eq!(page.counts["attention"],1);assert_eq!(page.counts["paused"],1);assert_eq!(page.counts["done"],1);
}
#[test]
fn search_is_literal_and_filters_do_not_change_scope_counts() {
    let (_d,s)=fixture();task(&s,"literal %_ ' 中文");task(&s,"other");
    let q=WorkbenchQuery{query:"%_ '".into(),..Default::default()};
    let page=s.workbench_page(&q,"p").unwrap();assert_eq!(page.matched_total,1);assert_eq!(page.counts["ready"],2);
    assert_eq!(s.workbench_page(&WorkbenchQuery{query:"中文".into(),..Default::default()},"p").unwrap().tasks.len(),1);
}
#[test]
fn thousand_tasks_keyset_pages_have_no_duplicates_or_missing_rows() {
    let (_d,s)=fixture();let mut c=s.conn().unwrap();let tx=c.transaction().unwrap();
    for i in 0..1000 {tx.execute("INSERT INTO tasks VALUES(?1,?2,'active',0,'{}',1,?3)",(uuid::Uuid::new_v4().to_string(),format!("task-{i}"),i/3)).unwrap();}tx.commit().unwrap();
    let mut q=WorkbenchQuery{limit:Some(50),..Default::default()};let mut seen=std::collections::BTreeSet::new();let mut pages=0;
    loop {let page=s.workbench_page(&q,"p").unwrap();assert!(page.tasks.len()<=50);assert_eq!(page.counts["ready"],1000);for t in page.tasks {assert!(seen.insert(t.task_id));}pages+=1;
        match page.next_cursor {Some(cursor)=>q.cursor=Some(cursor),None=>break}}
    assert_eq!(seen.len(),1000);assert_eq!(pages,20);
}
#[test]
fn bounded_goals_and_invalid_filters_are_checked() {
    let (_d,s)=fixture();task(&s,&"长".repeat(3000));
    let p=s.workbench_page(&WorkbenchQuery::default(),"p").unwrap();assert_eq!(p.tasks[0].goal.chars().count(),600);
    assert!(s.workbench_page(&WorkbenchQuery{filter:"' OR 1=1".into(),..Default::default()},"p").is_err());
    assert!(s.workbench_page(&WorkbenchQuery{limit:Some(51),..Default::default()},"p").is_err());
}
#[test]
fn view_does_not_reconcile_or_mutate_unknown_jobs() {
    let (_d,s)=fixture();let t=task(&s,"unknown");job(&s,&t,"unknown",None,1);
    let before=s.task_status(&t).unwrap();let jobs=s.job_list(Some(&t)).unwrap();
    s.workbench_page(&WorkbenchQuery::default(),"p").unwrap();
    assert_eq!(s.task_status(&t).unwrap(),before);assert_eq!(s.job_list(Some(&t)).unwrap(),jobs);
}
