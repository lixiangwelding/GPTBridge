import type { RuntimeStatus } from "$lib/types";

export type TaskState = "running" | "attention" | "queued" | "ready" | "paused" | "done";
export type TaskFilter = TaskState | "all";
export interface TaskCursor { updated: number; task_id: string; workspace_id: string }
export interface TaskQuery { query: string; filter: TaskFilter; cursor: TaskCursor | null; limit: number }
export interface TaskRow {
  task_id: string; goal: string; state: string; display_state: TaskState;
  revision: number; created: number; updated: number; summary: string; next_step: string;
  running_jobs: number; queued_jobs: number; unknown_jobs: number;
  workspace_id: string; workspace_name: string;
}
export interface Snapshot {
  tasks: TaskRow[]; counts: Record<TaskState, number>; matched_total: number;
  next_cursor: TaskCursor | null; observed_at: number; partial: boolean;
  warnings: { workspace_id: string; name: string; message: string }[];
  projects: { workspace_id: string; counts: Record<TaskState, number>; observed_at: number }[];
  source: string;
}
export interface JobSummary {
  job_id: string; task_id: string; request_id: string; status: string;
  exit_code: number | null; detail: string; waiting_for: string | null;
  created: number; updated: number; command_ok: boolean | null;
}
export interface TaskDetail {
  task_id: string; goal: string; state: string; revision: number; created: number; updated: number;
  workspace_id: string; workspace_name: string; workspace_path: string; handoff: string;
  checkpoint: { summary?: string; next_step?: string; remaining_issues?: unknown; source_revision?: unknown };
  steps: Record<string, { state: string; notes?: string; evidence?: unknown[] }>;
  step_counts: Record<string, number>; next_cursor: string | null;
  jobs: { jobs: JobSummary[]; truncated: boolean; limit: number };
}
export interface Skill {
  skill_id: string; name: string; description: string; scope: string;
  sha256: string; model_invocable: boolean; source_manual_only: boolean; user_disabled: boolean; source_ref: string;
}
export interface ToolDefinition { name: string; description: string; inputSchema: Record<string, unknown> }
export interface SkillPage {
  skills: Skill[]; tools: ToolDefinition[]; matches: number; next_cursor: string | null;
  preferences_revision: number; scan_truncated: boolean; skipped_entries: number;
}
export interface SkillContent {
  name: string; content: string; source_path: string; sha256: string;
  next_offset: number | null; offset: number; start_line: number; end_line: number; total_bytes: number;
}
export interface JobOutput {
  content: string; next_offset?: number; poll_offset?: number;
  retained_bytes?: number; may_be_truncated?: boolean; job_status?: string;
}
export interface Connections {
  workspace_id: string; mcp: RuntimeStatus; actions: RuntimeStatus;
  mcp_reachable: boolean; actions_reachable: boolean; client_connections: null; checked_at: number; note: string;
}
export interface AppInfo { name: string; subtitle: string; version: string; legacy_identifier: string; backend: string }
