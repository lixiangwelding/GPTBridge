<script lang="ts">
  import { Copy, FileText } from "@lucide/svelte";
  import { copyText } from "./api";
  import { readable, timestamp } from "./format";
  import { showToast } from "$lib/stores/toast";
  import type { TaskDetail, TaskRow, JobSummary } from "./types";
  import StateBadge from "./StateBadge.svelte";
  let { task, detail, loading, error, onRetry, onLog }: { task: TaskRow | null; detail: TaskDetail | null; loading: boolean; error: string; onRetry: () => void; onLog: (job: JobSummary) => void } = $props();
  let expanded = $state(false), copiedError = $state("");
  $effect(() => { void task?.task_id; expanded = false; copiedError = ""; });
  async function copy() {
    if (!detail) return;
    try { await copyText(detail.handoff); showToast("接续指令已复制；粘贴到已连接 TaskDock 的 AI 客户端继续。"); }
    catch (e) { copiedError = String(e); }
  }
</script>
<div class="td-inspector">
  {#if !task}<div class="td-empty"><h2>先选择一个任务</h2><p>目标、进展和下一步集中显示在这里。</p></div>
  {:else}
    <header><div class="td-eyebrow">TASK DETAIL / {task.task_id.slice(0,8)}</div><h2>{task.goal.length > 70 ? `${task.goal.slice(0,70)}…` : task.goal}</h2><div class="td-actions"><StateBadge state={task.display_state}/><span class="td-muted" style="font-size:11px">{task.workspace_name}</span></div></header>
    {#if error}<div class="td-inspect-section"><div class="td-error" role="alert">{error}</div><button class="td-button small" onclick={onRetry}>重新读取任务</button></div>{/if}
    {#if loading && !detail}<div class="td-inspect-section" role="status"><span class="td-loading-dot"></span>读取持久任务记录…</div>{/if}
    {#if detail && detail.task_id === task.task_id && detail.workspace_id === task.workspace_id}
      <section class="td-inspect-section"><h3>这次要做什么</h3><p style="max-height:260px;overflow:auto">{expanded ? detail.goal : detail.goal.slice(0,320)}{!expanded && detail.goal.length > 320 ? "…" : ""}</p>{#if detail.goal.length > 320}<button class="td-link-button" onclick={() => expanded = !expanded}>{expanded ? "收起目标" : "查看完整目标"}</button>{/if}</section>
      <section class="td-inspect-section"><h3>当前进展 / 下一步</h3><p>{detail.checkpoint.summary || (task.display_state === "ready" ? "目标已保存，等待已连接的 AI 客户端接续。" : "尚无执行摘要；以下作业状态来自任务记录。")}</p>
        {#if detail.checkpoint.next_step}<div class="td-note">{detail.checkpoint.next_step}</div>{/if}
        {#if detail.checkpoint.remaining_issues}<details class="td-details"><summary>待处理事项</summary><pre class="td-code">{readable(detail.checkpoint.remaining_issues)}</pre></details>{/if}
        {#if task.unknown_jobs > 0}<div class="td-note">存在结果不明的作业。先核对输出与当前文件，不自动重放、不将失败改为成功。</div>{/if}
        <button class="td-button small" style="width:100%;margin-top:12px" onclick={copy}><Copy size={13}/>复制接续指令</button>
        {#if copiedError}<div class="td-error">{copiedError}</div>{/if}
        <details style="margin-top:10px"><summary class="td-link-button">查看接续文本</summary><pre class="td-code">{detail.handoff}</pre></details>
      </section>
      <section class="td-inspect-section"><h3>任务边界</h3><dl class="td-keyval"><dt>目录</dt><dd class="td-mono">{detail.workspace_path}</dd><dt>记录版本</dt><dd>revision {detail.revision}</dd><dt>持久状态</dt><dd>{detail.state}</dd><dt>最近更新</dt><dd>{timestamp(detail.updated)}</dd></dl><p class="td-muted" style="font-size:10px;margin-top:10px">任务检查点由执行方记录；命令退出成功不等于业务验收通过。</p></section>
      {#if Object.keys(detail.steps).length}<section class="td-inspect-section"><h3>执行记录</h3>{#each Object.entries(detail.steps) as [id, step]}<div class="td-step"><span>{step.state === "passed" ? "✓" : step.state === "failed" ? "!" : "·"}</span><div>{id}<small>{step.state}{step.notes ? ` · ${step.notes}` : ""}</small>{#if step.evidence?.length}<details><summary class="td-link-button">查看证据引用</summary><pre class="td-code">{readable(step.evidence)}</pre></details>{/if}</div></div>{/each}{#if detail.next_cursor}<p class="td-muted" style="font-size:10px;margin-top:10px">此处展示前20个步骤；其余记录可通过客户端 task_status 分页读取。</p>{/if}</section>{/if}
      <section class="td-inspect-section"><h3>作业与诊断 {detail.jobs.truncated ? "（最近20项）" : ""}</h3>{#if detail.jobs.jobs.length}{#each detail.jobs.jobs as job (job.job_id)}<div class="td-step"><FileText size={13}/><div style="min-width:0;flex:1"><div class="td-mono">{job.job_id.slice(0,8)} · {job.status}{job.exit_code !== null ? ` / ${job.exit_code}` : ""}</div><small>{job.waiting_for ? `等待资源：${job.waiting_for}` : job.detail}</small><button class="td-link-button" onclick={() => onLog(job)}>读取执行输出 ↗</button></div></div>{/each}{:else}<p class="td-muted">暂无作业。创建目标不会自动运行命令。</p>{/if}</section>
    {/if}
  {/if}
</div>
