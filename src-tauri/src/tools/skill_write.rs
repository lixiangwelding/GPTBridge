//! Explicit local-owner authorization for patching Skill libraries outside the
//! repository root. This module never accepts an absolute path from MCP.
use std::path::PathBuf;

use coding_tools_personal_runtime::digest;
use serde_json::{json, Value};

use super::{
    context::ToolContext,
    personal_patch,
    workspace::{tool_ok, Workspace, WorkspaceError},
};
use crate::workspace::{validate_skill_write_roots, SkillWriteRootConfig};

pub const READ_TOOLS: &[&str] = &["list_skill_write_roots"];
pub const WRITE_TOOLS: &[&str] = &["apply_skill_patch"];

pub fn contains(name: &str) -> bool {
    READ_TOOLS.contains(&name) || WRITE_TOOLS.contains(&name)
}

pub fn definition(name: &str) -> Value {
    match name {
        "list_skill_write_roots" => json!({
            "name": name,
            "title": "List approved Skill write roots",
            "description": "List only Skill directories explicitly approved in this workspace's local desktop settings. This cannot add or change an authorization.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false
            }
        }),
        "apply_skill_patch" => json!({
            "name": name,
            "title": "Apply approved Skill patch",
            "description": "Patch files under one locally approved Skill root. root_id comes from list_skill_write_roots; all patch paths and expected_hashes are relative to that root. Non-dry writes require a stable request_id and full-file SHA preconditions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "root_id": {"type":"string","minLength":1,"maxLength":64},
                    "patch": {"type":"string","minLength":1},
                    "dry_run": {"type":"boolean","default":false},
                    "confirm": {"type":"boolean","default":false},
                    "reason": {"type":"string","default":""},
                    "task_id": {"type":"string","description":"Optional explicit durable task ID in the current repository workspace."},
                    "request_id": {"type":"string","minLength":1,"maxLength":160,"description":"Stable ID per logical mutation; reuse only to retrieve the same attempt."},
                    "expected_hashes": {"type":"object","description":"Every touched root-relative path maps to its current SHA-256, or null for an absent file.","additionalProperties":{"type":["string","null"]}}
                },
                "required": ["root_id","patch"],
                "additionalProperties": false
            },
            "annotations": {
                "readOnlyHint": false,
                "destructiveHint": true,
                "idempotentHint": true,
                "openWorldHint": false
            }
        }),
        _ => json!({}),
    }
}

pub fn call(ctx: &ToolContext, name: &str, args: &Value) -> Result<Value, WorkspaceError> {
    match name {
        "list_skill_write_roots" => list(ctx, args),
        "apply_skill_patch" => apply(ctx, args),
        _ => Err(skill_error(
            "UNKNOWN_SKILL_WRITE_TOOL",
            "Unknown Skill write tool",
            false,
            json!({"tool":name}),
        )),
    }
}

fn list(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    if !args.as_object().is_some_and(serde_json::Map::is_empty) {
        return Err(skill_error(
            "INVALID_SKILL_WRITE_ARGUMENT",
            "list_skill_write_roots takes no arguments",
            false,
            json!({}),
        ));
    }
    let roots = approved_roots(ctx)?;
    Ok(tool_ok(json!({
        "roots": roots.iter().map(|root| json!({
            "root_id": root.config.id,
            "name": root.config.name,
            "path": root.workspace.root_display(),
            "status": "available",
            "write_scope": "relative-text-patch-only"
        })).collect::<Vec<_>>(),
        "count": roots.len(),
        "remote_authorization": false,
        "script_execution": false
    })))
}

fn apply(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    validate_apply_arguments(args)?;
    let root_id = args.get("root_id").and_then(Value::as_str).ok_or_else(|| {
        skill_error(
            "INVALID_SKILL_WRITE_ARGUMENT",
            "root_id is required",
            false,
            json!({}),
        )
    })?;
    let roots = approved_roots(ctx)?;
    if roots.is_empty() {
        return Err(skill_error(
            "SKILL_WRITE_ROOT_NOT_CONFIGURED",
            "No Skill write root is approved in local desktop settings",
            false,
            json!({"root_id":root_id}),
        ));
    }
    let root = roots
        .into_iter()
        .find(|root| root.config.id == root_id)
        .ok_or_else(|| {
            skill_error(
                "SKILL_WRITE_ROOT_NOT_ALLOWED",
                "The requested Skill write root is not approved",
                false,
                json!({"root_id":root_id}),
            )
        })?;

    let mut forwarded = args.clone();
    forwarded
        .as_object_mut()
        .expect("validated object")
        .remove("root_id");
    let root_digest = digest(root.workspace.root().to_string_lossy().as_bytes());
    let namespace = format!("skill-patch:{}:{root_digest}", root.config.id);
    let local_resource = format!("skill-root:{root_digest}");
    let mut result = personal_patch::apply_to(
        ctx,
        &root.workspace,
        &namespace,
        &local_resource,
        &forwarded,
    )?;
    result["skill_write_root_id"] = json!(root.config.id);
    result["skill_write_root_name"] = json!(root.config.name);
    result["skill_write_root_path"] = json!(root.workspace.root_display());
    result["authorization_source"] = json!("local-workspace-settings");
    Ok(result)
}

