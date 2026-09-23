#![cfg(unix)]
use std::{fs,io::{BufRead,BufReader,Write},path::{Path,PathBuf},process::{Child,Command,Stdio},sync::mpsc,time::{Duration,Instant}};
use serde_json::{json,Value};
use coding_tools_personal_runtime::{limits::ConcurrencyLimits,locks};
struct Owned(Option<Child>);
impl Drop for Owned {fn drop(&mut self){if let Some(c)=&mut self.0{let _=c.kill();let _=c.wait();}}}
fn command(home:&Path)->Command {
    let mut c=Command::new(env!("CARGO_BIN_EXE_coding-tools-mcp-personal"));
    c.args(["--personal-stdio","high-stdio"]).env("CODING_TOOLS_PERSONAL_HOME",home)
        .env("CODING_TOOLS_PERSONAL_IMPORT","off").env("HOME",home).env("XDG_DATA_HOME",home.join("isolated"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());c
}
#[test]
fn saturated_32_worker_stdio_keeps_control_calls_and_returns_bounded_overload() {
    let root=tempfile::tempdir().unwrap();let home=root.path().join("home");let workspace=root.path().join("workspace");
    fs::create_dir_all(home.join("data")).unwrap();fs::create_dir(&workspace).unwrap();
    let config=json!({"profiles":[{"id":"high-stdio","name":"fixture","path":workspace.canonicalize().unwrap(),
        "tunnel":{"type":"none"},"auth":{"type":"noauth"},"runtime":{"tool_profile":"core","permission_mode":"trusted"}}]});
    fs::write(home.join("data/profiles.json"),config.to_string()).unwrap();
    let mut probe=command(&home).spawn().unwrap();
    writeln!(probe.stdin.take().unwrap(),"{}",json!({"jsonrpc":"2.0","id":0,"method":"tools/call","params":{"name":"server_info","arguments":{}}})).unwrap();
    let output=probe.wait_with_output().unwrap();assert!(output.status.success());
    let info:Value=serde_json::from_slice(&output.stdout).unwrap();
    let path=PathBuf::from(info["result"]["structuredContent"]["concurrency"]["configuration_path"].as_str().unwrap());
    fs::write(&path,serde_json::to_vec(&ConcurrencyLimits::performance()).unwrap()).unwrap();
    let state=path.parent().unwrap();let db=rusqlite::Connection::open(state.join("runtime.sqlite3")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();let job=uuid::Uuid::new_v4().to_string();
    db.execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace','stdio-pressure','fixture','{}','running',0,?2)",
        (&job,coding_tools_personal_runtime::now_ms())).unwrap();
    let _alive=locks::try_gate(state,&format!("alive:{job}"),true).unwrap().unwrap();
    let mut child=Owned(Some(command(&home).spawn().unwrap()));
    let mut input=child.0.as_mut().unwrap().stdin.take().unwrap();let output=child.0.as_mut().unwrap().stdout.take().unwrap();
    let (tx,rx)=mpsc::channel::<Value>();
    let reader=std::thread::spawn(move||{for line in BufReader::new(output).lines(){tx.send(serde_json::from_str(&line.unwrap()).unwrap()).unwrap();}});
    for id in 1..=300 {writeln!(input,"{}",json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"write_stdin","arguments":{"session_id":format!("job-{job}"),"yield_time_ms":8000}}})).unwrap();}
    let start=Instant::now();
    writeln!(input,"{}",json!({"jsonrpc":"2.0","id":901,"method":"tools/call","params":{"name":"server_info","arguments":{}}})).unwrap();
    writeln!(input,"{}",json!({"jsonrpc":"2.0","id":902,"method":"tools/call","params":{"name":"kill_session","arguments":{"session_id":format!("job-{job}"),"wait_ms":0}}})).unwrap();
    input.flush().unwrap();let mut replies=Vec::new();let mut info_done=false;let mut cancel_done=false;
    while !info_done || !cancel_done {
        let v=rx.recv_timeout(Duration::from_secs(3)).expect("control lane must respond while ordinary workers wait");
        if v["id"]==901 {
            assert_eq!(v["result"]["structuredContent"]["concurrency"]["effective"]["stdio_workers"],32);
            assert_eq!(v["result"]["structuredContent"]["concurrency"]["file_descriptors"]["soft"].as_u64().unwrap()>=4096,true);
            info_done=true;
        }
        if v["id"]==902 {assert_eq!(v["result"]["structuredContent"]["cancel_requested"],true);cancel_done=true;}
        replies.push(v);
    }
    let control_ms=start.elapsed().as_millis();assert!(control_ms<3000);
    db.execute("UPDATE jobs SET state='cancelled',updated=?2 WHERE id=?1",(&job,coding_tools_personal_runtime::now_ms())).unwrap();
    drop(input);assert!(child.0.as_mut().unwrap().wait().unwrap().success());child.0.take();
    reader.join().unwrap();replies.extend(rx.try_iter());assert_eq!(replies.len(),302);
    let ids:std::collections::HashSet<_>=replies.iter().map(|v|v["id"].as_u64().unwrap()).collect();assert_eq!(ids.len(),302);
    let rejected=replies.iter().filter(|v|v["error"]["code"]==-32001).count();assert!(rejected>0);
    for v in replies.iter().filter(|v|v["error"]["code"]==-32001) {assert_eq!(v["error"]["data"]["request_executed"],false);}
    println!("{}",json!({"test":"stdio-control-reservation","submitted_polls":300,"workers":32,"total_queue_capacity":256,"rejected":rejected,"control_roundtrip_ms":control_ms,"missing_or_duplicate_replies":0}));
}
