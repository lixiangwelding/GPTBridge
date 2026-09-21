use std::{fs, sync::Arc};
use serde_json::{json, Value};
use crate::{audit::AuditRequestContext, tools::ToolContext};
use super::{gateway::{Member,WorkspaceHub},server::{McpState,SharedState},upstream::UpstreamMcpManager};

pub(super) fn fixture() -> (tempfile::TempDir,SharedState,SharedState,Arc<WorkspaceHub>) {
    let temp=tempfile::tempdir().unwrap();
    let make=|name:&str| {
        let root=temp.path().join(name);fs::create_dir(&root).unwrap();
        fs::write(root.join("same.txt"),name).unwrap();
        Arc::new(McpState{tools:Arc::new(ToolContext::for_test(root,temp.path().join("state")).unwrap()),upstream:Arc::new(UpstreamMcpManager::empty())})
    };
    let a=make("alpha");let b=make("beta");
    let hub=Arc::new(WorkspaceHub::new(a.clone(),vec![
        ("a".into(),Member{name:"Alpha".into(),state:a.clone()}),
        ("b".into(),Member{name:"Beta".into(),state:b.clone()})]).unwrap());
    (temp,a,b,hub)
}
fn request(hub:&WorkspaceHub,method:&str,params:Value)->Value {
    hub.handle(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),&AuditRequestContext::default())
}
fn call(hub:&WorkspaceHub,name:&str,args:Value)->Value {
    request(hub,"tools/call",json!({"name":name,"arguments":args,"_meta":{"openai/session":"one-conversation"}}))
}

