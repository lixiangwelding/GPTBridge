use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};

use crate::audit::{current_time_ms, AuditRequestContext};
use crate::tools::context::ToolContext;
use crate::tools::policy::{validate_tool_arguments_for_workspace, PolicyError};
use crate::tools::workspace::{tool_err, tool_err_code, tool_ok, WorkspaceError};
use crate::tools::{exec, file, git, history, image_tool, patch, session};

/// A dangerous-mode permission acknowledgement cannot override hard runtime
/// boundaries. Use the same cwd, policy and resolver as the eventual command.
fn executable_permission_failure(ctx: &ToolContext, args: &Value) -> Option<Value> {
    if !ctx.policy.skip_permission_gates()
        || args.get("tool_name").and_then(Value::as_str) != Some("exec_command")
        || args.get("permission").and_then(Value::as_str) != Some("privileged_executable") {
        return None;
    }
    let requested = args.get("arguments").unwrap_or(&Value::Null);
    let requested = apply_default_cwd(ctx, "exec_command", requested);
    let failure = match validate_tool_arguments_for_workspace("exec_command", &requested, &ctx.policy, Some(&ctx.workspace)) {
        Err(error) => Some(policy_tool_err(error)),
        Ok(()) => exec::preflight_executable(ctx, &requested).err().map(tool_err),
    };
    failure.map(|mut output| {
        output["status"] = json!("denied");
        output["grant_id"] = Value::Null;
        output["expires_at"] = Value::Null;
        output["preflight_only"] = json!(true);
        output["command_executed"] = json!(false);
        output
    })
}

fn policy_tool_err(err: PolicyError) -> Value {
    let dangerous = err
        .0
        .strip_prefix("DANGEROUS_OPERATION_REQUIRES_CONFIRMATION: ");
    let protected = err.0.strip_prefix("PROTECTED_REPOSITORY_ASSET: ");
    let code = if protected.is_some() {
        "PROTECTED_REPOSITORY_ASSET"
    } else if dangerous.is_some() {
        "DANGEROUS_OPERATION_REQUIRES_CONFIRMATION"
    } else {
        "POLICY_REJECTED"
    };
    let message = protected.or(dangerous).unwrap_or(&err.0).to_string();
    let (reason, suggestion) = if dangerous.is_some() {
        (
            "confirmation_required",
            "为危险操作补充 confirm=true，确认后再重试",
        )
    } else if message.contains("allowlisted") {
        ("command_rejected", "改用允许的命令，或调整工作区命令白名单")
    } else if message.contains("Shell chaining") {
        (
            "shell_syntax_rejected",
            "移除未加引号的 shell 操作符；引号内的程序参数可以保留",
        )
    } else {
        ("policy_rejected", "根据错误信息修正参数后重试")
    };
    tool_err(WorkspaceError::ToolDetails {
        code,
        message,
        category: "policy",
        retryable: false,
        details: json!({
            "stage": "policy",
            "reason": reason,
            "recoverable": reason != "confirmation_required",
            "suggestion": suggestion
        }),
    })
}

