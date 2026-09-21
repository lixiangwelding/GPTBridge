//! Contract tests exercise the actual MCP dispatcher, not a substitute implementation.
use std::{fs, sync::Arc};
use serde_json::{json, Value};
use coding_tools_personal_runtime::digest;
use super::{ToolContext, call_tool};

fn fixture() -> (tempfile::TempDir, Arc<ToolContext>) {
    let root=tempfile::tempdir().unwrap();
    let workspace=root.path().join("workspace"); fs::create_dir(&workspace).unwrap();
    let context=ToolContext::for_test(workspace,root.path().join("state")).unwrap();
    (root,Arc::new(context))
}
fn task(ctx:&ToolContext,request:&str)->Value {
    let result=call_tool(ctx,"task_open",&json!({"goal":format!("goal-{request}"),"request_id":request}));
    assert_eq!(result["ok"],true,"{result}"); result
}
fn rpc(state:&crate::mcp::server::SharedState,name:&str,args:Value,owner:&str)->Value {
    crate::mcp::server::handle_request(state,&json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":name,"arguments":args,"_meta":{"openai/session":owner}}}))["result"]["structuredContent"].clone()
}
fn patch(ctx:&ToolContext,task:&Value,request:&str,text:&str,hashes:Value)->Value {
    call_tool(ctx,"apply_patch",&json!({"task_id":task["task_id"],"request_id":request,"patch":text,"expected_hashes":hashes}))
}

#[test]
fn mcp_metadata_resumes_tasks_without_a_pasted_history_template() {
    let (_root,ctx)=fixture();
    let state=Arc::new(crate::mcp::server::McpState { tools:ctx,upstream:Arc::new(crate::mcp::upstream::UpstreamMcpManager::empty()) });
    let a=rpc(&state,"task_open",json!({"goal":"first goal"}),"conversation-a");
    let b=rpc(&state,"task_open",json!({"goal":"second goal"}),"conversation-b");
    assert_eq!(a["ok"],true,"{a}"); assert_eq!(b["ok"],true,"{b}"); assert_ne!(a["task_id"],b["task_id"]);
    assert_eq!(rpc(&state,"task_status",json!({}),"conversation-a")["task_id"],a["task_id"]);
    assert_eq!(rpc(&state,"task_open",json!({"task_id":a["task_id"]}),"new-conversation")["task_id"],a["task_id"]);
    assert_eq!(a["raw_input_captured"],false);
    assert!(a["instructions"].as_str().unwrap().contains("no pasted startup prompt"));
}

#[test]
fn no_metadata_requires_stable_creation_identity_and_deduplicates() {
    let (_root,ctx)=fixture();
    let missing=call_tool(&ctx,"task_open",&json!({"goal":"new"}));
    assert_eq!(missing["error"]["code"],"REQUEST_ID_REQUIRED");
    let a=task(&ctx,"new"); let b=task(&ctx,"new"); assert_eq!(a["task_id"],b["task_id"]);
    let list=call_tool(&ctx,"task_status",&json!({})); assert_eq!(list["tasks"].as_array().unwrap().len(),1);
}

#[test]
fn two_tasks_same_file_never_silently_overwrite_each_other() {
    let (_root,ctx)=fixture(); let a=task(&ctx,"a"); let b=task(&ctx,"b");
    fs::write(ctx.workspace.root().join("shared.txt"),"one\ntwo\n").unwrap();
    let before=call_tool(&ctx,"read_file",&json!({"path":"shared.txt"}))["file_sha256"].clone();
    let p1="*** Begin Patch\n*** Update File: shared.txt\n@@\n-one\n+ONE\n*** End Patch";
    let p2="*** Begin Patch\n*** Update File: shared.txt\n@@\n-two\n+TWO\n*** End Patch";
    assert_eq!(patch(&ctx,&a,"a-edit",p1,json!({"shared.txt":before}))["ok"],true);
    let conflict=patch(&ctx,&b,"b-edit",p2,json!({"shared.txt":before}));
    assert_eq!(conflict["error"]["code"],"STALE_FILE","{conflict}");
    assert_eq!(fs::read_to_string(ctx.workspace.root().join("shared.txt")).unwrap(),"ONE\ntwo\n");
    let fresh=call_tool(&ctx,"read_file",&json!({"path":"shared.txt"}))["file_sha256"].clone();
    assert_eq!(patch(&ctx,&b,"b-rebased",p2,json!({"shared.txt":fresh}))["ok"],true);
    assert_eq!(fs::read_to_string(ctx.workspace.root().join("shared.txt")).unwrap(),"ONE\nTWO\n");
}

#[test]
fn repeated_patch_returns_receipt_without_reapplying() {
    let (_root,ctx)=fixture(); let t=task(&ctx,"patch-once");
    let p="*** Begin Patch\n*** Add File: result.txt\n+once\n*** End Patch";
    let first=patch(&ctx,&t,"p1",p,json!({"result.txt":null}));
    let second=patch(&ctx,&t,"p1",p,json!({"result.txt":null}));
    assert_eq!(first["ok"],true,"{first}"); assert_eq!(second["deduplicated"],true,"{second}");
    assert_eq!(first["change_id"],second["change_id"]);
    assert_eq!(patch(&ctx,&t,"p1",&p.replace("once","different"),json!({"result.txt":null}))["error"]["code"],"IDEMPOTENCY_CONFLICT");
}

#[test]
fn independent_files_in_one_directory_can_be_submitted_concurrently() {
    let (_root,ctx)=fixture(); let mut threads=Vec::new();
    for index in 0..4 {
        let ctx=ctx.clone();
        threads.push(std::thread::spawn(move || {
            let t=task(&ctx,&format!("writer-{index}"));
            let path=format!("same-directory/file-{index}.txt");
            let result=patch(&ctx,&t,"edit",&format!("*** Begin Patch\n*** Add File: {path}\n+owned-{index}\n*** End Patch"),json!({path:null}));
            assert_eq!(result["ok"],true,"{result}");
        }));
    }
    for thread in threads { thread.join().unwrap(); }
    for index in 0..4 { assert_eq!(fs::read_to_string(ctx.workspace.root().join(format!("same-directory/file-{index}.txt"))).unwrap(),format!("owned-{index}\n")); }
}

#[test]
fn patch_preflight_is_read_only_and_supplies_preconditions() {
    let (_root,ctx)=fixture();
    let p="*** Begin Patch\n*** Add File: new.txt\n+candidate\n*** End Patch";
    let result=call_tool(&ctx,"patch_check",&json!({"patch":p}));
    assert_eq!(result["ok"],true,"{result}"); assert_eq!(result["expected_hashes"]["new.txt"],Value::Null);
    assert!(!ctx.workspace.root().join("new.txt").exists());
    let write=call_tool(&ctx,"apply_patch",&json!({"patch":p}));
    assert_eq!(write["error"]["code"],"REQUEST_ID_REQUIRED");
}

#[test]
fn file_hash_is_for_the_whole_file_even_with_a_small_text_slice() {
    let (_root,ctx)=fixture(); let text="第一行\nsecond line\nthird\n";
    fs::write(ctx.workspace.root().join("source.txt"),text).unwrap();
    let result=call_tool(&ctx,"read_file",&json!({"path":"source.txt","start_line":2,"end_line":2,"max_bytes":5}));
    assert_eq!(result["file_sha256"],digest(text)); assert_eq!(result["truncated"],true);
}

#[test]
fn thousand_steps_resume_with_bounded_context_and_no_lost_ids() {
    let (_root,ctx)=fixture(); let t=task(&ctx,"long");
    let mut steps=serde_json::Map::new();
    for index in (0..1000).rev() { steps.insert(format!("step-{index:04}"),json!({"state":"passed","evidence":["declared fixture evidence"]})); }
    steps.insert("step-1000".into(),json!({"state":"pending"}));
    let saved=call_tool(&ctx,"task_checkpoint",&json!({"task_id":t["task_id"],"request_id":"batch","expected_revision":0,
        "checkpoint":{"steps":steps,"next_step":"step-1000"}}));
    assert_eq!(saved["ok"],true,"{saved}");
    let status=call_tool(&ctx,"task_status",&json!({"task_id":t["task_id"]}));
    assert_eq!(status["step_counts"]["passed"],1000); assert_eq!(status["steps"].as_object().unwrap().len(),1);
    assert!(status.to_string().len()<10000);
    let mut cursor=Value::Null; let mut all=std::collections::BTreeSet::new();
    loop {
        let mut args=json!({"task_id":t["task_id"],"include_passed":true,"limit":100});
        if cursor.is_string() { args["cursor"]=cursor; }
        let page=call_tool(&ctx,"task_status",&args);
        for key in page["steps"].as_object().unwrap().keys() { assert!(all.insert(key.clone())); }
        cursor=page["next_cursor"].clone(); if cursor.is_null() { break; }
    }
    assert_eq!(all.len(),1001);
}

#[test]
fn stale_checkpoint_preserves_newer_progress() {
    let (_root,ctx)=fixture(); let t=task(&ctx,"checkpoint");
    let args=json!({"task_id":t["task_id"],"request_id":"first","expected_revision":0,"checkpoint":{"next_step":"second"}});
    assert_eq!(call_tool(&ctx,"task_checkpoint",&args)["revision"],1);
    let stale=call_tool(&ctx,"task_checkpoint",&json!({"task_id":t["task_id"],"request_id":"stale","expected_revision":0,"checkpoint":{"next_step":"old"}}));
    assert_eq!(stale["error"]["code"],"STALE_CHECKPOINT");
    assert_eq!(call_tool(&ctx,"task_status",&json!({"task_id":t["task_id"]}))["checkpoint"]["next_step"],"second");
}

#[test]
fn task_registry_and_server_info_agree_and_annotations_do_not_hide_writes() {
    let (_root,ctx)=fixture();
    let tools=super::registry::list_tools_for_profile("core");
    let info=call_tool(&ctx,"server_info",&json!({})); assert_eq!(info["tool_count"].as_u64().unwrap(),tools.len() as u64);
    let legacy=tools.iter().find(|v|v["name"]=="history_session_bootstrap").unwrap();
    assert!(legacy["description"].as_str().unwrap().contains("Legacy"));
    assert!(!legacy["description"].as_str().unwrap().contains("At the start of every new ChatGPT"));
    for name in super::personal::TOOLS {
        let tool=tools.iter().find(|v|v["name"]==*name).unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"],false);
        assert_eq!(tool["inputSchema"]["additionalProperties"],false);
    }
    assert!(super::registry::list_tools_for_profile("read-only").iter().all(|v| !super::personal::TOOLS.contains(&v["name"].as_str().unwrap())));
    assert_eq!(super::registry::list_tools_for_profile("compat-readonly-all").iter().find(|v|v["name"]=="apply_patch").unwrap()["annotations"]["readOnlyHint"],false);
}

#[test]
fn durable_dispatch_uses_original_policy_and_reopens_output() {
    let (_root,ctx)=fixture(); let t=task(&ctx,"command");
    let args=json!({"task_id":t["task_id"],"request_id":"python","cmd":"python3 -c \"print('durable-dispatch')\"","mode":"read","yield_time_ms":5000});
    let result=call_tool(&ctx,"exec_command",&args); assert_eq!(result["command_ok"],true,"{result}");
    assert_eq!(result["execution_mode"],"durable_worker");
    assert_eq!(result["termination_reason"],"exited");
    assert_eq!(result["stderr_truncated"],false);
    let polled=call_tool(&ctx,"write_stdin",&json!({"session_id":result["session_id"],"yield_time_ms":0}));
    assert_eq!(polled["transport_ok"],true);
    assert_eq!(polled["termination_reason"],"exited");
    let replay=call_tool(&ctx,"exec_command",&args); assert_eq!(replay["job_id"],result["job_id"]); assert_eq!(replay["deduplicated"],true);
    let output=call_tool(&ctx,"read_output",&json!({"output_ref":result["output_refs"]["stdout"],"offset":0,"limit":200}));
    assert!(output["content"].as_str().unwrap().contains("durable-dispatch"));
    let rejected=call_tool(&ctx,"exec_command",&json!({"cmd":"echo a && echo b","request_id":"no-chaining","task_id":t["task_id"]}));
    assert_eq!(rejected["ok"],false); assert_ne!(rejected["command_ok"],true);
}