fn validate_apply_arguments(args: &Value) -> Result<(), WorkspaceError> {
    const ALLOWED: &[&str] = &[
        "root_id",
        "patch",
        "dry_run",
        "confirm",
        "reason",
        "task_id",
        "request_id",
        "expected_hashes",
    ];
    let object = args.as_object().ok_or_else(|| {
        skill_error(
            "INVALID_SKILL_WRITE_ARGUMENT",
            "Skill patch arguments must be an object",
            false,
            json!({}),
        )
    })?;
    if let Some(key) = object.keys().find(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(skill_error(
            "INVALID_SKILL_WRITE_ARGUMENT",
            "Unknown Skill patch argument; roots can only be configured locally",
            false,
            json!({"argument":key}),
        ));
    }
    if !object
        .get("root_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty() && value.len() <= 64)
    {
        return Err(skill_error(
            "INVALID_SKILL_WRITE_ARGUMENT",
            "root_id must be a non-empty string no longer than 64 bytes",
            false,
            json!({}),
        ));
    }
    if !object
        .get("patch")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
    {
        return Err(skill_error(
            "INVALID_SKILL_WRITE_ARGUMENT",
            "patch must be a non-empty string",
            false,
            json!({}),
        ));
    }
    for key in ["dry_run", "confirm"] {
        if object.get(key).is_some_and(|value| !value.is_boolean()) {
            return Err(skill_error(
                "INVALID_SKILL_WRITE_ARGUMENT",
                format!("{key} must be a boolean"),
                false,
                json!({"argument":key}),
            ));
        }
    }
    for key in ["reason", "task_id"] {
        if object.get(key).is_some_and(|value| !value.is_string()) {
            return Err(skill_error(
                "INVALID_SKILL_WRITE_ARGUMENT",
                format!("{key} must be a string"),
                false,
                json!({"argument":key}),
            ));
        }
    }
    if object.get("request_id").is_some_and(|value| {
        !value
            .as_str()
            .is_some_and(|text| !text.is_empty() && text.len() <= 160)
    }) {
        return Err(skill_error(
            "INVALID_SKILL_WRITE_ARGUMENT",
            "request_id must be a non-empty string no longer than 160 bytes",
            false,
            json!({"argument":"request_id"}),
        ));
    }
    if let Some(expected) = object.get("expected_hashes") {
        let hashes = expected.as_object().ok_or_else(|| {
            skill_error(
                "INVALID_SKILL_WRITE_ARGUMENT",
                "expected_hashes must be an object",
                false,
                json!({"argument":"expected_hashes"}),
            )
        })?;
        if let Some((path, _)) = hashes.iter().find(|(_, value)| {
            !value.is_null()
                && !value.as_str().is_some_and(|hash| {
                    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
        }) {
            return Err(skill_error(
                "INVALID_SKILL_WRITE_ARGUMENT",
                "expected_hashes values must be null or a 64-character hexadecimal SHA-256",
                false,
                json!({"path":path}),
            ));
        }
    }
    Ok(())
}

struct ApprovedRoot {
    config: SkillWriteRootConfig,
    workspace: Workspace,
}

fn approved_roots(ctx: &ToolContext) -> Result<Vec<ApprovedRoot>, WorkspaceError> {
    let configs = &ctx.policy.skill_write_roots;
    validate_skill_write_roots(configs).map_err(|message| {
        skill_error(
            "SKILL_WRITE_ROOT_CONFIGURATION_INVALID",
            message,
            false,
            json!({}),
        )
    })?;
    configs
        .iter()
        .cloned()
        .map(|config| {
            let configured = PathBuf::from(&config.path);
            let canonical = configured.canonicalize().map_err(|_| {
                skill_error(
                    "SKILL_WRITE_ROOT_UNAVAILABLE",
                    "Approved Skill write root is no longer available",
                    true,
                    json!({"root_id":config.id}),
                )
            })?;
            if canonical != configured {
                return Err(skill_error(
                    "SKILL_WRITE_ROOT_CHANGED",
                    "Approved Skill write root no longer resolves to its saved canonical path",
                    false,
                    json!({"root_id":config.id}),
                ));
            }
            let workspace = Workspace::new(canonical).map_err(|error| {
                skill_error(
                    "SKILL_WRITE_ROOT_UNAVAILABLE",
                    error.message(),
                    true,
                    json!({"root_id":config.id}),
                )
            })?;
            Ok(ApprovedRoot { config, workspace })
        })
        .collect()
}

fn skill_error(
    code: &'static str,
    message: impl Into<String>,
    retryable: bool,
    details: Value,
) -> WorkspaceError {
    WorkspaceError::ToolDetails {
        code,
        message: message.into(),
        category: "permission",
        retryable,
        details,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn fixture() -> (tempfile::TempDir, ToolContext, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let skills = temp.path().join("skills");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&skills).unwrap();
        let mut ctx = ToolContext::for_test(project, temp.path().join("state")).unwrap();
        ctx.policy.skill_write_roots = vec![SkillWriteRootConfig {
            id: "global-skills".into(),
            name: "Global Skills".into(),
            path: skills
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        }];
        (temp, ctx, skills)
    }

    fn add_patch(request_id: &str) -> Value {
        json!({
            "root_id":"global-skills",
            "patch":"*** Begin Patch\n*** Add File: sample/SKILL.md\n+---\n+name: sample\n+---\n+safe\n*** End Patch",
            "request_id":request_id,
            "expected_hashes":{"sample/SKILL.md":null}
        })
    }

    #[test]
    fn skill_write_requires_local_authorization_and_revocation_is_immediate() {
        let (_temp, mut ctx, skills) = fixture();
        ctx.policy.skill_write_roots.clear();
        let error = apply(&ctx, &add_patch("not-authorized")).unwrap_err();
        assert_eq!(
            error.to_error_value()["code"],
            "SKILL_WRITE_ROOT_NOT_CONFIGURED"
        );
        assert!(!skills.join("sample/SKILL.md").exists());
    }

    #[test]
    fn approved_root_supports_dry_run_hashes_and_idempotent_write() {
        let (_temp, ctx, skills) = fixture();
        let listed = list(&ctx, &json!({})).unwrap();
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["roots"][0]["root_id"], "global-skills");
        assert_eq!(listed["roots"][0]["status"], "available");
        assert_eq!(
            listed["roots"][0]["path"].as_str(),
            Some(skills.canonicalize().unwrap().to_string_lossy().as_ref())
        );
        let mut dry = add_patch("unused-for-dry-run");
        dry["dry_run"] = json!(true);
        let preview = apply(&ctx, &dry).unwrap();
        assert_eq!(preview["expected_hashes"]["sample/SKILL.md"], Value::Null);
        assert!(!skills.join("sample/SKILL.md").exists());

        let first = apply(&ctx, &add_patch("create-sample")).unwrap();
        let replay = apply(&ctx, &add_patch("create-sample")).unwrap();
        assert_eq!(first["change_id"], replay["change_id"]);
        assert_eq!(replay["deduplicated"], true);
        assert_eq!(
            fs::read_to_string(skills.join("sample/SKILL.md")).unwrap(),
            "---\nname: sample\n---\nsafe\n"
        );

        let before = digest(fs::read(skills.join("sample/SKILL.md")).unwrap());
        let update = json!({
            "root_id":"global-skills",
            "patch":"*** Begin Patch\n*** Update File: sample/SKILL.md\n@@\n-safe\n+updated\n*** End Patch",
            "request_id":"update-sample",
            "expected_hashes":{"sample/SKILL.md":before}
        });
        assert_eq!(apply(&ctx, &update).unwrap()["ok"], true);
        assert_eq!(
            fs::read_to_string(skills.join("sample/SKILL.md")).unwrap(),
            "---\nname: sample\n---\nupdated\n"
        );
    }

    #[test]
    fn unapproved_root_and_root_relative_escape_are_rejected() {
        let (_temp, ctx, skills) = fixture();
        let mut wrong = add_patch("wrong-root");
        wrong["root_id"] = json!("other");
        assert_eq!(
            apply(&ctx, &wrong).unwrap_err().to_error_value()["code"],
            "SKILL_WRITE_ROOT_NOT_ALLOWED"
        );

        let outside = skills.parent().unwrap().join("outside.txt");
        let escape = json!({
            "root_id":"global-skills",
            "patch":"*** Begin Patch\n*** Add File: ../outside.txt\n+no\n*** End Patch",
            "request_id":"escape",
            "expected_hashes":{"../outside.txt":null}
        });
        assert!(apply(&ctx, &escape).is_err());
        assert!(!outside.exists());

        let absolute_target = skills.parent().unwrap().join("absolute.txt");
        let absolute = json!({
            "root_id":"global-skills",
            "patch":format!("*** Begin Patch\n*** Add File: {}\n+no\n*** End Patch", absolute_target.display()),
            "request_id":"absolute",
            "expected_hashes":{}
        });
        assert!(apply(&ctx, &absolute).is_err());
        assert!(!absolute_target.exists());
    }

    #[test]
    fn stale_hash_and_changed_replay_are_rejected() {
        let (_temp, ctx, skills) = fixture();
        fs::write(skills.join("existing.md"), "current\n").unwrap();
        let stale_hash = "0".repeat(64);
        let stale = json!({
            "root_id":"global-skills",
            "patch":"*** Begin Patch\n*** Update File: existing.md\n@@\n-current\n+updated\n*** End Patch",
            "request_id":"stale",
            "expected_hashes":{"existing.md":stale_hash}
        });
        assert_eq!(apply(&ctx, &stale).unwrap()["ok"], false);
        assert_eq!(
            fs::read_to_string(skills.join("existing.md")).unwrap(),
            "current\n"
        );

        apply(&ctx, &add_patch("same-request")).unwrap();
        let mut changed = add_patch("same-request");
        changed["patch"] = json!(changed["patch"]
            .as_str()
            .unwrap()
            .replace("+safe\n", "+different\n"));
        assert!(apply(&ctx, &changed)
            .unwrap_err()
            .to_string()
            .contains("IDEMPOTENCY_CONFLICT"));
    }

    #[test]
    fn non_dry_write_requires_complete_hash_preconditions() {
        let (_temp, ctx, skills) = fixture();
        let mut missing = add_patch("missing-precondition");
        missing.as_object_mut().unwrap().remove("expected_hashes");
        let result = apply(&ctx, &missing).unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["error"]["code"], "PRECONDITION_REQUIRED");
        assert!(!skills.join("sample/SKILL.md").exists());
    }

    #[test]
    fn malformed_arguments_fail_before_any_write() {
        let (_temp, ctx, skills) = fixture();
        for args in [
            json!({"root_id":"", "patch":"x"}),
            json!({"root_id":"global-skills", "patch":""}),
            json!({"root_id":"global-skills", "patch":"x", "dry_run":"yes"}),
            json!({"root_id":"global-skills", "patch":"x", "request_id":false}),
            json!({"root_id":"global-skills", "patch":"x", "expected_hashes":[]}),
            json!({"root_id":"global-skills", "patch":"x", "expected_hashes":{"a":"bad"}}),
        ] {
            assert_eq!(
                apply(&ctx, &args).unwrap_err().to_error_value()["code"],
                "INVALID_SKILL_WRITE_ARGUMENT"
            );
        }
        assert!(!skills.join("x").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected_for_existing_and_new_targets() {
        use std::os::unix::fs::symlink;

        let (_temp, ctx, skills) = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("target.md"), "outside\n").unwrap();
        symlink(outside.path().join("target.md"), skills.join("linked.md")).unwrap();
        symlink(outside.path(), skills.join("linked-dir")).unwrap();

        for (request, path) in [
            ("existing-link", "linked.md"),
            ("parent-link", "linked-dir/new.md"),
        ] {
            let args = json!({
                "root_id":"global-skills",
                "patch":format!("*** Begin Patch\n*** Add File: {path}\n+blocked\n*** End Patch"),
                "request_id":request,
                "expected_hashes":{}
            });
            assert!(apply(&ctx, &args).is_err());
        }
        assert_eq!(
            fs::read_to_string(outside.path().join("target.md")).unwrap(),
            "outside\n"
        );
        assert!(!outside.path().join("new.md").exists());
    }
}
