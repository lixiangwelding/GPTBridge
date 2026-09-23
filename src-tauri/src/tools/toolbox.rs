//! Read-only, bounded diagnostics. These tools never grant authority or run commands.
use std::collections::{BTreeMap, BTreeSet};
use std::time::UNIX_EPOCH;

use serde_json::{json, Value};

use super::context::ToolContext;
use super::workspace::{tool_err, tool_ok, WorkspaceError};

pub(super) const TOOLS: &[&str] = &[
    "check_command", "tool_catalog_check", "read_files", "stat_path",
];

pub(super) fn definition(name: &str) -> Value {
    let (description, properties, required) = match name {
        "check_command" => (
            "Check an exec_command request with the same local policy and executable resolver, without executing it or granting permission. Passing does not prove upstream client authorization, worker availability or command success.",
            json!({"arguments":super::registry::input_schema("exec_command")}),
            vec!["arguments"],
        ),
        "tool_catalog_check" => (
            "Compare client-observed tool names and input fields with the running server. Declare whether the supplied list is complete; a names-only match never proves full schema synchronization. No refresh or settings change is performed.",
            json!({
                "client_tools":{"type":"array","maxItems":256,"items":{"type":"object","properties":{
                    "name":{"type":"string","minLength":1,"maxLength":128},
                    "input_fields":{"type":"array","maxItems":128,"items":{"type":"string","minLength":1,"maxLength":128}}
                },"required":["name"],"additionalProperties":false}},
                "complete":{"type":"boolean","default":false},
                "client_schema_sha256":{"type":"string","pattern":"^[0-9a-fA-F]{64}$"}
            }),
            vec![],
        ),
        "read_files" => (
            "Read up to 20 workspace-relative UTF-8 files in one bounded call. Returns individual results/errors and existing file SHA metadata; no all-files atomic snapshot is implied. Follow next_index for files not read within the aggregate content budget.",
            json!({
                "paths":{"type":"array","minItems":1,"maxItems":20,"items":{"type":"string","minLength":1,"maxLength":4096}},
                "start_index":{"type":"integer","minimum":0,"maximum":19,"default":0},
                "start_line":{"type":"integer","minimum":1,"default":1},
                "end_line":{"type":"integer","minimum":1},
                "max_bytes_per_file":{"type":"integer","minimum":4,"maximum":65536,"default":16384},
                "max_total_bytes":{"type":"integer","minimum":4,"maximum":262144,"default":131072}
            }),
            vec!["paths"],
        ),
        "stat_path" => (
            "Return metadata for one existing workspace-relative file or directory, without reading its contents, listing a directory, following an escaping symlink or changing permissions.",
            json!({"path":{"type":"string","minLength":1,"maxLength":4096}}),
            vec!["path"],
        ),
        _ => unreachable!("registered toolbox tool"),
    };
    json!({
        "name":name,"title":name,"description":description,
        "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
    })
}

pub(super) fn call(ctx: &ToolContext, name: &str, args: &Value) -> Result<Value, WorkspaceError> {
    if !args.is_object() { return Err(WorkspaceError::invalid_argument("arguments must be an object")); }
    match name {
        "check_command" => {
            let requested = args.get("arguments").filter(|value| value.is_object())
                .ok_or_else(|| WorkspaceError::invalid_argument("arguments must contain an exec_command object"))?;
            Ok(super::dispatch::command_preflight(ctx, requested))
        }
        "tool_catalog_check" => catalog_check(ctx, args),
        "read_files" => read_files(ctx, args),
        "stat_path" => stat_path(ctx, args),
        _ => Err(WorkspaceError::invalid_argument("unknown toolbox tool")),
    }
}

fn bounded_number(args: &Value, key: &str, default: u64, min: u64, max: u64) -> Result<u64, WorkspaceError> {
    match args.get(key) {
        None => Ok(default),
        Some(value) => value.as_u64().filter(|value| (min..=max).contains(value))
            .ok_or_else(|| WorkspaceError::invalid_argument(format!("{key} must be an integer between {min} and {max}"))),
    }
}

