//! Advisory, process-shared locks for cooperating tools. Not an OS sandbox.
use std::{fs::{File,OpenOptions}, path::Path, time::{Duration,Instant}};
use fs2::FileExt;
use crate::{digest,Error,Result};

pub struct Guard(File);
#[cfg(unix)]
impl Guard {
    pub fn raw_fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd;
        self.0.as_raw_fd()
    }
}
impl Drop for Guard { fn drop(&mut self) { let _=FileExt::unlock(&self.0); } }
pub fn try_gate(root: &Path, resource: &str, exclusive: bool) -> Result<Option<Guard>> {
    let path=root.join("locks").join(format!("{}.lock",digest(resource)));
    let file=OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
    let result=if exclusive {FileExt::try_lock_exclusive(&file)} else {FileExt::try_lock_shared(&file)};
    match result { Ok(())=>Ok(Some(Guard(file))), Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>Ok(None), Err(e)=>Err(e.into()) }
}
pub fn gate(root: &Path, resource: &str, exclusive: bool, timeout: Duration) -> Result<Guard> {
    let start=Instant::now();
    loop { if let Some(g)=try_gate(root,resource,exclusive)? {return Ok(g)}
        if start.elapsed()>=timeout {return Err(Error::contract("RESOURCE_BUSY","resource is held by another operation; continue independent work"))}
        std::thread::sleep(Duration::from_millis(15));
    }
}
pub fn slot(root: &Path, pool: &str, count: usize) -> Result<Option<Guard>> {
    for n in 0..count {if let Some(g)=try_gate(root,&format!("slot:{pool}:{n}"),true)? {return Ok(Some(g))}}
    Ok(None)
}
