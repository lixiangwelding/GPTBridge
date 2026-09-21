//! Regression tests use temporary skill libraries and the real tool dispatcher.
use std::{fs,path::{Path,PathBuf},sync::Arc};
use serde_json::{json,Value};
use super::{call_tool,ToolContext,skill_catalog::Catalog};

fn write_skill(root:&Path,folder:&str,name:&str,description:&str)->PathBuf{
    let dir=root.join(folder);fs::create_dir_all(&dir).unwrap();
    let path=dir.join("SKILL.md");
    fs::write(&path,format!("---\nname: {name}\ndescription: {description}\n---\n技能正文\n第二行\n")).unwrap();path
}

#[cfg(unix)]
#[test]
fn harmless_reference_name_cannot_alias_a_credential_file(){
    let (_t,ctx,_)=fixture();let root=ctx.workspace.root().join(".agents/skills/repair");
    fs::write(root.join("credentials.json"),"SYNTHETIC_PRIVATE_CONTENT").unwrap();
    std::os::unix::fs::symlink(root.join("credentials.json"),root.join("guide.md")).unwrap();
    let result=call_tool(&ctx,"read_skill",&json!({"skill_id":id(&ctx),"file":"guide.md"}));
    assert_eq!(result["error"]["code"],"SKILL_PATH_DENIED","{result}");
    assert!(!result.to_string().contains("SYNTHETIC_PRIVATE_CONTENT"));
}

#[test]
fn ordinary_markdown_emphasis_inside_description_is_not_an_alias(){
    let meta=super::skill_io::metadata("---\nname: sample\ndescription: \"Read *files* and !notes\"\n---\nbody").unwrap();
    assert_eq!(meta.description,"Read *files* and !notes");
}

#[test]
fn source_reference_is_consistent_between_search_and_read(){
    let (_t,ctx,_)=fixture();let list=search(&ctx,"$repair");let result=call_tool(&ctx,"read_skill",&json!({"skill_id":id(&ctx)}));
    assert_eq!(list["skills"][0]["source_ref"],result["source_ref"]);
}
fn fixture()->(tempfile::TempDir,ToolContext,PathBuf){
    let tmp=tempfile::tempdir().unwrap();let workspace=tmp.path().join("workspace");let global=tmp.path().join("globals");
    fs::create_dir_all(&workspace).unwrap();fs::create_dir_all(&global).unwrap();
    let mut ctx=ToolContext::for_test(workspace.clone(),tmp.path().join("state")).unwrap();
    ctx.skills=Catalog::new(ctx.workspace.root().to_path_buf(),vec![global.clone()]);
    write_skill(&workspace.join(".agents/skills"),"repair","修复技能","用于排查和修复接口");
    (tmp,ctx,global)
}
fn search(ctx:&ToolContext,q:&str)->Value{call_tool(ctx,"search_skills",&json!({"query":q}))}
fn id(ctx:&ToolContext)->String{search(ctx,"$修复技能")["skills"][0]["skill_id"].as_str().unwrap().into()}

