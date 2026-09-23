import type { TaskState, TaskRow } from "./types";
export const labels: Record<TaskState, string> = { running: "运行中", attention: "待处理", queued: "排队中", ready: "待接续", paused: "已暂停", done: "已完成" };
export const taskKey = (task: Pick<TaskRow, "workspace_id" | "task_id">) => `${task.workspace_id}:${task.task_id}`;
export function relativeTime(time: number): string {
  if (!Number.isFinite(time) || time <= 0) return "未知";
  const minutes = Math.max(0, Math.floor((Date.now() - time) / 60000));
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)}h`;
  return `${Math.floor(minutes / 1440)}d`;
}
export const timestamp = (time: number) => time > 0 ? new Date(time).toLocaleString("zh-CN", { hour12: false }) : "未知";
export function stage(task: TaskRow): string {
  if (task.running_jobs > 0) return `${task.running_jobs} 个作业执行中`;
  if (task.queued_jobs > 0) return `${task.queued_jobs} 个作业等待资源`;
  if (task.unknown_jobs > 0) return "结果待核实，不自动重放";
  if (task.display_state === "ready") return "等待 AI 客户端继续";
  return task.next_step || task.summary || labels[task.display_state];
}
export function readable(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === null || value === undefined) return "";
  if (Array.isArray(value)) return value.map(readable).filter(Boolean).join("\n");
  return JSON.stringify(value, null, 2);
}
