//! Explicit local transports, before GUI initialization. Never edits configuration,
//! reclaims occupied ports, starts tunnels or terminates another service.
use std::{fs, io::{self, BufRead, Write}, path::Path, sync::{Arc, Mutex, mpsc}};
use serde_json::{json, Value};
use crate::{audit::AuditRequestContext, data::AppData, mcp::{server, upstream::UpstreamMcpManager},
    platform::platform, secret::SecretStore, tools::{Workspace, PolicySettings}, workspace::WorkspaceProfile};

const MAX_FRAME: usize = 2 * 1024 * 1024;

fn select_profile(path: &Path, selector: &str) -> Result<WorkspaceProfile, String> {
    let meta=fs::symlink_metadata(path).map_err(|_|"personal configuration unavailable")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len()>16*1024*1024 {
        return Err("invalid personal configuration file".into());
    }
    let bytes=fs::read(path).map_err(|_|"cannot read personal configuration")?;
    let data: AppData=serde_json::from_slice(&bytes).map_err(|_|"configuration failed native parsing")?;
    let matches: Vec<_>=data.profiles.iter().filter(|p|p.id==selector).collect();
    if matches.len()!=1 {return Err("select exactly one saved profile by its id".into());}
    let profile=matches[0].clone();
    crate::mcp::gateway::validate_members(&profile,&data.profiles)?;
    if !Path::new(&profile.path).is_absolute() || !Path::new(&profile.path).is_dir() {
        return Err("saved workspace must be an existing absolute directory".into());
    }
    Ok(profile)
}

fn saved_profile(selector: &str)->Result<WorkspaceProfile,String> {
    let path=platform().app_config_dir().map_err(|_|"personal home unavailable")?.join("data/profiles.json");
    select_profile(&path,selector)
}

fn secret(profile: &WorkspaceProfile,key: &str)->Result<Option<String>,String> {
    (if profile.auth.use_shared_secrets {SecretStore::get_shared(key)} else {SecretStore::get(&profile.id,key)})
        .map_err(|_|"saved authentication unavailable".into())
}

pub fn run(args: &[String])->Option<i32> {
    let mode=args.get(1)?.as_str();
    if !matches!(mode,"--personal-stdio"|"--personal-serve") {return None;}
    let result=(||->Result<(),String>{
        if (mode=="--personal-stdio" && args.len()!=3) || (mode=="--personal-serve" && args.len()!=4) {
            return Err("usage: --personal-stdio PROFILE_ID | --personal-serve PROFILE_ID PORT".into());
        }
        let profile=saved_profile(&args[2])?;
        let port=if mode=="--personal-serve" {
            Some(args[3].parse::<u16>().ok().filter(|p|*p>=1024).ok_or("port must be 1024..65535")?)
        } else {None};
        let tools=crate::tools::registry::exposed_tool_names(&profile.runtime.tool_profile);
        let upstream=Arc::new(tauri::async_runtime::block_on(
            UpstreamMcpManager::start(&profile.runtime.upstream_mcps,&tools))?);
        let result=if mode=="--personal-stdio" {stdio(profile,upstream.clone())} else {
            http(profile,port.unwrap(),upstream.clone())
        };
        tauri::async_runtime::block_on(upstream.shutdown());
        result
    })();
    Some(match result {Ok(())=>0,Err(error)=>{eprintln!("personal transport: {error}");1}})
}

fn http(profile: WorkspaceProfile,port:u16,upstream:Arc<UpstreamMcpManager>)->Result<(),String> {
    let mut auth=profile.auth.clone();
    if auth.use_shared_secrets {
        if let Some(id)=SecretStore::get_shared("oauth_client_id").map_err(|_|"saved client unavailable")? {
            auth.oauth_client_id=id;
        }
    }
    let password=if auth.oauth_enabled(){secret(&profile,"oauth_password")?}else{None};
    let token=if auth.oauth_enabled(){secret(&profile,"oauth_token_secret")?}else{None};
    if auth.oauth_enabled() && (password.as_deref().unwrap_or("").is_empty() || token.as_deref().unwrap_or("").is_empty()) {
        return Err("saved OAuth authentication is incomplete".into());
    }
    let public_url=profile.effective_public_url();
    tauri::async_runtime::block_on(async move {
        let (_shutdown,handle)=crate::mcp::spawn_listener(port,profile.path.into(),profile.id,
            auth,public_url,None,password,token,profile.runtime,upstream)?;
        eprintln!("personal HTTP {} listening on 127.0.0.1:{port}; no GUI/tunnel changes",env!("CARGO_PKG_VERSION"));
        handle.await.map_err(|_|"personal listener terminated unexpectedly".to_string())?;
        Ok(())
    })
}