#[test]
fn chinese_dollar_search_and_folder_alias(){
    let (_t,ctx,_)=fixture();assert_eq!(search(&ctx,"$修复技能")["matches"],1);
    assert_eq!(search(&ctx,"$repair")["matches"],1);assert_eq!(search(&ctx,"排查")["matches"],1);
    assert_eq!(search(&ctx,"$")["matches"],1);
}
#[test]
fn metadata_list_omits_full_instruction_body(){
    let (_t,ctx,_)=fixture();let v=search(&ctx,"");assert!(!v.to_string().contains("技能正文"));assert_eq!(v["native_dollar_picker"],false);
}
#[test]
fn invoke_returns_instructions_without_executing(){
    let (_t,ctx,_)=fixture();let v=call_tool(&ctx,"invoke_skill",&json!({"query":"$修复技能"}));
    assert_eq!(v["ok"],true,"{v}");assert!(v["content"].as_str().unwrap().contains("技能正文"));assert_eq!(v["scripts_executed"],false);
}
#[test]
fn globals_and_legacy_project_directory_are_discovered(){
    let (_t,ctx,g)=fixture();write_skill(&g,"global","global-skill","shared");
    write_skill(&ctx.workspace.root().join(".codex/skills"),"legacy","legacy-skill","legacy");
    assert_eq!(search(&ctx,"global")["matches"],1);assert_eq!(search(&ctx,"legacy")["matches"],1);
}
#[test]
fn duplicate_names_require_explicit_id(){
    let (_t,ctx,g)=fixture();write_skill(&g,"global","修复技能","另一实现");
    assert_eq!(call_tool(&ctx,"invoke_skill",&json!({"query":"$修复技能"}))["error"]["code"],"AMBIGUOUS_SKILL");
    assert_eq!(call_tool(&ctx,"invoke_skill",&json!({"skill_id":id(&ctx)}))["ok"],true);
}
#[test]
fn fuzzy_match_is_search_only_not_implicit_invocation(){
    let (_t,ctx,_)=fixture();assert_eq!(search(&ctx,"$修复")["matches"],1);
    assert_eq!(call_tool(&ctx,"invoke_skill",&json!({"query":"$修复"}))["error"]["code"],"SKILL_NOT_FOUND");
}
#[test]
fn same_name_in_other_repository_has_no_access(){
    let (_t,a,_)=fixture();let (_t2,b,_)=fixture();
    assert_ne!(id(&a),id(&b));assert_eq!(call_tool(&b,"read_skill",&json!({"skill_id":id(&a)}))["error"]["code"],"SKILL_NOT_FOUND");
}
#[test]
fn references_have_exact_hash_and_lines(){
    let (_t,ctx,_)=fixture();let p=ctx.workspace.root().join(".agents/skills/repair/references/guide.md");fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p,"第一行\n第二行\n").unwrap();let v=call_tool(&ctx,"read_skill",&json!({"skill_id":id(&ctx),"file":"references/guide.md"}));
    assert_eq!(v["ok"],true,"{v}");assert_eq!(v["sha256"],coding_tools_personal_runtime::digest(fs::read(&p).unwrap()));
    assert_eq!(v["start_line"],1);assert_eq!(v["end_line"],2);assert_eq!(v["citation_kind"],"plain_source_reference_not_native_filecite");
}
#[test]
fn shared_reference_stays_inside_approved_library(){
    let (_t,ctx,_)=fixture();let dir=ctx.workspace.root().join(".agents/skills/shared");fs::create_dir_all(&dir).unwrap();fs::write(dir.join("workflow.md"),"shared instructions").unwrap();
    assert_eq!(call_tool(&ctx,"read_skill",&json!({"skill_id":id(&ctx),"file":"../shared/workflow.md"}))["ok"],true);
}
#[test]
fn outside_reference_and_secret_names_denied(){
    let (_t,ctx,_)=fixture();for name in ["/etc/passwd","../../../../outside.md","C:\\data.txt",".env","credentials.json","token.json"] {
        let v=call_tool(&ctx,"read_skill",&json!({"skill_id":id(&ctx),"file":name}));assert_eq!(v["ok"],false,"{name} {v}");
    }
}
#[test]
fn scripts_are_only_read_and_never_run(){
    let (_t,ctx,_)=fixture();let p=ctx.workspace.root().join(".agents/skills/repair/scripts/danger.sh");fs::create_dir_all(p.parent().unwrap()).unwrap();fs::write(&p,"touch must-not-exist\n").unwrap();
    assert_eq!(call_tool(&ctx,"read_skill",&json!({"skill_id":id(&ctx),"file":"scripts/danger.sh"}))["ok"],true);
    assert!(!ctx.workspace.root().join("must-not-exist").exists());
}
#[test]
fn utf8_pagination_preserves_full_bytes(){
    let (_t,ctx,_)=fixture();let ident=id(&ctx);let mut args=json!({"skill_id":ident,"max_bytes":7});let mut text=String::new();
    loop{let v=call_tool(&ctx,"read_skill",&args);assert_eq!(v["ok"],true,"{v}");text.push_str(v["content"].as_str().unwrap());
        if v["next_offset"].is_null(){break;}args["offset"]=v["next_offset"].clone();args["expected_sha256"]=v["sha256"].clone();}
    assert_eq!(text,fs::read_to_string(ctx.workspace.root().join(".agents/skills/repair/SKILL.md")).unwrap());
}
#[test]
fn stale_hash_and_missing_revision_are_rejected(){
    let (_t,ctx,_)=fixture();let ident=id(&ctx);
    assert_eq!(call_tool(&ctx,"read_skill",&json!({"skill_id":ident,"offset":4}))["error"]["code"],"SKILL_REVISION_REQUIRED");
    assert_eq!(call_tool(&ctx,"read_skill",&json!({"skill_id":ident,"expected_sha256":"0".repeat(64)}))["error"]["code"],"SKILL_SOURCE_CHANGED");
}
#[test]
fn new_edited_and_removed_skills_refresh_without_restart(){
    let (_t,ctx,g)=fixture();assert_eq!(search(&ctx,"")["matches"],1);let p=write_skill(&g,"new","new-skill","new");
    assert_eq!(search(&ctx,"new")["matches"],1);fs::remove_file(p).unwrap();assert_eq!(search(&ctx,"new")["matches"],0);
}
#[test]
fn cursor_scope_and_revision_are_checked(){
    let (_t,ctx,g)=fixture();write_skill(&g,"b","b-skill","test");
    let v=call_tool(&ctx,"list_skills",&json!({"limit":1}));let cursor=v["next_cursor"].clone();assert!(cursor.is_string());
    assert_eq!(call_tool(&ctx,"list_skills",&json!({"cursor":cursor,"limit":1}))["ok"],true);
    write_skill(&g,"c","c-skill","test");assert_eq!(call_tool(&ctx,"list_skills",&json!({"cursor":cursor}))["error"]["code"],"STALE_SKILL_CURSOR");
}
#[test]
fn no_remote_root_override_or_execution_fields(){
    let (_t,ctx,_)=fixture();for args in [json!({"root":"/"}),json!({"query":"$repair","cmd":"echo bad"}),json!({"limit":true})]{assert_eq!(call_tool(&ctx,"list_skills",&args)["ok"],false);}
}
#[test]
fn invalid_yaml_is_skipped_without_leaking_its_text(){
    let (_t,ctx,g)=fixture();let p=write_skill(&g,"invalid","broken","bad");fs::write(&p,"---\nname: [BAD_PRIVATE_VALUE\n---\n").unwrap();
    let v=search(&ctx,"");assert_eq!(v["matches"],1);assert_eq!(v["skipped_entries"],1);assert!(!v.to_string().contains("BAD_PRIVATE_VALUE"));
}
#[test]
fn folded_yaml_and_bom_are_supported(){
    let (_t,ctx,g)=fixture();let p=write_skill(&g,"folded","folded","test");fs::write(p,"\u{feff}---\nname: folded\ndescription: >-\n  multi line\n  description\n---\nbody").unwrap();
    assert_eq!(search(&ctx,"folded")["skills"][0]["description"],"multi line description");
}
#[test]
fn aliases_and_duplicate_required_metadata_are_rejected(){
    for text in ["---\nname: &a hello\ndescription: *a\n---\n", "---\nname: one\nname: two\ndescription: text\n---\n"] {
        assert!(super::skill_io::metadata(text).is_err());
    }
}
#[test]
fn disabled_skill_bridge_is_honest(){
    let (_t,mut ctx,_)=fixture();ctx.skills.enabled=false;
    assert_eq!(search(&ctx,"")["error"]["code"],"SKILLS_DISABLED");assert_eq!(call_tool(&ctx,"server_info",&json!({}))["skill_bridge"]["enabled"],false);
}
#[test]
fn catalog_schema_metadata_and_readonly_profile_agree(){
    let (_t,ctx,_)=fixture();for profile in ["core","full","read-only"]{
        let tools=super::registry::list_tools_for_profile(profile);
        for name in super::skills::TOOLS {let t=tools.iter().find(|t|t["name"]==*name).unwrap();assert_eq!(t["annotations"]["readOnlyHint"],true);assert_eq!(t["inputSchema"]["additionalProperties"],false);}
    }
    assert_eq!(call_tool(&ctx,"server_info",&json!({}))["skill_bridge"]["executes_scripts"],false);
}
#[test]
fn concurrent_reads_do_not_change_the_global_workspace(){
    let (_t,ctx,_)=fixture();let ctx=Arc::new(ctx);let mut threads=Vec::new();for _ in 0..8{let ctx=ctx.clone();threads.push(std::thread::spawn(move||search(&ctx,"$repair")));}
    for thread in threads{assert_eq!(thread.join().unwrap()["matches"],1);}
}
#[cfg(unix)]
#[test]
fn symlink_to_unapproved_library_is_not_followed(){
    let (_t,ctx,_)=fixture();let outside=tempfile::tempdir().unwrap();let p=write_skill(outside.path(),"outside","outside-skill","secret");
    std::os::unix::fs::symlink(p.parent().unwrap(),ctx.workspace.root().join(".agents/skills/linked")).unwrap();
    assert_eq!(search(&ctx,"outside")["matches"],0);
}
#[cfg(unix)]
#[test]
fn symlink_to_approved_global_skill_is_deduplicated(){
    let (_t,ctx,g)=fixture();let p=write_skill(&g,"global","global-skill","shared");
    std::os::unix::fs::symlink(p.parent().unwrap(),ctx.workspace.root().join(".agents/skills/linked")).unwrap();
    assert_eq!(search(&ctx,"global")["matches"],1);
}
#[cfg(unix)]
#[test]
fn reference_symlink_escape_is_rejected(){
    let (_t,ctx,_)=fixture();let outside=tempfile::NamedTempFile::new().unwrap();
    std::os::unix::fs::symlink(outside.path(),ctx.workspace.root().join(".agents/skills/repair/escape.md")).unwrap();
    assert_eq!(call_tool(&ctx,"read_skill",&json!({"skill_id":id(&ctx),"file":"escape.md"}))["error"]["code"],"SKILL_PATH_DENIED");
}

