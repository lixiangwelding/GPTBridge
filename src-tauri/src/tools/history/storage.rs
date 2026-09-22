use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use sha2::{Digest, Sha256};

use crate::tools::workspace::{relative_display, Workspace, WorkspaceError, WorkspaceResult};

use super::markdown;
use super::model::{
    HistoryDocument, HistoryIndex, IndexEntry, ManifestEntry, MemoryManifest, MemoryReference,
    MemoryState, ScanReport,
};

pub const DEFAULT_HISTORY_DIR: &str = "docs/history-session";
const STATE_ITEM_LIMIT: usize = 12;
const STATE_TEXT_LIMIT: usize = 512;
const STATE_FOCUS_LIMIT: usize = 2_048;
const STATE_REFERENCE_LIMIT: usize = 8;

pub struct HistoryLock {
    file: File,
}

impl Drop for HistoryLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn resolve_history_dir(
    workspace: &Workspace,
    workspace_root: Option<&str>,
    history_dir: Option<&str>,
) -> WorkspaceResult<PathBuf> {
    if let Some(requested_root) = workspace_root {
        let requested_path = Path::new(requested_root.trim());
        let candidate = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            workspace.root().join(requested_path)
        };
        let requested = candidate
            .canonicalize()
            .map_err(|_| WorkspaceError::invalid_argument("workspace_root does not exist"))?;
        if requested != workspace.root() {
            return Err(WorkspaceError::path_outside_workspace());
        }
    }

    let raw = history_dir.unwrap_or(DEFAULT_HISTORY_DIR).trim();
    if raw.is_empty() || workspace.reject_unsafe_text(raw).is_err() {
        return Err(WorkspaceError::path_outside_workspace());
    }
    let candidate = workspace
        .root()
        .join(raw.replace('/', std::path::MAIN_SEPARATOR_STR));
    ensure_safe_candidate(workspace, &candidate)?;
    if candidate.exists() && !candidate.is_dir() {
        return Err(WorkspaceError::not_a_directory(
            "history_dir must be a directory",
        ));
    }
    Ok(candidate)
}

fn ensure_safe_candidate(workspace: &Workspace, candidate: &Path) -> WorkspaceResult<()> {
    if candidate.exists() || candidate.is_symlink() {
        let resolved = candidate
            .canonicalize()
            .map_err(|_| WorkspaceError::path_outside_workspace())?;
        if !resolved.starts_with(workspace.root()) {
            return Err(WorkspaceError::path_outside_workspace());
        }
        return Ok(());
    }
    let mut ancestor = candidate.parent();
    while let Some(path) = ancestor {
        if path.exists() || path.is_symlink() {
            let resolved = path
                .canonicalize()
                .map_err(|_| WorkspaceError::path_outside_workspace())?;
            if !resolved.starts_with(workspace.root()) {
                return Err(WorkspaceError::path_outside_workspace());
            }
            return Ok(());
        }
        ancestor = path.parent();
    }
    Err(WorkspaceError::path_outside_workspace())
}

pub fn ensure_directory(path: &Path) -> WorkspaceResult<()> {
    fs::create_dir_all(path).map_err(|error| io_error("HISTORY_WRITE_FAILED", error, true))
}

