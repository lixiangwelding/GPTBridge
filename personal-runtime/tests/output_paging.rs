//! Real Store::job_output calls with private SQLite/log fixtures; no worker or service is started.
use std::{fs, time::{SystemTime, UNIX_EPOCH}};
use coding_tools_personal_runtime::Store;
use serde_json::Value;

macro_rules! check { ($n:ident, $condition:expr) => {{ $n += 1; assert!($condition, "{}", stringify!($condition)); }} }
fn report(name: &str, count: usize) { println!("OUTPUT_PAGING_ASSERTIONS name={name} count={count}"); }
fn fixture(bytes: &[u8], state: &str) -> (tempfile::TempDir, Store, String) {
    let root=tempfile::tempdir().unwrap();
    let workspace=root.path().join("workspace"); fs::create_dir(&workspace).unwrap();
    let store=Store::open(&root.path().join("state"),&workspace).unwrap();
    let id=uuid::Uuid::new_v4().to_string();
    let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    store.conn().unwrap().execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,exit_code,created,updated) VALUES(?1,'workspace','output-fixture','fixture','{}',?2,0,?3,?3)",(&id,state,now)).unwrap();
    let dir=store.dir.join("jobs").join(&id); fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("stdout.log"),bytes).unwrap(); fs::write(dir.join("stderr.log"),b"").unwrap();
    (root,store,id)
}
#[test]
fn mixed_unicode_roundtrips_at_small_and_large_page_limits() {
    let mut checks=0;
    let text="A中😀é\n日本語尾".repeat(7);
    let (_root,store,id)=fixture(text.as_bytes(),"exited");
    for limit in [1,2,3,4,5,7,8,9,10,4096,1_048_576] {
        let mut offset=0; let mut rebuilt=String::new();
        loop {
            let page=store.job_output(&id,"stdout",offset,limit).unwrap();
            let part=page["content"].as_str().unwrap();
            let next=page["poll_offset"].as_u64().unwrap();
            check!(checks,!part.contains('\u{fffd}'));
            check!(checks,page["content_lossy"]==false);
            check!(checks,next>offset);
            check!(checks,next-offset==part.len() as u64);
            check!(checks,next-offset<=(limit+3) as u64);
            check!(checks,page["bytes_read"]==next-offset);
            rebuilt.push_str(part); offset=next;
            if page["next_offset"].is_null() { break; }
            check!(checks,page["next_offset"]==next);
            check!(checks,offset<=text.len() as u64);
        }
        check!(checks,rebuilt==text);
        check!(checks,offset==text.len() as u64);
    }
    report("mixed_unicode",checks);
}
#[test]
fn stdout_and_stderr_have_independent_unicode_offsets() {
    let mut checks=0;
    let (_root,store,id)=fixture("中A".as_bytes(),"exited");
    fs::write(store.dir.join("jobs").join(&id).join("stderr.log"),"错😀").unwrap();
    let out=store.job_output(&id,"stdout",0,1).unwrap();
    let err=store.job_output(&id,"stderr",0,4).unwrap();
    check!(checks,out["content"]=="中"); check!(checks,out["poll_offset"]==3);
    check!(checks,err["content"]=="错"); check!(checks,err["poll_offset"]==3);
    check!(checks,store.job_output(&id,"stdout",3,1).unwrap()["content"]=="A");
    check!(checks,store.job_output(&id,"stderr",3,1).unwrap()["content"]=="😀");
    report("streams",checks);
}
#[test]
fn incomplete_live_codepoint_is_deferred_then_read_once() {
    let mut checks=0;
    let bytes="中".as_bytes();
    let (_root,store,id)=fixture(&bytes[..2],"queued");
    let page=store.job_output(&id,"stdout",0,1).unwrap();
    check!(checks,page["content"]==""); check!(checks,page["poll_offset"]==0);
    check!(checks,page["next_offset"]==Value::Null);
    check!(checks,page["pending_utf8_bytes"]==2); check!(checks,page["content_lossy"]==false);
    fs::write(store.dir.join("jobs").join(&id).join("stdout.log"),bytes).unwrap();
    let ready=store.job_output(&id,"stdout",0,1).unwrap();
    check!(checks,ready["content"]=="中"); check!(checks,ready["poll_offset"]==3);
    check!(checks,ready["pending_utf8_bytes"]==0);
    report("live_partial",checks);
}
#[test]
fn valid_prefix_precedes_deferred_live_emoji() {
    let mut checks=0;
    let bytes="😀".as_bytes(); let mut log=b"OK".to_vec(); log.extend_from_slice(&bytes[..2]);
    let (_root,store,id)=fixture(&log,"queued");
    let first=store.job_output(&id,"stdout",0,4096).unwrap();
    check!(checks,first["content"]=="OK"); check!(checks,first["poll_offset"]==2);
    let pending=store.job_output(&id,"stdout",2,4096).unwrap();
    check!(checks,pending["content"]==""); check!(checks,pending["next_offset"].is_null());
    check!(checks,pending["pending_utf8_bytes"]==2);
    log.extend_from_slice(&bytes[2..]); fs::write(store.dir.join("jobs").join(&id).join("stdout.log"),&log).unwrap();
    let last=store.job_output(&id,"stdout",2,4096).unwrap();
    check!(checks,last["content"]=="😀"); check!(checks,last["poll_offset"]==6);
    report("live_prefix",checks);
}
#[test]
fn invalid_and_terminal_incomplete_bytes_are_marked_lossy_without_stalling() {
    let mut checks=0;
    for bytes in [vec![0xff,b'A'],vec![0xe4,0xb8]] {
        let (_root,store,id)=fixture(&bytes,"exited");
        let page=store.job_output(&id,"stdout",0,4096).unwrap();
        check!(checks,page["content_lossy"]==true);
        check!(checks,page["bytes_read"]==bytes.len() as u64);
        check!(checks,page["poll_offset"]==bytes.len() as u64);
        check!(checks,page["pending_utf8_bytes"]==0);
        check!(checks,page["next_offset"].is_null());
    }
    report("invalid_terminal",checks);
}
#[test]
fn eof_and_out_of_range_offsets_do_not_duplicate_output() {
    let mut checks=0;
    let (_root,store,id)=fixture(b"abc","exited");
    for offset in [3,u64::MAX] {
        let page=store.job_output(&id,"stdout",offset,1).unwrap();
        check!(checks,page["content"]==""); check!(checks,page["offset"]==3);
        check!(checks,page["poll_offset"]==3); check!(checks,page["bytes_read"]==0);
        check!(checks,page["next_offset"].is_null()); check!(checks,page["content_lossy"]==false);
    }
    report("eof",checks);
}
#[test]
fn maximum_page_budget_and_unicode_boundary_are_both_bounded() {
    let mut checks=0;
    let text="中".repeat(400_000);
    let (_root,store,id)=fixture(text.as_bytes(),"exited");
    let page=store.job_output(&id,"stdout",0,usize::MAX).unwrap();
    check!(checks,page["content_lossy"]==false);
    check!(checks,page["bytes_read"].as_u64().unwrap()<=1_048_576);
    check!(checks,page["content"].as_str().unwrap().len()==1_048_575);
    check!(checks,page["next_offset"]==1_048_575);
    report("max_budget",checks);
}
#[test]
fn unknown_job_and_invalid_stream_remain_rejected() {
    let mut checks=0;
    let (_root,store,id)=fixture(b"owned","exited");
    let (_other_root,other,_other)=fixture(b"foreign","exited");
    check!(checks,other.job_output(&id,"stdout",0,10).unwrap_err().code()=="JOB_NOT_FOUND");
    check!(checks,store.job_output(&id,"other",0,10).unwrap_err().code()=="INVALID_STREAM");
    report("isolation",checks);
}

#[test]
fn invalid_prefix_does_not_split_following_valid_unicode() {
    let mut bytes = vec![0xff];
    bytes.extend_from_slice("中🙂A".as_bytes());
    bytes.extend_from_slice(&[0xe4, 0xb8]);
    let (_root, store, id) = fixture(&bytes, "exited");
    for limit in 1..=8 {
        let mut offset = 0;
        let mut rebuilt = String::new();
        loop {
            let page = store.job_output(&id, "stdout", offset, limit).unwrap();
            rebuilt.push_str(page["content"].as_str().unwrap());
            let next = page["poll_offset"].as_u64().unwrap();
            assert!(next > offset && next - offset <= (limit + 3) as u64);
            offset = next;
            if page["next_offset"].is_null() { break; }
        }
        assert_eq!(rebuilt, String::from_utf8_lossy(&bytes), "limit={limit}");
        assert_eq!(offset, bytes.len() as u64);
    }
}
