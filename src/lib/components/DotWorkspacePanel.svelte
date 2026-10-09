<script lang="ts">
  import { RefreshCw, Copy, ShieldCheck, FileText } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { getConnections, getOperationReceipts, getProtocolDiagnostics, getSnapshot, getTask, copyText } from "$lib/taskdock/api";
  import type { Connections, OperationReceiptCursor, OperationReceiptPage, ProtocolDiagnostics, Snapshot, TaskRow, TaskDetail, JobSummary } from "$lib/taskdock/types";
  import type { WorkspaceProfile } from "$lib/types";
  import { timestamp } from "$lib/taskdock/format";
  import { belongsToTask, connectionRows, policySummary, usagePrompt, workspaceTasks } from "$lib/dot/model";
  import { receiptHash, receiptPageMatches, receiptSource, receiptTiming } from "$lib/dot/receipts";
  import StateBadge from "$lib/taskdock/StateBadge.svelte";
  import JobLogDialog from "$lib/taskdock/JobLogDialog.svelte";
  import { showToast } from "$lib/stores/toast";
  let { profile }: { profile: WorkspaceProfile } = $props();
  let connection = $state<Connections | null>(null), snapshot = $state<Snapshot | null>(null);
  let selected = $state<TaskRow | null>(null), detail = $state<TaskDetail | null>(null), job = $state<JobSummary | null>(null);
  let protocol = $state<ProtocolDiagnostics | null>(null), receipts = $state<OperationReceiptPage | null>(null);
  let protocolBusy = $state(false), protocolError = $state(""), receiptBusy = $state(false), receiptError = $state("");
  let busy = $state(false), detailBusy = $state(false), connectionError = $state(""), taskError = $state(""), detailError = $state("");
  let generation = 0, detailGeneration = 0, protocolGeneration = 0, receiptGeneration = 0;
  const diagnostics = $derived(connectionRows(connection, profile, protocol));
  const policy = $derived(policySummary(profile, protocol));
  const tasks = $derived(workspaceTasks(snapshot?.tasks ?? [], profile.id));
  const selectedDetail = $derived(belongsToTask(detail, selected, profile.id) ? detail : null);

  async function selectTask(row: TaskRow) {
    if (row.workspace_id !== profile.id) return;
    const token = ++detailGeneration, id = profile.id;
    const changed = selected?.task_id !== row.task_id;
    detail = null; job = null;
    selected = row; detailBusy = true; detailError = "";
    if (changed) void loadReceipts(row);
    try {
      const next = await getTask(id, row.task_id);
      if (token === detailGeneration && id === profile.id && belongsToTask(next, row, id)) detail = next;
      else if (token === detailGeneration && id === profile.id) { detail = null; job = null; detailError = "回执归属与选中任务不一致，未显示该结果。"; }
    } catch (error) { if (token === detailGeneration && id === profile.id) { detail = null; job = null; detailError = String(error); } }
    finally { if (token === detailGeneration && id === profile.id) detailBusy = false; }
  }
  async function diagnose(id = profile.id) {
    if (protocolBusy || id !== profile.id) return;
    const token = ++protocolGeneration; protocolBusy = true; protocol = null; protocolError = "";
    try {
      const result = await getProtocolDiagnostics(id);
      if (token !== protocolGeneration || id !== profile.id) return;
      if (result.workspace_id !== id) throw new Error("协议观察工作区不一致，未显示该结果。");
      protocol = result;
    } catch (error) { if (token === protocolGeneration && id === profile.id) { protocol = null; protocolError = String(error); } }
    finally { if (token === protocolGeneration && id === profile.id) protocolBusy = false; }
  }
  async function loadReceipts(row: TaskRow, cursor: OperationReceiptCursor | null = null) {
    if (row.workspace_id !== profile.id || row.task_id !== selected?.task_id) return;
    const token = ++receiptGeneration, id = profile.id, taskId = row.task_id;
    receiptBusy = true; receiptError = ""; receipts = null;
    try {
      const page = await getOperationReceipts(id, taskId, cursor, 20);
      if (token !== receiptGeneration || id !== profile.id || selected?.task_id !== taskId) return;
      if (!receiptPageMatches(page, id, taskId)) throw new Error("操作回执与当前查询任务不一致，未显示该结果。");
      receipts = page;
    } catch (error) { if (token === receiptGeneration && id === profile.id && selected?.task_id === taskId) { receipts = null; receiptError = String(error); } }
    finally { if (token === receiptGeneration && id === profile.id && selected?.task_id === taskId) receiptBusy = false; }
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
      else { selected = null; detail = null; job = null; detailGeneration++; detailBusy = false; receipts = null; receiptGeneration++; receiptBusy = false; receiptError = ""; }
    } else { snapshot = null; selected = null; detail = null; job = null; detailGeneration++; detailBusy = false; receipts = null; receiptGeneration++; receiptBusy = false; receiptError = ""; taskError = String(records.reason); }
    busy = false;
  }
  async function copy(value: string, label: string) {
    try { await copyText(value); showToast(`${label}已复制；粘贴到已连接 GPTBridge 的客户端。`); }
    catch (error) { showToast(String(error), { title: "复制失败", kind: "error" }); }
  }
  $effect(() => {
    const id = profile.id;
    generation++; detailGeneration++; protocolGeneration++; receiptGeneration++; connection = null; snapshot = null; selected = null; detail = null; job = null; protocol = null; receipts = null;
    connectionError = ""; taskError = ""; detailError = ""; busy = false; detailBusy = false;
    protocolBusy = false; protocolError = ""; receiptBusy = false; receiptError = "";
    untrack(() => void refresh(id));
    untrack(() => void diagnose(id));
    const poll = () => { if (document.visibilityState === "visible") void refresh(id); };
    const timer = window.setInterval(poll, 15_000);
    document.addEventListener("visibilitychange", poll);
    return () => { generation++; detailGeneration++; protocolGeneration++; receiptGeneration++; window.clearInterval(timer); document.removeEventListener("visibilitychange", poll); };
  });
