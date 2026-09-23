//! Real loopback HTTP tests, including the actual listener's authentication layer.
use super::*;
use crate::mcp::gateway_tests::fixture;

fn state() -> (tempfile::TempDir,ListenerState) {
    let (temp,a,_b,hub)=fixture();
    let state=ListenerState{mcp:a,auth:AuthConfig{auth_type:"bearer".into(),..Default::default()},
        workspace_id:"gateway-test".into(),workspace_path:"fixture".into(),bind_port:0,
        configured_public_url:"https://gateway.example.invalid".into(),bearer_token:Some("synthetic-test-token".into()),
        oauth:None,oauth_client_secret:None,gateway:Some(hub),request_slots:Arc::new(Semaphore::new(32)),waiting_slots:Arc::new(Semaphore::new(24)),work_slots:Arc::new(Semaphore::new(28))};
    (temp,state)
}

#[tokio::test]
async fn skill_search_and_read_work_through_authenticated_http(){
    let (temp,state)=state();
    let folder=temp.path().join("alpha/.agents/skills/frontend");std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("SKILL.md"),"---\nname: 前端开发\ndescription: 页面修复\n---\n真实测试技能正文\n").unwrap();
    let server=start(state).await;let c=client();
    let search=json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"search_skills","arguments":{"workspace_id":"a","query":"$前端开发"}}});
    assert_eq!(c.post(format!("{}/mcp",server.url)).json(&search).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
    let response:Value=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&search).send().await.unwrap().json().await.unwrap();
    let skill=&response["result"]["structuredContent"]["skills"][0]["skill_id"];assert!(skill.is_string(),"{response}");
    let invoke=json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"invoke_skill","arguments":{"workspace_id":"a","skill_id":skill}}});
    let response:Value=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&invoke).send().await.unwrap().json().await.unwrap();
    assert!(response["result"]["structuredContent"]["content"].as_str().unwrap().contains("真实测试技能正文"));
    assert_eq!(response["result"]["structuredContent"]["scripts_executed"],false);stop(server).await;
}
struct Server { url:String, stop:Option<oneshot::Sender<()>>, handle:tokio::task::JoinHandle<()> }
impl Drop for Server {fn drop(&mut self){if let Some(stop)=self.stop.take(){let _=stop.send(());}}}
async fn start(state:ListenerState)->Server {
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url=format!("http://{}",listener.local_addr().unwrap());
    let (stop,done)=oneshot::channel();
    let handle=tokio::spawn(async move{axum::serve(listener,router(state)).with_graceful_shutdown(async{let _=done.await;}).await.unwrap();});
    Server{url,stop:Some(stop),handle}
}
fn client()->reqwest::Client {reqwest::Client::builder().no_proxy().timeout(std::time::Duration::from_secs(10)).build().unwrap()}
fn body(id:&str)->Value {json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read_file","arguments":{"workspace_id":id,"path":"same.txt"}}})}
async fn stop(mut server:Server) {let _=server.stop.take().unwrap().send(());(&mut server.handle).await.unwrap();}

#[tokio::test]
async fn mcp_get_rejects_event_stream_without_breaking_health_discovery() {
    let (_temp,state)=state();let server=start(state).await;let c=client();
    let endpoint=format!("{}/mcp",server.url);
    let stream=c.get(&endpoint).header("accept","text/event-stream").send().await.unwrap();
    assert_eq!(stream.status(),StatusCode::METHOD_NOT_ALLOWED);
    assert!(stream.bytes().await.unwrap().is_empty());
    let health=c.get(&endpoint).send().await.unwrap();
    assert_eq!(health.status(),StatusCode::OK);
    assert_eq!(health.headers()["cache-control"],"no-store");
    let discovery:Value=health.json().await.unwrap();
    assert_eq!(discovery["protocolVersion"],"2025-06-18");
    stop(server).await;
}

#[tokio::test]
async fn one_http_endpoint_routes_two_repositories_and_requires_authentication() {
    let (_temp,state)=state();let server=start(state).await;let c=client();
    assert_eq!(c.post(format!("{}/mcp",server.url)).json(&body("a")).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
    assert_eq!(c.post(format!("{}/mcp",server.url)).bearer_auth("wrong").json(&body("a")).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
    for (id,text) in [("a","alpha"),("b","beta")] {
        let result:Value=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&body(id)).send().await.unwrap().json().await.unwrap();
        assert_eq!(result["result"]["structuredContent"]["content"],text);
    }
    stop(server).await;
}

#[tokio::test]
async fn http_notifications_return_202_without_a_json_body() {
    let (_temp,state)=state();let server=start(state).await;
    let response=client().post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token")
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).send().await.unwrap();
    assert_eq!(response.status(),StatusCode::ACCEPTED);assert!(response.bytes().await.unwrap().is_empty());
    stop(server).await;
}

#[tokio::test]
async fn http_capacity_and_request_size_are_bounded() {
    let (_temp,state)=state();let held=state.request_slots.clone().acquire_many_owned(32).await.unwrap();
    let server=start(state).await;let c=client();
    let response=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&body("a")).send().await.unwrap();
    assert_eq!(response.status(),StatusCode::TOO_MANY_REQUESTS);assert_eq!(response.headers()["retry-after"],"1");
    drop(held);
    let response=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token")
        .json(&json!({"oversized":"x".repeat(2*1024*1024)})).send().await.unwrap();
    assert_eq!(response.status(),StatusCode::PAYLOAD_TOO_LARGE);
    stop(server).await;
}

