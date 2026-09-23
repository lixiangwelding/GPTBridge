<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { Plus, Search, RefreshCw, AlertTriangle, ArrowLeft, ArrowRight } from "@lucide/svelte";
  import { workspaces } from "$lib/stores/app";
  import { getSnapshot, getTask } from "./api";
  import { addProjectOpen, newTaskOpen, connectionProject, projectError, projectLoading, refreshProjects, compact, setCompact, taskCreated } from "./state";
  import { labels, relativeTime, timestamp, taskKey } from "./format";
  import type { Snapshot, TaskRow, TaskDetail, TaskFilter, TaskState, TaskCursor, JobSummary } from "./types";
  import TaskTable from "./TaskTable.svelte";
  import TaskInspector from "./TaskInspector.svelte";
  import StateBadge from "./StateBadge.svelte";
  import Dialog from "./Dialog.svelte";
  import JobLogDialog from "./JobLogDialog.svelte";

  let snapshot = $state<Snapshot | null>(null), error = $state(""), busy = $state(false);
  let searchText = $state(""), filter = $state<TaskFilter>("all"), pages = $state<(TaskCursor | null)[]>([null]), pageIndex = $state(0);
  let selected = $state<TaskRow | null>(null), detail = $state<TaskDetail | null>(null), detailLoading = $state(false), detailError = $state("");
  let narrow = $state(false), drawerOpen = $state(false), logJob = $state<JobSummary | null>(null);
  let searchInput = $state<HTMLInputElement>();
  let disposed = false, inflight = false, pending = false, detailToken = 0, failures = 0;
  const scope = $derived($page.url.searchParams.get("workspace"));
  const scopeName = $derived(scope ? $workspaces.find(p => p.id === scope)?.name || "所选项目" : "全部项目");
  const filters: TaskFilter[] = ["all", "running", "attention", "ready", "queued", "done", "paused"];
  const stats: TaskState[] = ["running", "attention", "queued", "done"];
  const requestKey = () => JSON.stringify([scope, searchText.trim(), filter, pages[pageIndex]]);

  $effect(() => {
    void scope; void searchText; void filter; void $workspaces;
    pages = [null]; pageIndex = 0; selected = null; detail = null; snapshot = null; detailToken++;
    const timeout = setTimeout(() => void load(), 250);
    return () => clearTimeout(timeout);
  });

  async function load(quiet = false): Promise<void> {
    if (disposed) return;
    if (inflight) { pending = true; return; }
    inflight = true; if (!quiet || !snapshot) busy = true;
    const key = requestKey();
    try {
      const result = await getSnapshot(scope, { query: searchText.trim(), filter, cursor: pages[pageIndex] || null, limit: 30 });
      if (disposed || key !== requestKey()) return;
      snapshot = result; error = ""; failures = 0;
      const hint = $page.url.searchParams.get("task");
      const row = result.tasks.find(t => selected && taskKey(t) === taskKey(selected)) || result.tasks.find(t => t.task_id === hint) || result.tasks[0] || null;
      selected = row;
      if (row) void loadDetail(row); else { detail = null; detailToken++; }
    } catch (e) { if (!disposed && key === requestKey()) { error = String(e); failures++; } }
    finally { inflight = false; if (!disposed) { busy = false; if (pending) { pending = false; void load(); } } }
  }

  async function loadDetail(row = selected) {
    if (!row) return; const token = ++detailToken; detailLoading = true; detailError = "";
    try {
      const result = await getTask(row.workspace_id, row.task_id);
      if (!disposed && token === detailToken && selected && taskKey(selected) === taskKey(row)) detail = result;
    } catch (e) { if (!disposed && token === detailToken) detailError = String(e); }
    finally { if (!disposed && token === detailToken) detailLoading = false; }
  }

  function select(row: TaskRow) {
    selected = row; detail = null; logJob = null; void loadDetail(row);
    if (narrow) drawerOpen = true;
  }
  function changePage(next: boolean) {
    if (busy) return;
    if (next && snapshot?.next_cursor) { pages = [...pages.slice(0, pageIndex + 1), snapshot.next_cursor]; pageIndex++; }
    else if (!next && pageIndex > 0) pageIndex--;
    else return;
    selected = null; detail = null; void load();
  }
  function keydown(event: KeyboardEvent) {
    if (document.querySelector("dialog[open]")) return;
    const target = event.target as HTMLElement | null;
    const typing = target && (target.matches("input,textarea,select") || target.isContentEditable);
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") { event.preventDefault(); searchInput?.focus(); }
    else if (!typing && !event.metaKey && !event.ctrlKey && !event.altKey && event.key.toLowerCase() === "n") { event.preventDefault(); newTaskOpen.set(true); }
  }
  onMount(() => {
    disposed = false;
    const media = matchMedia("(max-width:1100px)");
    const resize = () => { narrow = media.matches; if (!narrow) drawerOpen = false; };
    resize(); media.addEventListener("change", resize);
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => { timer = setTimeout(async () => { if (!document.hidden) await load(true); if (!disposed) schedule(); }, Math.min(30000, 5000 * Math.max(1, 2 ** failures))); };
    const visible = () => { if (!document.hidden) void load(true); };
    schedule(); document.addEventListener("visibilitychange", visible);
    const unsubscribe = taskCreated.subscribe(value => { if (value) void load(); });
    return () => { disposed = true; detailToken++; clearTimeout(timer); media.removeEventListener("change", resize); document.removeEventListener("visibilitychange", visible); unsubscribe(); };
  });
