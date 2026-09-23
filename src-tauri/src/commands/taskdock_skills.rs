use std::{collections::BTreeMap,path::PathBuf,sync::{LazyLock,Mutex}};
use tauri::State;
use serde_json::{json,Value};
use crate::{app_state::AppState,error::AppResult,tools::skill_catalog::Catalog};
use super::taskdock::{failure,profile,store_for};

// Catalog clones share metadata caches only for the same canonical repository.
static CATALOGS:LazyLock<Mutex<BTreeMap<PathBuf,Catalog>>>=LazyLock::new(||Mutex::new(BTreeMap::new()));
fn catalog(path:PathBuf)->Catalog {
    let mut cache=CATALOGS.lock().unwrap_or_else(|p|p.into_inner());
    if let Some(c)=cache.get(&path) {return c.clone();}
    if cache.len()>=16 {if let Some(key)=cache.keys().next().cloned(){cache.remove(&key);}}
    let value=Catalog::production(path.clone());cache.insert(path,value.clone());value
}

#[tauri::command]
pub async fn taskdock_skills(state:State<'_,AppState>,workspace_id:String,query:Option<String>,cursor:Option<String>)->AppResult<Value> {
    let p=profile(&state,&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move||{
        let s=store_for(&p)?;let c=catalog(s.workspace.clone());
        let before=s.skill_preferences().map_err(failure)?;
        let mut result=c.list(&json!({"query":query.unwrap_or_default(),"cursor":cursor,"limit":20})).map_err(failure)?;
        let after=s.skill_preferences().map_err(failure)?;
        if before.revision!=after.revision {return Err(failure("STALE_SKILL_PREFERENCES：偏好已变化，请刷新目录"));}
        result["preferences_revision"]=json!(after.revision);
        result["tools"]=json!(crate::tools::registry::list_tools_for_profile(&p.runtime.tool_profile));
        result["workspace_id"]=json!(p.id);Ok(result)
    }).await.map_err(failure)?
}

#[tauri::command]
pub async fn taskdock_skill_read(state:State<'_,AppState>,workspace_id:String,skill_id:String,offset:Option<usize>,expected_sha256:Option<String>)->AppResult<Value> {
    let p=profile(&state,&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move||{
        let s=store_for(&p)?;
        catalog(s.workspace.clone()).read(&json!({"skill_id":skill_id,"offset":offset.unwrap_or(0),"expected_sha256":expected_sha256,"max_bytes":16384})).map_err(failure)
    }).await.map_err(failure)?
}

#[tauri::command]
pub async fn taskdock_skill_preference(state:State<'_,AppState>,workspace_id:String,skill_id:String,enabled:bool,expected_revision:i64)->AppResult<Value> {
    let p=profile(&state,&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move||{
        let s=store_for(&p)?;let scan=catalog(s.workspace.clone()).scan().map_err(failure)?;
        let skill=scan.skills.iter().find(|item|item.id==skill_id).ok_or_else(||failure("技能不存在，请刷新目录"))?;
        if enabled&&skill.source_manual_only {return Err(failure("技能源文件禁止自动调用，项目偏好不能覆盖此限制"));}
        let result=s.set_skill_preference(&skill_id,enabled,expected_revision).map_err(failure)?;
        Ok(json!({"saved":true,"preferences_revision":result.revision,"skill_id":skill_id,"enabled":enabled,"execution_permissions_changed":false,"services_restarted":false}))
    }).await.map_err(failure)?
}