/// **唯一工具执行入口**。MCP `tools/call` 与 Actions `POST /actions/{tool}` 必须且只能调用此函数。
/// 策略校验、分发、错误格式在此统一，两路传输层不得另做执行前校验（Actions 仅允许额外的暴露层 `validate_actions_exposure`）。
pub fn call_tool(ctx: &ToolContext, name: &str, args: &Value) -> Value {
    let effective_args = apply_default_cwd(ctx, name, args);
    if let Err(e) = validate_tool_arguments_for_workspace(
        name,
        &effective_args,
        &ctx.policy,
        Some(&ctx.workspace),
    ) {
        return policy_tool_err(e);
    }

    if super::personal::TOOLS.contains(&name) {
        let mut output = match super::personal::call(ctx, name, &effective_args) { Ok(v) => v, Err(e) => tool_err(e) };
        super::skill_discovery::attach(ctx, name, &mut output);
        return output;
    }
    if super::skills::TOOLS.contains(&name) {
        return match super::skills::call(ctx, name, &effective_args) { Ok(v) => v, Err(e) => tool_err(e) };
    }
    let durable_session = effective_args.get("session_id").and_then(Value::as_str).is_some_and(|s|s.starts_with("job-"));
    let durable_output = effective_args.get("output_ref").and_then(Value::as_str).is_some_and(|s|s.starts_with("job:"));
    if matches!(name, "apply_patch" | "patch_check" | "exec_command")
        || (durable_session && matches!(name, "write_stdin" | "kill_session"))
        || (durable_output && name == "read_output")
    {
        // Do not inherit Harness's workspace-global current_task. Every personal write
        // uses its supplied task_id, so another conversation cannot steal its scope.
        let result = match name {
            "apply_patch" => super::personal_patch::apply(ctx, &effective_args),
            "patch_check" => {
                let mut check = effective_args.clone(); check["dry_run"] = json!(true);
                super::personal_patch::apply(ctx, &check)
            }
            "exec_command" => exec::exec_command(ctx, &effective_args),
            "read_output" => super::personal::job_output(ctx, &effective_args),
            "write_stdin" => super::personal::job_poll(ctx, &effective_args, false),
            _ => super::personal::job_poll(ctx, &effective_args, true),
        };
        let mut output = match result { Ok(v) => v, Err(e) => tool_err(e) };
        let outcome = if output["ok"] == false { "rejected" }
            else if output["command_ok"] == false { "failed" }
            else if matches!(output["status"].as_str(), Some("queued"|"running"|"unknown")) { output["status"].as_str().unwrap() }
            else { "succeeded" }.to_string();
        output["operation_status"] = json!(outcome);
        output["task_scope"] = effective_args.get("task_id").cloned().unwrap_or(Value::Null);
        output["recovery_hint"] = json!("Use the existing task_id and job_id/session_id; query uncertain outcomes before a new attempt. No worktree or whole-workspace rollback.");
        return output;
    }

    if crate::harness::tools::TOOL_NAMES.contains(&name) {
        return match crate::harness::tools::call(ctx, name, args) {
            Ok(value) => value,
            Err(error) => attach_harness_status(ctx, tool_err(error), false),
        };
    }

    let task_id = if requires_write_baseline(name, &effective_args) {
        let task = ctx.harness.current_task().ok().flatten();
        if let Some(task) = task {
            if let Err(error) = ctx.harness.check_baseline(&task.id) {
                return attach_harness_status(
                    ctx,
                    tool_err_code(error.code(), error.to_string(), "permission"),
                    false,
                );
            }
            let _ = ctx.harness.record_event(
                &task.id,
                "operation_started",
                Some(name),
                operation_input(args),
                json!({"ok": true, "tracking": "task"}),
            );
            Some(task.id)
        } else {
            None
        }
    } else {
        None
    };

    let operation = if should_log_operation(name) {
        ctx.harness
            .record_operation(
                None,
                task_id.as_deref(),
                name,
                "started",
                json!({"arguments_present": !args.is_null()}),
                json!({"ok": true}),
            )
            .ok()
    } else {
        None
    };

    let ws = &ctx.workspace;
    let result = match name {
        "history_session_bootstrap" => history::bootstrap(ctx, &effective_args),
        "history_session_checkpoint" => history::checkpoint(ctx, &effective_args),
        "history_session_validate" => history::validate(ctx, &effective_args),
        "history_session_search" => history::search(ctx, &effective_args),
        "history_session_read" => history::read(ctx, &effective_args),
        "server_info" => server_info(ctx),
        "check_exec_environment" => check_exec_environment(ctx),
        "exec_health_check" => exec::exec_health_check(ctx),
        "get_default_cwd" => get_default_cwd(ctx),
        "set_default_cwd" => set_default_cwd(ctx, &effective_args),
        "read_file" => file::read_file(ws, &effective_args),
        "list_dir" => file::list_dir(ws, &effective_args),
        "list_files" => file::list_files(ws, &effective_args),
        "search_text" | "grep_text" | "grep" => file::search_text(ws, &effective_args),
        "patch_check" => patch::patch_check(ctx, &effective_args),
        "apply_patch" => patch::apply_patch(ctx, &effective_args),
        "exec_command" => exec::exec_command(ctx, &effective_args),
        "read_output" => session::read_output(&ctx.sessions, &effective_args),
        "write_stdin" => session::write_stdin(&ctx.sessions, &effective_args),
        "kill_session" => session::kill_session(&ctx.sessions, &effective_args),
        "git_status" => git::git_status(ws, &effective_args),
        "git_diff" => git::git_diff(ws, &effective_args),
        "git_log" => git::git_log(ws, &effective_args),
        "git_show" => git::git_show(ws, &effective_args),
        "git_blame" => git::git_blame(ws, &effective_args),
        "view_image" => image_tool::view_image(ws, &effective_args),
        "request_permissions" => {
            if let Some(failure) = executable_permission_failure(ctx, &effective_args) {
                Ok(failure)
            } else if ctx.policy.skip_permission_gates() {
                Ok(tool_ok(json!({
                    "ok": true,
                    "status": "granted",
                    "grant_id": "dangerously-skip-all-permissions",
                    "expires_at": null,
                    "constraints": {
                        "mode": "dangerous",
                        "workspace": ctx.workspace.root_display(),
                        "requested": effective_args
                    },
                    "warnings": [
                        "dangerous permission mode skips permission gates only; workspace, executable and argument checks still apply"
                    ]
                })))
            } else {
                Ok(tool_ok(json!({
                    "ok": false,
                    "status": "unsupported",
                    "grant_id": null,
                    "expires_at": null,
                    "next_actions": [],
                    "error": {
                        "code": "ELICITATION_UNSUPPORTED",
                        "message": "Permission elicitation is not available for this client.",
                        "category": "permission",
                        "retryable": false,
                        "details": { "requested": effective_args }
                    }
                })))
            }
        }
        _ => {
            return tool_err_code(
                "INVALID_ARGUMENT",
                format!("Unknown tool: {name}"),
                "validation",
            )
        }
    };
    let mut output = match result {
        Ok(v) => v,
        Err(e) => tool_err(e),
    };
    super::skill_discovery::attach(ctx, name, &mut output);
    if task_id.is_none()
        && standalone_operation(name)
        && output.get("ok") == Some(&Value::Bool(true))
    {
        let hint = if output.get("status").and_then(Value::as_str) == Some("running") {
            "命令仍在运行；使用 session_id 调用 write_stdin（chars 为空）等待完成；使用 output_refs 调用 read_output 读取输出。"
        } else if output.get("command_ok") == Some(&Value::Bool(false)) {
            "命令未成功；请检查 stderr、exit_code 或调整参数后重试。"
        } else {
            "当前操作已在 standalone 模式完成；如需继续，直接调用下一个开发工具。"
        };
        attach_standalone_metadata(&mut output, hint);
    }
    if let Some(operation) = operation.as_ref() {
        if let Some(object) = output.as_object_mut() {
            object.insert("operation_id".into(), Value::String(operation.id.clone()));
        }
    }
    if output.get("ok").and_then(Value::as_bool) == Some(false) {
        output = attach_harness_status(ctx, output, task_id.is_none());
    }
    if let Some(task_id) = task_id.as_deref() {
        let succeeded = output.get("ok").and_then(Value::as_bool) == Some(true);
        let _ = ctx.harness.record_event(
            task_id,
            "operation_finished",
            Some(name),
            operation_input(args),
            json!({"ok": succeeded, "tool": name}),
        );
        if succeeded {
            let _ = ctx.harness.refresh_expected_state(task_id);
        }
    }
    if let Some(operation) = operation {
        let succeeded = output.get("ok").and_then(Value::as_bool) == Some(true);
        let _ = ctx.harness.record_operation(
            Some(&operation.id),
            task_id.as_deref(),
            name,
            if succeeded { "completed" } else { "failed" },
            operation_input(args),
            json!({
                "ok": succeeded,
                "tool": name,
                "affected_files": output.get("affected_files")
            }),
        );
    }
    output
}