pub fn lock_directory(path: &Path) -> WorkspaceResult<HistoryLock> {
    ensure_directory(path)?;
    let lock_path = path.join(".history.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|error| io_error("HISTORY_LOCK_FAILED", error, true))?;
    FileExt::lock_exclusive(&file).map_err(|error| io_error("HISTORY_LOCK_FAILED", error, true))?;
    Ok(HistoryLock { file })
}

pub fn scan(workspace: &Workspace, history_dir: &Path) -> WorkspaceResult<ScanReport> {
    if !history_dir.exists() {
        return Ok(ScanReport::default());
    }
    ensure_safe_candidate(workspace, history_dir)?;
    let mut report = ScanReport::default();
    let entries =
        fs::read_dir(history_dir).map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
        let file_type = entry
            .file_type()
            .map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), "README.md" | "index.json" | ".history.lock")
            || name.starts_with(".history-tmp-")
        {
            continue;
        }
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            report.invalid_files.push(name);
            continue;
        };
        let is_markdown = path.extension().and_then(|value| value.to_str()) == Some("md");
        let number = stem.parse::<u64>().ok();
        if !is_markdown
            || number.is_none()
            || number == Some(0)
            || number.map(|value| value.to_string()) != Some(stem.to_string())
        {
            report.invalid_files.push(name);
            continue;
        }
        let number = number.expect("validated number");
        let bytes =
            fs::read(&path).map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
        let content = String::from_utf8(bytes).map_err(|error| WorkspaceError::ToolDetails {
            code: "HISTORY_INVALID_UTF8",
            message: "History Markdown must be UTF-8.".into(),
            category: "validation",
            retryable: false,
            details: serde_json::json!({"file": name, "error": error.to_string()}),
        })?;
        if content.trim().is_empty() {
            report.empty_files.push(name.clone());
        }
        report.documents.push(HistoryDocument {
            number,
            path: relative_display(workspace.root(), &path),
            session_key: markdown::metadata(&content, "Session key"),
            created_at: markdown::metadata(&content, "Created"),
            updated_at: markdown::metadata(&content, "Updated"),
            content,
        });
    }
    report.documents.sort_by_key(|document| document.number);
    report.invalid_files.sort();
    report.empty_files.sort();
    report.numbers = report
        .documents
        .iter()
        .map(|document| document.number)
        .collect();
    if let Some(latest) = report.latest_number() {
        let present = report.numbers.iter().copied().collect::<BTreeSet<_>>();
        report.missing_numbers = (1..=latest)
            .filter(|number| !present.contains(number))
            .collect();
    }
    let mut keys = BTreeMap::<String, usize>::new();
    for key in report
        .documents
        .iter()
        .filter_map(|document| document.session_key.as_ref())
    {
        *keys.entry(key.clone()).or_default() += 1;
    }
    report.duplicate_session_keys = keys
        .into_iter()
        .filter_map(|(key, count)| (count > 1).then_some(key))
        .collect();
    Ok(report)
}

pub fn rebuild_index(report: &ScanReport) -> HistoryIndex {
    let duplicates = report
        .duplicate_session_keys
        .iter()
        .collect::<BTreeSet<_>>();
    let mut index = HistoryIndex {
        latest_number: report.latest_number().unwrap_or(0),
        ..HistoryIndex::default()
    };
    for document in &report.documents {
        let Some(session_key) = document.session_key.as_ref() else {
            continue;
        };
        if duplicates.contains(session_key) {
            continue;
        }
        index.sessions.insert(
            session_key.clone(),
            IndexEntry {
                number: document.number,
                path: document.path.clone(),
                created_at: document.created_at.clone().unwrap_or_default(),
                updated_at: document.updated_at.clone().unwrap_or_default(),
            },
        );
    }
    index
}

pub fn read_index(history_dir: &Path) -> WorkspaceResult<Option<HistoryIndex>> {
    read_json(
        &history_dir.join("index.json"),
        "HISTORY_INDEX_INVALID",
        "History index",
    )
}

pub fn write_index(history_dir: &Path, index: &HistoryIndex) -> WorkspaceResult<()> {
    write_json(&history_dir.join("index.json"), index, "history index")
}

pub fn memory_dir(history_dir: &Path) -> PathBuf {
    history_dir.join("memory")
}

pub fn read_manifest(history_dir: &Path) -> WorkspaceResult<Option<MemoryManifest>> {
    read_json(
        &memory_dir(history_dir).join("manifest.json"),
        "HISTORY_MANIFEST_INVALID",
        "History manifest",
    )
}

pub fn write_manifest(history_dir: &Path, manifest: &MemoryManifest) -> WorkspaceResult<()> {
    write_json(
        &memory_dir(history_dir).join("manifest.json"),
        manifest,
        "history manifest",
    )
}