fn bounded_text<'a>(value: &'a Value, label: &str, max: usize) -> Result<&'a str, WorkspaceError> {
    value.as_str().filter(|value| !value.is_empty() && value.len() <= max && !value.contains('\0'))
        .ok_or_else(|| WorkspaceError::invalid_argument(format!("{label} must be non-empty text of at most {max} bytes")))
}

fn stat_path(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let path = bounded_text(&args["path"], "path", 4096)?;
    let resolved = ctx.workspace.resolve_existing(path)?;
    let metadata = std::fs::metadata(&resolved.path).map_err(|_| WorkspaceError::not_found("metadata unavailable"))?;
    Ok(tool_ok(json!({
        "path":resolved.display,"exists":true,
        "type":if metadata.is_file() {"file"} else if metadata.is_dir() {"directory"} else {"other"},
        "size_bytes":metadata.len(),
        "modified_ms":metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|time| time.as_millis()),
        "filesystem_readonly":metadata.permissions().readonly(),
        "write_authorized":null,"content_read":false,"permissions_changed":false
    })))
}

fn read_files(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let paths = args["paths"].as_array().filter(|paths| !paths.is_empty() && paths.len() <= 20)
        .ok_or_else(|| WorkspaceError::invalid_argument("paths must contain 1 to 20 entries"))?;
    // Validate the entire request before reading the first item.
    for path in paths { bounded_text(path, "path", 4096)?; }
    let start = bounded_number(args, "start_index", 0, 0, paths.len() as u64 - 1)? as usize;
    let per_file = bounded_number(args, "max_bytes_per_file", 16_384, 4, 65_536)? as usize;
    let budget = bounded_number(args, "max_total_bytes", 131_072, 4, 262_144)? as usize;
    let start_line = bounded_number(args, "start_line", 1, 1, usize::MAX as u64)?;
    let end_line = args.get("end_line").map(|_| bounded_number(args, "end_line", 1, 1, usize::MAX as u64)).transpose()?;
    if end_line.is_some_and(|end| end < start_line) {
        return Err(WorkspaceError::invalid_argument("end_line must not precede start_line"));
    }
    let mut results = Vec::new();
    let mut used = 0usize;
    let mut next = start;
    for (index, path) in paths.iter().enumerate().skip(start) {
        if budget - used < 4 { break; }
        let text = path.as_str().expect("validated text");
        let mut request = json!({"path":text,"start_line":start_line,"max_bytes":per_file.min(budget - used)});
        if let Some(end) = end_line { request["end_line"] = json!(end); }
        // Do not inherit the legacy single-file tool's explicit external reads.
        let result = ctx.workspace.resolve_existing(text).and_then(|resolved| {
            let metadata = std::fs::metadata(&resolved.path).map_err(|_| WorkspaceError::not_found("file unavailable"))?;
            if !metadata.is_file() { return Err(WorkspaceError::invalid_argument("batch reads require regular files")); }
            // Use an isolated strict workspace so a concurrently replaced symlink
            // cannot opt into the legacy explicit external-read behavior.
            let strict = super::workspace::Workspace::new(ctx.workspace.root().to_path_buf())?;
            strict.restrict_reads_to_root();
            super::file::read_file(&strict, &request)
        });
        let mut output = result.unwrap_or_else(tool_err);
        used += output.get("content").and_then(Value::as_str).map(str::len).unwrap_or(0);
        output["requested_path"] = path.clone();
        output["index"] = json!(index);
        results.push(output);
        next = index + 1;
    }
    let failed = results.iter().filter(|item| item["ok"] == false).count();
    let content_truncated = results.iter().any(|item| item["truncated"] == true);
    Ok(tool_ok(json!({
        "results":results,"content_bytes":used,"max_total_bytes":budget,
        "failed_count":failed,"partial":failed > 0 || next < paths.len() || content_truncated,
        "next_index":if next < paths.len() {Some(next)} else {None},
        "snapshot_atomic":false,"file_writes":false
    })))
}

