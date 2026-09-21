//! One authenticated /mcp endpoint with explicit, owner-configured repository routing.
//! No global selected repository, URL supplied by a caller, or implicit filesystem expansion.
use std::{collections::{BTreeMap, BTreeSet}, sync::Arc};
use serde_json::{json, Value};
use crate::{audit::AuditRequestContext, data::DataStore, tools::{ToolContext, Workspace}, workspace::WorkspaceProfile};
use super::{server::{handle_request_with_context, McpState, SharedState}, upstream::UpstreamMcpManager};

pub const MAX_REPOSITORIES: usize = 32;
const ROUTING: &str = "This endpoint serves multiple owner-approved repositories. First call workspace_list. Every other tool requires an explicit workspace_id from that list, even when resuming task_id/job_id. Never infer a repository from the last call, change a global cwd, or use another repository's task/command IDs. Each repository retains its own paths, policy, task state and output. No worktree is needed.";

pub struct Member { pub name: String, pub state: SharedState }
pub struct WorkspaceHub { members: BTreeMap<String, Member>, catalog: Vec<Value>, host: SharedState }

pub fn validate_members(host: &WorkspaceProfile, profiles: &[WorkspaceProfile]) -> Result<(), String> {
    if host.runtime.gateway_workspace_ids.is_empty() { return Ok(()); }
    if !host.auth.bearer_enabled() && !host.auth.oauth_enabled() {
        return Err("多仓库入口必须启用 Bearer 或 OAuth 鉴权".into());
    }
    if host.runtime.gateway_workspace_ids.len() >= MAX_REPOSITORIES {
        return Err(format!("一个入口最多包含 {MAX_REPOSITORIES} 个仓库"));
    }
    let mut ids = BTreeSet::from([host.id.as_str()]);
    for id in &host.runtime.gateway_workspace_ids {
        if !ids.insert(id) { return Err("共享仓库 ID 重复或包含入口自身".into()); }
        let Some(profile) = profiles.iter().find(|p| &p.id == id) else { return Err("共享仓库不存在，请重新选择".into()); };
        if profile.runtime.upstream_mcps.iter().any(|p|p.enabled) {
            return Err("共享入口暂不聚合成员的外部 MCP；请先关闭该成员的外部 MCP 或使用其独立入口".into());
        }
    }
    if host.runtime.upstream_mcps.iter().any(|p|p.enabled) {
        return Err("共享入口暂不聚合外部 MCP，请使用内置工具或独立入口".into());
    }
    Ok(())
}

/// Prevent saved permissions/membership from silently diverging from a live snapshot.
pub fn validate_update(current:&WorkspaceProfile,next:&WorkspaceProfile,profiles:&[WorkspaceProfile],running:&BTreeSet<String>)->Result<(),String> {
    let sensitive = current.path!=next.path || serde_json::to_value(&current.runtime).ok()!=serde_json::to_value(&next.runtime).ok()
        || serde_json::to_value(&current.auth).ok()!=serde_json::to_value(&next.auth).ok();
    if sensitive {
        for host in profiles {
            let is_self=host.id==current.id && (!current.runtime.gateway_workspace_ids.is_empty() || !next.runtime.gateway_workspace_ids.is_empty());
            if running.contains(&host.id) && (is_self || host.runtime.gateway_workspace_ids.contains(&current.id)) {
                return Err("请先手动停止个人版共享入口，再修改成员、路径或权限；不会自动停止其他任务".into());
            }
        }
    }
    let mut updated=profiles.to_vec();
    if let Some(p)=updated.iter_mut().find(|p|p.id==next.id){*p=next.clone();}
    for host in &updated {
        if host.id==next.id || host.runtime.gateway_workspace_ids.contains(&next.id) {validate_members(host,&updated)?;}
    }
    Ok(())
}

impl WorkspaceHub {
    pub fn load(host_id: &str, host: SharedState, members: &[String]) -> Result<Option<Arc<Self>>, String> {
        if members.is_empty() { return Ok(None); }
        let profiles = DataStore::read_file(|data|Ok(data.profiles.clone())).map_err(|e|e.to_string())?;
        let primary = profiles.iter().find(|p|p.id==host_id).ok_or("入口工作区不存在")?;
        // Validate exactly the list passed to this listener, not a newer unrelated GUI edit.
        let mut selected = primary.clone();
        selected.runtime.gateway_workspace_ids = members.to_vec();
        selected.auth = host.tools.auth.clone();
        if !host.upstream.public_tools().is_empty() {return Err("共享入口不能同时聚合外部 MCP".into());}
        validate_members(&selected,&profiles)?;
        let mut states = vec![(host_id.to_string(),Member{name:primary.name.clone(),state:host.clone()})];
        for id in members {
            let p = profiles.iter().find(|p|&p.id==id).ok_or("共享仓库不存在")?;
            let workspace = Workspace::new(p.path.clone().into()).map_err(|e|e.message())?;
            let tools = ToolContext::from_workspace(workspace,p.auth.clone(),
                crate::tools::policy::PolicySettings::from_runtime(&p.runtime),p.runtime.tool_profile.clone(),p.runtime.permission_mode.clone()).with_audit(p.id.clone());
            states.push((id.clone(),Member{name:p.name.clone(),state:Arc::new(McpState{
                tools:Arc::new(tools),upstream:Arc::new(UpstreamMcpManager::empty())})}));
        }
        Ok(Some(Arc::new(Self::new(host,states)?)))
    }

