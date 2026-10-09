<script lang="ts">
  import { RefreshCw, Copy, ShieldCheck, FileText } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { getConnections, getSnapshot, getTask, copyText } from "$lib/taskdock/api";
  import type { Connections, Snapshot, TaskRow, TaskDetail, JobSummary } from "$lib/taskdock/types";
  import type { WorkspaceProfile } from "$lib/types";
  import { timestamp } from "$lib/taskdock/format";
  import { belongsToTask, connectionRows, policySummary, usagePrompt, workspaceTasks } from "$lib/dot/model";
  import StateBadge from "$lib/taskdock/StateBadge.svelte";
  import JobLogDialog from "$lib/taskdock/JobLogDialog.svelte";
  import { showToast } from "$lib/stores/toast";
  let { profile }: { profile: WorkspaceProfile } = $props();
  let connection = $state<Connections | null>(null), snapshot = $state<Snapshot | null>(null);
  let selected = $state<TaskRow | null>(null), detail = $state<TaskDetail | null>(null), job = $state<JobSummary | null>(null);
  let busy = $state(false), detailBusy = $state(false), connectionError = $state(""), taskError = $state(""), detailError = $state("");
  let generation = 0, detailGeneration = 0;
  const diagnostics = $derived(connectionRows(connection, profile));
  const policy = $derived(policySummary(profile));
  const tasks = $derived(workspaceTasks(snapshot?.tasks ?? [], profile.id));
  const selectedDetail = $derived(belongsToTask(detail, selected, profile.id) ? detail : null);

  async function selectTask(row: TaskRow) {
    if (row.workspace_id !== profile.id) return;
    const token = ++detailGeneration, id = profile.id;
    detail = null; job = null;
    selected = row; detailBusy = true; detailError = "";
    try {
      const next = await getTask(id, row.task_id);
      if (token === detailGeneration && id === profile.id && belongsToTask(next, row, id)) detail = next;
      else if (token === detailGeneration && id === profile.id) { detail = null; job = null; detailError = "回执归属与选中任务不一致，未显示该结果。"; }
    } catch (error) { if (token === detailGeneration && id === profile.id) { detail = null; job = null; detailError = String(error); } }
    finally { if (token === detailGeneration && id === profile.id) detailBusy = false; }
  }
  async function refresh(id = profile.id) {
    if (busy || id !== profile.id) return;
    const token = ++generation; busy = true;
    const results = await Promise.allSettled([
      getConnections(id), getSnapshot(id, { query: "", filter: "all", cursor: null, limit: 6 }),
    ]);
    if (token !== generation || id !== profile.id) return;
    const [connections, records] = results;
    if (connections.status === "fulfilled" && connections.value.workspace_id === id) { connection = connections.value; connectionError = ""; }
    else { connection = null; connectionError = connections.status === "rejected" ? String(connections.reason) : "连接观察归属不一致。"; }
    if (records.status === "fulfilled") {
      snapshot = records.value; taskError = "";
      const rows = workspaceTasks(records.value.tasks, id);
      const row = rows.find(item => item.task_id === selected?.task_id) ?? rows[0];
      if (row) { if (!job) void selectTask(row); }
      else { selected = null; detail = null; job = null; detailGeneration++; detailBusy = false; }
    } else { snapshot = null; selected = null; detail = null; job = null; detailGeneration++; detailBusy = false; taskError = String(records.reason); }
    busy = false;
  }
  async function copy(value: string, label: string) {
    try { await copyText(value); showToast(`${label}已复制；粘贴到已连接 GPTBridge 的客户端。`); }
    catch (error) { showToast(String(error), { title: "复制失败", kind: "error" }); }
  }
  $effect(() => {
    const id = profile.id;
    generation++; detailGeneration++; connection = null; snapshot = null; selected = null; detail = null; job = null;
    connectionError = ""; taskError = ""; detailError = ""; busy = false; detailBusy = false;
    untrack(() => void refresh(id));
    const poll = () => { if (document.visibilityState === "visible") void refresh(id); };
    const timer = window.setInterval(poll, 15_000);
    document.addEventListener("visibilitychange", poll);
    return () => { generation++; detailGeneration++; window.clearInterval(timer); document.removeEventListener("visibilitychange", poll); };
  });