// 审计包装紧贴唯一 dispatcher：执行前固化补全默认 cwd 后的实际参数，用单调时钟计时，
// 执行后记录同一份结果；HTTP 元数据仍由传输入口提供。写审计失败只告警，不改变工具响应。
pub fn call_tool_with_audit(
    ctx: &ToolContext,
    name: &str,
    args: &Value,
    request: &AuditRequestContext,
) -> Value {
    let started_at_ms = current_time_ms();
    let started = Instant::now();
    let audited_args = apply_default_cwd(ctx, name, args);
    let output = call_tool(ctx, name, args);
    let elapsed_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;
    let finished_at_ms = started_at_ms.saturating_add(elapsed_ms);
    if let Some(audit) = ctx.audit_store() {
        if let Err(error) = audit.record_tool_call(
            request,
            ctx.audit_workspace_id(),
            &ctx.workspace_path(),
            name,
            &audited_args,
            &output,
            started_at_ms,
            finished_at_ms,
        ) {
            eprintln!("audit tool record failed: {error}");
        }
    }
    output
}

// 入口拒绝发生在 dispatcher 之前，无法走正常包装；集中在这里落库，使 MCP 与 Actions
// 共享工作区身份、状态分类和 fail-open 策略。
pub fn record_tool_rejection_with_audit(
    ctx: &ToolContext,
    request: &AuditRequestContext,
    name: &str,
    args: &Value,
    error_code: &str,
    error_message: &str,
) {
    let Some(audit) = ctx.audit_store() else {
        return;
    };
    if let Err(error) = audit.record_tool_rejection(
        request,
        ctx.audit_workspace_id(),
        &ctx.workspace_path(),
        name,
        args,
        error_code,
        error_message,
    ) {
        eprintln!("audit tool rejection record failed: {error}");
    }
}