#[tokio::test]
async fn oauth_discovery_stays_on_one_gateway_and_invalid_tokens_are_rejected() {
    let (_temp,mut state)=state();
    state.auth.auth_type="oauth".into();
    state.oauth=Some(Arc::new(OAuthRuntime::new("https://gateway.example.invalid".into(),"fixture-client".into(),None,"fixture-password".into(),"fixture-signing-key".into())));
    let server=start(state).await;let c=client();
    let result:Value=c.get(format!("{}/.well-known/oauth-protected-resource",server.url)).send().await.unwrap().json().await.unwrap();
    assert_eq!(result["authorization_servers"][0],"https://gateway.example.invalid");
    let result:Value=c.get(format!("{}/.well-known/oauth-authorization-server",server.url)).send().await.unwrap().json().await.unwrap();
    assert!(result["authorization_endpoint"].as_str().unwrap().starts_with("https://gateway.example.invalid/"));
    let response=c.post(format!("{}/mcp",server.url)).bearer_auth("invalid").json(&body("a")).send().await.unwrap();
    assert_eq!(response.status(),StatusCode::UNAUTHORIZED);
    stop(server).await;
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn long_polls_reserve_capacity_for_other_conversations() {
    let (_temp,state)=state();let job=uuid::Uuid::new_v4().to_string();let store=&state.mcp.tools.personal;
    store.conn().unwrap().execute(
        "INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace','capacity-fixture','fixture','{}','running',0,?2)",
        (&job,coding_tools_personal_runtime::now_ms())).unwrap();
    let _alive=coding_tools_personal_runtime::locks::try_gate(&store.dir,&format!("alive:{job}"),true).unwrap().unwrap();
    let slots=state.request_slots.clone();let server=start(state).await;let c=client();
    let request=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"write_stdin",
        "arguments":{"workspace_id":"a","session_id":format!("job-{job}"),"chars":"","yield_time_ms":3000}}});
    let mut polls=tokio::task::JoinSet::new();
    for _ in 0..24 {
        let c=c.clone();let url=format!("{}/mcp",server.url);let request=request.clone();
        polls.spawn(async move {c.post(url).bearer_auth("synthetic-test-token").json(&request).send().await.unwrap()});
    }
    tokio::time::timeout(std::time::Duration::from_secs(2),async {
        while slots.available_permits()!=8 {tokio::time::sleep(std::time::Duration::from_millis(5)).await;}
    }).await.expect("24 requests must reach the real listener");
    let overflow=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&request).send().await.unwrap();
    assert_eq!(overflow.status(),StatusCode::TOO_MANY_REQUESTS,"long waits must leave eight slots for queries");
    let response=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&body("b")).send().await.unwrap();
    assert_eq!(response.status(),StatusCode::OK);
    assert_eq!(response.json::<Value>().await.unwrap()["result"]["structuredContent"]["content"],"beta");
    // Saturated long waits must not consume authentication or cancellation capacity.
    assert_eq!(c.post(format!("{}/mcp",server.url)).json(&request).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
    let cancel=json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kill_session",
        "arguments":{"workspace_id":"a","session_id":format!("job-{job}"),"wait_ms":0}}});
    let cancelled:Value=c.post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&cancel).send().await.unwrap().json().await.unwrap();
    assert_eq!(cancelled["result"]["structuredContent"]["cancel_requested"],true);
    while let Some(result)=polls.join_next().await {assert_eq!(result.unwrap().status(),StatusCode::OK);}
    assert_eq!(slots.available_permits(),32);stop(server).await;
}