#[test]
fn catalog_exposes_explicit_repository_choice_without_global_selection() {
    let (_t,_a,_b,hub)=fixture();
    let list=request(&hub,"tools/list",json!({}));
    let tools=list["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t|t["name"]=="workspace_list"));
    assert!(!tools.iter().any(|t|t["name"]=="set_default_cwd"));
    for tool in tools.iter().filter(|t|t["name"]!="workspace_list") {
        assert!(tool["inputSchema"]["required"].as_array().unwrap().contains(&json!("workspace_id")));
    }
    assert!(request(&hub,"initialize",json!({}))["result"]["instructions"].as_str().unwrap().contains("workspace_list"));
}

#[test]
fn no_default_unknown_repository_and_root_override_are_rejected() {
    let (_t,_a,b,hub)=fixture();
    assert_eq!(call(&hub,"read_file",json!({"path":"same.txt"}))["result"]["structuredContent"]["error"]["code"],"WORKSPACE_REQUIRED");
    assert_eq!(call(&hub,"read_file",json!({"workspace_id":"other","path":"same.txt"}))["result"]["structuredContent"]["error"]["code"],"WORKSPACE_NOT_ALLOWED");
    assert_eq!(call(&hub,"history_session_search",json!({"workspace_id":"a","workspace_root":b.tools.workspace.root_display()}))["result"]["structuredContent"]["error"]["code"],"WORKSPACE_MISMATCH");
}

#[test]
fn concurrent_same_relative_path_never_leaks_between_repositories() {
    let (_t,_a,_b,hub)=fixture();
    let mut threads=Vec::new();
    for (id,expected) in [("a","alpha"),("b","beta"),("a","alpha"),("b","beta")] {
        let hub=hub.clone();
        threads.push(std::thread::spawn(move||{
            for _ in 0..6 {
                let result=call(&hub,"read_file",json!({"workspace_id":id,"path":"same.txt"}));
                assert_eq!(result["result"]["structuredContent"]["content"],expected);
                assert_eq!(result["result"]["structuredContent"]["workspace_id"],id);
            }
        }));
    }
    for t in threads {t.join().unwrap();}
}

#[test]
fn task_bindings_and_idempotency_are_repository_local() {
    let (_t,_a,_b,hub)=fixture();
    let a=call(&hub,"task_open",json!({"workspace_id":"a","goal":"same goal","request_id":"same"}))["result"]["structuredContent"].clone();
    let b=call(&hub,"task_open",json!({"workspace_id":"b","goal":"same goal","request_id":"same"}))["result"]["structuredContent"].clone();
    assert_eq!(a["ok"],true);assert_eq!(b["ok"],true);assert_ne!(a["task_id"],b["task_id"]);
    assert_eq!(call(&hub,"task_status",json!({"workspace_id":"a"}))["result"]["structuredContent"]["task_id"],a["task_id"]);
    assert_eq!(call(&hub,"task_status",json!({"workspace_id":"b"}))["result"]["structuredContent"]["task_id"],b["task_id"]);
    assert_eq!(call(&hub,"task_status",json!({"workspace_id":"b","task_id":a["task_id"]}))["result"]["structuredContent"]["error"]["code"],"TASK_NOT_FOUND");
}

#[test]
fn cross_repository_absolute_path_is_rejected() {
    let (_t,_a,b,hub)=fixture();
    let result=call(&hub,"read_file",json!({"workspace_id":"a","path":b.tools.workspace.root().join("same.txt")}));
    assert_eq!(result["result"]["isError"],true,"{result}");
}

#[test]
fn readonly_member_keeps_its_original_tool_boundary() {
    let (temp,a,_b,_hub)=fixture();
    let root=temp.path().join("readonly");fs::create_dir(&root).unwrap();
    let mut ctx=ToolContext::for_test(root,temp.path().join("state")).unwrap();
    ctx.tool_profile="read-only".into();
    let readonly=Arc::new(McpState{tools:Arc::new(ctx),upstream:Arc::new(UpstreamMcpManager::empty())});
    let hub=WorkspaceHub::new(a.clone(),vec![("a".into(),Member{name:"A".into(),state:a}),
        ("r".into(),Member{name:"Readonly".into(),state:readonly})]).unwrap();
    let result=call(&hub,"apply_patch",json!({"workspace_id":"r","patch":"invalid"}));
    assert_eq!(result["error"]["data"]["reason"],"unknown_tool");
}

#[test]
fn duplicated_canonical_roots_are_rejected() {
    let (_temp,a,_b,_hub)=fixture();
    assert!(WorkspaceHub::new(a.clone(),vec![("a".into(),Member{name:"A".into(),state:a.clone()}),
        ("alias".into(),Member{name:"Alias".into(),state:a})]).is_err());
}

#[test]
fn member_selection_requires_auth_and_explicit_nonduplicate_ids() {
    let mut host=crate::workspace::WorkspaceProfile::new("/host".into(),Some("host".into()));
    let child=crate::workspace::WorkspaceProfile::new("/child".into(),Some("child".into()));
    host.runtime.gateway_workspace_ids=vec![child.id.clone()];
    assert!(super::gateway::validate_members(&host,&[host.clone(),child.clone()]).is_ok());
    host.auth.auth_type="noauth".into();
    assert!(super::gateway::validate_members(&host,&[host.clone(),child.clone()]).is_err());
    host.auth.auth_type="oauth".into();
    host.runtime.gateway_workspace_ids.push(child.id.clone());
    assert!(super::gateway::validate_members(&host,&[host.clone(),child.clone()]).is_err());
    host.runtime.gateway_workspace_ids=vec!["unknown".into()];
    assert!(super::gateway::validate_members(&host,&[host.clone(),child]).is_err());
}

#[test]
fn running_gateway_blocks_member_permission_and_topology_changes() {
    use std::collections::BTreeSet;
    let mut host=crate::workspace::WorkspaceProfile::new("/host".into(),Some("host".into()));
    let child=crate::workspace::WorkspaceProfile::new("/child".into(),Some("child".into()));
    host.runtime.gateway_workspace_ids=vec![child.id.clone()];
    let profiles=vec![host.clone(),child.clone()];
    let running=BTreeSet::from([host.id.clone()]);
    let mut moved=child.clone();moved.path="/new-location".into();
    assert!(super::gateway::validate_update(&child,&moved,&profiles,&running).is_err());
    let mut unshared=host.clone();unshared.runtime.gateway_workspace_ids.clear();
    assert!(super::gateway::validate_update(&host,&unshared,&profiles,&running).is_err());
    assert!(super::gateway::validate_update(&host,&unshared,&profiles,&BTreeSet::new()).is_ok());
}

#[test]
fn routed_info_catalog_and_text_agree() {
    let (_t,_a,_b,hub)=fixture();
    let info=call(&hub,"server_info",json!({"workspace_id":"a"}));
    let catalog=request(&hub,"tools/list",json!({}));
    assert_eq!(info["result"]["structuredContent"]["tool_count"].as_u64().unwrap() as usize,catalog["result"]["tools"].as_array().unwrap().len());
    let text:Value=serde_json::from_str(info["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text,info["result"]["structuredContent"]);
}
