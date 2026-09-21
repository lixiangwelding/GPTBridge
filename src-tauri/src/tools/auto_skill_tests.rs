//! Model-facing discovery contracts: no explicit $ invocation is required.
use std::{fs, path::Path, sync::Arc};
use serde_json::{json, Value};
use super::{call_tool, ToolContext};
use crate::mcp::{server::{handle_request, McpState}, upstream::UpstreamMcpManager};

fn skill(root: &Path, folder: &str, manual_only: bool) {
    let dir = root.join(".agents/skills").join(folder);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), format!(
        "---\nname: {folder}\ndescription: 页面测试和接口排查\ndisable-model-invocation: {manual_only}\n---\nBODY_ONLY_ON_ACTIVATION\n"
    )).unwrap();
}
fn fixture() -> (tempfile::TempDir, Arc<McpState>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace"); fs::create_dir(&root).unwrap();
    skill(&root, "ui-check", false);
    let ctx = ToolContext::for_test(root, temp.path().join("state")).unwrap();
    (temp, Arc::new(McpState { tools: Arc::new(ctx), upstream: Arc::new(UpstreamMcpManager::empty()) }))
}
fn rpc(state: &Arc<McpState>, method: &str) -> Value {
    handle_request(state, &json!({"jsonrpc":"2.0", "id":1, "method":method}))
}
#[test]
fn initialize_discloses_available_skills_without_a_dollar_request() {
    let (_temp, state) = fixture(); let value = rpc(&state, "initialize");
    let instructions = value["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("ui-check"), "{value}");
    assert!(instructions.contains("automatically"));
    assert!(instructions.contains("untrusted"));
    assert!(!instructions.contains("BODY_ONLY_ON_ACTIVATION"));
    assert_eq!(value["result"]["capabilities"]["tools"]["listChanged"], false);
}
#[test]
fn tool_catalog_carries_discovery_for_clients_ignoring_initialize_hints() {
    let (_temp, state) = fixture(); let value = rpc(&state, "tools/list");
    let tools = value["result"]["tools"].as_array().unwrap();
    let activation = tools.iter().find(|t| t["name"] == "invoke_skill").unwrap();
    assert!(activation["description"].as_str().unwrap().contains("ui-check"));
    assert!(!value.to_string().contains("BODY_ONLY_ON_ACTIVATION"));
    assert_eq!(tools.len(), super::list_tools_for_profile(&state.tools.tool_profile).len());
}
#[test]
fn opening_an_ordinary_task_returns_catalog_not_loaded_skill_bodies() {
    let (_temp, state) = fixture();
    let value = call_tool(&state.tools, "task_open", &json!({"goal":"检查页面交互", "request_id":"auto-test-ui"}));
    assert_eq!(value["ok"], true, "{value}");
    assert_eq!(value["skill_discovery"]["selection_mode"], "model_auto");
    assert_eq!(value["skill_discovery"]["available_skills"][0]["name"], "ui-check");
    assert_eq!(value["skill_discovery"]["scripts_executed"], false);
    assert!(!value.to_string().contains("BODY_ONLY_ON_ACTIVATION"));
}
#[test]
fn legacy_bootstrap_keeps_identity_and_includes_automatic_discovery() {
    let (_temp, state) = fixture();
    let value = call_tool(&state.tools, "history_session_bootstrap", &json!({
        "session_key":"auto-skill-test", "initial_user_input":"检查接口性能"
    }));
    assert_eq!(value["ok"], true, "{value}");
    assert_eq!(value["session_key"], "auto-skill-test"); assert_eq!(value["initial_input_captured"], true);
    assert_eq!(value["skill_discovery"]["available_skills"][0]["name"], "ui-check");
}
#[test]
fn ordinary_reads_and_task_polls_do_not_repeat_catalogs() {
    let (_temp, state) = fixture(); fs::write(state.tools.workspace.root().join("sample.txt"), "hello").unwrap();
    let file = call_tool(&state.tools, "read_file", &json!({"path":"sample.txt"}));
    assert_eq!(file["ok"], true); assert!(file.get("skill_discovery").is_none());
    let task = call_tool(&state.tools, "task_open", &json!({"goal":"check", "request_id":"auto-test-poll"}));
    let status = call_tool(&state.tools, "task_status", &json!({"task_id":task["task_id"]}));
    assert!(status.get("skill_discovery").is_none());
    let info = call_tool(&state.tools, "server_info", &json!({}));
    assert_eq!(info["skill_discovery"]["available_count"], 1);
}
#[test]
fn manual_only_skills_are_hidden_from_automatic_but_remain_explicitly_usable() {
    let (_temp, state) = fixture(); skill(state.tools.workspace.root(), "manual-publish", true);
    let init = rpc(&state, "initialize");
    assert!(!init["result"]["instructions"].as_str().unwrap().contains("manual-publish"));
    let auto = call_tool(&state.tools, "list_skills", &json!({"automatic_only":true}));
    assert_eq!(auto["ok"], true, "{auto}"); assert_eq!(auto["matches"], 1);
    let manual = call_tool(&state.tools, "invoke_skill", &json!({"query":"$manual-publish"}));
    assert_eq!(manual["ok"], true); assert!(manual["content"].as_str().unwrap().contains("BODY_ONLY_ON_ACTIVATION"));
    assert_eq!(manual["scripts_executed"], false);
}
#[test]
fn disabled_or_empty_discovery_does_not_break_task_creation() {
    let temp = tempfile::tempdir().unwrap(); let root = temp.path().join("workspace"); fs::create_dir(&root).unwrap();
    let mut ctx = ToolContext::for_test(root, temp.path().join("state")).unwrap();
    let empty = call_tool(&ctx, "task_open", &json!({"goal":"no skills", "request_id":"auto-test-empty"}));
    assert_eq!(empty["ok"], true); assert_eq!(empty["skill_discovery"]["available_count"], 0);
    ctx.skills.enabled = false; let disabled = call_tool(&ctx, "server_info", &json!({}));
    assert_eq!(disabled["ok"], true); assert_eq!(disabled["skill_discovery"]["status"], "disabled");
}

#[test]
fn repeated_tool_catalog_is_stable_after_cache_warmup() {
    let (_temp,state)=fixture();
    assert_eq!(rpc(&state,"tools/list"),rpc(&state,"tools/list"));
}
#[test]
fn large_catalog_is_bounded_and_complete_metadata_can_be_paged() {
    let (_temp,state)=fixture();
    for n in 0..70 {
        let folder=format!("skill-{n:03}");skill(state.tools.workspace.root(),&folder,false);
        let path=state.tools.workspace.root().join(".agents/skills").join(folder).join("SKILL.md");
        let text=fs::read_to_string(&path).unwrap().replace("页面测试和接口排查", &"中文测试🧪".repeat(400));
        fs::write(path,text).unwrap();
    }
    let snapshot=super::skill_discovery::snapshot(&state.tools);
    assert_eq!(snapshot["available_count"],71);assert_eq!(snapshot["partial"],true);
    assert!(snapshot["shown_count"].as_u64().unwrap()<=32);
    assert!(snapshot.to_string().len()<12*1024,"metadata budget exceeded");
    assert!(!snapshot.to_string().contains("BODY_ONLY_ON_ACTIVATION"));
    let mut args=json!({"automatic_only":true,"limit":20});let mut ids=std::collections::BTreeSet::new();
    for _ in 0..80 {
        let page=call_tool(&state.tools,"list_skills",&args);assert_eq!(page["ok"],true,"{page}");
        for item in page["skills"].as_array().unwrap(){assert!(ids.insert(item["skill_id"].as_str().unwrap().to_string()));}
        if page["next_cursor"].is_null(){break;}args["cursor"]=page["next_cursor"].clone();
    }
    assert_eq!(ids.len(),71);
}
#[test]
fn added_removed_and_manual_only_flag_changes_refresh_discovery() {
    let (_temp,state)=fixture();let first=super::skill_discovery::snapshot(&state.tools);
    skill(state.tools.workspace.root(),"new-repair",false);
    let next=super::skill_discovery::snapshot(&state.tools);assert_eq!(next["available_count"],2);
    assert_ne!(first["catalog_revision"],next["catalog_revision"]);
    skill(state.tools.workspace.root(),"new-repair",true);
    let manual=super::skill_discovery::snapshot(&state.tools);assert_eq!(manual["available_count"],1);
    assert!(!manual.to_string().contains("new-repair"));
    fs::remove_file(state.tools.workspace.root().join(".agents/skills/ui-check/SKILL.md")).unwrap();
    assert_eq!(super::skill_discovery::snapshot(&state.tools)["available_count"],0);
}
#[test]
fn automatic_filter_is_boolean_and_part_of_cursor_scope() {
    let (_temp,state)=fixture();skill(state.tools.workspace.root(),"manual-only",true);
    let list=call_tool(&state.tools,"list_skills",&json!({"limit":1}));
    assert!(list["next_cursor"].is_string());
    let mismatch=call_tool(&state.tools,"list_skills",&json!({"automatic_only":true,"cursor":list["next_cursor"]}));
    assert_eq!(mismatch["error"]["code"],"STALE_SKILL_CURSOR");
    let invalid=call_tool(&state.tools,"list_skills",&json!({"automatic_only":"true"}));
    assert_eq!(invalid["error"]["code"],"INVALID_SKILL_ARGUMENT");
}
#[test]
fn rejected_task_does_not_disclose_catalog_or_turn_into_success() {
    let (_temp,state)=fixture();
    let value=call_tool(&state.tools,"task_open",&json!({"goal":"missing id"}));
    assert_eq!(value["ok"],false);assert!(value.get("skill_discovery").is_none());
}
#[test]
fn eight_conversations_receive_independent_identical_catalogs() {
    let (_temp,state)=fixture();let expected=rpc(&state,"tools/list");
    let barrier=Arc::new(std::sync::Barrier::new(8));
    let threads:Vec<_>=(0..8).map(|_|{let state=state.clone();let barrier=barrier.clone();
        std::thread::spawn(move||{barrier.wait();rpc(&state,"tools/list")})}).collect();
    for thread in threads{assert_eq!(thread.join().unwrap(),expected);}
}
#[test]
fn malformed_skill_does_not_break_task_or_leak_its_contents() {
    let (_temp,state)=fixture();
    fs::write(state.tools.workspace.root().join(".agents/skills/ui-check/SKILL.md"),"---\nname: [PRIVATE_BAD_VALUE\n---\n").unwrap();
    let value=call_tool(&state.tools,"task_open",&json!({"goal":"ordinary work","request_id":"bad-skill-test"}));
    assert_eq!(value["ok"],true);assert_eq!(value["skill_discovery"]["partial"],true);
    assert_eq!(value["skill_discovery"]["available_count"],0);
    assert!(!value.to_string().contains("PRIVATE_BAD_VALUE"));
}