</script>

<section class="dot-panel" aria-label="dot 原生工作区">
  <header class="dot-head"><div><div class="td-eyebrow">SAME MCP · NATIVE TOOLS</div><h2>dot · 原生工作区</h2><p>原有 @GPTBridge 与 dot 共用连接和任务记录。</p></div><button class="td-button" disabled={busy} onclick={() => void refresh()}><RefreshCw size={13}/>{busy ? "观察中…" : "刷新观察"}</button></header>
  <div class="dot-grid">
    <section class="dot-section dot-guide"><h3>同一连接，直接操作</h3><ol class="dot-steps"><li><span class="dot-index">01</span><div><strong>确认工具目录与目标</strong><p>让客户端读取 tools/list 与 server_info。共享入口调用携带登记的 workspace_id。</p></div></li><li><span class="dot-index">02</span><div><strong>读取 → 哈希补丁 → 持久命令</strong><p>以当前工具目录为准；task_open / task_checkpoint 用于持久记账，无需外部 Agent。</p></div></li><li><span class="dot-index">03</span><div><strong>从原句柄取回输出</strong><p>结果不明先查询原 request_id / job_id，核对终态和退出码，再决定下一步。</p></div></li></ol><button class="td-button small" onclick={() => void copy(usagePrompt(profile), "使用提示")}><Copy size={12}/>复制使用提示</button><div class="dot-tools" aria-label="现有原生工具"><code>read_file</code><code>apply_patch</code><code>exec_command</code><code>read_output</code></div></section>
    <section class="dot-section"><div class="dot-task-head"><h3>连接诊断</h3><button class="td-button small" disabled={protocolBusy} onclick={() => void diagnose()}>{protocolBusy ? "检查中…" : "检查协议"}</button></div><dl class="dot-diagnostics" aria-live="polite">{#each diagnostics as row (row.label)}<div><dt>{row.label}</dt><dd class:dot-ok={row.tone === "ok"} class:dot-attention={row.tone === "warning"}>{row.value}</dd></div>{/each}</dl>
      {#if protocol}<dl class="dot-protocol-meta"><dt>协议观察</dt><dd>{timestamp(protocol.observed_at)}</dd><dt>服务版本</dt><dd>{protocol.handshake.server_version ?? "未返回"}</dd><dt>目录 SHA256</dt><dd class="td-mono">{protocol.catalog.status === "passed" ? protocol.catalog.sha256 ?? "未返回" : "尚未取得"}</dd></dl><p class="dot-caption">{protocol.auth.detail} {protocol.handshake.detail} {protocol.catalog.detail} {protocol.runtime_policy.detail}</p>{/if}
      <div class="dot-warning">服务端认证、握手和目录检查与客户端缓存/dot 路由分开。客户端目录刷新和真实调用仍需在客户端验证。{protocol?.catalog.list_changed_supported === false ? "服务未提供 listChanged 通知。" : ""}</div>{#if protocolError}<p class="dot-error" role="alert">协议检查失败，已清除旧运行策略。{protocolError}</p>{/if}{#if connectionError}<p class="dot-error" role="alert">端口观察失败，已清除旧结果。{connectionError}</p>{/if}<p class="dot-caption">{connection ? `端口观察：${timestamp(connection.checked_at)}。` : "尚无端口观察。"}协议仅初次和手动检查；定时刷新不重复认证，不启动服务。</p></section>
  </div>
  <div class="dot-safety"><ShieldCheck size={14}/><span>保存权限 <strong>{policy.permission}</strong></span><span>保存读取配置 <strong>{policy.configuredReadScope}</strong></span><span>命令隔离基线 <strong>policy_only · 无 OS 沙箱</strong></span></div>
  <div class="dot-runtime" aria-live="polite">{#if policy.runtime}<strong>运行策略 · 本次 server_info 观察</strong><span>{policy.runtime.permission} / {policy.runtime.profile}</span><span>读取范围：{policy.runtime.readScope}</span><span>隔离：{policy.runtime.isolation}；OS 沙箱：{policy.runtime.sandbox === null ? "未返回" : policy.runtime.sandbox ? "服务报告已启用" : "服务报告未启用"}</span>{:else}<span>尚未取得有效运行策略；保存配置不代表运行中的服务已经应用。</span>{/if}</div>
  <section class="dot-section"><div class="dot-task-head"><div><h3>任务与恢复回执</h3><p class="dot-caption">复用持久任务，选择记录查看原句柄与结果。{snapshot ? `观察于 ${timestamp(snapshot.observed_at)}` : ""}</p></div><a href={`/?workspace=${encodeURIComponent(profile.id)}`}>打开完整工作台 ↗</a></div>
    {#if taskError}<p class="dot-error" role="alert">任务读取失败，已清除旧观察与回执。{taskError}</p>{/if}
    {#if snapshot?.partial}<div class="dot-warning">任务观察不完整。{snapshot.warnings.filter(item => item.workspace_id === profile.id).map(item => item.message).join("；")}请刷新核对，不据此判断任务已结束。</div>{/if}
    {#if tasks.length}<div class="dot-tasks">{#each tasks as task (task.task_id)}<button class="dot-task" class:selected={selected?.task_id === task.task_id} onclick={() => void selectTask(task)} aria-pressed={selected?.task_id === task.task_id}><strong>{task.goal}</strong><StateBadge state={task.display_state}/><small>{task.task_id} · revision {task.revision}</small><small>{task.running_jobs} 运行 · {task.unknown_jobs} 结果不明</small></button>{/each}</div>
    {:else if !snapshot}<p class="dot-empty" aria-live="polite">{busy ? "读取现有持久任务…" : "尚未取得任务记录；不会替换为示例数据。"}</p>
    {:else if !snapshot.partial}<p class="dot-empty">此工作区暂无持久任务。请在已连接的客户端按原有 task_open 生命周期开始；此页不执行命令。</p>{/if}
    {#if snapshot?.next_cursor}<p class="dot-caption">此处只显示最近 6 项；其余记录在完整工作台分页读取。</p>{/if}
    {#if detailBusy}<p class="dot-caption" role="status">读取选中任务回执…</p>{/if}{#if detailError}<p class="dot-error" role="alert">{detailError}</p>{/if}
    {#if selectedDetail}<div class="dot-receipt"><div class="dot-task-head"><strong>原任务回执 · {selectedDetail.state}</strong><button class="td-button small" onclick={() => void copy(selectedDetail!.handoff, "接续指令")}><Copy size={12}/>复制接续指令</button></div><p>{selectedDetail.checkpoint.summary || "尚无检查点摘要。任务状态来自持久记录，命令退出成功不等于业务验收。"}</p>{#if selectedDetail.checkpoint.next_step}<p class="dot-next">下一步：{selectedDetail.checkpoint.next_step}</p>{/if}<dl class="dot-receipt-meta"><div><dt>查询工作区</dt><dd>{selectedDetail.workspace_id}</dd></div><div><dt>task_id</dt><dd>{selectedDetail.task_id}</dd></div><div><dt>revision</dt><dd>{selectedDetail.revision}</dd></div><div><dt>任务创建</dt><dd>{timestamp(selectedDetail.created)}</dd></div><div><dt>最近更新</dt><dd>{timestamp(selectedDetail.updated)}</dd></div></dl>
      {#if selectedDetail.jobs.jobs.length}<div class="dot-jobs">{#each selectedDetail.jobs.jobs as item (item.job_id)}<div class="dot-job"><FileText size={13}/><div><strong>{item.status}{item.exit_code !== null ? ` · 退出码 ${item.exit_code}` : " · 尚无退出码"}</strong><small>job_id {item.job_id} · request_id {item.request_id}</small><p>{item.waiting_for ? `等待：${item.waiting_for}` : item.detail || "尚无作业摘要"}</p></div><button class="td-button small" onclick={() => job = item}>读取输出</button></div>{/each}</div>{:else}<p>暂无作业回执。保存任务不代表命令已运行。</p>{/if}
      {#if selectedDetail.jobs.truncated}<p>仅显示最近 20 个作业；更多信息由原客户端按既有协议读取。</p>{/if}<div class="dot-warning">结果不明时保留原请求，先核对输出与当前文件。日志可能含敏感信息，请勿直接公开分享。</div></div>{/if}
    {#if selected}<div class="dot-operations"><div class="dot-task-head"><div><h3>操作回执</h3><p class="dot-caption">查询工作区 {profile.id} · 任务 {selected.task_id}</p></div><button class="td-button small" disabled={receiptBusy} onclick={() => selected && void loadReceipts(selected)}>刷新回执</button></div>
      {#if receiptBusy}<p class="dot-caption" role="status">读取真实调用审计和持久回执…</p>{/if}{#if receiptError}<p class="dot-error" role="alert">操作回执读取失败，未沿用旧结果。{receiptError}</p>{/if}
      {#if receipts}{#if receipts.partial || receipts.warnings.length}<div class="dot-warning">观察不完整：{receipts.warnings.join("；") || "部分来源不可读取。"}</div>{/if}
        {#each receipts.items as receipt (receipt.id)}<article class="dot-operation"><div class="dot-task-head"><strong>{receipt.tool_name} · {receipt.status}</strong><small>{receiptSource(receipt)}</small></div><dl class="dot-operation-meta"><dt>原调用工作区</dt><dd>{receipt.workspace_id ?? "未记录；此回执按实际目录查询，不能补猜原 profile"}</dd><dt>task_id</dt><dd>{receipt.task_id}</dd><dt>request_id</dt><dd>{receipt.request_id ?? "未记录"}</dd><dt>协议请求 ID</dt><dd>{receipt.protocol_request_id ?? "未记录"}</dd><dt>job_id</dt><dd>{receipt.job_id ?? "无作业句柄"}</dd><dt>调用开始</dt><dd>{receipt.started_at_ms === null ? "未记录" : timestamp(receipt.started_at_ms)}</dd><dt>调用结束</dt><dd>{receipt.finished_at_ms === null ? "未记录" : timestamp(receipt.finished_at_ms)}</dd>{#if receipt.job_id}<dt>命令进程开始</dt><dd>{receipt.job_started_at_ms === null ? "未记录" : timestamp(receipt.job_started_at_ms)}</dd><dt>执行结束记录</dt><dd>{receipt.job_finished_at_ms === null ? "未记录" : timestamp(receipt.job_finished_at_ms)}</dd><dt>记录退出码</dt><dd>{receipt.exit_code ?? "未记录"}</dd>{/if}</dl><p class="dot-caption">{receiptTiming(receipt)}</p>
          {#each receipt.patch_hashes as file (file.path)}<div class="dot-hash"><strong>{file.path}</strong><dl><dt>补丁前 SHA256</dt><dd>{receiptHash(file.before_sha256, file.before_exists)}</dd><dt>补丁后 SHA256</dt><dd>{receiptHash(file.after_sha256, file.after_exists)}</dd></dl></div>{/each}
          {#if receipt.details_state !== "complete"}<p class="dot-caption">{receipt.details_state === "truncated" ? "此回执详情已截断；未展示字段不能据此判断为空。" : "此历史回执未保存完整详情。"}</p>{/if}</article>
        {/each}
        {#if receipts.items.length === 0}<p class="dot-empty">当前查询没有操作回执；未记录不代表从未执行。</p>{/if}<div class="dot-task-head"><p class="dot-caption">观察于 {timestamp(receipts.observed_at)} · 当前页 {receipts.items.length} 项</p><button class="td-button small" disabled={receiptBusy || !receipts.next_cursor} onclick={() => selected && receipts && void loadReceipts(selected, receipts.next_cursor)}>下一页回执</button></div>
      {/if}
    </div>{/if}
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
  .dot-protocol-meta,.dot-operation-meta{display:grid;grid-template-columns:auto minmax(0,1fr);gap:5px 12px;font-size:11px;margin:10px 0}.dot-protocol-meta dt,.dot-operation-meta dt{color:var(--td-muted)}.dot-protocol-meta dd,.dot-operation-meta dd{margin:0;overflow-wrap:anywhere}.dot-runtime{display:flex;gap:6px 18px;flex-wrap:wrap;padding:10px 18px;border-bottom:1px solid var(--td-line);font-size:11px;color:var(--td-muted)}.dot-runtime strong{color:var(--td-green)}.dot-operations{border-top:1px solid var(--td-line);padding-top:16px;margin-top:16px;overflow-wrap:anywhere}.dot-operation{border:1px solid var(--td-line);border-radius:4px;padding:12px;margin:12px 0;background:var(--td-bg)}.dot-operation strong{font-size:12px}.dot-operation small{font-size:10px;color:var(--td-muted)}.dot-hash{border-top:1px solid var(--td-line);margin-top:10px;padding-top:10px;font-size:11px}.dot-hash dl{display:grid;gap:4px;margin:7px 0}.dot-hash dt{color:var(--td-muted)}.dot-hash dd{margin:0;font:10px/1.5 var(--td-mono);overflow-wrap:anywhere}
  .dot-tasks{margin-top:12px;border:1px solid var(--td-line);border-radius:4px;overflow:hidden}.dot-task{padding:11px 12px;display:grid;grid-template-columns:minmax(0,1fr) auto;gap:5px 14px;width:100%;text-align:left;border:0;background:var(--td-panel);color:var(--td-ink)}.dot-task+.dot-task{border-top:1px solid var(--td-line)}.dot-task:hover,.dot-task.selected{background:var(--td-soft)}.dot-task strong{font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.dot-task small{font:10px var(--td-mono);overflow-wrap:anywhere}.dot-task small:last-child{text-align:right;color:var(--td-muted)}
  .dot-receipt{padding:12px;border:1px solid var(--td-line);border-radius:4px;margin-top:10px;font-size:12px;background:var(--td-bg);overflow-wrap:anywhere}.dot-receipt p{font-size:11px;color:var(--td-muted);margin:8px 0}.dot-receipt-meta{display:flex;gap:12px 22px;flex-wrap:wrap;margin:10px 0}.dot-receipt-meta div{display:flex;gap:7px;font-size:10px}.dot-receipt-meta dt{color:var(--td-muted)}.dot-receipt-meta dd{margin:0;font-family:var(--td-mono)}
  .dot-jobs{margin:12px 0;border-top:1px solid var(--td-line)}.dot-job{display:flex;align-items:flex-start;gap:9px;border-bottom:1px solid var(--td-line);padding:10px 0}.dot-job>div{min-width:0;flex:1}.dot-job strong{font-size:11px}.dot-job small{display:block;font:10px var(--td-mono);margin-top:3px;overflow-wrap:anywhere}.dot-job p{margin:3px 0 0}.dot-job :global(svg){margin-top:3px;flex-shrink:0}.dot-empty{font-size:12px;color:var(--td-muted);border:1px dashed var(--td-line);padding:14px;margin:12px 0 0;border-radius:4px}.dot-error{color:var(--td-error);font-size:11px;overflow-wrap:anywhere;margin:10px 0}.dot-panel :global(.td-button:active){transform:translateY(1px)}
  @media(max-width:850px){.dot-grid{grid-template-columns:1fr}.dot-guide{border-right:0;border-bottom:1px solid var(--td-line)}}
  @media(max-width:500px){.dot-head{align-items:flex-start}.dot-head,.dot-section{padding:14px}.dot-head h2{font-size:17px}.dot-head p{max-width:215px}.dot-safety{padding:10px 14px}.dot-task-head{flex-wrap:wrap}.dot-job{flex-wrap:wrap}.dot-job>div{flex-basis:calc(100% - 25px)}.dot-job>button{margin-left:22px}}
</style>