#[test]
fn benchmark_multi_conversation_skill_catalog() {
    let (_temp,ctx,global)=fixture();
    for n in 0..200 {
        let path=write_skill(&global,&format!("skill-{n:03}"),&format!("skill-{n:03}"),"synthetic benchmark");
        let mut text=fs::read_to_string(&path).unwrap();text.push_str(&"x".repeat(32768));fs::write(path,text).unwrap();
    }
    ctx.skills.scan().unwrap();
    let catalog=Arc::new(ctx.skills);let barrier=Arc::new(std::sync::Barrier::new(8));let start=std::time::Instant::now();
    let threads:Vec<_>=(0..8).map(|_|{
        let catalog=catalog.clone();let barrier=barrier.clone();
        std::thread::spawn(move||{barrier.wait();(0..3).map(|_|{
            let start=std::time::Instant::now();assert_eq!(catalog.scan().unwrap().skills.len(),201);
            start.elapsed().as_secs_f64()*1000.0
        }).collect::<Vec<_>>()})
    }).collect();
    let mut latency:Vec<_>=threads.into_iter().flat_map(|t|t.join().unwrap()).collect();latency.sort_by(f64::total_cmp);
    println!("PERF_SKILL_CATALOG {}",json!({"conversations":8,"requests":latency.len(),"skills":201,
        "wall_ms":start.elapsed().as_secs_f64()*1000.0,"p50_ms":latency[12],"p95_ms":latency[22]}));
}


