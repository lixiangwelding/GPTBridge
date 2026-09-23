<script lang="ts">
  import { Plus, RefreshCw, ArrowRight, FolderOpen } from "@lucide/svelte";
  import { workspaces } from "$lib/stores/app";
  import { openWorkspaceDirectory } from "$lib/api/workspaces";
  import { showToast } from "$lib/stores/toast";
  import { addProjectOpen, connectionProject, projectError, projectLoading, refreshProjects } from "$lib/taskdock/state";
  import { getSnapshot } from "$lib/taskdock/api";
  import type { Snapshot } from "$lib/taskdock/types";
  let snapshot = $state<Snapshot | null>(null), error = $state("");
  let generation = 0;
  $effect(() => { void $workspaces; const token = ++generation; void getSnapshot(null, { query: "", filter: "all", cursor: null, limit: 1 }).then(result => {if(token===generation){snapshot=result;error="";}}).catch(e=>{if(token===generation)error=String(e);}); return () => { generation++; }; });
  async function open(path: string) { try {await openWorkspaceDirectory(path);}catch(e){showToast(String(e),{kind:"error"});} }
</script>
<svelte:head><title>项目 · GPTBridge</title></svelte:head>
<div class="td-page-head"><div><h1>项目</h1><p>目录记住一次。日常选名称，不必反复粘贴路径。</p></div><div class="td-actions"><button class="td-button" disabled={$projectLoading} onclick={() => refreshProjects()} aria-label="刷新项目"><RefreshCw size={14}/></button><button class="td-button primary" onclick={() => addProjectOpen.set(true)}><Plus size={14}/>添加项目</button></div></div>
{#if $projectError || error}<div class="td-error" role="alert">{$projectError || error}</div>{/if}
{#if snapshot?.partial}<div class="td-alert"><details><summary>部分项目无法读取，计数不完整</summary>{#each snapshot.warnings as warning}<p>{warning.name}：{warning.message}</p>{/each}</details></div>{/if}
<section class="td-panel" aria-label="项目目录">
  {#each $workspaces as project (project.id)}
    {@const counts = snapshot?.projects.find(p => p.workspace_id === project.id)?.counts}
    <div class="td-list-row"><div><h2>{project.name}</h2><p class="td-mono">{project.path}</p><div class="td-actions" style="margin-top:8px"><button class="td-link-button" onclick={() => open(project.path)}><FolderOpen size={12} style="display:inline;margin-right:4px"/>打开目录</button><button class="td-link-button" onclick={() => connectionProject.set(project.id)}>连接</button><a class="td-link-button" href={`/workspace/${encodeURIComponent(project.id)}`}>高级配置</a></div></div><div class="td-project-count"><span class="td-state running">● {counts ? counts.running : "—"} 个运行中</span><p>{counts ? `${counts.attention} 待处理 · ${counts.ready} 待接续` : "计数不可用或与其他项目共享目录"}</p></div><div class="td-actions"><a class="td-button small" href={`/?workspace=${encodeURIComponent(project.id)}`}>查看任务<ArrowRight size={13}/></a></div></div>
  {:else}<div class="td-empty"><h2>{$projectLoading ? "正在读取项目…" : "还没有项目"}</h2><p>添加本地目录后，任务、连接和技能会围绕项目组织。</p><button class="td-button primary" onclick={() => addProjectOpen.set(true)}>选择项目目录</button></div>{/each}
</section>
<div class="td-info-band">同一物理目录的任务记录会去重，不把项目别名当作资源隔离。原有端口、认证、上游和隧道配置仍在“高级配置”中；添加项目不会自动启动它们。</div>
