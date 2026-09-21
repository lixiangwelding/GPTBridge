//! Real loopback HTTP tests, including the actual listener's authentication layer.
use super::*;
use crate::mcp::gateway_tests::fixture;

fn state() -> (tempfile::TempDir,ListenerState) {
    let (temp,a,_b,hub)=fixture();
    let state=ListenerState{mcp:a,auth:AuthConfig{auth_type:"bearer".into(),..Default::default()},
        workspace_id:"gateway-test".into(),workspace_path:"fixture".into(),bind_port:0,
        configured_public_url:"https://gateway.example.invalid".into(),bearer_token:Some("synthetic-test-token".into()),
        oauth:None,oauth_client_secret:None,gateway:Some(hub),request_slots:Arc::new(Semaphore::new(32))};
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
