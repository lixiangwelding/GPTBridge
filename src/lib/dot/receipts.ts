import type { OperationReceipt, OperationReceiptPage } from "../taskdock/types";

export function receiptPageMatches(page: OperationReceiptPage, workspaceId: string, taskId: string): boolean {
  return page.workspace_id === workspaceId && page.task_id === taskId && page.items.every(item => item.task_id === taskId);
}
export function receiptSource(receipt: OperationReceipt): string {
  return receipt.source === "audit" ? "真实调用审计" : "持久回执 · 原工作区 ID 未记录";
}
export function receiptTiming(receipt: OperationReceipt): string {
  return receipt.timing_source === "dispatcher_audit" ? "以下为工具分派调用时间，不是命令进程全生命周期。"
    : receipt.timing_source === "task_event_end_only" ? "只记录任务事件的完成时刻；开始时间未记录。"
    : "历史记录没有保存调用开始与结束时间。";
}
export function receiptHash(hash: string | null, exists: boolean | null): string {
  if (exists === false) return "该阶段文件不存在";
  return hash ?? "未记录";
}
