<script lang="ts">
  import { Search, RefreshCw, ArrowRight } from "@lucide/svelte";
  import { page } from "$app/stores";
  import { workspaces } from "$lib/stores/app";
  import { addProjectOpen } from "$lib/taskdock/state";
  import { getSkills, setSkillPreference } from "$lib/taskdock/api";
  import type { Skill, SkillPage, ToolDefinition } from "$lib/taskdock/types";
  import SkillDialog from "$lib/taskdock/SkillDialog.svelte";
  import Dialog from "$lib/taskdock/Dialog.svelte";
  import { showToast } from "$lib/stores/toast";
  let projectId = $state(""), search = $state(""), tab = $state<"skills"|"tools">("skills");
  let data = $state<SkillPage | null>(null), busy = $state(false), changing = $state(""), error = $state("");
  let selected = $state<{workspaceId:string;skill:Skill}|null>(null), tool = $state<ToolDefinition|null>(null);
  let generation = 0;
  $effect(() => { if(!$workspaces.some(p=>p.id===projectId))projectId=$page.url.searchParams.get("workspace") || $workspaces[0]?.id || ""; });
  $effect(() => { const id=projectId, query=search; data=null; error=""; generation++; const timeout=setTimeout(()=>{if(id)void load(false,id,query);},250);return()=>clearTimeout(timeout); });
  async function load(append=false,id=projectId,query=search) {
    if(!id)return;const token=++generation,cursor=append?data?.next_cursor||null:null;busy=true;
    try {
      const result=await getSkills(id,query,cursor);
      if(token!==generation)return;
      if(append&&data){const seen=new Set(data.skills.map(s=>s.skill_id));result.skills=[...data.skills,...result.skills.filter(s=>!seen.has(s.skill_id))];}
      data=result;error="";
    } catch(e){if(token===generation)error=String(e);}finally{if(token===generation)busy=false;}
  }
  async function toggle(skill:Skill,event:Event) {
    const input=event.currentTarget as HTMLInputElement,enabled=input.checked;input.checked=skill.model_invocable;
    if(!data||changing)return;const id=projectId,revision=data.preferences_revision;changing=skill.skill_id;
    try{await setSkillPreference(id,skill.skill_id,enabled,revision);if(id===projectId)await load();showToast(`已${enabled?"启用":"停用"}项目内的自动匹配；没有重启服务或更改执行权限。`);}
    catch(e){if(id===projectId)error=String(e);}finally{changing="";}
  }
  const visibleTools=$derived(data?.tools.filter(t=>`${t.name} ${t.description}`.toLowerCase().includes(search.toLowerCase()))||[]);
</script>
<svelte:head><title>工具与技能 · TaskDock</title></svelte:head>
<div class="td-page-head"><div><h1>工具与技能</h1><p>默认按目标自动匹配。你能看见来源，也能随时收紧自动使用范围。</p></div><button class="td-button" disabled={busy||!projectId} onclick={()=>load()}><RefreshCw size={14}/>{busy?"扫描中…":"重新扫描"}</button></div>
<div class="td-toolbar" style="padding:0 0 16px;border:0"><label><span class="td-sr-only">选择技能项目</span><select class="td-select" bind:value={projectId}><option value="" disabled>选择项目</option>{#each $workspaces as p}<option value={p.id}>{p.name}</option>{/each}</select></label><label class="td-search"><Search size={13}/><span class="td-sr-only">搜索工具或技能</span><input type="search" bind:value={search} placeholder="搜索名称 / 功能" maxlength={100}/></label></div>
{#if error}<div class="td-error" role="alert"><p>{error}</p><button class="td-button small" onclick={()=>load()}>刷新目录后重试</button></div>{/if}
{#if !$workspaces.length}<div class="td-panel td-empty"><h2>先添加一个项目</h2><p>技能发现遵循项目目录与已配置的全局技能根。</p><button class="td-button primary" onclick={()=>addProjectOpen.set(true)}>添加项目</button></div>
{:else}<section class="td-panel"><header class="td-toolbar"><div class="td-tabs"><button class:active={tab==="skills"} onclick={()=>tab="skills"}>技能 {data?data.matches:"—"}</button><button class:active={tab==="tools"} onclick={()=>tab="tools"}>内置工具 {data?data.tools.length:"—"}</button></div><span class="td-pill">真实目录 · 不复制源文件</span></header>
  {#if data?.scan_truncated}<div class="td-alert">扫描达到边界，当前不是完整目录；请缩小搜索或核对技能根。</div>{/if}
  {#if data?.skipped_entries}<div class="td-muted" style="padding:9px 14px;font-size:11px">本次跳过 {data.skipped_entries} 个无法解析或不可访问的条目。</div>{/if}
  {#if tab==="skills"}
    {#each data?.skills||[] as skill (skill.skill_id)}<div class="td-list-row skill"><div><h2>{skill.name} <span class="td-pill">{skill.scope==="user"?"全局":"项目"}</span></h2><p>{skill.description}</p>{#if skill.source_manual_only}<p>源文件限定：仅手动调用</p>{/if}</div><div class="td-actions"><label class="td-switch"><input type="checkbox" checked={skill.model_invocable} disabled={!!changing||skill.source_manual_only} onchange={event=>toggle(skill,event)} aria-label={`${skill.name}自动匹配`}/>自动</label><button class="td-button small" onclick={()=>selected={workspaceId:projectId,skill}}>详情</button></div></div>
    {:else}<div class="td-empty">{busy?"正在扫描真实技能目录…":"没有匹配的技能。检查项目目录、搜索词或高级技能配置。"}</div>{/each}
    {#if data?.next_cursor}<div class="td-table-footer"><span>已展示 {data.skills.length} / {data.matches} 个匹配项</span><button class="td-button small" disabled={busy} onclick={()=>load(true)}>读取更多<ArrowRight size={13}/></button></div>{/if}
  {:else}{#each visibleTools as item (item.name)}<div class="td-list-row tool"><div><h2 class="td-mono">{item.name}</h2><p>{item.description.slice(0,240)}{item.description.length>240?"…":""}</p></div><button class="td-button small" onclick={()=>tool=item}>合同</button></div>{:else}<div class="td-empty">{busy?"读取工具定义…":"没有匹配的内置工具。"}</div>{/each}{/if}
</section>{/if}
<div class="td-info-band">自动匹配偏好仅降低当前项目的自动发现范围，不授予执行权限。新版服务每次发现时读取偏好；已经运行的旧版服务不会因此被自动重启，请在安全时机升级它。上游 MCP 的逐工具可见性仍在项目高级配置中管理。</div>
<SkillDialog {selected} onClose={()=>selected=null}/>
<Dialog open={!!tool} title={tool?.name||"工具合同"} onClose={()=>tool=null} wide>{#if tool}<p>{tool.description}</p><pre class="td-code">{JSON.stringify(tool.inputSchema,null,2)}</pre><p>这是当前项目工具配置对应的真实定义。查看合同不会调用工具。</p>{/if}</Dialog>