fn apply_default_cwd(ctx: &ToolContext, name: &str, args: &Value) -> Value {
    if args.get("task_id").is_some() || super::personal::TOOLS.contains(&name) { return args.clone(); }
    let base = if ctx.default_cwd_path() == ctx.workspace.root() {
        ".".to_string()
    } else {
        ctx.default_cwd_display()
    };
    if base == "." {
        return args.clone();
    }

    let mut effective = args.clone();
    match name {
        "exec_command" if effective.get("workdir").is_none() && effective.get("cwd").is_none() => {
            effective["workdir"] = Value::String(base.clone());
        }
        "list_dir" | "list_files" | "git_status" | "git_log" => {
            let path = effective.get("path").and_then(Value::as_str).unwrap_or(".");
            effective["path"] = Value::String(prefix_relative_path(&base, path));
        }
        "read_file" | "search_text" | "grep_text" | "grep" | "git_blame" | "view_image" => {
            if let Some(path) = effective.get("path").and_then(Value::as_str) {
                effective["path"] = Value::String(prefix_relative_path(&base, path));
            }
        }
        "git_diff" => {
            if let Some(path) = effective.get("path").and_then(Value::as_str) {
                effective["path"] = Value::String(prefix_relative_path(&base, path));
            }
            if let Some(paths) = effective.get("paths").and_then(Value::as_array).cloned() {
                effective["paths"] = Value::Array(
                    paths
                        .iter()
                        .map(|path| {
                            path.as_str()
                                .map(|value| Value::String(prefix_relative_path(&base, value)))
                                .unwrap_or_else(|| path.clone())
                        })
                        .collect(),
                );
            }
        }
        "apply_patch" | "patch_check" => {
            if let Some(patch) = effective.get("patch").and_then(Value::as_str) {
                effective["patch"] = Value::String(prefix_patch_paths(&base, patch));
            }
            if let Some(hashes) = effective.get("expected_hashes").and_then(Value::as_object).cloned() {
                effective["expected_hashes"] = Value::Object(hashes.into_iter().map(|(path, hash)| (prefix_relative_path(&base, &path), hash)).collect());
            }
        }
        _ => {}
    }
    effective
}

fn prefix_relative_path(base: &str, path: &str) -> String {
    if path == "." || path.is_empty() {
        return base.to_string();
    }
    if Path::new(path).is_absolute() || path.starts_with("..") {
        return path.to_string();
    }
    format!("{base}/{}", path.trim_start_matches("./"))
}

