#![cfg(unix)]
use std::{fs, io::Write, path::Path, process::{Command,Stdio,Child}, time::{Duration,Instant}};
use serde_json::{json,Value};

fn fixture(auth:&str)->(tempfile::TempDir,std::path::PathBuf,Vec<u8>) {
    let dir=tempfile::tempdir().unwrap();let home=dir.path().join("home");let workspace=dir.path().join("workspace");
    fs::create_dir_all(home.join("data")).unwrap();fs::create_dir(&workspace).unwrap();
    let bytes=serde_json::to_vec(&json!({"profiles":[{"id":"desktop-fixture","name":"fixture","path":workspace.canonicalize().unwrap(),
        "tunnel":{"type":"none"},"auth":{"type":auth,"use_shared_secrets":false},"runtime":{"tool_profile":"core","permission_mode":"trusted"}}],
        "workspace_secrets":{"desktop-fixture":{"bearer_token":"fixture-secret"}}})).unwrap();
    fs::write(home.join("data/profiles.json"),&bytes).unwrap();(dir,home,bytes)
}
fn command(home:&Path)->Command {
    let mut cmd=Command::new(env!("CARGO_BIN_EXE_coding-tools-mcp-personal"));
    cmd.env("CODING_TOOLS_PERSONAL_HOME",home).env("CODING_TOOLS_PERSONAL_IMPORT","off")
        .env("HOME",home).env("XDG_DATA_HOME",home.join("isolated-data"));
    cmd
}
#[test]
fn stdio_uses_real_dispatch_and_does_not_pollute_stdout_or_rewrite_config() {
    let (_dir,home,before)=fixture("noauth");
    let mut child=command(&home).args(["--personal-stdio","desktop-fixture"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut input=child.stdin.take().unwrap();
    for request in [json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"server_info","arguments":{}}}),
        json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"task_open","arguments":{"goal":"must-not-run"}}})] {
        writeln!(input,"{request}").unwrap();
    }
    writeln!(input,"broken-json").unwrap();drop(input);
    let output=child.wait_with_output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let values:Vec<Value>=String::from_utf8(output.stdout).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect();
    assert_eq!(values.len(),5);
    let initialize=values.iter().find(|v|v["id"]==1).unwrap();
    assert_eq!(initialize["result"]["serverInfo"]["version"],env!("CARGO_PKG_VERSION"));
    assert!(initialize["result"]["instructions"].as_str().unwrap().contains("absolute paths"));
    let tools=values.iter().find(|v|v["id"]==2).unwrap()["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(),37);
    for name in ["check_command", "tool_catalog_check", "read_files", "stat_path"] { assert!(tools.iter().any(|tool|tool["name"]==name)); }
    let info=&values.iter().find(|v|v["id"]==3).unwrap()["result"]["structuredContent"];
    assert_eq!(info["delivery"]["mode"],"local_files");assert_eq!(info["runtime_pressure"]["active"],0);
    assert!(values.iter().any(|v|v["error"]["code"]==-32600));
    assert!(values.iter().any(|v|v["error"]["code"]==-32700));
    assert_eq!(fs::read(home.join("data/profiles.json")).unwrap(),before);
}
#[test]
fn unknown_profile_and_invalid_port_exit_without_gui_or_config_writes() {
    let (_dir,home,before)=fixture("noauth");
    for args in [["--personal-stdio","missing"].to_vec(),["--personal-serve","desktop-fixture","22"].to_vec()] {
        let result=command(&home).args(args).output().unwrap();assert!(!result.status.success());assert!(result.stdout.is_empty());
    }
    assert_eq!(fs::read(home.join("data/profiles.json")).unwrap(),before);
}
struct OwnedServer(Child);
impl Drop for OwnedServer {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
#[tokio::test]
async fn headless_http_preserves_bearer_auth_and_does_not_reclaim_ports() {
    let (_dir,home,before)=fixture("bearer");
    let reservation=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let port=reservation.local_addr().unwrap().port();drop(reservation);
    let _server=OwnedServer(command(&home).args(["--personal-serve","desktop-fixture",&port.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let client=reqwest::Client::builder().timeout(Duration::from_millis(500)).build().unwrap();
    let url=format!("http://127.0.0.1:{port}/mcp");let start=Instant::now();
    loop {
        if let Ok(response)=client.get(&url).send().await {if response.status().is_success(){
            assert_eq!(response.json::<Value>().await.unwrap()["version"],env!("CARGO_PKG_VERSION"));break;
        }}
        assert!(start.elapsed()<Duration::from_secs(10),"headless listener failed to start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let request=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"server_info","arguments":{}}});
    assert_eq!(client.post(&url).json(&request).send().await.unwrap().status(),401);
    let response=client.post(&url).bearer_auth("fixture-secret").json(&request).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.json::<Value>().await.unwrap()["result"]["structuredContent"]["version"],env!("CARGO_PKG_VERSION"));
    let occupied=command(&home).args(["--personal-serve","desktop-fixture",&port.to_string()]).output().unwrap();
    assert!(!occupied.status.success());assert!(client.get(&url).send().await.unwrap().status().is_success());
    assert_eq!(fs::read(home.join("data/profiles.json")).unwrap(),before);
}
