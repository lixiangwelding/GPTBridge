<script lang="ts">
  import type { Snippet } from "svelte";
  import { page } from "$app/stores";
  import { LayoutDashboard, Folder, Sparkles, Settings2, Plus, Menu, X, Plug } from "@lucide/svelte";
  import { workspaces } from "$lib/stores/app";
  import { addProjectOpen, appInfo, compact, connectionProject } from "./state";
  let { children }: { children: Snippet } = $props();
  let menuOpen = $state(false);
  let side = $state<HTMLElement>();
  const entries = [{ path: "/", title: "工作台", icon: LayoutDashboard }, { path: "/projects", title: "项目", icon: Folder }, { path: "/workspace/", title: "MCP 连接", icon: Plug }, { path: "/skills", title: "工具与技能", icon: Sparkles }, { path: "/settings", title: "设置", icon: Settings2 }];
  const active = (path: string) => path === "/" ? $page.url.pathname === "/" : $page.url.pathname.startsWith(path);
  const title = $derived(entries.find(item => active(item.path))?.title ?? "高级配置");
  const scope = $derived($page.url.searchParams.get("workspace"));
  const configWorkspaceId = $derived($page.url.pathname.startsWith("/workspace/")
    ? decodeURIComponent($page.url.pathname.slice("/workspace/".length))
    : ($workspaces.some(project => project.id === scope) ? scope : $workspaces[0]?.id));
  $effect(() => { void $page.url.pathname; void $page.url.search; menuOpen = false; });
  $effect(() => { if (menuOpen && side) side.querySelector<HTMLButtonElement>("button")?.focus(); });
  function menuKey(event: KeyboardEvent) {
    if (!menuOpen) return;
    if (event.key === "Escape") { menuOpen = false; return; }
    if (event.key !== "Tab" || !side) return;
    const focusable = Array.from(side.querySelectorAll<HTMLElement>("a[href],button:not([disabled])"));
    const first = focusable[0], last = focusable.at(-1);
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
    if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
  }
</script>
<svelte:window onkeydown={menuKey} />
<div class="td-app" class:td-dense={$compact}>
  {#if menuOpen}<button type="button" class="td-menu-dismiss" aria-label="收起导航" onclick={() => menuOpen = false}></button>{/if}
  <aside class="td-side" class:open={menuOpen} id="taskdock-navigation" aria-label="主导航" bind:this={side}>
    <div class="td-brand"><div class="td-brand-mark">gb<span>_</span></div><div><strong>GPTBridge</strong><small>AI WORKSPACE</small></div><button class="td-icon-button td-mobile-only" aria-label="关闭导航" onclick={() => menuOpen = false}><X size={17}/></button></div>
    <nav class="td-nav">{#each entries as item}<a href={item.path === "/workspace/" ? (configWorkspaceId ? `/workspace/${encodeURIComponent(configWorkspaceId)}` : "/projects") : item.path} class:active={active(item.path)} aria-current={active(item.path) ? "page" : undefined}><item.icon size={16}/>{item.title}</a>{/each}</nav>
    <div class="td-side-label"><span>项目快捷切换</span><button aria-label="添加项目" onclick={() => addProjectOpen.set(true)}><Plus size={15}/></button></div>
    <div class="td-project-links"><a href="/" class:active={!scope && $page.url.pathname === "/"}><i></i>全部项目</a>{#each $workspaces as project (project.id)}<a href={`/?workspace=${encodeURIComponent(project.id)}`} class:active={scope === project.id} title={project.name}><i></i><span>{project.name}</span></a>{/each}</div>
    <div class="td-side-bottom"><div><span class="td-eyebrow">CHAT TO WORK.</span><strong>把对话，接到工作现场。</strong><p>连接项目、工具与技能。<br>任务进展，一处看清。</p></div><div class="td-profile"><span>个人工作空间</span><span>LOCAL</span></div></div>
  </aside>
  <div class="td-body" inert={menuOpen}>
    <header class="td-topbar"><div class="td-breadcrumb"><button class="td-icon-button td-mobile-only" aria-label="打开导航" aria-expanded={menuOpen} aria-controls="taskdock-navigation" onclick={() => menuOpen = true}><Menu size={18}/></button><span class="td-muted td-desktop-label">个人空间</span><span class="slash td-desktop-label">/</span><span>{title}</span></div><div class="td-top-actions"><span class="td-status-small" class:unknown={!$appInfo}><i></i>{$appInfo ? `本地工作台 · v${$appInfo.version}` : "原生服务未连接"}</span><button class="td-button" onclick={() => connectionProject.set(scope || $workspaces[0]?.id || "")}><Plug size={14}/>管理连接</button></div></header>
    <main class="td-main">{@render children()}<footer class="td-footer"><span>GPTBridge · 任务记录保存在本地，执行仍遵循项目权限。</span><span class="td-mono">{$appInfo ? `DESKTOP ${$appInfo.version}` : "BACKEND UNAVAILABLE"}</span></footer></main>
  </div>
</div>