#[tokio::test]
async fn total_capacity_rejection_returns_the_wait_permit() {
    let (_temp,state)=state();let waits=state.waiting_slots.clone();
    let held=state.request_slots.clone().acquire_many_owned(32).await.unwrap();
    let server=start(state).await;
    let request=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"exec_command","arguments":{"workspace_id":"a","cmd":"not-executed"}}});
    let response=client().post(format!("{}/mcp",server.url)).bearer_auth("synthetic-test-token").json(&request).send().await.unwrap();
    assert_eq!(response.status(),StatusCode::TOO_MANY_REQUESTS);assert_eq!(waits.available_permits(),24);
    drop(held);stop(server).await;
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn disconnected_request_holds_capacity_until_its_worker_finishes() {
    let (_temp,state)=state();let job=uuid::Uuid::new_v4().to_string();let store=&state.mcp.tools.personal;
    store.conn().unwrap().execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace','disconnect','fixture','{}','running',0,?2)",
        (&job,coding_tools_personal_runtime::now_ms())).unwrap();
    let _alive=coding_tools_personal_runtime::locks::try_gate(&store.dir,&format!("alive:{job}"),true).unwrap().unwrap();
    let slots=state.request_slots.clone();let waits=state.waiting_slots.clone();
    let mut headers=HeaderMap::new();headers.insert("authorization","Bearer synthetic-test-token".parse().unwrap());
    let request=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"write_stdin",
        "arguments":{"workspace_id":"a","session_id":format!("job-{job}"),"yield_time_ms":700}}});
    let handler=tokio::spawn(mcp_post(State(state),headers,Json(request)));
    tokio::time::timeout(std::time::Duration::from_secs(2),async {
        while slots.available_permits()==32 {tokio::time::sleep(std::time::Duration::from_millis(2)).await;}
    }).await.unwrap();
    handler.abort();let _=handler.await;
    assert_eq!(slots.available_permits(),31);assert_eq!(waits.available_permits(),23);
    tokio::time::timeout(std::time::Duration::from_secs(3),async {
        while slots.available_permits()!=32 {tokio::time::sleep(std::time::Duration::from_millis(5)).await;}
    }).await.unwrap();
    assert_eq!(waits.available_permits(),24);
}

#[tokio::test]
async fn automatic_skill_catalog_is_authenticated_and_repository_scoped() {
    let (temp,state)=state();
    for (repo,name) in [("alpha","alpha-local-skill"),("beta","beta-local-skill")] {
        let dir=temp.path().join(repo).join(".agents/skills/test");std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"),format!("---\nname: {name}\ndescription: isolated task guidance\n---\nACTIVATION_ONLY\n")).unwrap();
    }
    let server=start(state).await;let c=client();let endpoint=format!("{}/mcp",server.url);
    let init=json!({"jsonrpc":"2.0","id":11,"method":"initialize"});
    assert_eq!(c.post(&endpoint).json(&init).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
    let initialized:Value=c.post(&endpoint).bearer_auth("synthetic-test-token").json(&init).send().await.unwrap().json().await.unwrap();
    let instructions=initialized["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("automatically"));assert!(instructions.contains("workspace_id"));
    assert!(!instructions.contains("alpha-local-skill"));assert!(!instructions.contains("beta-local-skill"));
    let mut alpha_id=Value::Null;
    for (repo,name,other) in [("a","alpha-local-skill","beta-local-skill"),("b","beta-local-skill","alpha-local-skill")] {
        let request=json!({"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"task_open",
            "arguments":{"workspace_id":repo,"goal":"fix ordinary page","request_id":"http-auto-test"}}});
        let value:Value=c.post(&endpoint).bearer_auth("synthetic-test-token").json(&request).send().await.unwrap().json().await.unwrap();
        let result=&value["result"]["structuredContent"];assert_eq!(result["ok"],true,"{value}");
        assert_eq!(result["workspace_id"],repo);
        let catalog=&result["skill_discovery"];assert_eq!(catalog["available_count"],1);
        assert_eq!(catalog["available_skills"][0]["name"],name);assert!(!catalog.to_string().contains(other));
        assert!(!catalog.to_string().contains("ACTIVATION_ONLY"));
        if repo=="a"{alpha_id=catalog["available_skills"][0]["skill_id"].clone();}
    }
    let invoke=json!({"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"invoke_skill",
        "arguments":{"workspace_id":"a","skill_id":alpha_id}}});
    let value:Value=c.post(&endpoint).bearer_auth("synthetic-test-token").json(&invoke).send().await.unwrap().json().await.unwrap();
    assert!(value["result"]["structuredContent"]["content"].as_str().unwrap().contains("ACTIVATION_ONLY"));
    assert_eq!(value["result"]["structuredContent"]["scripts_executed"],false);
    let mut wrong=invoke;wrong["params"]["arguments"]["workspace_id"]=json!("b");
    let value:Value=c.post(&endpoint).bearer_auth("synthetic-test-token").json(&wrong).send().await.unwrap().json().await.unwrap();
    assert_eq!(value["result"]["structuredContent"]["error"]["code"],"SKILL_NOT_FOUND");
    stop(server).await;
}

