//! Windows owner-controlled scheduler. Codex submits queue files; no MCP dispatch is exposed.
use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct SchedulerRequest {
    action: String,
    #[serde(default)] workspace_id: String,
    #[serde(default)] client: String,
    #[serde(default)] model: String,
    #[serde(default)] effort: String,
    #[serde(default)] web_fallback: bool,
    #[serde(default)] batch: Option<Value>,
}

#[tauri::command]
pub async fn chatgpt_queue_control(state:State<'_,AppState>,request:SchedulerRequest)->AppResult<Value> {
    if !["start","status","pause","resume","stop","submit"].contains(&request.action.as_str()) {
        return Err(AppError::Message("invalid queue action".into()));
    }
    #[cfg(not(target_os="windows"))]
    { let _=(state,request);Err(AppError::Message("独立调度器仅支持 Windows + WSL，不操作 Mac。".into())) }
    #[cfg(target_os="windows")]
    {
        let action=request.action.clone();
        let started=std::time::Instant::now();
        let result=windows_control(&state,request).await;
        // Keep polling quiet; log controls locally without workspace or prompt contents.
        if action!="status" {
            if let Ok(dir)=crate::platform::platform().app_config_dir() {
                use std::io::Write;
                let folder=dir.join("chatgpt-task-queue/logs");
                let _=std::fs::create_dir_all(&folder);
                let path=folder.join("windows-controls.jsonl");
                if std::fs::metadata(&path).map(|m|m.len()>2*1024*1024).unwrap_or(false) {
                    let _=std::fs::remove_file(folder.join("windows-controls.previous.jsonl"));
                    let _=std::fs::rename(&path,folder.join("windows-controls.previous.jsonl"));
                }
                if let Ok(mut file)=std::fs::OpenOptions::new().append(true).create(true).open(path) {
                    let at=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
                    let record=json!({"at":at,"action":action,"ok":result.is_ok(),"durationMs":started.elapsed().as_millis(),"status":result.as_ref().ok().and_then(|v|v.get("status"))});
                    let _=writeln!(file,"{}",record);
                }
            }
        }
        result
    }
}

#[cfg(target_os="windows")]
fn workspace_mapping(path:&str)->AppResult<(String,String)> {
    let normalized=path.replace("\\\\?\\UNC\\","\\\\");
    let parts:Vec<_>=normalized.split('\\').filter(|s|!s.is_empty()).collect();
    if parts.len()<3 || !["wsl.localhost","wsl$"].contains(&parts[0].to_ascii_lowercase().as_str()) || [".",".."].contains(&parts[1]) || !parts[1].bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-' || b==b'_' || b==b'.') || parts[2..].contains(&"..") {
        return Err(AppError::Message("请选择已登记的 Windows WSL 项目；当前自动化使用该项目已有 WSL Python。".into()));
    }
    Ok((parts[1].to_owned(),format!("/{}",parts[2..].join("/"))))
}

#[cfg(target_os="windows")]
fn resume_requires_start(marker_exists:bool,marker_pid:Option<u64>,heartbeat:&Value,stop_requested:bool,now:f64)->AppResult<bool> {
    let status=heartbeat["status"].as_str().unwrap_or("not_started");
    if !marker_exists {
        if ["stopped","not_started"].contains(&status) {return Ok(true);}
        return Err(AppError::Message("调度器所有者标记与心跳不一致；请核对原请求，不重复启动。".into()));
    }
    let updated=heartbeat["updatedAt"].as_f64().unwrap_or(0.0);
    if stop_requested || ["stopped","not_started"].contains(&status) || marker_pid.is_none()
        || marker_pid!=heartbeat["pid"].as_u64() || updated>now+5.0 || now-updated>240.0 {
        return Err(AppError::Message("调度器仍在启动、停止或心跳已失效；未恢复、未启动第二份进程，请查看启动日志。".into()));
    }
    Ok(false)
}

