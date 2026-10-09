import type { JobOutput } from "../taskdock/types";

export interface OutputIdentity {
  workspace_id: string; task_id: string; job_id: string; stream: "stdout" | "stderr";
}
export interface OutputPage {
  content: string; offset: number; next: number; retainedBytes: number | null;
  truncated: boolean | null; status: string | null; identityVerified: boolean;
}

/** New responses carry the full identity; old desktop backends may omit all identity fields. */
export function outputPage(result: JobOutput, expected: OutputIdentity, offset: number): OutputPage {
  const fields = ["workspace_id", "task_id", "job_id", "stream"] as const;
  const identityPresent = fields.some(field => result[field] !== undefined);
  const identityVerified = fields.every(field => result[field] === expected[field]);
  if (identityPresent && !identityVerified) throw new Error("输出归属与选中工作区、任务、作业或日志流不一致，已清除该结果。");
  const next = result.next_offset ?? result.poll_offset ?? offset;
  if (!Number.isFinite(next) || next < offset) throw new Error("输出分页位置无效，未沿用旧输出。");
  return {
    content: result.content || "", offset, next,
    retainedBytes: typeof result.retained_bytes === "number" && Number.isFinite(result.retained_bytes) && result.retained_bytes >= 0 ? result.retained_bytes : null,
    truncated: typeof result.may_be_truncated === "boolean" ? result.may_be_truncated : null,
    status: typeof result.job_status === "string" && result.job_status ? result.job_status : null,
    identityVerified,
  };
}