fn prefix_patch_paths(base: &str, patch: &str) -> String {
    patch
        .lines()
        .map(|line| {
            for marker in ["--- a/", "+++ b/", "*** Add File: ", "*** Update File: ", "*** Delete File: "] {
                if let Some(path) = line.strip_prefix(marker) {
                    return format!("{marker}{}", prefix_relative_path(base, path));
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn requires_write_baseline(name: &str, args: &Value) -> bool {
    match name {
        "exec_command" => true,
        "apply_patch" => !args
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
}

fn standalone_operation(name: &str) -> bool {
    matches!(name, "patch_check" | "apply_patch" | "exec_command")
}

fn should_log_operation(name: &str) -> bool {
    standalone_operation(name)
        || matches!(
            name,
            "git_status" | "git_diff" | "git_log" | "git_show" | "git_blame"
        )
}

fn operation_input(args: &Value) -> Value {
    json!({
        "arguments_present": !args.is_null(),
        "reason": args.get("reason")
    })
}

fn attach_harness_status(ctx: &ToolContext, mut output: Value, standalone: bool) -> Value {
    if let Ok(mut status) = ctx.harness.status() {
        if standalone && status.task_id.is_none() {
            status.next_actions.clear();
        }
        status.next_actions = filter_exposed_actions(ctx, status.next_actions);
        if let Some(object) = output.as_object_mut() {
            object.insert(
                "harness".into(),
                serde_json::to_value(status).unwrap_or_else(|_| {
                    json!({
                        "status": "unavailable",
                        "reason": "无法序列化 Harness 状态"
                    })
                }),
            );
            if standalone {
                attach_standalone_metadata(
                    &mut output,
                    "命令未成功；请检查 stderr、exit_code 或调整参数后重试。",
                );
            }
        }
    }
    output
}

fn attach_standalone_metadata(output: &mut Value, recovery_hint: &str) {
    if let Some(object) = output.as_object_mut() {
        object.insert("harness_mode".into(), Value::String("standalone".into()));
        object.insert("task_required".into(), Value::Bool(false));
        object.insert("next_actions".into(), json!([]));
        object.insert(
            "recovery_hint".into(),
            Value::String(recovery_hint.to_string()),
        );
    }
}

fn filter_exposed_actions(ctx: &ToolContext, actions: Vec<String>) -> Vec<String> {
    let exposed = crate::tools::registry::exposed_tool_names(&ctx.tool_profile);
    actions
        .into_iter()
        .filter(|action| exposed.contains(&action.as_str()))
        .collect()
}

pub fn server_info(ctx: &ToolContext) -> Result<Value, WorkspaceError> {
    let tools = crate::tools::registry::exposed_tool_names(&ctx.tool_profile);
    Ok(tool_ok(json!({
        "server": "coding-tools-mcp",
        "title": "Coding Tools MCP Personal",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": "2025-06-18",
        "workspace": ctx.workspace.root_display(),
        "permission_mode": ctx.permission_mode,
        "default_cwd": ctx.default_cwd_display(),
        "network_allowed": ctx.policy.network_allowed(),
        "tool_profile": ctx.tool_profile,
        "auth_enabled": ctx.auth.auth_enabled(),
        "auth_type": ctx.auth.auth_type,
        "endpoint_path": "/mcp",
        "tools": tools,
        "tool_count": tools.len(),
        "tool_contract": super::registry::catalog_contract(&ctx.tool_profile),
        "skill_bridge": {"enabled":ctx.skills.enabled,"tools":super::skills::TOOLS,"local_files":true,
            "textual_dollar_alias":true,"native_dollar_picker":false,"executes_scripts":false},
        "personal_runtime": {"enabled":true,"task_scope":"explicit_task_id","durable_jobs":true,"same_directory":true,"worktree_required":false,
            "limits":{"running":8,"heavy":2,"queued_and_running":32},"config_isolated":true,"raw_transcript_capture":"only_explicitly_supplied_text"}
    })))
}

pub fn check_exec_environment(ctx: &ToolContext) -> Result<Value, WorkspaceError> {
    Ok(tool_ok(json!({
        "workspace": ctx.workspace.root_display(),
        "permission_mode": ctx.permission_mode,
        "network_allowed": ctx.policy.network_allowed(),
        "landlock_enabled": false,
        "filesystem_sandbox": {
            "available": false,
            "enforced": false,
            "default_scope": "workspace",
            "host_scope_available": false
        },
        "global_tmp_write": if ctx.permission_mode == "dangerous" { "allowed" } else { "tmp-prefix" },
        "workspace_exec_available": true,
        "workspace_exec_sandbox_enforced": false,
        "workspace_exec_boundary": "policy_only",
        "system_command_allowlist": ctx.policy.allowed_commands.iter().cloned().collect::<Vec<_>>(),
        "workspace_local_entries": {
            "enabled": ctx.policy.workspace_local_entries,
            "script_extensions": ctx.policy.workspace_script_extensions.iter().cloned().collect::<Vec<_>>(),
            "resolution": "workdir_first"
        },
        // Backward-compatible alias for older MCP clients.
        "allowed_commands": ctx.policy.allowed_commands.iter().cloned().collect::<Vec<_>>(),
        "warnings": ["Workspace 子进程当前允许执行，但尚未启用操作系统级文件系统沙箱"]
    })))
}

pub fn get_default_cwd(ctx: &ToolContext) -> Result<Value, WorkspaceError> {
    Ok(tool_ok(json!({
        "workspace": ctx.workspace.root_display(),
        "default_cwd": ctx.default_cwd_display(),
        "resolved_cwd": ctx.default_cwd_path().display().to_string()
    })))
}

pub fn set_default_cwd(ctx: &ToolContext, args: &Value) -> Result<Value, WorkspaceError> {
    let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let resolved = ctx.workspace.resolve_existing(path)?;
    if !resolved.path.is_dir() {
        return Err(WorkspaceError::not_a_directory(
            "Default cwd must be a directory",
        ));
    }
    ctx.set_default_cwd(resolved.path.clone());
    Ok(tool_ok(json!({
        "workspace": ctx.workspace.root_display(),
        "default_cwd": resolved.display,
        "resolved_cwd": resolved.path.display().to_string()
    })))
}
