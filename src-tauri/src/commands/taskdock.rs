//! Narrow desktop API. No generic tool dispatch, command execution or permission bypass.
use std::{collections::{BTreeMap,BTreeSet},path::PathBuf,time::Duration};
use coding_tools_personal_runtime::{Store,now_ms,workbench::{WorkbenchQuery,WorkbenchCursor,TaskRow,DISPLAY_STATES}};
use serde_json::{json,Value};
use rusqlite::OptionalExtension;
use tauri::State;
use crate::{app_state::AppState,error::{AppError,AppResult},workspace::WorkspaceProfile};

pub(super) fn failure(e:impl std::fmt::Display)->AppError {AppError::Message(e.to_string())}

#[tauri::command]
pub fn taskdock_app_info()->Value {
    json!({"name":"GPTBridge","subtitle":"AI 任务工作台","version":env!("CARGO_PKG_VERSION"),
        "legacy_identifier":"com.lixiangwelding.codingtools.personal","protocol_compatible":true,
        "configuration_migrated":false,"backend":"tauri-native"})
}
pub(super) fn profile(state:&AppState,id:&str)->AppResult<WorkspaceProfile> {
    state.with_workspaces(|s|s.get(id).cloned().ok_or_else(||failure("项目不存在，请刷新项目列表")))
}
pub(super) fn store_for(profile:&WorkspaceProfile)->AppResult<Store> {
    let path=PathBuf::from(&profile.path);
    if !path.is_absolute() || !path.is_dir() {return Err(failure("项目目录不可用；未创建虚假空任务库"));}
    let root=crate::harness::Harness::default_root().map_err(failure)?.join("personal-runtime");
    Store::open(&root,&path).map_err(failure)
}

pub(crate) fn snapshot(profiles:Vec<WorkspaceProfile>,query:WorkbenchQuery)->AppResult<Value> {
    let limit=query.validate().map_err(failure)?;
    let mut counts:BTreeMap<String,u64>=DISPLAY_STATES.iter().map(|s|(s.to_string(),0)).collect();
    let mut tasks:Vec<TaskRow>=Vec::new();let mut warnings=Vec::new();let mut seen=BTreeSet::new();
    let mut matched_total=0u64;let mut has_more=false;let mut project_counts=Vec::new();
    for p in profiles {
        let canonical=match PathBuf::from(&p.path).canonicalize() {
            Ok(path) if path.is_dir()=>path,
            _=>{warnings.push(json!({"workspace_id":p.id,"name":p.name,"message":"目录不存在或不可访问"}));continue;}
        };
        if !seen.insert(canonical) {continue;}
        let page=match store_for(&p).and_then(|s|s.workbench_page(&query,&p.id).map_err(failure)) {
            Ok(page)=>page,
            Err(e)=>{warnings.push(json!({"workspace_id":p.id,"name":p.name,"message":e.to_string()}));continue;}
        };
        for (key,value) in &page.counts {*counts.entry(key.clone()).or_default()+=value;}
        project_counts.push(json!({"workspace_id":p.id,"counts":page.counts,"observed_at":page.observed_at}));
        matched_total+=page.matched_total;has_more|=page.next_cursor.is_some();
        for mut row in page.tasks {row.workspace_name=p.name.clone();tasks.push(row);}
    }
    tasks.sort_by(|a,b|b.updated.cmp(&a.updated).then(b.task_id.cmp(&a.task_id)).then(b.workspace_id.cmp(&a.workspace_id)));
    has_more|=tasks.len()>limit;tasks.truncate(limit);
    let cursor=if has_more {tasks.last().map(|t|WorkbenchCursor{updated:t.updated,task_id:t.task_id.clone(),workspace_id:t.workspace_id.clone()})}else{None};
    Ok(json!({"tasks":tasks,"counts":counts,"matched_total":matched_total,"next_cursor":cursor,"observed_at":now_ms(),"warnings":warnings,"partial":!warnings.is_empty(),"projects":project_counts,"source":"persistent_task_store","state_semantics":"stored jobs; active without running jobs is ready"}))
}

#[tauri::command]
pub async fn taskdock_snapshot(state:State<'_,AppState>,workspace_id:Option<String>,query:WorkbenchQuery)->AppResult<Value> {
    let profiles=state.with_workspaces(|s|Ok(s.list().to_vec()))?;
    let profiles=if let Some(id)=workspace_id {vec![profiles.into_iter().find(|p|p.id==id).ok_or_else(||failure("项目不存在"))?]}else{profiles};
    tauri::async_runtime::spawn_blocking(move||snapshot(profiles,query)).await.map_err(failure)?
}

