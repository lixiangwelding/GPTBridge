import { invoke, isTauri } from "@tauri-apps/api/core";
import type { WorkspaceProfile, RuntimeStatus } from "$lib/types";
import type { AppInfo, Connections, JobOutput, SkillContent, SkillPage, Snapshot, TaskDetail, TaskQuery } from "./types";

export const nativeAvailable = (): boolean => typeof window !== "undefined" && isTauri();
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!nativeAvailable()) throw new Error("原生服务未连接。请在 TaskDock 桌面应用中打开；浏览器预览不会替换为演示数据。");
  return invoke<T>(command, args);
}
export const getAppInfo = () => call<AppInfo>("taskdock_app_info");
export const getProjects = () => call<WorkspaceProfile[]>("list_workspaces");
export const addProject = (path: string, name?: string) => call<WorkspaceProfile>("taskdock_add_project", { path, name: name?.trim() || null });
export const getSnapshot = (workspaceId: string | null, query: TaskQuery) => call<Snapshot>("taskdock_snapshot", { workspaceId, query });
export const getTask = (workspaceId: string, taskId: string) => call<TaskDetail>("taskdock_task", { workspaceId, taskId });
export const createTask = (workspaceId: string, goal: string, requestId: string) => call<{ task_id: string; workspace_id: string; requires_client: boolean }>("taskdock_create", { workspaceId, goal, requestId });
export const getConnections = (workspaceId: string) => call<Connections>("taskdock_connections", { workspaceId });
export const startService = (workspaceId: string, service: "mcp" | "actions") => call<RuntimeStatus>("taskdock_start_service", { workspaceId, service });
export const getSkills = (workspaceId: string, query = "", cursor: string | null = null) => call<SkillPage>("taskdock_skills", { workspaceId, query, cursor });
export const readSkill = (workspaceId: string, skillId: string, offset = 0, expectedSha256: string | null = null) => call<SkillContent>("taskdock_skill_read", { workspaceId, skillId, offset, expectedSha256 });
export const setSkillPreference = (workspaceId: string, skillId: string, enabled: boolean, expectedRevision: number) => call<{ preferences_revision: number }>("taskdock_skill_preference", { workspaceId, skillId, enabled, expectedRevision });
export const getJobOutput = (workspaceId: string, taskId: string, jobId: string, stream: "stdout" | "stderr", offset = 0) => call<JobOutput>("taskdock_job_output", { workspaceId, taskId, jobId, stream, offset });

export async function copyText(text: string): Promise<void> {
  if (!navigator.clipboard) throw new Error("剪贴板不可用，请选中文本后复制。");
  await navigator.clipboard.writeText(text);
}