pub fn read_state(history_dir: &Path) -> WorkspaceResult<Option<MemoryState>> {
    read_json(
        &memory_dir(history_dir).join("state.json"),
        "HISTORY_STATE_INVALID",
        "History state",
    )
}

pub fn write_state(history_dir: &Path, state: &MemoryState) -> WorkspaceResult<()> {
    write_json(
        &memory_dir(history_dir).join("state.json"),
        state,
        "history state",
    )
}

pub fn build_manifest(report: &ScanReport) -> MemoryManifest {
    let entries = report
        .documents
        .iter()
        .map(|document| ManifestEntry {
            number: document.number,
            path: document.path.clone(),
            title: markdown::document_title(&document.content, document.number),
            created_at: document.created_at.clone().unwrap_or_default(),
            updated_at: document.updated_at.clone().unwrap_or_default(),
            bytes: document.content.len() as u64,
            sha256: sha256(document.content.as_bytes()),
            keywords: keywords(document),
        })
        .collect::<Vec<_>>();
    let mut digest = Sha256::new();
    for entry in &entries {
        digest.update(entry.number.to_le_bytes());
        digest.update(entry.sha256.as_bytes());
    }
    MemoryManifest {
        version: 2,
        archive_revision: format!("sha256:{:x}", digest.finalize()),
        entries,
    }
}

pub fn build_state(
    report: &ScanReport,
    manifest: &MemoryManifest,
    current_number: Option<u64>,
    timestamp: &str,
    state_revision: u64,
) -> MemoryState {
    let current_document = current_number.and_then(|number| {
        report
            .documents
            .iter()
            .find(|document| document.number == number)
    });
    let current_focus = current_document
        .and_then(|document| latest_user_focus(&document.content))
        .or_else(|| {
            current_document
                .map(|document| markdown::document_title(&document.content, document.number))
        })
        .unwrap_or_else(|| "尚未记录当前焦点".to_string());
    let mut recent_changes = Vec::new();
    let mut open_items = Vec::new();
    // The active summary belongs only to this explicit session. Other sessions
    // remain searchable references, never implicit work assigned to this caller.
    if let Some(document) = current_document {
        let records = markdown::parse_checkpoint_records(&document.content);
        let latest = latest_revisions(&records);
        // remaining_issues/next_actions are the newest checkpoint's snapshot,
        // not a union with old issues which may already have been resolved.
        if let Some(record) = latest.last() {
            push_bounded(&mut open_items, record.remaining_issues.iter().map(String::as_str));
            push_bounded(&mut open_items, record.next_actions.iter().map(String::as_str));
        }
        for record in latest.into_iter().rev() {
            push_bounded(&mut recent_changes, record.files_changed.iter().map(String::as_str));
            push_bounded(&mut recent_changes, record.decisions.iter().map(String::as_str));
        }
    }
    let references = manifest
        .entries
        .iter()
        .rev()
        .filter(|entry| Some(entry.number) != current_number)
        .take(STATE_REFERENCE_LIMIT)
        .map(|entry| MemoryReference {
            number: entry.number,
            path: entry.path.clone(),
            reason: "其他会话的只读背景；按需检索，不属于当前任务".into(),
        })
        .collect();
    MemoryState {
        version: 2,
        state_revision,
        archive_revision: manifest.archive_revision.clone(),
        generated_at: timestamp.to_string(),
        current_session: current_number.and_then(|number| {
            manifest
                .entries
                .iter()
                .find(|entry| entry.number == number)
                .map(|entry| MemoryReference {
                    number: entry.number,
                    path: entry.path.clone(),
                    reason: "当前 ChatGPT 会话档案".into(),
                })
        }),
        current_focus: truncate_text(&current_focus, STATE_FOCUS_LIMIT),
        recent_changes,
        open_items,
        references,
    }
}

