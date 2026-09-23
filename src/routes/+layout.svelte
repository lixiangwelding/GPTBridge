<script lang="ts">
  import "../app.css";
  import "$lib/taskdock/taskdock.css";
  import { onMount, type Snippet } from "svelte";
  import { page } from "$app/stores";
  import Shell from "$lib/taskdock/Shell.svelte";
  import ProjectDialog from "$lib/taskdock/ProjectDialog.svelte";
  import CreateTaskDialog from "$lib/taskdock/CreateTaskDialog.svelte";
  import ConnectionDialog from "$lib/taskdock/ConnectionDialog.svelte";
  import ToastHost from "$lib/components/ToastHost.svelte";
  import CloseConfirmDialog from "$lib/components/CloseConfirmDialog.svelte";
  import { initialize } from "$lib/taskdock/state";
  import { nativeAvailable } from "$lib/taskdock/api";
  import { startUiMemoryGuard } from "$lib/ui-memory-guard";
  import { startCloseGuard } from "$lib/close-guard";

  let { children }: { children: Snippet } = $props();
  let closeConfirmOpen = $state(false);
  const legacy = $derived($page.url.pathname.startsWith("/workspace/") || $page.url.pathname.startsWith("/settings/"));
  onMount(() => {
    void initialize();
    if (!nativeAvailable()) return;
    const stopMemory = startUiMemoryGuard();
    const stopClose = startCloseGuard(() => closeConfirmOpen = true);
    return () => { stopMemory(); stopClose(); };
  });
</script>

<Shell>
  {#if legacy}<div class="td-legacy-crumb"><a href="/settings">← 返回设置</a><span>高级配置 · 保留现有功能与数据</span></div><div class="td-legacy">{@render children()}</div>{:else}{@render children()}{/if}
</Shell>
<ProjectDialog/><CreateTaskDialog/><ConnectionDialog/>
<ToastHost/>
<CloseConfirmDialog open={closeConfirmOpen} onCancel={() => closeConfirmOpen = false}/>
