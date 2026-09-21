use std::{fs, path::{Path, PathBuf}, time::{Duration, SystemTime, UNIX_EPOCH}};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}: {1}")]
    Contract(&'static str, String),
    #[error("state I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("state database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("state JSON: {0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn code(&self) -> &'static str {
        match self { Self::Contract(code, _) => code, Self::Io(_) => "STATE_IO", Self::Sql(_) => "STATE_DB", Self::Json(_) => "STATE_JSON" }
    }
    pub fn contract(code: &'static str, message: impl Into<String>) -> Self { Self::Contract(code, message.into()) }
}
pub fn digest(bytes: impl AsRef<[u8]>) -> String { format!("{:x}", Sha256::digest(bytes.as_ref())) }
pub fn now_ms() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64 }
pub fn private_dir(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) { return Err(Error::contract("UNSAFE_STATE_PATH", "state directory must not be a symlink")); }
    fs::create_dir_all(path)?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(path, fs::Permissions::from_mode(0o700))?; }
    Ok(())
}
pub fn id(value: &str) -> Result<&str> {
    uuid::Uuid::parse_str(value).map_err(|_| Error::contract("INVALID_ID", "expected UUID"))?; Ok(value)
}
pub fn text<'a>(value: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    value.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty() && s.len() <= max)
        .ok_or_else(|| Error::contract("INVALID_ARGUMENT", format!("{key} must be nonempty and <= {max} bytes")))
}

#[derive(Clone, Debug)]
pub struct Store { pub dir: PathBuf, pub workspace: PathBuf }
impl Store {
    pub fn open(base: &Path, workspace: &Path) -> Result<Self> {
        let workspace = workspace.canonicalize()?;
        let dir = base.join(digest(workspace.to_string_lossy().as_bytes()));
        private_dir(&dir)?;
        Self::at(dir, workspace)
    }
    pub fn at(dir: PathBuf, workspace: PathBuf) -> Result<Self> {
        // WAL-reset race is fixed in SQLite 3.51.3+. Never start a concurrent
        // task store on an older bundled/system library.
        if rusqlite::version_number() < 3_051_003 {
            return Err(Error::contract("SQLITE_UPGRADE_REQUIRED", "personal WAL state requires SQLite >=3.51.3"));
        }
        private_dir(&dir)?; private_dir(&dir.join("locks"))?; private_dir(&dir.join("jobs"))?;
        let s = Self { dir, workspace: workspace.canonicalize()? };
        let c = s.conn()?;
        c.pragma_update(None, "journal_mode", "WAL")?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY, goal TEXT NOT NULL, state TEXT NOT NULL, revision INTEGER NOT NULL, checkpoint TEXT NOT NULL, created INTEGER NOT NULL, updated INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS bindings(owner TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES tasks(id));
        CREATE TABLE IF NOT EXISTS events(seq INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL REFERENCES tasks(id), kind TEXT NOT NULL, body TEXT NOT NULL, created INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS events_task_seq ON events(task_id,seq);
        CREATE TABLE IF NOT EXISTS receipts(scope TEXT NOT NULL, request_id TEXT NOT NULL, input_hash TEXT NOT NULL, state TEXT NOT NULL, result TEXT, PRIMARY KEY(scope,request_id));
        CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, task_id TEXT, scope TEXT NOT NULL, request_id TEXT NOT NULL, input_hash TEXT NOT NULL, spec TEXT NOT NULL, state TEXT NOT NULL, exit_code INTEGER, created INTEGER NOT NULL, updated INTEGER NOT NULL, cancel INTEGER NOT NULL DEFAULT 0, detail TEXT NOT NULL DEFAULT '', UNIQUE(scope,request_id));
        CREATE INDEX IF NOT EXISTS jobs_task_updated ON jobs(task_id,updated);")?;
        Ok(s)
    }
    pub fn conn(&self) -> Result<Connection> {
        let c = Connection::open(self.dir.join("runtime.sqlite3"))?;
        c.busy_timeout(Duration::from_secs(10))?;
        c.pragma_update(None, "foreign_keys", "ON")?;
        c.pragma_update(None, "synchronous", "FULL")?;
        Ok(c)
    }
    pub fn event(&self, task_id: &str, kind: &str, body: &Value) -> Result<()> {
        id(task_id)?;
        let encoded = serde_json::to_string(body)?;
        if encoded.len() > 32_768 { return Err(Error::contract("EVENT_TOO_LARGE", "event summaries are limited to 32KiB")); }
        self.conn()?.execute("INSERT INTO events(task_id,kind,body,created) VALUES(?1,?2,?3,?4)", (task_id,kind,encoded,now_ms()))?; Ok(())
    }
    // Called under the source gate. Intent is durable before filesystem side effects.
    pub fn begin_receipt(&self, scope: &str, request: &str, hash: &str) -> Result<Option<Value>> {
        if request.is_empty() || request.len() > 160 { return Err(Error::contract("INVALID_REQUEST_ID", "request_id is required (<=160 bytes)")); }
        let mut c = self.conn()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let prior: Option<(String,String,Option<String>)> = tx.query_row("SELECT input_hash,state,result FROM receipts WHERE scope=?1 AND request_id=?2", (scope,request), |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((h,state,result)) = prior {
            if h != hash { return Err(Error::contract("IDEMPOTENCY_CONFLICT", "request_id already used with different input")); }
            if state != "complete" { return Err(Error::contract("OPERATION_INDETERMINATE", "previous attempt may have changed files; inspect evidence, do not replay")); }
            return Ok(Some(serde_json::from_str(&result.unwrap_or_else(|| "null".into()))?));
        }
        tx.execute("INSERT INTO receipts(scope,request_id,input_hash,state) VALUES(?1,?2,?3,'started')", (scope,request,hash))?;
        tx.commit()?; Ok(None)
    }
    pub fn finish_receipt(&self, scope: &str, request: &str, result: &Value) -> Result<()> {
        let n=self.conn()?.execute("UPDATE receipts SET state='complete',result=?3 WHERE scope=?1 AND request_id=?2", (scope,request,result.to_string()))?;
        if n!=1 { return Err(Error::contract("RECEIPT_MISSING", "operation intent not found")); } Ok(())
    }
    pub fn events(&self, task: &str, after: i64, limit: usize) -> Result<Value> {
        id(task)?;
        let c=self.conn()?; let mut q=c.prepare("SELECT seq,kind,body,created FROM events WHERE task_id=?1 AND seq>?2 ORDER BY seq LIMIT ?3")?;
        let rows=q.query_map((task,after,(limit.clamp(1,100)+1) as i64), |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?)))?;
        let mut out=Vec::new(); for row in rows { let (seq,kind,body,t)=row?; out.push(json!({"seq":seq,"kind":kind,"body":serde_json::from_str::<Value>(&body)?,"created":t})); }
        let more=out.len()>limit.clamp(1,100); if more {out.pop();}
        let next=if more {out.last().map(|v|v["seq"].clone())} else {None};
        Ok(json!({"events":out,"next_cursor":next}))
    }
}
