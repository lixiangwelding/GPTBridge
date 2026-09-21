//! Copy-only configuration inheritance. Never moves, renames, edits or starts the source installation.
use std::{fs,io::Write,path::Path};
use serde_json::{json,Value};
use crate::{Error,Result,digest};

pub fn inherit(source:&Path,destination:&Path)->Result<Value>{
    if source==destination || destination.exists() {return Err(Error::contract("CONFIG_EXISTS","destination must be a new personal configuration"))}
    let meta=fs::symlink_metadata(source)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len()>16*1024*1024 {return Err(Error::contract("INVALID_CONFIG","source must be a regular JSON file <=16MiB"))}
    let original=fs::read(source)?;let before=digest(&original);
    let mut data:Value=serde_json::from_slice(&original)?;
    if !data.is_object() || !data.get("profiles").is_some_and(Value::is_array) {return Err(Error::contract("INVALID_CONFIG","source does not contain profiles"))}
    disable_auto_start(&mut data);
    let mut old_ports=std::collections::HashSet::new();
    for p in data["profiles"].as_array().unwrap(){for key in ["runtime","actions"]{if let Some(n)=p[key]["local_port"].as_u64(){old_ports.insert(n);}}}
    let mut port=38766u64;let mut ports=Vec::new();
    for profile in data["profiles"].as_array_mut().unwrap(){
        if !profile.is_object(){return Err(Error::contract("INVALID_CONFIG","profile must be an object"))}
        for key in ["runtime","actions"]{
            if !profile.get(key).is_some_and(Value::is_object){profile[key]=json!({});}
            while old_ports.contains(&port){port+=1;}
            if port>65000{return Err(Error::contract("PORT_RANGE","too many imported profiles"))}
            profile[key]["local_port"]=json!(port);ports.push(port);old_ports.insert(port);port+=1;
            // Legacy external launchers may point at the still-running installation.
            // Preserve the source file, but use this build's native runtime by default.
            profile[key]["runtime_command"]=json!("");
        }
        if !profile.get("tunnel").is_some_and(Value::is_object){profile["tunnel"]=json!({});}
        profile["tunnel"]["type"]=json!("none");profile["tunnel"]["public_url"]=json!("");
        profile["actions"]["tunnel_type"]=json!("none");profile["actions"]["public_url"]=json!("");
        if let Some(upstreams)=profile["runtime"].get_mut("upstream_mcps").and_then(Value::as_array_mut){for upstream in upstreams{upstream["enabled"]=json!(false);}}
    }
    let parent=destination.parent().ok_or_else(||Error::contract("INVALID_CONFIG_PATH","configuration parent required"))?;
    crate::store::private_dir(parent)?;
    if fs::read(source)?!=original{return Err(Error::contract("SOURCE_CHANGED","configuration changed during import; source was not modified"))}
    let mut options=fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    let mut file=options.open(destination)?;
    let encoded=serde_json::to_vec_pretty(&data)?;file.write_all(&encoded)?;file.write_all(b"\n")?;file.sync_all()?;
    Ok(json!({"source_sha256":before,"destination_sha256":digest(fs::read(destination)?),"profiles":ports.len()/2,"local_ports":ports,"source_modified":false,"services_started":false,"tunnels_enabled":false,"upstreams_enabled":false,"secrets_copied_locally":true,"note":"ports are assigned, not reserved; check availability before manual startup"}))
}
fn disable_auto_start(value:&mut Value){
    match value {
        Value::Object(map)=>for(key,value) in map {if matches!(key.as_str(),"auto_start"|"autostart"|"autoStart"|"start_on_launch"|"startOnLaunch"|"launch_at_login"){*value=Value::Bool(false)}else{disable_auto_start(value)}},
        Value::Array(array)=>for value in array{disable_auto_start(value)},
        _=>{}
    }
}
