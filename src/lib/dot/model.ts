import type { WorkspaceProfile } from "../types";
import type { Connections, ProtocolDiagnostics, TaskDetail, TaskRow } from "../taskdock/types";

export interface DiagnosticRow { label: string; value: string; tone: "neutral" | "ok" | "warning" }

/** TCP observations remain separate from authenticated MCP protocol evidence and client discovery. */
export function connectionRows(connection: Connections | null, profile: WorkspaceProfile, diagnostics: ProtocolDiagnostics | null = null): DiagnosticRow[] {
  const matches = connection?.workspace_id === profile.id;
  const probe = diagnostics?.workspace_id === profile.id ? diagnostics : null;
  const probeValue = (status: string | undefined) => status === "passed" ? "已验证" : status === "failed" ? "检查失败" : status === "unavailable" ? "无法检查" : "尚未检查";
  const probeTone = (status: string | undefined): DiagnosticRow["tone"] => status === "passed" ? "ok" : status ? "warning" : "neutral";
  return [
    { label: "本地 MCP 端口", value: !matches ? "尚未观察" : connection.mcp_reachable ? "端口可达" : "端口不可达", tone: !matches ? "neutral" : connection.mcp_reachable ? "ok" : "warning" },
    { label: "入口配置", value: (profile.runtime.gateway_workspace_ids?.length ?? 0) > 0 ? "共享入口 · 已保存配置" : "单工作区 · 已保存配置", tone: "neutral" },
    { label: "认证", value: probeValue(probe?.auth.status), tone: probeTone(probe?.auth.status) },
    { label: "MCP 握手", value: probeValue(probe?.handshake.status), tone: probeTone(probe?.handshake.status) },
    { label: "服务端工具目录", value: probe?.catalog.status === "passed" ? probe.catalog.count === null ? "已读取 · 工具数量未返回" : `${probe.catalog.count} 个工具 · 已读取` : probeValue(probe?.catalog.status), tone: probeTone(probe?.catalog.status) },
    { label: "客户端目录缓存", value: "本地服务无法确认", tone: "warning" },
    { label: "dot 实际路由", value: "本地服务无法确认", tone: "warning" },
  ];
}

export function policySummary(profile: WorkspaceProfile, diagnostics: ProtocolDiagnostics | null = null) {
  const readonly = ["read-only", "compat-readonly-all", "ai-ops-readonly"].includes(profile.runtime.tool_profile);
  const runtime = diagnostics?.workspace_id === profile.id && diagnostics.runtime_policy.status === "passed" ? diagnostics.runtime_policy : null;
  const security = runtime?.direct_workspace?.security;
  const securityFields = security && typeof security === "object" && !Array.isArray(security) ? security as Record<string, unknown> : null;
  return {
    permission: `${profile.runtime.permission_mode}${readonly ? " · 只读工具" : ""}`,
    profile: profile.runtime.tool_profile,
    configuredReadScope: (profile.runtime.gateway_workspace_ids?.length ?? 0) > 0
      ? "共享入口：根目录内读取"
      : "单工作区：旧读取策略可读外部路径",
    runtime: runtime ? {
      permission: runtime.permission_mode ?? "服务未返回", profile: runtime.tool_profile ?? "服务未返回",
      readScope: typeof securityFields?.read_scope === "string" ? securityFields.read_scope : "服务未返回读取范围",
      isolation: typeof securityFields?.execution_isolation === "string" ? securityFields.execution_isolation : "服务未返回隔离信息",
      sandbox: typeof securityFields?.sandbox_enforced === "boolean" ? securityFields.sandbox_enforced : null,
    } : null,
  };
}

export function workspaceTasks(rows: TaskRow[], workspaceId: string): TaskRow[] {
  return rows.filter(row => row.workspace_id === workspaceId);
}

export function belongsToTask(detail: TaskDetail | null, row: TaskRow | null, workspaceId: string): boolean {
  return !!detail && !!row && detail.workspace_id === workspaceId && row.workspace_id === workspaceId && detail.task_id === row.task_id;
}

export function usagePrompt(profile: WorkspaceProfile): string {
  return `使用当前 GPTBridge MCP 连接在已登记工作区 ${profile.id} 中完成任务。先确认 tools/list 与 server_info 返回的工具和实际权限。共享入口的业务调用携带 workspace_id。使用原生 read_file → 带 expected_hashes 与稳定 request_id 的 apply_patch → exec_command → 从原 job_id 读取输出。按现有 task_open / task_checkpoint 生命周期持久记账，写入和命令携带 task_id；无需外部 Agent。结果不明先查询原请求，核对退出码和实际文件，不盲目重放。当前命令隔离是 policy_only，无 OS 沙箱；不要扩大权限。客户端目录刷新和是否路由到 GPTBridge 需在客户端实际验证。`;
}