#[tauri::command]
pub async fn taskdock_task(state:State<'_,AppState>,workspace_id:String,task_id:String)->AppResult<Value> {
    let p=profile(&state,&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move||{
        let s=store_for(&p)?;
        let mut detail=crate::tools::personal::task_view(&s,&task_id,&json!({"limit":20,"include_passed":true})).map_err(failure)?;
        detail["workspace_id"]=json!(p.id);detail["workspace_name"]=json!(p.name);
        detail["workspace_path"]=json!(s.workspace);
        detail["handoff"]=json!(format!("使用 GPTBridge 工具继续任务。工作区：{}；task_id：{}。先调用 task_open(task_id)，读取最新任务状态和当前文件，再继续；不要重建任务或恢复旧快照。",s.workspace.display(),task_id));
        Ok(detail)
    }).await.map_err(failure)?
}

#[tauri::command]
pub async fn taskdock_create(state:State<'_,AppState>,workspace_id:String,goal:String,request_id:String)->AppResult<Value> {
    let p=profile(&state,&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move||{
        let s=store_for(&p)?;
        let mut receipt=s.open_task_request(&json!({"goal":goal.trim(),"request_id":request_id,"new_task":true,"raw_user_input":goal})).map_err(failure)?;
        receipt["requires_client"]=json!(true);receipt["display_state"]=json!("ready");receipt["workspace_id"]=json!(p.id);
        Ok(receipt)
    }).await.map_err(failure)?
}

#[tauri::command]
pub async fn taskdock_job_output(state:State<'_,AppState>,workspace_id:String,task_id:String,job_id:String,stream:String,offset:Option<u64>)->AppResult<Value> {
    let p=profile(&state,&workspace_id)?;
    tauri::async_runtime::spawn_blocking(move||{
        let s=store_for(&p)?;
        read_job_output(&s,&task_id,&job_id,&stream,offset.unwrap_or(0))
    }).await.map_err(failure)?
}

pub(crate) fn read_job_output(s:&Store,task:&str,job:&str,stream:&str,offset:u64)->AppResult<Value> {
    coding_tools_personal_runtime::store::id(task).map_err(failure)?;
    coding_tools_personal_runtime::store::id(job).map_err(failure)?;
    let owner:Option<String>=s.conn().map_err(failure)?.query_row("SELECT task_id FROM jobs WHERE id=?1",[job],|r|r.get(0)).optional().map_err(failure)?;
    if owner.as_deref()!=Some(task) {return Err(failure("JOB_TASK_MISMATCH：该作业不属于选中任务"));}
    if !["stdout","stderr"].contains(&stream) {return Err(failure("无效日志流"));}
    s.job_output(job,stream,offset,8192).map_err(failure)
}

#[tauri::command]
pub fn taskdock_add_project(state:State<'_,AppState>,path:String,name:Option<String>)->AppResult<WorkspaceProfile> {
    let candidate=PathBuf::from(path.trim());
    if !candidate.is_absolute() {return Err(failure("请选择绝对目录"));}
    let canonical=candidate.canonicalize().map_err(failure)?;
    if !canonical.is_dir() {return Err(failure("所选路径不是目录"));}
    if name.as_ref().is_some_and(|n|n.trim().is_empty()||n.len()>240) {return Err(failure("项目名称须为1至240字节"));}
    let existing=state.with_workspaces(|s|Ok(s.list().iter().find(|p|PathBuf::from(&p.path).canonicalize().ok().as_ref()==Some(&canonical)).cloned()))?;
    if let Some(p)=existing {return Ok(p);}
    super::workspace::create_workspace(state,canonical.to_string_lossy().into_owned(),name)
}

#[tauri::command]
pub async fn taskdock_connections(state:State<'_,AppState>,workspace_id:String)->AppResult<Value> {
    let p=profile(&state,&workspace_id)?;
    let (mcp,actions)=state.with_runtime(|r|{
        r.refresh_mcp(&p);r.refresh_actions(&p);Ok((r.mcp_status(&p),r.actions_status(&p)))
    })?;
    async fn reachable(port:u16)->bool {
        matches!(tokio::time::timeout(Duration::from_millis(300),tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST,port))).await,Ok(Ok(_)))
    }
    let (mcp_reachable,actions_reachable)=tokio::join!(reachable(p.runtime.local_port),reachable(p.actions.local_port));
    Ok(json!({"workspace_id":p.id,"mcp":mcp,"actions":actions,"mcp_reachable":mcp_reachable,"actions_reachable":actions_reachable,"checked_at":now_ms(),"client_connections":null,"note":"端口可达不是客户端握手证明；非本实例服务不接管、不重启"}))
}