#[tokio::test]
async fn performance_general_saturation_keeps_32_control_slots() {
    let (_temp,mut state)=state();
    state.request_slots=Arc::new(Semaphore::new(256));state.waiting_slots=Arc::new(Semaphore::new(192));
    state.work_slots=Arc::new(Semaphore::new(224));
    let held=state.work_slots.clone().acquire_many_owned(224).await.unwrap();
    let server=start(state).await;let c=client();let endpoint=format!("{}/mcp",server.url);
    assert_eq!(c.post(&endpoint).bearer_auth("synthetic-test-token").json(&body("a")).send().await.unwrap().status(),StatusCode::TOO_MANY_REQUESTS);
    let control=json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"server_info","arguments":{"workspace_id":"a"}}});
    let response=c.post(&endpoint).bearer_auth("synthetic-test-token").json(&control).send().await.unwrap();
    assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.json::<Value>().await.unwrap()["result"]["structuredContent"]["ok"],true);
    assert_eq!(c.post(&endpoint).json(&control).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
    drop(held);stop(server).await;
}
#[tokio::test(flavor="multi_thread",worker_threads=4)]
async fn performance_192_simultaneous_waits_keep_queries_and_cancel_responsive() {
    coding_tools_personal_runtime::limits::raise_file_capacity(4096);
    let (_temp,mut state)=state();state.request_slots=Arc::new(Semaphore::new(256));
    state.waiting_slots=Arc::new(Semaphore::new(192));state.work_slots=Arc::new(Semaphore::new(224));
    let job=uuid::Uuid::new_v4().to_string();let store=state.mcp.tools.personal.clone();
    store.conn().unwrap().execute("INSERT INTO jobs(id,scope,request_id,input_hash,spec,state,created,updated) VALUES(?1,'workspace','high-polls','fixture','{}','running',0,?2)",(&job,coding_tools_personal_runtime::now_ms())).unwrap();
    let _alive=coding_tools_personal_runtime::locks::try_gate(&store.dir,&format!("alive:{job}"),true).unwrap().unwrap();
    let slots=state.request_slots.clone();let server=start(state).await;let c=client();
    let request=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"write_stdin","arguments":{"workspace_id":"a","session_id":format!("job-{job}"),"yield_time_ms":8000}}});
    let mut polls=tokio::task::JoinSet::new();
    for n in 0..192 {let c=c.clone();let url=format!("{}/mcp",server.url);let body=request.clone();
        polls.spawn(async move{c.post(url).bearer_auth("synthetic-test-token").json(&body).send().await.unwrap()});
        // Measure in-flight concurrency, not macOS's 128-entry TCP backlog.
        // Earlier requests remain pending; no tool call is retried.
        if (n+1)%32==0 {
            tokio::time::timeout(std::time::Duration::from_secs(2),async {
                while slots.available_permits()>256-(n+1) {
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }
            }).await.expect("each connection batch must be admitted");
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(6),async{
        while slots.available_permits()!=64 {tokio::time::sleep(std::time::Duration::from_millis(5)).await;}
    }).await.expect("192 real long-poll handlers must be admitted");
    let endpoint=format!("{}/mcp",server.url);let begin=std::time::Instant::now();
    assert_eq!(c.post(&endpoint).bearer_auth("synthetic-test-token").json(&request).send().await.unwrap().status(),StatusCode::TOO_MANY_REQUESTS);
    let result:Value=c.post(&endpoint).bearer_auth("synthetic-test-token").json(&body("b")).send().await.unwrap().json().await.unwrap();
    assert_eq!(result["result"]["structuredContent"]["content"],"beta");
    let cancel=json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kill_session","arguments":{"workspace_id":"a","session_id":format!("job-{job}"),"wait_ms":0}}});
    let result:Value=c.post(&endpoint).bearer_auth("synthetic-test-token").json(&cancel).send().await.unwrap().json().await.unwrap();
    assert_eq!(result["result"]["structuredContent"]["cancel_requested"],true);
    let control_ms=begin.elapsed().as_millis();assert!(control_ms<2000);
    store.finish_job(&job,"cancelled",None,"isolated fixture complete").unwrap();
    while let Some(r)=polls.join_next().await {assert_eq!(r.unwrap().status(),StatusCode::OK);}
    assert_eq!(slots.available_permits(),256);
    println!("{}",json!({"test":"http-high-concurrency","simultaneous_waits":192,"reserved_remaining":64,"control_roundtrip_ms":control_ms}));
    stop(server).await;
}