#[cfg(target_os="windows")]
fn worker_log(root:&std::path::Path,name:&str)->AppResult<std::fs::File> {
    let folder=root.join("logs");std::fs::create_dir_all(&folder)?;
    let path=folder.join(name);
    if std::fs::metadata(&path).map(|m|m.len()>=2*1024*1024).unwrap_or(false) {
        let previous=folder.join(format!("{name}.previous"));
        if previous.exists(){std::fs::remove_file(&previous)?;}
        std::fs::rename(&path,previous)?;
    }
    Ok(std::fs::OpenOptions::new().append(true).create(true).open(path)?)
}

#[cfg(all(test,target_os="windows"))]
mod workspace_mapping_tests {
    use super::{workspace_mapping,resume_requires_start,worker_log};
    use serde_json::json;

    #[test]
    fn accepts_installed_ubuntu_dotted_name() {
        let (distro,path)=workspace_mapping(r"\\wsl.localhost\Ubuntu-24.04\home\lixiang\my-project-java\codeVerifyRe0").unwrap();
        assert_eq!(distro,"Ubuntu-24.04");
        assert_eq!(path,"/home/lixiang/my-project-java/codeVerifyRe0");
    }

    #[test]
    fn accepts_wsl_alias_and_extended_unc() {
        for path in [r"\\wsl$\Ubuntu-24.04\home\user",r"\\?\UNC\wsl.localhost\Ubuntu-24.04\home\user"] {
            assert_eq!(workspace_mapping(path).unwrap(),("Ubuntu-24.04".into(),"/home/user".into()));
        }
    }

    #[test]
    fn rejects_foreign_host_traversal_and_invalid_distro() {
        for path in [r"\\other\Ubuntu-24.04\home\user",r"\\wsl.localhost\..\home\user",r"\\wsl.localhost\.\home\user",r"\\wsl.localhost\Ubuntu-24.04\home\..\user",r"\\wsl.localhost\Ubuntu;cmd\home\user",r"C:\home\user"] {
            assert!(workspace_mapping(path).is_err(),"accepted {path}");
        }
    }

    #[test]
    fn resume_restarts_a_stopped_owner_instead_of_only_writing_a_marker() {
        assert!(resume_requires_start(false,None,&json!({"status":"stopped","pid":123,"updatedAt":10}),true,1000.0).unwrap());
        assert!(resume_requires_start(false,None,&json!({}),false,1000.0).unwrap());
        assert!(!resume_requires_start(true,Some(123),&json!({"status":"paused","pid":123,"updatedAt":999}),false,1000.0).unwrap());
        assert!(!resume_requires_start(true,Some(123),&json!({"status":"waiting_client","pid":123,"updatedAt":999}),false,1000.0).unwrap());
    }

    #[test]
    fn resume_never_restarts_uncertain_or_stopping_owners() {
        for (exists,pid,hb,stop) in [
            (true,Some(123),json!({"status":"stopped","pid":123,"updatedAt":999}),false),
            (true,None,json!({}),false),
            (true,Some(123),json!({"status":"paused","pid":456,"updatedAt":999}),false),
            (true,Some(123),json!({"status":"paused","pid":123,"updatedAt":1}),false),
            (true,Some(123),json!({"status":"paused","pid":123,"updatedAt":2000}),false),
            (true,Some(123),json!({"status":"waiting_client","pid":123,"updatedAt":999}),true),
            (false,None,json!({"status":"waiting_receipt","pid":123,"updatedAt":999}),false),
        ] {assert!(resume_requires_start(exists,pid,&hb,stop,1000.0).is_err());}
    }

    #[test]
    fn startup_logs_append_and_rotate_before_a_new_worker() {
        use std::io::Write;
        let dir=tempfile::tempdir().unwrap();
        let mut first=worker_log(dir.path(),"worker-stderr.log").unwrap();
        first.write_all(b"first startup failure\n").unwrap();drop(first);
        let mut next=worker_log(dir.path(),"worker-stderr.log").unwrap();
        next.write_all(b"second startup\n").unwrap();drop(next);
        let path=dir.path().join("logs/worker-stderr.log");
        assert_eq!(std::fs::read_to_string(&path).unwrap(),"first startup failure\nsecond startup\n");
        let large=std::fs::OpenOptions::new().write(true).open(&path).unwrap();large.set_len(2*1024*1024).unwrap();drop(large);
        drop(worker_log(dir.path(),"worker-stderr.log").unwrap());
        assert_eq!(std::fs::metadata(path).unwrap().len(),0);
        assert_eq!(std::fs::metadata(dir.path().join("logs/worker-stderr.log.previous")).unwrap().len(),2*1024*1024);
    }
}