fn latest_user_focus(content: &str) -> Option<String> {
    let records = markdown::parse_checkpoint_records(content);
    latest_revisions(&records)
        .last()
        .and_then(|record| {
            if record.raw_user_input.trim().is_empty() {
                (!record.user_intent.trim().is_empty()).then(|| record.user_intent.clone())
            } else {
                Some(record.raw_user_input.clone())
            }
        })
        .or_else(|| {
            markdown::parse_initial_input_records(content)
                .into_iter()
                .max_by_key(|record| record.revision)
                .map(|record| record.raw_user_input)
        })
}

fn latest_revisions(
    records: &[super::model::CheckpointRecord],
) -> Vec<&super::model::CheckpointRecord> {
    // Revision numbers are local to a turn. Select the latest version of each
    // turn, then retain archive append order rather than lexical turn IDs.
    let mut latest = BTreeMap::new();
    for (position, record) in records.iter().enumerate() {
        let should_replace = latest
            .get(record.turn_id.as_str())
            .map(|(_, existing): &(usize, &super::model::CheckpointRecord)| record.revision >= existing.revision)
            .unwrap_or(true);
        if should_replace {
            // A correction replaces its own turn but must not make that old
            // turn newer than an already-recorded subsequent user request.
            let first_position = latest.get(record.turn_id.as_str()).map(|(first, _)| *first).unwrap_or(position);
            latest.insert(record.turn_id.as_str(), (first_position, record));
        }
    }
    let mut ordered = latest.into_values().collect::<Vec<_>>();
    ordered.sort_by_key(|(position, _)| *position);
    ordered.into_iter().map(|(_, record)| record).collect()
}

fn push_bounded<'a>(target: &mut Vec<String>, values: impl Iterator<Item = &'a str>) {
    for value in values {
        let value = value.trim();
        if value.is_empty() || target.iter().any(|existing| existing == value) {
            continue;
        }
        if target.len() >= STATE_ITEM_LIMIT {
            return;
        }
        target.push(truncate_text(value, STATE_TEXT_LIMIT));
    }
}

fn keywords(document: &HistoryDocument) -> Vec<String> {
    let mut values = Vec::new();
    values.push(markdown::document_title(&document.content, document.number));
    values.extend(
        markdown::parse_initial_input_records(&document.content)
            .into_iter()
            .map(|record| record.raw_user_input),
    );
    values.extend(
        markdown::parse_checkpoint_records(&document.content)
            .into_iter()
            .flat_map(|record| [record.user_intent, record.raw_user_input]),
    );
    tokenize(&values.join(" ")).into_iter().take(32).collect()
}

pub fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = value
        .split(|character: char| !character.is_alphanumeric())
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.chars().count() >= 2)
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens
}

pub fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut text = value.chars().take(max_chars).collect::<String>();
    text.push_str("...");
    text
}

pub fn write_markdown(path: &Path, content: &str) -> WorkspaceResult<()> {
    atomic_write(path, content.as_bytes())
}

pub fn sha256(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

fn read_json<T>(path: &Path, invalid_code: &'static str, label: &str) -> WorkspaceResult<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(path).map_err(|error| io_error("HISTORY_READ_FAILED", error, true))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|error| WorkspaceError::ToolDetails {
            code: invalid_code,
            message: format!("{label} is not valid JSON."),
            category: "validation",
            retryable: true,
            details: serde_json::json!({"error": error.to_string()}),
        })
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T, label: &str) -> WorkspaceResult<()> {
    let content =
        serde_json::to_vec_pretty(value).map_err(|error| WorkspaceError::ToolDetails {
            code: "HISTORY_WRITE_FAILED",
            message: format!("Unable to serialize {label}."),
            category: "internal",
            retryable: true,
            details: serde_json::json!({"error": error.to_string()}),
        })?;
    atomic_write(path, &content)
}

fn atomic_write(target: &Path, content: &[u8]) -> WorkspaceResult<()> {
    let parent = target
        .parent()
        .ok_or_else(|| WorkspaceError::invalid_argument("History target has no parent"))?;
    ensure_directory(parent)?;
    let temp = parent.join(format!(".history-tmp-{}", uuid::Uuid::new_v4()));
    let result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(content)?;
        file.sync_all()?;
        atomic_replace(&temp, target)?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|error| io_error("HISTORY_WRITE_FAILED", error, true))
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| io::Error::other(error.to_string()))
    }
}

