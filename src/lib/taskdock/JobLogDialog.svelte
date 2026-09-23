<script lang="ts">
  import Dialog from "./Dialog.svelte";
  import { getJobOutput } from "./api";
  import type { JobSummary, TaskRow } from "./types";
  let { task, job, onClose }: { task: TaskRow | null; job: JobSummary | null; onClose: () => void } = $props();
  let stream = $state<"stdout" | "stderr">("stdout"), content = $state(""), offset = $state(0), next = $state(0), busy = $state(false), error = $state("");
  let token = 0;
  $effect(() => { void task?.task_id; void job?.job_id; const selectedStream = stream; token++; content = ""; offset = 0; next = 0; if (task && job) void load(0, selectedStream); });
  async function load(start = 0, selectedStream = stream) {
    if (!task || !job) return; const t = task, j = job, current = ++token; busy = true; error = "";
    try {
      const result = await getJobOutput(t.workspace_id, t.task_id, j.job_id, selectedStream, start);
      if (current !== token) return;
      content = result.content || ""; offset = start; next = result.next_offset ?? result.poll_offset ?? start;
    } catch (e) { if (current === token) error = String(e); }
    finally { if (current === token) busy = false; }
  }
</script>
<Dialog open={!!job} title="任务执行输出" onClose={onClose} wide>
  <p>仅展示选中任务的真实作业日志。每页最多8 KiB，不执行命令、不重放作业。日志可能含敏感内容，请勿直接公开分享。</p>
  {#if job}<div class="td-keyval"><dt>作业</dt><dd class="td-mono">{job.job_id}</dd><dt>状态</dt><dd>{job.status} {job.exit_code !== null ? `· 退出码 ${job.exit_code}` : ""}</dd></div>{/if}
  <div class="td-tabs" style="margin-top:14px"><button class:active={stream === "stdout"} onclick={() => stream = "stdout"}>标准输出</button><button class:active={stream === "stderr"} onclick={() => stream = "stderr"}>错误输出</button></div>
  {#if error}<div class="td-error" role="alert">{error}</div>{/if}
  <pre class="td-code" aria-live="polite">{busy ? "读取中…" : content || "当前范围没有输出。"}</pre><span class="td-mono td-muted">字节偏移 {offset} → {next}</span>
  {#snippet footer()}<button class="td-button" disabled={busy} onclick={() => load(0)}>从头读取</button><button class="td-button" disabled={busy} onclick={() => load(offset)}>刷新本页</button><button class="td-button" disabled={busy || next <= offset} onclick={() => load(next)}>下一段</button><button class="td-button primary" onclick={onClose}>关闭</button>{/snippet}
</Dialog>
