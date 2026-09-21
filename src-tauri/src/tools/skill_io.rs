//! Personal skill bridge: bounded, read-only file and metadata access.
use std::{fs::File, io::Read, path::{Component, Path}};
use serde::Deserialize;
use super::workspace::WorkspaceError;

pub const MAX_FILE: usize = 2 * 1024 * 1024;
pub const MAX_HEADER: usize = 32 * 1024;
pub const MAX_PAGE: usize = 48 * 1024;

pub fn error(code: &'static str, message: &str) -> WorkspaceError {
    WorkspaceError::Tool { code, message: message.into(), category: "skill_bridge", retryable: false }
}

pub fn read(path: &Path) -> Result<String, WorkspaceError> {
    let mut file = open_regular(path).map_err(|_| error("SKILL_FILE_UNAVAILABLE", "Skill file is unavailable within the approved roots"))?;
    let before = file.metadata().map_err(|_| error("SKILL_FILE_UNAVAILABLE", "Cannot inspect skill file"))?;
    if !before.is_file() { return Err(error("SKILL_NOT_REGULAR", "Only regular text files are readable")); }
    if before.len() > MAX_FILE as u64 { return Err(error("SKILL_FILE_TOO_LARGE", "Skill text exceeds 2MiB")); }
    let mut bytes = Vec::new();
    (&mut file).take((MAX_FILE + 1) as u64).read_to_end(&mut bytes)
        .map_err(|_| error("SKILL_FILE_UNAVAILABLE", "Cannot read skill text"))?;
    let after = file.metadata().map_err(|_| error("SKILL_FILE_UNAVAILABLE", "Cannot inspect skill file"))?;
    if bytes.len() > MAX_FILE { return Err(error("SKILL_FILE_TOO_LARGE", "Skill grew beyond 2MiB")); }
    if before.len()!=after.len() || before.modified().ok()!=after.modified().ok() {
        return Err(error("SKILL_SOURCE_CHANGED", "Skill changed during reading; search again"));
    }
    if bytes.contains(&0) { return Err(error("SKILL_BINARY_FILE", "Binary files are not skill text")); }
    String::from_utf8(bytes).map_err(|_| error("SKILL_INVALID_UTF8", "Skill text must be UTF-8"))
}

#[cfg(unix)]
fn open_regular(path: &Path) -> std::io::Result<File> {
    use std::{ffi::CString, os::{fd::{AsRawFd, FromRawFd}, unix::ffi::OsStrExt}};
    if !path.is_absolute() { return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput)); }
    let mut current = File::open("/")?;
    let parts: Vec<_> = path.components().collect();
    for (index, component) in parts.iter().enumerate().skip(1) {
        let Component::Normal(part) = component else { return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput)); };
        let name = CString::new(part.as_bytes()).map_err(|_|std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        let flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW
            | if index + 1 == parts.len() {libc::O_NONBLOCK} else {libc::O_DIRECTORY};
        let fd = unsafe {libc::openat(current.as_raw_fd(), name.as_ptr(), flags)};
        if fd < 0 { return Err(std::io::Error::last_os_error()); }
        current = unsafe {File::from_raw_fd(fd)};
    }
    Ok(current)
}

#[cfg(not(unix))]
fn open_regular(path: &Path) -> std::io::Result<File> {
    if path.symlink_metadata()?.file_type().is_symlink() {
        return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    }
    File::open(path)
}

#[derive(Deserialize)]
pub struct Metadata { pub name: String, pub description: String }

pub fn metadata(text: &str) -> Result<Metadata, WorkspaceError> {
    let mut lines = text.trim_start_matches('\u{feff}').split_inclusive('\n');
    if lines.next().map(str::trim)!=Some("---") { return Err(error("INVALID_SKILL", "SKILL.md needs YAML frontmatter")); }
    let mut header = String::new();
    let mut closed = false;
    for line in lines {
        if matches!(line.trim(), "---"|"...") {closed=true; break;}
        if header.len()+line.len()>MAX_HEADER {return Err(error("INVALID_SKILL", "Skill metadata exceeds 32KiB"));}
        header.push_str(line);
    }
    if !closed {return Err(error("INVALID_SKILL", "Unterminated skill metadata"));}
    // No alias/tag expansion is needed for names/descriptions. Fail closed on
    // these constructs instead of permitting resource amplification via YAML.
    static SPECIAL:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let special=SPECIAL.get_or_init(||regex::Regex::new(r"(?m)^\s*(?:[A-Za-z0-9_-]+:|-)\s*[&*!][A-Za-z0-9_!]").expect("static regex"));
    if special.is_match(&header) {return Err(error("INVALID_SKILL", "YAML anchors, aliases and explicit tags are unsupported"));}
    let mut meta:Metadata=serde_yaml_ng::from_str(&header)
        .map_err(|_|error("INVALID_SKILL", "Invalid skill metadata; name and description are required"))?;
    meta.name=meta.name.trim().to_string(); meta.description=meta.description.trim().to_string();
    if meta.name.is_empty() || meta.name.len()>512 || meta.description.is_empty() || meta.description.len()>8192
        || meta.name.chars().any(|c|c.is_control() || c=='/' || c=='\\') {
        return Err(error("INVALID_SKILL", "Skill name/description is empty, unsafe or too long"));
    }
    Ok(meta)
}

pub fn query(raw:&str)->Result<String,WorkspaceError>{
    let value=raw.trim();
    let value=if let Some(rest)=value.strip_prefix("${") {
        rest.strip_suffix('}').ok_or_else(||error("INVALID_SKILL_QUERY", "Use ${name with spaces} as one query"))?
    }else{value.strip_prefix('$').unwrap_or(value)};
    if value.len()>1024 {return Err(error("INVALID_SKILL_QUERY", "Skill query exceeds 1024 bytes"));}
    Ok(value.trim().to_lowercase())
}

pub fn uri_component(text:&str)->String{
    text.bytes().map(|b|if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {(b as char).to_string()}else{format!("%{b:02X}")}).collect()
}

pub fn safe_reference(raw:&str)->bool{
    if raw.is_empty() || raw.len()>1024 || raw.contains(['\\',':','\0']) || Path::new(raw).is_absolute(){return false;}
    let path=Path::new(raw);
    let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
    if !["md","txt","rst","py","sh","ps1","js","ts","json","yaml","yml","toml"].contains(&ext.as_str()){return false;}
    !path.components().any(|part|match part {
        Component::Normal(s)=>{let s=s.to_string_lossy().to_lowercase();
            s.starts_with('.') || ["secret","credential","cookie","token"].iter().any(|word|s.split(['.','-','_']).any(|p|p==*word || p==format!("{word}s")))},
        Component::RootDir|Component::Prefix(_)=>true,
        _=>false,
    })
}