fn io_error(code: &'static str, error: io::Error, retryable: bool) -> WorkspaceError {
    WorkspaceError::ToolDetails {
        code,
        message: error.to_string(),
        category: "filesystem",
        retryable,
        details: serde_json::json!({"kind": format!("{:?}", error.kind())}),
    }
}


#[cfg(test)]
mod session87_regressions {
    use super::*;
    use super::super::model::CheckpointRecord;

    fn record(turn: &str, revision: u64, focus: &str, issues: &[&str]) -> CheckpointRecord {
        CheckpointRecord {
            turn_id: turn.into(), revision, raw_user_input: focus.into(),
            remaining_issues: issues.iter().map(|v| v.to_string()).collect(),
            ..CheckpointRecord::default()
        }
    }
    fn document(number: u64, records: &[CheckpointRecord]) -> HistoryDocument {
        let mut content = markdown::render_document(number, "fixture", &format!("session-{number}"), "unix:1", None);
        for record in records { content = markdown::append_checkpoint_record(&content, record); }
        HistoryDocument { number, path: format!("docs/history-session/{number}.md"),
            session_key: Some(format!("session-{number}")), created_at: Some("unix:1".into()),
            updated_at: Some("unix:2".into()), content }
    }
    fn state(documents: Vec<HistoryDocument>, current: u64) -> MemoryState {
        let report = ScanReport { documents, ..ScanReport::default() };
        build_state(&report, &build_manifest(&report), Some(current), "unix:3", 1)
    }
    #[test]
    fn session87_state_is_scoped_to_current_session() {
        let value = state(vec![document(1, &[record("mine", 1, "MCP", &["mine"])]),
            document(2, &[record("other", 1, "game", &["foreign"])])], 1);
        assert_eq!(value.open_items, vec!["mine"]);
        assert_eq!(value.current_focus, "MCP");
        assert_eq!(value.references.len(), 1);
    }
    #[test]
    fn session87_new_session_has_no_foreign_tasks() {
        let value = state(vec![document(1, &[record("other", 1, "game", &["foreign"])]), document(2, &[])], 2);
        assert!(value.open_items.is_empty());
        assert!(value.recent_changes.is_empty());
    }
    #[test]
    fn session87_focus_follows_last_turn_not_largest_revision() {
        let doc = document(1, &[record("z-old", 8, "old request", &[]), record("a-new", 1, "new request", &[])]);
        assert_eq!(latest_user_focus(&doc.content).as_deref(), Some("new request"));
    }
    #[test]
    fn session87_resolved_items_do_not_resurface() {
        let value = state(vec![document(1, &[record("z-old", 1, "old", &["resolved issue"]), record("a-new", 1, "fixed", &[])])], 1);
        assert!(value.open_items.is_empty(), "{:?}", value.open_items);
    }
    #[test]
    fn session87_latest_revisions_keep_append_order() {
        let records = vec![record("z-old", 1, "old", &[]), record("a-new", 1, "new", &[])];
        let latest = latest_revisions(&records);
        assert_eq!(latest.iter().map(|r| r.turn_id.as_str()).collect::<Vec<_>>(), vec!["z-old", "a-new"]);
    }
    #[test]
    fn session87_correcting_an_old_turn_does_not_replace_newer_focus() {
        let records = vec![record("old", 1, "old", &["old issue"]),
            record("new", 1, "new", &[]), record("old", 2, "corrected old", &["old issue"])];
        let value = state(vec![document(1, &records)], 1);
        assert_eq!(value.current_focus, "new");
        assert!(value.open_items.is_empty());
        let latest = latest_revisions(&records);
        assert_eq!(latest[0].revision, 2);
        assert_eq!(latest[1].turn_id, "new");
    }

}