fn valid_request(body:&Value)->bool {
    body.is_object() && body["jsonrpc"]=="2.0" && body["method"].is_string()
        && (body["id"].is_string() || body["id"].is_number()
            || (body["id"].is_null() && body["method"].as_str().unwrap_or("").starts_with("notifications/")))
}

fn read_frame(reader:&mut impl BufRead)->io::Result<Option<Vec<u8>>> {
    let mut frame=Vec::new();
    loop {
        let buffer=reader.fill_buf()?;
        if buffer.is_empty(){return Ok(if frame.is_empty(){None}else{Some(frame)});}
        let end=buffer.iter().position(|b|*b==b'\n').map(|n|n+1);
        let count=end.unwrap_or(buffer.len());
        if frame.len()+count>MAX_FRAME {return Err(io::Error::new(io::ErrorKind::InvalidData,"JSON-RPC frame exceeds 2MiB"));}
        frame.extend_from_slice(&buffer[..count]);reader.consume(count);
        if end.is_some(){return Ok(Some(frame));}
    }
}

fn write_response(writer:&Mutex<io::BufWriter<io::Stdout>>,response:&Value)->Result<(),String> {
    if response.is_null(){return Ok(());}
    let mut writer=writer.lock().map_err(|_|"stdout lock unavailable")?;
    serde_json::to_writer(&mut *writer,response).map_err(|_|"stdout disconnected")?;
    writer.write_all(b"\n").and_then(|_|writer.flush()).map_err(|_|"stdout disconnected".into())
}

fn stdio(profile:WorkspaceProfile,upstream:Arc<UpstreamMcpManager>)->Result<(),String> {
    // Local stdio is authorized by OS process launch, not a public HTTP token.
    // HTTP keeps the original auth profile; both transports retain tool policy.
    let workspace=Workspace::new(profile.path.into()).map_err(|e|e.message())?;
    let state=server::new_state(workspace,profile.id,profile.auth,
        PolicySettings::from_runtime(&profile.runtime),profile.runtime.tool_profile,profile.runtime.permission_mode,upstream);
    let (send,receive)=mpsc::sync_channel::<Value>(32);
    let receive=Arc::new(Mutex::new(receive));
    let writer=Arc::new(Mutex::new(io::BufWriter::new(io::stdout())));
    let mut workers=Vec::new();
    for _ in 0..4 {
        let receive=receive.clone();let writer=writer.clone();let state=state.clone();
        workers.push(std::thread::spawn(move||->Result<(),String>{
            loop {
                let body=match receive.lock().map_err(|_|"input queue unavailable")?.recv(){Ok(v)=>v,Err(_)=>return Ok(())};
                let request=AuditRequestContext {transport:"stdio".into(),method:body["method"].as_str().map(str::to_string),
                    request_id:body.get("id").and_then(crate::audit::request_id_from_value),route:Some("stdio".into()),..Default::default()};
                write_response(&writer,&server::handle_request_with_context(&state,&body,&request))?;
            }
        }));
    }
    let read_result=(||->Result<(),String>{
        let stdin=io::stdin();let mut input=stdin.lock();
        while let Some(frame)=read_frame(&mut input).map_err(|e|e.to_string())? {
            let body=serde_json::from_slice::<Value>(&frame);
            match body {
                Ok(body) if valid_request(&body)=>send.send(body).map_err(|_|"stdio workers unavailable")?,
                Ok(_)=>write_response(&writer,&json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request"}}))?,
                Err(_)=>write_response(&writer,&json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}))?,
            }
        }
        Ok(())
    })();
    drop(send);
    for worker in workers {worker.join().map_err(|_|"stdio worker failed")??;}
    read_result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_mutating_notifications_and_malformed_rpc() {
        assert!(!valid_request(&json!({"jsonrpc":"2.0","method":"tools/call"})));
        assert!(!valid_request(&json!({"id":1,"method":"tools/call"})));
        assert!(valid_request(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})));
        assert!(valid_request(&json!({"jsonrpc":"2.0","id":"a","method":"tools/list"})));
    }
    #[test]
    fn frames_are_bounded_and_eof_is_supported() {
        let mut input=io::Cursor::new(b"one\ntwo".to_vec());
        assert_eq!(read_frame(&mut input).unwrap().unwrap(),b"one\n");
        assert_eq!(read_frame(&mut input).unwrap().unwrap(),b"two");
        assert!(read_frame(&mut input).unwrap().is_none());
        assert!(read_frame(&mut io::Cursor::new(vec![b'x';MAX_FRAME+1])).is_err());
    }
    #[test]
    fn missing_or_invalid_config_is_not_created_or_repaired() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("profiles.json");
        assert!(select_profile(&path,"none").is_err());assert!(!path.exists());
        fs::write(&path,b"broken").unwrap();assert!(select_profile(&path,"none").is_err());
        assert_eq!(fs::read(path).unwrap(),b"broken");
    }
}