</script>

<section class="dot-panel" aria-label="dot 原生工作区">
  <header class="dot-head"><div><div class="td-eyebrow">SAME MCP · NATIVE TOOLS</div><h2>dot · 原生工作区</h2><p>原有 @GPTBridge 与 dot 共用连接和任务记录。</p></div><button class="td-button" disabled={busy} onclick={() => void refresh()}><RefreshCw size={13}/>{busy ? "观察中…" : "刷新观察"}</button></header>
  <div class="dot-grid">
    <section class="dot-section dot-guide"><h3>同一连接，直接操作</h3><ol class="dot-steps"><li><span class="dot-index">01</span><div><strong>确认工具目录与目标</strong><p>让客户端读取 tools/list 与 server_info。共享入口调用携带登记的 workspace_id。</p></div></li><li><span class="dot-index">02</span><div><strong>读取 → 哈希补丁 → 持久命令</strong><p>以当前工具目录为准；task_open / task_checkpoint 用于持久记账，无需外部 Agent。</p></div></li><li><span class="dot-index">03</span><div><strong>从原句柄取回输出</strong><p>结果不明先查询原 request_id / job_id，核对终态和退出码，再决定下一步。</p></div></li></ol><button class="td-button small" onclick={() => void copy(usagePrompt(profile), "使用提示")}><Copy size={12}/>复制使用提示</button><div class="dot-tools" aria-label="现有原生工具"><code>read_file</code><code>apply_patch</code><code>exec_command</code><code>read_output</code></div></section>
    <section class="dot-section"><h3>连接诊断</h3><dl class="dot-diagnostics" aria-live="polite">{#each diagnostics as row (row.label)}<div><dt>{row.label}</dt><dd class:dot-ok={row.tone === "ok"} class:dot-attention={row.tone === "warning"}>{row.value}</dd></div>{/each}</dl><div class="dot-warning">端口可达不代表认证、客户端目录或 dot 路由已验证。服务无 listChanged 通知；请用客户端支持的刷新或新会话验证。</div>{#if connectionError}<p class="dot-error" role="alert">观察失败，已清除旧连接结果。{connectionError}</p>{/if}<p class="dot-caption">{connection ? `端口观察：${timestamp(connection.checked_at)}。` : "尚无端口观察。"}读取现有状态，不启动服务。</p></section>
  </div>
  <div class="dot-safety"><ShieldCheck size={14}/><span>权限 <strong>{policy.permission}</strong> · 已保存配置</span><span>命令隔离 <strong>policy_only · 无 OS 沙箱</strong></span><span>读取配置 <strong>{policy.configuredReadScope}</strong></span><span>运行权限与读取范围以 server_info 为准。</span></div>
  <section class="dot-section"><div class="dot-task-head"><div><h3>任务与恢复回执</h3><p class="dot-caption">复用持久任务，选择记录查看原句柄与结果。{snapshot ? `观察于 ${timestamp(snapshot.observed_at)}` : ""}</p></div><a href={`/?workspace=${encodeURIComponent(profile.id)}`}>打开完整工作台 ↗</a></div>
    {#if taskError}<p class="dot-error" role="alert">任务读取失败，已清除旧观察与回执。{taskError}</p>{/if}
    {#if snapshot?.partial}<div class="dot-warning">任务观察不完整。{snapshot.warnings.filter(item => item.workspace_id === profile.id).map(item => item.message).join("；")}请刷新核对，不据此判断任务已结束。</div>{/if}
    {#if tasks.length}<div class="dot-tasks">{#each tasks as task (task.task_id)}<button class="dot-task" class:selected={selected?.task_id === task.task_id} onclick={() => void selectTask(task)} aria-pressed={selected?.task_id === task.task_id}><strong>{task.goal}</strong><StateBadge state={task.display_state}/><small>{task.task_id} · revision {task.revision}</small><small>{task.running_jobs} 运行 · {task.unknown_jobs} 结果不明</small></button>{/each}</div>
    {:else if !snapshot}<p class="dot-empty" aria-live="polite">{busy ? "读取现有持久任务…" : "尚未取得任务记录；不会替换为示例数据。"}</p>
    {:else if !snapshot.partial}<p class="dot-empty">此工作区暂无持久任务。请在已连接的客户端按原有 task_open 生命周期开始；此页不执行命令。</p>{/if}
    {#if snapshot?.next_cursor}<p class="dot-caption">此处只显示最近 6 项；其余记录在完整工作台分页读取。</p>{/if}
    {#if detailBusy}<p class="dot-caption" role="status">读取选中任务回执…</p>{/if}{#if detailError}<p class="dot-error" role="alert">{detailError}</p>{/if}
    {#if selectedDetail}<div class="dot-receipt"><div class="dot-task-head"><strong>原任务回执 · {selectedDetail.state}</strong><button class="td-button small" onclick={() => void copy(selectedDetail!.handoff, "接续指令")}><Copy size={12}/>复制接续指令</button></div><p>{selectedDetail.checkpoint.summary || "尚无检查点摘要。任务状态来自持久记录，命令退出成功不等于业务验收。"}</p>{#if selectedDetail.checkpoint.next_step}<p class="dot-next">下一步：{selectedDetail.checkpoint.next_step}</p>{/if}<dl class="dot-receipt-meta"><div><dt>task_id</dt><dd>{selectedDetail.task_id}</dd></div><div><dt>revision</dt><dd>{selectedDetail.revision}</dd></div><div><dt>最近更新</dt><dd>{timestamp(selectedDetail.updated)}</dd></div></dl>
      {#if selectedDetail.jobs.jobs.length}<div class="dot-jobs">{#each selectedDetail.jobs.jobs as item (item.job_id)}<div class="dot-job"><FileText size={13}/><div><strong>{item.status}{item.exit_code !== null ? ` · 退出码 ${item.exit_code}` : " · 尚无退出码"}</strong><small>job_id {item.job_id} · request_id {item.request_id}</small><p>{item.waiting_for ? `等待：${item.waiting_for}` : item.detail || "尚无作业摘要"}</p></div><button class="td-button small" onclick={() => job = item}>读取输出</button></div>{/each}</div>{:else}<p>暂无作业回执。保存任务不代表命令已运行。</p>{/if}
      {#if selectedDetail.jobs.truncated}<p>仅显示最近 20 个作业；更多信息由原客户端按既有协议读取。</p>{/if}<div class="dot-warning">结果不明时保留原请求，先核对输出与当前文件。日志可能含敏感信息，请勿直接公开分享。</div></div>{/if}
  </section>
</section>
<JobLogDialog task={selected} {job} onClose={() => job = null}/>

<style>
  .dot-panel{border:1px solid var(--td-line);border-radius:var(--td-radius);background:var(--td-panel);overflow:hidden;min-width:0}
  .dot-head{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:16px 18px;border-bottom:1px solid var(--td-line)}
  .dot-head h2{font-size:19px;font-weight:650;line-height:1.4;margin:3px 0 0}.dot-head p{font-size:12px;color:var(--td-muted);margin:4px 0 0}
  .dot-grid{display:grid;grid-template-columns:minmax(0,1.05fr) minmax(280px,.95fr)}.dot-section{padding:17px 18px;min-width:0}.dot-section h3{font-size:14px;font-weight:600;margin:0}.dot-guide{border-right:1px solid var(--td-line)}
  .dot-steps{display:grid;gap:12px;padding:0;margin:14px 0;list-style:none}.dot-steps li{display:flex;gap:10px}.dot-index{flex-shrink:0;font:11px var(--td-mono);color:var(--td-green);background:var(--td-soft);border:1px solid var(--td-line);border-radius:4px;width:25px;height:25px;display:grid;place-items:center}.dot-steps strong{font-size:12px;font-weight:600}.dot-steps p{font-size:11px;color:var(--td-muted);margin:3px 0 0}
  .dot-tools{display:flex;gap:5px;flex-wrap:wrap;margin-top:11px}.dot-tools code{font:10px var(--td-mono);border:1px solid var(--td-line);border-radius:3px;padding:3px 6px;color:var(--td-green)}
  .dot-diagnostics{display:grid;margin:12px 0}.dot-diagnostics>div{display:flex;justify-content:space-between;gap:12px;padding:8px 0;border-bottom:1px solid var(--td-line);font-size:12px}.dot-diagnostics dt{color:var(--td-muted)}.dot-diagnostics dd{margin:0;text-align:right}.dot-ok{color:var(--td-green)}.dot-attention{color:var(--td-amber)}
  .dot-warning{color:var(--td-amber);background:var(--td-amber-bg);border:1px solid #e7d6b0;padding:8px 10px;border-radius:4px;font-size:11px;overflow-wrap:anywhere}.dot-safety{display:flex;gap:8px 18px;flex-wrap:wrap;align-items:center;border-top:1px solid var(--td-line);border-bottom:1px solid var(--td-line);background:var(--td-soft);padding:10px 18px;font-size:11px}.dot-safety strong{font-weight:600}.dot-safety :global(svg){flex-shrink:0}
  .dot-caption{color:var(--td-muted);font-size:10px;margin:8px 0 0}.dot-task-head{display:flex;align-items:center;justify-content:space-between;gap:12px}.dot-task-head a{font-size:11px;white-space:nowrap}.dot-task-head .dot-caption{margin:3px 0 0}
  .dot-tasks{margin-top:12px;border:1px solid var(--td-line);border-radius:4px;overflow:hidden}.dot-task{padding:11px 12px;display:grid;grid-template-columns:minmax(0,1fr) auto;gap:5px 14px;width:100%;text-align:left;border:0;background:var(--td-panel);color:var(--td-ink)}.dot-task+.dot-task{border-top:1px solid var(--td-line)}.dot-task:hover,.dot-task.selected{background:var(--td-soft)}.dot-task strong{font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.dot-task small{font:10px var(--td-mono);overflow-wrap:anywhere}.dot-task small:last-child{text-align:right;color:var(--td-muted)}
  .dot-receipt{padding:12px;border:1px solid var(--td-line);border-radius:4px;margin-top:10px;font-size:12px;background:var(--td-bg);overflow-wrap:anywhere}.dot-receipt p{font-size:11px;color:var(--td-muted);margin:8px 0}.dot-receipt-meta{display:flex;gap:12px 22px;flex-wrap:wrap;margin:10px 0}.dot-receipt-meta div{display:flex;gap:7px;font-size:10px}.dot-receipt-meta dt{color:var(--td-muted)}.dot-receipt-meta dd{margin:0;font-family:var(--td-mono)}
  .dot-jobs{margin:12px 0;border-top:1px solid var(--td-line)}.dot-job{display:flex;align-items:flex-start;gap:9px;border-bottom:1px solid var(--td-line);padding:10px 0}.dot-job>div{min-width:0;flex:1}.dot-job strong{font-size:11px}.dot-job small{display:block;font:10px var(--td-mono);margin-top:3px;overflow-wrap:anywhere}.dot-job p{margin:3px 0 0}.dot-job :global(svg){margin-top:3px;flex-shrink:0}.dot-empty{font-size:12px;color:var(--td-muted);border:1px dashed var(--td-line);padding:14px;margin:12px 0 0;border-radius:4px}.dot-error{color:var(--td-error);font-size:11px;overflow-wrap:anywhere;margin:10px 0}.dot-panel :global(.td-button:active){transform:translateY(1px)}
  @media(max-width:850px){.dot-grid{grid-template-columns:1fr}.dot-guide{border-right:0;border-bottom:1px solid var(--td-line)}}
  @media(max-width:500px){.dot-head{align-items:flex-start}.dot-head,.dot-section{padding:14px}.dot-head h2{font-size:17px}.dot-head p{max-width:215px}.dot-safety{padding:10px 14px}.dot-task-head{flex-wrap:wrap}.dot-job{flex-wrap:wrap}.dot-job>div{flex-basis:calc(100% - 25px)}.dot-job>button{margin-left:22px}}
</style>
