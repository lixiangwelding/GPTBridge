//! Review regressions use native consumers, not just configuration equality.
use std::{fs, sync::Arc};
use serde_json::json;
use super::{ToolContext, call_tool};

fn context() -> (tempfile::TempDir, Arc<ToolContext>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir(&root).unwrap();
    let ctx = ToolContext::for_test(root, temp.path().join("state")).unwrap();
    (temp, Arc::new(ctx))
}

#[test]
fn imported_missing_upstreams_can_be_loaded_by_the_desktop() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.json");
    let destination = temp.path().join("personal/profiles.json");
    fs::write(&source, json!({"profiles":[{"id":"test","name":"fixture","path":"/fixture",
        "runtime":{},"auth":{},"tunnel":{}}]}).to_string()).unwrap();
    coding_tools_personal_runtime::config::inherit(&source,&destination).unwrap();
    let loaded = serde_json::from_slice::<crate::data::AppData>(&fs::read(destination).unwrap());
    assert!(loaded.is_ok(), "import must be consumable by actual desktop AppData: {loaded:?}");
}

#[test]
fn previously_imported_null_upstreams_are_backward_compatible() {
    let config = serde_json::from_value::<crate::workspace::RuntimeConfig>(json!({"upstream_mcps":null}));
    assert!(config.is_ok(), "repair existing personal imports without touching legacy configuration");
}

#[test]
fn invalid_task_open_does_not_create_a_task_or_poison_the_request() {
    let (_temp,ctx) = context();
    let invalid = json!({"goal":"review","request_id":"open-review","raw_user_input":"x".repeat(40000)});
    assert_eq!(call_tool(&ctx,"task_open",&invalid)["ok"],false);
    let all = call_tool(&ctx,"task_status",&json!({}));
    assert_eq!(all["tasks"].as_array().unwrap().len(),0,"invalid input must be rolled back");
    let valid = call_tool(&ctx,"task_open",&json!({"goal":"review","request_id":"open-review"}));
    assert_eq!(valid["ok"],true,"invalid request must not leave an indeterminate receipt: {valid}");
}

#[test]
fn completed_tasks_cannot_apply_new_patches() {
    let (_temp,ctx) = context();
    let task = call_tool(&ctx,"task_open",&json!({"goal":"done","request_id":"done"}));
    let result = call_tool(&ctx,"task_checkpoint",&json!({"task_id":task["task_id"],"request_id":"complete",
        "expected_revision":0,"state":"completed","checkpoint":{"steps":{"one":{"state":"passed","evidence":["fixture"]}}}}));
    assert_eq!(result["ok"],true);
    let patch = call_tool(&ctx,"apply_patch",&json!({"task_id":task["task_id"],"request_id":"late-write",
        "expected_hashes":{"late.txt":null},"patch":"*** Begin Patch\n*** Add File: late.txt\n+must not write\n*** End Patch"}));
    assert_eq!(patch["error"]["code"],"TASK_COMPLETED","{patch}");
    assert!(!ctx.workspace.root().join("late.txt").exists());
}

#[test]
fn repeated_file_sections_preserve_both_edits() {
    let (_temp,ctx)=context();
    fs::write(ctx.workspace.root().join("shared.txt"),"one\ntwo\n").unwrap();
    let result=super::patch::apply_patch(&ctx,&json!({"patch":"*** Begin Patch\n*** Update File: shared.txt\n@@\n-one\n+ONE\n*** Update File: shared.txt\n@@\n-two\n+TWO\n*** End Patch"})).unwrap();
    assert_eq!(result["ok"],true);
    assert_eq!(fs::read_to_string(ctx.workspace.root().join("shared.txt")).unwrap(),"ONE\nTWO\n");
}

#[test]
fn repeated_creation_request_is_atomic_across_threads() {
    let (_temp,ctx)=context();let mut threads=Vec::new();
    for _ in 0..8 {let ctx=ctx.clone();threads.push(std::thread::spawn(move||call_tool(&ctx,"task_open",&json!({"goal":"parallel","request_id":"one","raw_user_input":"same input"}))));}
    let results:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();
    for result in &results {assert_eq!(result["ok"],true,"{result}");assert_eq!(result["task_id"],results[0]["task_id"]);}
    let task=results[0]["task_id"].as_str().unwrap();
    assert_eq!(ctx.personal.events(task,0,100).unwrap()["events"].as_array().unwrap().len(),1);
}

#[test]
fn conflicting_later_patch_section_rolls_back_the_entire_request() {
    let (_temp,ctx)=context();fs::write(ctx.workspace.root().join("shared.txt"),"one\ntwo\n").unwrap();
    let result=super::patch::apply_patch(&ctx,&json!({"patch":"*** Begin Patch\n*** Update File: shared.txt\n@@\n-one\n+ONE\n*** Update File: shared.txt\n@@\n-one\n+conflict\n*** End Patch"}));
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(ctx.workspace.root().join("shared.txt")).unwrap(),"one\ntwo\n");
}

#[test]
fn add_then_update_in_one_patch_uses_staged_content() {
    let (_temp,ctx)=context();
    let result=super::patch::apply_patch(&ctx,&json!({"patch":"*** Begin Patch\n*** Add File: new.txt\n+one\n*** Update File: new.txt\n@@\n-one\n+ONE\n*** End Patch"}));
    assert!(result.is_ok());assert_eq!(fs::read_to_string(ctx.workspace.root().join("new.txt")).unwrap(),"ONE\n");
}

#[test]
fn resume_with_supplied_message_really_captures_it() {
    let (_temp,ctx)=context();
    let opened=ctx.personal.open_task_request(&json!({"goal":"resume","_host_session_key":"owner"})).unwrap();
    let resumed=ctx.personal.open_task_request(&json!({"goal":"resume","_host_session_key":"owner","raw_user_input":"继续审查"})).unwrap();
    assert_eq!(resumed["task_id"],opened["task_id"]);
    assert_eq!(resumed["raw_input_captured"],true);
    let events=ctx.personal.events(opened["task_id"].as_str().unwrap(),0,100).unwrap();
    assert_eq!(events["events"].as_array().unwrap().len(),1);
    assert_eq!(events["events"][0]["body"]["text"],"继续审查");
}
