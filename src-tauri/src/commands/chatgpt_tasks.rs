//! User-initiated desktop command only; intentionally absent from MCP tool registration.
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DispatchRequest {
    action: String, client: String, task_id: String, request_id: String,
    window_ref: String, model: String, effort: String, prompt: String,
}

#[tauri::command]
pub async fn chatgpt_task_dispatch(request: DispatchRequest) -> AppResult<serde_json::Value> {
    validate(&request)?;
    #[cfg(not(target_os = "windows"))]
    { let _ = request; Err(AppError::Message("客户端派发仅支持 Windows；不会控制 Mac 或从 Codex 转调。".into())) }
    #[cfg(target_os = "windows")]
    { windows_dispatch(request).await }
}

fn validate(r: &DispatchRequest) -> AppResult<()> {
    if !["inspect", "stage", "send"].contains(&r.action.as_str()) || !["desktop", "web"].contains(&r.client.as_str()) {
        return Err(AppError::Message("invalid dispatch action/client".into()));
    }
    for s in [&r.task_id, &r.request_id] {
        if s.is_empty() || s.len() > 80 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err(AppError::Message("invalid task/request id".into()));
        }
    }
    if r.prompt.is_empty() || r.prompt.len() > 100_000 || r.model.is_empty() || r.effort.is_empty() || r.model.len()>100 || r.effort.len()>100 {
        return Err(AppError::Message("invalid prompt/model/effort".into()));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
async fn windows_dispatch(r: DispatchRequest) -> AppResult<serde_json::Value> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use std::process::Stdio;
    use tokio::io::AsyncReadExt;
    if crate::platform::platform().app_config_dir()?.join("chatgpt-task-queue/auto-enabled").exists() { return Err(AppError::Message("独立调度器已启用，不能同时手动派发。请先停止调度器并核对原请求。".into())); }
    let home = crate::platform::platform().app_config_dir()?.join("chatgpt-task-dispatch");
    std::fs::create_dir_all(&home)?;
    // Exclusive process lock prevents two windows from typing into the same client concurrently.
    let lock_path = home.join("dispatch.lock");
    let lock = std::fs::OpenOptions::new().write(true).create_new(true).open(&lock_path)
        .map_err(|_| AppError::Message("另一个派发操作未结束。若应用崩溃，请先核原对话，再人工处理 dispatch.lock；不会自动重放。".into()))?;
    struct Lock(std::path::PathBuf, std::fs::File);
    impl Drop for Lock { fn drop(&mut self) { let _ = self.1.sync_all(); let _ = std::fs::remove_file(&self.0); } }
    let _lock = Lock(lock_path, lock);
    let binding = format!("{:x}", Sha256::digest(serde_json::to_vec(&serde_json::json!({"client":r.client,"task":r.task_id,"window":r.window_ref,"model":r.model,"effort":r.effort,"prompt":r.prompt}))?));
    let stage = home.join(format!("{}.prepared",r.request_id));
    let sent = home.join(format!("{}.send-attempt",r.request_id));
    if sent.exists() { return Err(AppError::Message("此请求已有发送尝试。请查询原对话，禁止重发。".into())); }
    if r.action == "send" {
        if std::fs::read_to_string(&stage).unwrap_or_default() != binding {
            return Err(AppError::Message("填入回执与当前内容不匹配，请重新核对；没有发送。".into()));
        }
        // Persist UNKNOWN before any send: a crash/timeout never becomes retryable.
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&sent)?;
        f.write_all(b"UNKNOWN: query original conversation; do not replay")?; f.sync_all()?;
    }
    let script = home.join("chatgpt-dispatch.ps1");
    std::fs::write(&script, include_str!("../../scripts/chatgpt-dispatch.ps1"))?;
    let input = home.join(format!("{}.input.json",r.request_id));
    std::fs::write(&input, serde_json::to_vec(&r)?)?;
    let ps = std::env::var_os("SystemRoot").map(std::path::PathBuf::from)
        .ok_or_else(|| AppError::Message("Windows SystemRoot missing".into()))?
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child = tokio::process::Command::new(ps).args(["-NoProfile","-NonInteractive","-ExecutionPolicy","Bypass","-File"])
        .arg(&script).arg("-RequestPath").arg(&input)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true).spawn()?;
    let output = child.stdout.take().ok_or_else(|| AppError::Message("missing adapter stdout".into()))?;
    let mut bytes = Vec::new();
    let result = tokio::time::timeout(std::time::Duration::from_secs(45), async {
        output.take(64*1024).read_to_end(&mut bytes).await?;
        child.wait().await
    }).await;
    let _ = std::fs::remove_file(&input);
    match result {
        Ok(Ok(status)) if status.success() => {},
        _ => return Err(AppError::Message("派发超时或失败；发送尝试保持 UNKNOWN，先查询原对话。".into())),
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if r.action == "stage" && value["status"] == "prepared" {
        let mut bound = r.clone(); bound.window_ref = value["windowRef"].as_str().unwrap_or("").into();
        let binding = format!("{:x}", Sha256::digest(serde_json::to_vec(&serde_json::json!({"client":bound.client,"task":bound.task_id,"window":bound.window_ref,"model":bound.model,"effort":bound.effort,"prompt":bound.prompt}))?));
        std::fs::write(&stage,binding)?;
    }
    if r.action == "send" { std::fs::write(&sent,serde_json::to_vec(&value)?)?; }
    value["requestId"] = serde_json::json!(r.request_id);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_dispatch_parameters_before_any_process() {
        let mut r=DispatchRequest{action:"inspect".into(),client:"desktop".into(),task_id:"T01".into(),request_id:"request-1".into(),window_ref:String::new(),model:"Pro".into(),effort:"深入思考".into(),prompt:"task".into()};
        assert!(validate(&r).is_ok()); r.request_id="../escape".into();assert!(validate(&r).is_err());
        r.request_id="request-1".into();r.client="mac".into();assert!(validate(&r).is_err());
    }
}