</script>
<svelte:head><title>工作台 · GPTBridge</title><meta name="description" content="GPTBridge：把对话，接到你的工作现场。连接本地项目、工具与技能，集中查看任务进展。"/></svelte:head>
<svelte:window onkeydown={keydown}/>
<div class="td-page-head"><div><h1>工作台</h1><p>任务在推进。只把需要你的事放到眼前。</p></div><div class="td-actions"><button class="td-button td-search-trigger" onclick={() => searchInput?.focus()}><Search size={14}/>查找 <span class="td-kbd">⌘ K</span></button><button class="td-button primary" onclick={() => newTaskOpen.set(true)}><Plus size={15}/>新建任务</button></div></div>
<div class="td-stats">
  {#each stats as state}<button class="td-stat" class:attention={state === "attention"} onclick={() => filter = state} aria-label={`筛选${labels[state]}`}><strong>{snapshot ? snapshot.counts[state] : "—"}</strong><span>{labels[state]}</span></button>{/each}
  <label class="td-scope"><span>当前任务范围</span><select aria-label="项目筛选" value={scope || ""} onchange={event => goto(event.currentTarget.value ? `/?workspace=${encodeURIComponent(event.currentTarget.value)}` : "/")}><option value="">全部项目</option>{#each $workspaces as project}<option value={project.id}>{project.name}</option>{/each}</select></label>
</div>
{#if $projectError}<div class="td-error" role="alert"><p>{$projectError}</p><button class="td-button small" onclick={() => refreshProjects()}>重新读取项目</button></div>{/if}
{#if !$projectLoading && !$projectError && !$workspaces.length}
  <section class="td-welcome"><div class="td-eyebrow">LET'S GET STARTED</div><h2>三步开始，不用先学一堆配置。</h2><p>选目录、检查连接、说清目标。任务和上下文留在项目里，不再每次从头设置。</p><div class="td-onboard"><div><h3>01 / 选择项目目录</h3><p>目录记住一次，以后直接选项目名称。</p><button class="td-button small" onclick={() => addProjectOpen.set(true)}>添加项目</button></div><div><h3>02 / 连接你的 AI 工具</h3><p>推荐 MCP；兼容配置按需展开。</p><button class="td-button small" onclick={() => connectionProject.set("")}>管理连接</button></div><div><h3>03 / 说清任务目标</h3><p>保存目标，再交给已连接的 AI 客户端。</p><button class="td-button small" onclick={() => newTaskOpen.set(true)}>新建任务</button></div></div></section>
{/if}
{#if snapshot?.counts.attention}<div class="td-alert"><AlertTriangle size={15}/><span><strong>{snapshot.counts.attention} 项需要处理</strong> · 查看原因与下一步；不影响其他任务的独立执行。</span><button class="td-link-button" onclick={() => filter = "attention"}>立即处理 →</button></div>{/if}
{#if snapshot?.partial}<div class="td-alert" role="alert"><AlertTriangle size={15}/><details><summary>部分项目不可读，当前结果不完整</summary>{#each snapshot.warnings as warning}<p>{warning.name}：{warning.message}</p>{/each}</details></div>{/if}
{#if error && !$projectError}<div class="td-error" role="alert"><p>{error}</p><button class="td-button small" disabled={busy} onclick={() => load()}>重试读取真实数据</button>{#if snapshot}<span class="td-muted"> 保留上次成功快照，未替换为演示结果。</span>{/if}</div>{/if}
<div class="td-workarea"><div>
  <section class="td-panel" aria-label="任务列表" aria-busy={busy}>
    <div class="td-toolbar"><div class="td-tabs" aria-label="任务状态筛选">{#each filters as state}<button class:active={filter === state} aria-pressed={filter === state} onclick={() => filter = state}>{state === "all" ? "全部" : labels[state]}{#if snapshot && state !== "all"}<span class="td-mono" style="margin-left:4px;font-size:9px">{snapshot.counts[state]}</span>{/if}</button>{/each}</div><label class="td-search"><Search size={13}/><span class="td-sr-only">搜索任务目标或编号</span><input bind:this={searchInput} bind:value={searchText} type="search" placeholder="搜索任务 / 编号" maxlength={250}/></label></div>
    <div class="td-table-wrap">
      {#if snapshot?.tasks.length}<TaskTable tasks={snapshot.tasks} selected={selected ? taskKey(selected) : null} onSelect={select}/>
      {:else if busy}<div class="td-empty" role="status"><span class="td-loading-dot"></span>正在读取 {scopeName} 的任务记录…</div>
      {:else if snapshot}<div class="td-empty"><h2>{snapshot.partial ? "暂未读取到任务" : filter !== "all" || searchText ? "没有匹配的任务" : "从第一个目标开始"}</h2><p>{snapshot.partial ? "请先处理上方不可读项目，不能据此判断任务库为空。" : "目标保存到本地，执行过程与证据随后跟随任务更新。"}</p><div class="td-actions" style="justify-content:center"><button class="td-button small" onclick={() => {filter = "all";searchText = "";}}>清除筛选</button><button class="td-button small primary" onclick={() => newTaskOpen.set(true)}>新建任务</button></div></div>
      {:else}<div class="td-empty"><h2>等待真实数据</h2><p>连接原生工作台后显示任务；这里不会填充演示记录。</p><button class="td-button" onclick={() => load()}>重新读取</button></div>{/if}
    </div>
    <footer class="td-table-footer"><span>{snapshot ? `本页 ${snapshot.tasks.length} 条 · 匹配 ${snapshot.matched_total} 个任务` : "尚未获取任务快照"}</span><div class="td-page-controls"><button class="td-link-button" onclick={() => setCompact(!$compact)}>{$compact ? "标准排列" : "紧凑排列"}</button><button class="td-icon-button" aria-label="上一页" disabled={busy || pageIndex === 0} onclick={() => changePage(false)}><ArrowLeft size={13}/></button><span class="td-mono">{pageIndex + 1}</span><button class="td-icon-button" aria-label="下一页" disabled={busy || !snapshot?.next_cursor} onclick={() => changePage(true)}><ArrowRight size={13}/></button></div></footer>
  </section>
  <section class="td-panel td-recent" aria-label="最近更新"><header class="td-section-head"><h2>当前页最近更新 <span class="td-muted td-mono">/ LIVE STORE</span></h2><button class="td-link-button" disabled={busy} onclick={() => load()}><RefreshCw size={12} style="display:inline;margin-right:5px"/>{busy ? "读取中" : "刷新"}</button></header>{#each snapshot?.tasks.slice(0,3) || [] as task (taskKey(task))}<div class="td-feed-row"><time class="td-mono td-muted" title={timestamp(task.updated)}>{relativeTime(task.updated)}</time><p><button class="td-link-button" style="text-align:left" onclick={() => select(task)}>{task.goal}</button></p><StateBadge state={task.display_state}/></div>{:else}<div class="td-empty" style="padding:18px">任务更新后，记录会出现在这里。</div>{/each}</section>
  {#if snapshot}<div class="td-muted" style="font-size:10px;margin-top:10px">{snapshot.partial ? "部分" : "真实"}任务快照 · {timestamp(snapshot.observed_at)} · 活动页自动刷新，隐藏页暂停。</div>{/if}
 </div>
 <aside class="td-panel td-desktop-inspector" aria-label="选中任务详情"><TaskInspector task={selected} {detail} loading={detailLoading} error={detailError} onRetry={() => loadDetail()} onLog={job => logJob = job}/></aside>
</div>
<Dialog open={narrow && drawerOpen} title="任务详情" onClose={() => drawerOpen = false}><TaskInspector task={selected} {detail} loading={detailLoading} error={detailError} onRetry={() => loadDetail()} onLog={job => logJob = job}/></Dialog>
<JobLogDialog task={selected} job={logJob} onClose={() => logJob = null}/>
