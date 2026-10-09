<script lang="ts">
  import Dialog from "./Dialog.svelte";
  import { untrack } from "svelte";
  import { getJobOutput } from "./api";
  import { outputPage, type OutputPage } from "$lib/dot/output";
  import type { JobSummary, TaskRow } from "./types";
  let { task, job, onClose }: { task: TaskRow | null; job: JobSummary | null; onClose: () => void } = $props();
  let stream = $state<"stdout" | "stderr">("stdout"), page = $state<OutputPage | null>(null), busy = $state(false), error = $state("");
  let token = 0;
  $effect(() => {
    void task?.workspace_id; void task?.task_id; void job?.job_id; const selectedStream = stream;
    token++; page = null; error = ""; busy = false;
    if (task && job) untrack(() => void load(0, selectedStream));
    return () => { token++; };
  });
  async function load(start = 0, selectedStream = stream) {
    if (!task || !job) return; const t = task, j = job, current = ++token;
    const identity = { workspace_id: t.workspace_id, task_id: t.task_id, job_id: j.job_id, stream: selectedStream };
    const currentSelection = () => current === token && task?.workspace_id === identity.workspace_id && task?.task_id === identity.task_id && job?.job_id === identity.job_id && stream === selectedStream;
    busy = true; error = ""; page = null;
    try {
      const result = await getJobOutput(t.workspace_id, t.task_id, j.job_id, selectedStream, start);
      if (!currentSelection()) return;
      page = outputPage(result, identity, start);
    } catch (e) { if (currentSelection()) { page = null; error = String(e); } }
    finally { if (currentSelection()) busy = false; }
  }
</script>
<Dialog open={!!job} title="任务执行输出" onClose={onClose} wide>
  <p>仅展示选中任务的真实作业日志。每页最多8 KiB，不执行命令、不重放作业。日志可能含敏感内容，请勿直接公开分享。</p>
  {#if task && job}<dl class="td-keyval"><dt>工作区</dt><dd class="td-mono">{task.workspace_id}</dd><dt>任务</dt><dd class="td-mono">{task.task_id}</dd><dt>作业</dt><dd class="td-mono">{job.job_id}</dd><dt>状态</dt><dd>{page?.status ?? "尚未取得本次输出状态"}{page?.status ? " · 本次输出响应" : `；任务记录：${job.status}`}</dd><dt>记录退出码</dt><dd>{job.exit_code !== null ? job.exit_code : "未记录"}</dd></dl>{/if}
  <div class="td-tabs" style="margin-top:14px"><button class:active={stream === "stdout"} onclick={() => stream = "stdout"}>标准输出</button><button class:active={stream === "stderr"} onclick={() => stream = "stderr"}>错误输出</button></div>
  {#if error}<div class="td-error" role="alert">{error}</div>{/if}
  <pre class="td-code" aria-live="polite">{busy ? "读取中…" : page ? page.content || "当前范围没有输出。" : "尚未取得本次输出；不会沿用旧内容。"}</pre>
  {#if page}<dl class="td-keyval"><dt>字节偏移</dt><dd class="td-mono">{page.offset} → {page.next}</dd><dt>保留输出</dt><dd>{page.retainedBytes === null ? "服务未返回保留字节数" : `${page.retainedBytes} 字节`}</dd><dt>完整性</dt><dd>{page.truncated === null ? "服务未返回截断信息" : page.truncated ? "可能已截断" : "服务未报告截断"}</dd></dl>{#if page.truncated}<div class="td-note">输出可能因保留上限被截断；当前日志不能证明全部输出已取回。</div>{/if}{#if !page.identityVerified}<p class="td-muted">旧版服务未返回完整输出归属字段；此处仍只通过原任务和原作业接口读取。</p>{/if}{/if}
  {#snippet footer()}<button class="td-button" disabled={busy} onclick={() => load(0)}>从头读取</button><button class="td-button" disabled={busy} onclick={() => load(page?.offset ?? 0)}>刷新本页</button><button class="td-button" disabled={busy || !page || page.next <= page.offset} onclick={() => page && load(page.next)}>下一段</button><button class="td-button primary" onclick={onClose}>关闭</button>{/snippet}
</Dialog>