    pub fn new(host: SharedState, entries: Vec<(String, Member)>) -> Result<Self,String> {
        if entries.is_empty() || entries.len()>MAX_REPOSITORIES {return Err("无效的共享仓库数量".into());}
        let mut members = BTreeMap::new();
        let mut roots = BTreeSet::new();
        for (id,member) in entries {
            if id.is_empty() || id.len()>128 || members.contains_key(&id) {return Err("共享仓库 ID 无效或重复".into());}
            if !roots.insert(member.state.tools.workspace.root().to_path_buf()) {return Err("同一个实际仓库不能重复注册".into());}
            members.insert(id,member);
        }
        for member in members.values() {member.state.tools.workspace.restrict_reads_to_root();}
        // The entry's exposed profile is an upper bound; targets check their own profile again.
        let mut catalog = crate::tools::list_tools_for_profile(&host.tools.tool_profile);
        catalog.retain(|tool|tool["name"]!="set_default_cwd");
        for tool in &mut catalog {
            tool["inputSchema"]["properties"]["workspace_id"] = json!({"type":"string","enum":members.keys().collect::<Vec<_>>(),
                "description":"Required repository ID from workspace_list; never a filesystem path"});
            if !tool["inputSchema"]["required"].is_array() {tool["inputSchema"]["required"]=json!([]);}
            tool["inputSchema"]["required"].as_array_mut().unwrap().push(json!("workspace_id"));
        }
        catalog.insert(0,json!({"name":"workspace_list","description":"List only repositories explicitly approved for this endpoint; does not switch any global repository or expose credentials.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false,"idempotentHint":true}}));
        Ok(Self{members,catalog,host})
    }

    pub fn handle(&self, body: &Value, request: &AuditRequestContext) -> Value {
        let id = body.get("id").cloned().unwrap_or(Value::Null);
        match body["method"].as_str() {
            Some("initialize") => {
                let mut result=handle_request_with_context(&self.host,body,request);
                result["result"]["instructions"]=json!(format!("{ROUTING} {} {}",crate::tools::personal::INSTRUCTIONS,crate::tools::skills::INSTRUCTIONS));
                result
            }
            Some("tools/list") => json!({"jsonrpc":"2.0","id":id,"result":{"tools":self.catalog}}),
            Some("tools/call") => self.call(body,request),
            _ => handle_request_with_context(&self.host,body,request),
        }
    }

    fn call(&self, body: &Value, request: &AuditRequestContext) -> Value {
        let id=body.get("id").cloned().unwrap_or(Value::Null);
        let name=body["params"]["name"].as_str().unwrap_or("");
        let empty=json!({});
        let Some(args)=body["params"].get("arguments").unwrap_or(&empty).as_object() else {return failure(id,"INVALID_ARGUMENT","arguments must be an object");};
        if name=="workspace_list" {
            if !args.is_empty() {return failure(id,"INVALID_ARGUMENT","workspace_list takes no arguments");}
            let rows:Vec<_>=self.members.iter().map(|(id,m)|json!({"workspace_id":id,"name":m.name,
                "path":m.state.tools.workspace.root_display(),"tool_profile":m.state.tools.tool_profile,
                "permission_mode":m.state.tools.permission_mode})).collect();
            return tool_result(id,json!({"ok":true,"workspaces":rows,"switches_global_workspace":false,"endpoint":"/mcp"}));
        }
        if !self.catalog.iter().any(|t|t["name"]==name) {return failure(id,"UNKNOWN_TOOL","tool is not exposed by this gateway");}
        let Some(repo)=args.get("workspace_id").and_then(Value::as_str) else {return failure(id,"WORKSPACE_REQUIRED","pass workspace_id from workspace_list; no default repository is selected");};
        let Some(member)=self.members.get(repo) else {return failure(id,"WORKSPACE_NOT_ALLOWED","repository is not approved for this endpoint");};
        if let Some(root)=args.get("workspace_root") {
            if root.as_str().and_then(|p|std::path::Path::new(p).canonicalize().ok()).as_deref()!=Some(member.state.tools.workspace.root()) {
                return failure(id,"WORKSPACE_MISMATCH","workspace_root cannot override the selected repository");
            }
        }
        let mut forwarded=body.clone();
        forwarded["params"]["arguments"].as_object_mut().unwrap().remove("workspace_id");
        let mut result=handle_request_with_context(&member.state,&forwarded,request);
        if let Some(structured)=result.get_mut("result").and_then(|r|r.get_mut("structuredContent")) {
            if structured.is_object() {
                structured["workspace_id"]=json!(repo);
                if name=="server_info" {
                    structured["gateway_enabled"]=json!(true);
                    structured["tool_count"]=json!(self.catalog.len());
                    structured["tools"]=json!(self.catalog.iter().map(|t|t["name"].clone()).collect::<Vec<_>>());
                }
            }
        }
        if result["result"]["content"].as_array().is_some_and(|items|items.len()==1 && items[0]["type"]=="text") && result["result"]["structuredContent"].is_object() {
            result["result"]["content"][0]["text"]=json!(result["result"]["structuredContent"].to_string());
        }
        result
    }
}

fn tool_result(id:Value,value:Value)->Value {
    json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":value.to_string()}],"isError":value["ok"]==false,"structuredContent":value}})
}
fn failure(id:Value,code:&str,message:&str)->Value {
    tool_result(id,json!({"ok":false,"error":{"code":code,"message":message,"retryable":false}}))
}