fn catalog_check(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let server = super::registry::catalog_contract(&ctx.tool_profile);
    let expected = server["input_fields"].as_object().expect("registry contract fields");
    let complete = match args.get("complete") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(|| WorkspaceError::invalid_argument("complete must be boolean"))?,
    };
    let empty = Vec::new();
    let supplied = match args.get("client_tools") {
        None => &empty,
        Some(value) => value.as_array().filter(|items| items.len() <= 256)
            .ok_or_else(|| WorkspaceError::invalid_argument("client_tools must be an array of at most 256 entries"))?,
    };
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    let mut missing_fields = BTreeMap::new();
    let mut extra_fields = BTreeMap::new();
    let mut fields_unreported = BTreeSet::new();
    for tool in supplied {
        let name = bounded_text(&tool["name"], "tool name", 128)?;
        if !seen.insert(name.to_owned()) { duplicates.insert(name.to_owned()); }
        let fields = if let Some(fields) = tool.get("input_fields") {
            let fields = fields.as_array().filter(|fields| fields.len() <= 128)
                .ok_or_else(|| WorkspaceError::invalid_argument("input_fields must contain at most 128 strings"))?;
            Some(fields.iter().map(|field| bounded_text(field, "input field", 128).map(str::to_owned))
                .collect::<Result<BTreeSet<_>, _>>()?)
        } else { None };
        let Some(wanted) = expected.get(name) else { unknown.insert(name.to_owned()); continue; };
        let Some(fields) = fields else { fields_unreported.insert(name.to_owned()); continue; };
        let wanted: BTreeSet<_> = wanted.as_array().expect("field array").iter().map(|field| field.as_str().unwrap().to_owned()).collect();
        let missing: Vec<_> = wanted.difference(&fields).cloned().collect();
        let extra: Vec<_> = fields.difference(&wanted).cloned().collect();
        if !missing.is_empty() { missing_fields.insert(name.to_owned(), missing); }
        if !extra.is_empty() { extra_fields.insert(name.to_owned(), extra); }
    }
    let missing: Vec<_> = expected.keys().filter(|name| !seen.contains(*name)).cloned().collect();
    let hash_matches = match args.get("client_schema_sha256") {
        None => None,
        Some(value) => {
            let hash = value.as_str().filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .ok_or_else(|| WorkspaceError::invalid_argument("client_schema_sha256 must be 64 hexadecimal characters"))?;
            Some(hash.eq_ignore_ascii_case(server["schema_sha256"].as_str().unwrap()))
        }
    };
    let drift = !duplicates.is_empty() || !unknown.is_empty() || !missing_fields.is_empty()
        || !extra_fields.is_empty() || hash_matches == Some(false) || (complete && !missing.is_empty());
    let fields_match = complete && !drift && fields_unreported.is_empty() && missing.is_empty();
    let synchronized = if drift {Some(false)} else if fields_match && hash_matches == Some(true) {Some(true)} else {None};
    let status = if drift {"drift_detected"} else if synchronized == Some(true) {"synchronized"}
        else if fields_match {"names_and_fields_match"} else {"unverified"};
    Ok(tool_ok(json!({
        "status":status,"client_catalog_synchronized":synchronized,
        "client_report_complete":complete,"client_tool_count":supplied.len(),
        "missing_tools":missing,"missing_tools_definitive":complete,
        "unexpected_tools":unknown,"duplicate_tools":duplicates,
        "missing_input_fields":missing_fields,"unexpected_input_fields":extra_fields,
        "fields_not_reported":fields_unreported,"schema_hash_matches":hash_matches,
        "server_contract":server,"settings_changed":false,
        "refresh_hint":"Refresh the client's tools/list catalog and compare the full names, fields and schema fingerprint. Server restart alone is not proof of client synchronization."
    })))
}

#[cfg(test)]
#[path = "toolbox_tests.rs"]
mod tests;
