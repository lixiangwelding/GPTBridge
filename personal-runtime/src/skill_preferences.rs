//! Per-project automatic-discovery preferences. These never grant execution rights.
use std::{collections::BTreeSet, path::Path, time::Duration};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use crate::{Error, Result, Store};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillPreferences { pub revision: i64, pub disabled: BTreeSet<String> }

fn from_connection(c: &Connection) -> Result<SkillPreferences> {
    let exists: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='taskdock_skill_preferences')",[],|r|r.get(0))?;
    if !exists { return Ok(SkillPreferences::default()); }
    let row: Option<(i64,String)> = c.query_row("SELECT revision,disabled FROM taskdock_skill_preferences WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((revision,raw)) = row else {return Err(Error::contract("INVALID_SKILL_PREFERENCES","preference row is missing"));};
    if revision < 0 || raw.len()>100_000 {return Err(Error::contract("INVALID_SKILL_PREFERENCES","preference state is invalid"));}
    let disabled:BTreeSet<String> = serde_json::from_str(&raw)?;
    if disabled.len()>2000 || disabled.iter().any(|s|s.len()!=32 || !s.bytes().all(|c|c.is_ascii_hexdigit())) {
        return Err(Error::contract("INVALID_SKILL_PREFERENCES","invalid skill identifiers"));
    }
    Ok(SkillPreferences{revision,disabled})
}

/// Discovery does not create state directories, migrate schemas or enable skills
/// on read errors. A pre-TaskDock store has no preferences and keeps its defaults.
pub fn read_file(path: &Path) -> Result<SkillPreferences> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => return Ok(SkillPreferences::default()),
        Err(e) => return Err(e.into()),
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => return Err(Error::contract("UNSAFE_STATE_PATH","invalid preference database path")),
        _ => {}
    }
    let c=Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    c.busy_timeout(Duration::from_millis(1000))?;
    from_connection(&c)
}

impl Store {
    pub fn skill_preferences(&self) -> Result<SkillPreferences> {from_connection(&self.conn()?)}
    pub fn set_skill_preference(&self,skill_id:&str,enabled:bool,expected_revision:i64)->Result<SkillPreferences> {
        if skill_id.len()!=32 || !skill_id.bytes().all(|b|b.is_ascii_hexdigit()) {
            return Err(Error::contract("INVALID_SKILL_ID","expected a skill catalog identifier"));
        }
        let mut c=self.conn()?;let tx=c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut preferences=from_connection(&tx)?;
        if preferences.revision!=expected_revision {return Err(Error::contract("STALE_SKILL_PREFERENCES","preferences changed; reload before changing this skill"));}
        if enabled {preferences.disabled.remove(skill_id);} else {preferences.disabled.insert(skill_id.to_string());}
        if preferences.disabled.len()>2000 {return Err(Error::contract("SKILL_PREFERENCE_LIMIT","too many skill preferences"));}
        preferences.revision=preferences.revision.checked_add(1).ok_or_else(||Error::contract("INVALID_SKILL_PREFERENCES","revision overflow"))?;
        tx.execute("UPDATE taskdock_skill_preferences SET revision=?1,disabled=?2 WHERE id=1",(preferences.revision,serde_json::to_string(&preferences.disabled)?))?;
        tx.commit()?;Ok(preferences)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persists_and_rejects_lost_updates() {
        let d=tempfile::tempdir().unwrap();let s=Store::at(d.path().join("state"),d.path().to_path_buf()).unwrap();
        let id="a".repeat(32);
        let p=s.set_skill_preference(&id,false,0).unwrap();assert_eq!(p.revision,1);assert!(p.disabled.contains(&id));
        assert_eq!(read_file(&s.dir.join("runtime.sqlite3")).unwrap().revision,1);
        assert!(s.set_skill_preference(&id,true,0).is_err());
        assert!(s.set_skill_preference(&id,true,1).unwrap().disabled.is_empty());
    }
    #[test]
    fn missing_does_not_create_and_corrupt_does_not_enable() {
        let d=tempfile::tempdir().unwrap();let p=d.path().join("absent.sqlite3");
        assert!(read_file(&p).unwrap().disabled.is_empty());assert!(!p.exists());
        std::fs::write(&p,b"not a database").unwrap();assert!(read_file(&p).is_err());
    }
}
