//! One workspace-owned concurrency snapshot, shared by all local transports.
//! Changes require an idle, coordinated service restart; environment variables
//! must not give different workers independent budgets for the same lock pool.
use std::{fs, io::Read, path::Path};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use crate::{Error,Result};

pub const CONFIG_FILE:&str="concurrency.json";

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(default,deny_unknown_fields)]
pub struct ConcurrencyLimits {
    pub running:usize,
    pub heavy:usize,
    pub queued_and_running:usize,
    pub http_requests:usize,
    pub http_waiting:usize,
    pub http_control_reserved:usize,
    pub stdio_workers:usize,
    pub stdio_control_workers:usize,
    pub stdio_queue:usize,
}
impl Default for ConcurrencyLimits {
    fn default()->Self {Self {running:8,heavy:2,queued_and_running:32,
        http_requests:32,http_waiting:24,http_control_reserved:4,
        stdio_workers:4,stdio_control_workers:1,stdio_queue:32}}
}
impl ConcurrencyLimits {
    pub fn performance()->Self {Self {running:64,heavy:4,queued_and_running:256,
        http_requests:256,http_waiting:192,http_control_reserved:32,
        stdio_workers:32,stdio_control_workers:4,stdio_queue:256}}
    pub fn validate(&self)->Result<()> {
        let valid=(1..=256).contains(&self.running) && (1..=16).contains(&self.heavy)
            && self.heavy<=self.running && (self.running..=1024).contains(&self.queued_and_running)
            && (8..=1024).contains(&self.http_requests)
            && (1..self.http_requests).contains(&self.http_control_reserved)
            && (1..=self.http_requests-self.http_control_reserved).contains(&self.http_waiting)
            && (2..=128).contains(&self.stdio_workers)
            && (1..self.stdio_workers).contains(&self.stdio_control_workers)
            && (self.stdio_workers..=1024).contains(&self.stdio_queue);
        if !valid {return Err(Error::contract("INVALID_CONCURRENCY","concurrency values exceed bounds or leave no control capacity"));}
        Ok(())
    }
    pub fn load(dir:&Path)->Result<Self> {
        let path=dir.join(CONFIG_FILE);
        let meta=match fs::symlink_metadata(&path) {
            Ok(meta)=>meta,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(Self::default()),
            Err(e)=>return Err(e.into()),
        };
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len()>16384 {
            return Err(Error::contract("INVALID_CONCURRENCY","configuration must be a regular file no larger than 16KiB"));
        }
        let mut options=fs::OpenOptions::new();options.read(true);
        #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.custom_flags(libc::O_NOFOLLOW);}
        let mut bytes=Vec::new();options.open(path)?.take(16385).read_to_end(&mut bytes)?;
        if bytes.len()>16384 {return Err(Error::contract("INVALID_CONCURRENCY","configuration grew beyond 16KiB"));}
        let value:Self=serde_json::from_slice(&bytes).map_err(|_|Error::contract("INVALID_CONCURRENCY","invalid concurrency JSON or unknown field"))?;
        value.validate()?;Ok(value)
    }
    pub fn require_unchanged(&self,dir:&Path)->Result<()> {
        if Self::load(dir)?!=*self {return Err(Error::contract("CONCURRENCY_RESTART_REQUIRED",
            "workspace concurrency changed; query existing jobs and perform an idle coordinated restart before new work"));}
        Ok(())
    }
    pub fn jobs_json(&self)->Value {json!({"running":self.running,"heavy":self.heavy,"queued_and_running":self.queued_and_running})}
    pub fn profile(&self)->&'static str {if *self==Self::performance(){"performance"}else if *self==Self::default(){"conservative"}else{"custom"}}
}

/// Raise only this process's soft descriptor budget; never lower an existing
/// limit or change the hard/system limits. Failure is observable, not hidden.
pub fn raise_file_capacity(target:u64)->Value {
    #[cfg(unix)] {
        let mut current=libc::rlimit{rlim_cur:0,rlim_max:0};
        if unsafe{libc::getrlimit(libc::RLIMIT_NOFILE,&mut current)}!=0 {
            return json!({"available":false,"error":"getrlimit_failed"});
        }
        let before=current.rlim_cur;
        let desired=(target as libc::rlim_t).min(current.rlim_max).max(before);
        let changed=if desired>before {let new=libc::rlimit{rlim_cur:desired,rlim_max:current.rlim_max};
            unsafe{libc::setrlimit(libc::RLIMIT_NOFILE,&new)==0}}else{false};
        json!({"available":true,"before":before,"soft":if changed{desired}else{before},
            "hard":current.rlim_max,"target":target,"raised":changed,"system_limit_modified":false})
    }
    #[cfg(not(unix))] {let _=target;json!({"available":false,"platform":"non-unix"})}
}
pub fn file_capacity()->Value {raise_file_capacity(0)}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_and_performance_are_valid_and_explicit() {
        ConcurrencyLimits::default().validate().unwrap();ConcurrencyLimits::performance().validate().unwrap();
        assert_eq!(ConcurrencyLimits::performance().running,64);
        assert_eq!(ConcurrencyLimits::performance().queued_and_running,256);
        let dir=tempfile::tempdir().unwrap();assert_eq!(ConcurrencyLimits::load(dir.path()).unwrap().profile(),"conservative");
        assert!(!dir.path().join(CONFIG_FILE).exists());
    }
    #[test]
    fn rejects_unbounded_and_contradictory_capacities() {
        for (key,value) in [("running",0),("running",257),("heavy",17),("queued_and_running",2),
            ("http_control_reserved",32),("http_waiting",32),("stdio_control_workers",4),("stdio_queue",0)] {
            let mut data=serde_json::to_value(ConcurrencyLimits::default()).unwrap();data[key]=json!(value);
            assert!(serde_json::from_value::<ConcurrencyLimits>(data).unwrap().validate().is_err(),"{key}");
        }
    }
    #[test]
    fn same_workspace_snapshot_detects_changes_without_repairing_config() {
        let dir=tempfile::tempdir().unwrap();let before=ConcurrencyLimits::default();
        let text=serde_json::to_vec(&ConcurrencyLimits::performance()).unwrap();fs::write(dir.path().join(CONFIG_FILE),&text).unwrap();
        assert_eq!(before.require_unchanged(dir.path()).unwrap_err().code(),"CONCURRENCY_RESTART_REQUIRED");
        ConcurrencyLimits::load(dir.path()).unwrap().require_unchanged(dir.path()).unwrap();
        assert_eq!(fs::read(dir.path().join(CONFIG_FILE)).unwrap(),text);
    }
    #[test]
    fn invalid_oversized_and_unknown_settings_fail_closed() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join(CONFIG_FILE);
        for bytes in [b"broken".to_vec(),br#"{"unlimited":true}"#.to_vec(),vec![b' ';16385]] {
            fs::write(&path,&bytes).unwrap();assert!(ConcurrencyLimits::load(dir.path()).is_err());assert_eq!(fs::read(&path).unwrap(),bytes);
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlink_configuration_is_rejected() {
        let dir=tempfile::tempdir().unwrap();fs::write(dir.path().join("other"),b"{}").unwrap();
        std::os::unix::fs::symlink(dir.path().join("other"),dir.path().join(CONFIG_FILE)).unwrap();
        assert!(ConcurrencyLimits::load(dir.path()).is_err());
    }
}
