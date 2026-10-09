import type { WorkspaceProfile } from "../types";
import type { Connections, TaskDetail, TaskRow } from "../taskdock/types";

export interface DiagnosticRow { label: string; value: string; tone: "neutral" | "ok" | "warning" }

/** Connection API probes TCP reachability, not authentication or client discovery. */
export function connectionRows(connection: Connections | null, profile: WorkspaceProfile): DiagnosticRow[] {
  const matches = connection?.workspace_id === profile.id;
  return [
    { label: "本地 MCP 端口", value: !matches ? "尚未观察" : connection.mcp_reachable ? "端口可达" : "端口不可达", tone: !matches ? "neutral" : connection.mcp_reachable ? "ok" : "warning" },
    { label: "入口配置", value: (profile.runtime.gateway_workspace_ids?.length ?? 0) > 0 ? "共享入口 · 已保存配置" : "单工作区 · 已保存配置", tone: "neutral" },
    { label: "认证与握手", value: "未验证", tone: "warning" },
    { label: "客户端工具目录 / dot 路由", value: "未验证", tone: "warning" },
  ];
}

export function policySummary(profile: WorkspaceProfile) {
  const readonly = ["read-only", "compat-readonly-all", "ai-ops-readonly"].includes(profile.runtime.tool_profile);
  return {
    permission: `${profile.runtime.permission_mode}${readonly ? " · 只读工具" : ""}`,
    profile: profile.runtime.tool_profile,
    configuredReadScope: (profile.runtime.gateway_workspace_ids?.length ?? 0) > 0
      ? "共享入口：根目录内读取"
      : "单工作区：旧读取策略可读外部路径",
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