#[cfg(target_os="windows")]
async fn windows_control(state:&AppState,r:SchedulerRequest)->AppResult<Value> {
    use std::io::Write;
    use std::process::Stdio;
    let root=crate::platform::platform().app_config_dir()?.join("chatgpt-task-queue");
    if r.action=="status" {
        let heartbeat=root.join("scheduler-status.json");
        let status:Value=std::fs::read(&heartbeat).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or(json!({"status":"not_started"}));
        let mut batches=Vec::new();
        if let Ok(entries)=std::fs::read_dir(root.join("states")) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s|s.to_str())==Some("json") {
                    if let Ok(bytes)=std::fs::read(entry.path()) {if let Ok(v)=serde_json::from_slice::<Value>(&bytes){batches.push(v);}}
                }
            }
        }
        let mut pending=Vec::new();
        if let Ok(entries)=std::fs::read_dir(root.join("inbox")) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s|s.to_str())==Some("json") {
                    if let Ok(bytes)=std::fs::read(entry.path()) {if let Ok(v)=serde_json::from_slice::<Value>(&bytes){pending.push(json!({"batchId":v["batchId"],"title":v["title"],"taskIds":v["tasks"].as_array().map(|ts|ts.iter().map(|t|t["id"].clone()).collect::<Vec<_>>())}));}}
                }
            }
        }
        return Ok(json!({"queueRoot":root,"scheduler":status,"batches":batches,"pending":pending}));
    }
    std::fs::create_dir_all(&root)?;
    if ["pause","stop"].contains(&r.action.as_str()) {
        std::fs::write(root.join(&r.action),b"owner requested")?;
        return Ok(json!({"status":format!("{}_requested",r.action)}));
    }
    let resume_restart=if r.action=="resume" {
        let marker=root.join("auto-enabled");
        let pid=std::fs::read(&marker).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).and_then(|v|v["pid"].as_u64());
        let heartbeat=std::fs::read(root.join("scheduler-status.json")).ok().and_then(|b|serde_json::from_slice::<Value>(&b).ok()).unwrap_or(json!({}));
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
        resume_requires_start(marker.exists(),pid,&heartbeat,root.join("stop").exists(),now)?
    } else {false};
    if r.action=="resume" && !resume_restart {
        let _=std::fs::remove_file(root.join("pause"));
        std::fs::write(root.join("retry-preparation"),b"Windows owner requested proven-unsent preparation recovery")?;
        return Ok(json!({"status":"resume_requested","unknownReplayed":false,"retryProvenUnsentPreparation":true}));
    }
    let profile=super::taskdock::profile(state,&r.workspace_id)?;
    let (distro,workspace)=workspace_mapping(&profile.path)?;
    if r.action=="submit" {
        let mut batch=r.batch.ok_or_else(||AppError::Message("missing batch".into()))?;
        let id=batch["batchId"].as_str().unwrap_or("").to_owned();
        if id.is_empty() || id.len()>80 || !id.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-' || b==b'_') || batch["connector"]!="GPTBridge Windows" {
            return Err(AppError::Message("invalid Windows batch".into()));
        }
        batch["workspace"]=json!(workspace);
        let inbox=root.join("inbox");std::fs::create_dir_all(&inbox)?;
        let path=inbox.join(format!("{id}.json"));
        let temp=inbox.join(format!("{}.tmp",uuid::Uuid::new_v4()));
        let bytes=serde_json::to_vec_pretty(&batch)?;
        if bytes.len()>2*1024*1024 {return Err(AppError::Message("batch exceeds 2 MB".into()));}
        let mut f=std::fs::OpenOptions::new().create_new(true).write(true).open(&temp)?;
        f.write_all(&bytes)?;f.sync_all()?;drop(f);
        let result=std::fs::hard_link(&temp,&path);let _=std::fs::remove_file(&temp);result?;
        return Ok(json!({"status":"queued","batchId":id,"path":path,"dispatched":false}));
    }
    if !["desktop","web"].contains(&r.client.as_str()) || r.model.trim().is_empty() || r.effort.trim().is_empty() || r.model.len()>100 || r.effort.len()>100 {
        return Err(AppError::Message("请指定桌面/网页、模型和思考强度。".into()));
    }
    let marker=root.join("auto-enabled");
    let mut owner_marker=std::fs::OpenOptions::new().write(true).create_new(true).open(&marker).map_err(|_|AppError::Message("独立调度器已启用或存在待核对的上次运行；请先查看队列状态，不重复启动。".into()))?;
    owner_marker.write_all(b"owner-start-requested")?;owner_marker.sync_all()?;drop(owner_marker);
    struct StartingMarker {path:std::path::PathBuf,started:bool}
    impl Drop for StartingMarker {fn drop(&mut self){if !self.started {let _=std::fs::remove_file(&self.path);}}}
    let mut starting=StartingMarker{path:marker.clone(),started:false};
    // The owner's start/resume also restores this selected Windows connection
    // after an app update. Existing runtimes are reused; foreign ports stay owned.
    let connection=super::runtime::start_mcp_service_with_reclaim(state,&r.workspace_id,false).await?;
    if connection.state!="running" {
        return Err(AppError::Message("Windows 本地连接尚未运行，未启动调度器；请查看该项目连接日志。".into()));
    }
    // Only this GUI command enables and starts the independent owner process.
    let wsl=std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or_else(||AppError::Message("SystemRoot missing".into()))?).join("System32/wsl.exe");
    let converted=tokio::process::Command::new(&wsl).args(["-d",&distro,"--exec","wslpath","-u"]).arg(&root).output().await?;
    if !converted.status.success(){return Err(AppError::Message("无法映射独立队列到已登记 WSL；没有启动调度器。".into()));}
    let queue=String::from_utf8_lossy(&converted.stdout).trim().to_owned();
    std::fs::write(root.join("scheduler.py"),include_str!("../../../scripts/chatgpt-tasks/scheduler.py"))?;
    let adapter=root.join("chatgpt-dispatch.ps1");std::fs::write(&adapter,include_str!("../../scripts/chatgpt-dispatch.ps1"))?;
    let config=json!({"ownerEnabled":true,"workspace":workspace,"queue":queue,"adapterWindowsPath":adapter,"client":r.client,"model":r.model,"effort":r.effort,"webFallback":r.web_fallback,"timeoutSeconds":7200});
    std::fs::write(root.join("scheduler-config.json"),serde_json::to_vec_pretty(&config)?)?;
    if resume_restart {std::fs::write(root.join("retry-preparation"),b"Windows owner requested proven-unsent preparation recovery")?;}
    let _=std::fs::remove_file(root.join("stop"));let _=std::fs::remove_file(root.join("pause"));
    let stdout=worker_log(&root,"worker-stdout.log")?;
    let stderr=worker_log(&root,"worker-stderr.log")?;
    let child=std::process::Command::new(wsl).args(["-d",&distro,"--exec","python3",&format!("{queue}/scheduler.py"),"--config",&format!("{queue}/scheduler-config.json")])
        .stdin(Stdio::null()).stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr)).spawn().map_err(|e|{let _=std::fs::remove_file(&marker);AppError::Io(e)})?;
    starting.started=true;
    Ok(json!({"status":"start_requested","ownerPid":child.id(),"queueRoot":root,"resumedStoppedOwner":resume_restart,"note":"进程已启动请求；请刷新确认心跳，不以本回执冒充调度器运行成功。"}))
}