#[cfg(unix)]
#[test]
fn unchanged_skill_metadata_is_reused_across_catalog_clones() {
    let (_temp,ctx,_)=fixture();
    let first=ctx.skills.scan().unwrap();assert_eq!(first.metadata_reads,1);
    let clone=ctx.skills.clone();let second=clone.scan().unwrap();
    assert_eq!(second.metadata_reads,0);assert_eq!(second.metadata_cache_hits,1);
    assert_eq!(first.skills[0].sha,second.skills[0].sha);
}

#[cfg(unix)]
#[test]
fn cached_metadata_detects_same_size_edit_even_when_mtime_is_restored() {
    let (_temp,ctx,_)=fixture();let path=ctx.workspace.root().join(".agents/skills/repair/SKILL.md");
    let first=search(&ctx,"");let before=fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let old=fs::read_to_string(&path).unwrap();let new=old.replace("修复接口","修复路径");assert_eq!(old.len(),new.len());
    fs::write(&path,new).unwrap();
    fs::OpenOptions::new().write(true).open(&path).unwrap().set_times(fs::FileTimes::new().set_modified(before)).unwrap();
    let after=search(&ctx,"路径");assert_eq!(after["matches"],1);
    assert_ne!(first["skills"][0]["sha256"],after["skills"][0]["sha256"]);
    assert_eq!(after["metadata_reads"],1);
    assert_eq!(call_tool(&ctx,"read_skill",&json!({"skill_id":first["skills"][0]["skill_id"],"expected_sha256":first["skills"][0]["sha256"]}))["error"]["code"],"SKILL_SOURCE_CHANGED");
}

#[cfg(unix)]
#[test]
fn cached_metadata_detects_atomic_replacement_and_drops_invalid_yaml() {
    let (_temp,ctx,_)=fixture();let path=ctx.workspace.root().join(".agents/skills/repair/SKILL.md");
    let first=search(&ctx,"");let before=fs::metadata(&path).unwrap().modified().unwrap();
    let replacement=path.with_extension("tmp");
    fs::write(&replacement,fs::read_to_string(&path).unwrap().replace("修复接口","修复路径")).unwrap();
    fs::OpenOptions::new().write(true).open(&replacement).unwrap().set_times(fs::FileTimes::new().set_modified(before)).unwrap();
    fs::rename(replacement,&path).unwrap();
    let second=search(&ctx,"路径");assert_eq!(second["matches"],1);assert_ne!(first["catalog_revision"],second["catalog_revision"]);
    fs::write(&path,"---\nname: [invalid\n---\n").unwrap();assert_eq!(search(&ctx,"")["matches"],0);
}

#[cfg(unix)]
#[test]
fn warmed_catalog_does_not_follow_a_new_unapproved_symlink() {
    let (_temp,ctx,_)=fixture();let ident=id(&ctx);
    let path=ctx.workspace.root().join(".agents/skills/repair/SKILL.md");
    let outside=tempfile::tempdir().unwrap();let target=write_skill(outside.path(),"outside","outside-skill","private");
    fs::remove_file(&path).unwrap();std::os::unix::fs::symlink(target,&path).unwrap();
    assert_eq!(search(&ctx,"")["matches"],0);
    assert_eq!(call_tool(&ctx,"read_skill",&json!({"skill_id":ident}))["error"]["code"],"SKILL_NOT_FOUND");
}
