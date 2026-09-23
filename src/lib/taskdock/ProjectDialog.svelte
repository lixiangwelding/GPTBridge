<script lang="ts">
  import { open as chooseDirectory } from "@tauri-apps/plugin-dialog";
  import { FolderOpen } from "@lucide/svelte";
  import Dialog from "./Dialog.svelte";
  import { addProject, nativeAvailable } from "./api";
  import { addProjectOpen, refreshProjects } from "./state";
  import { showToast } from "$lib/stores/toast";
  let name = $state(""), path = $state(""), error = $state(""), busy = $state(false);
  async function choose() {
    try {
      if (!nativeAvailable()) throw new Error("目录选择需要 TaskDock 桌面应用。");
      const selected = await chooseDirectory({ directory: true, multiple: false, title: "选择项目目录" });
      if (typeof selected === "string") { path = selected; error = ""; }
    } catch (e) { error = String(e); }
  }
  async function save(event: SubmitEvent) {
    event.preventDefault(); if (busy || !path.trim()) return; busy = true; error = "";
    try { await addProject(path.trim(), name); await refreshProjects(); addProjectOpen.set(false); name = ""; path = ""; showToast("项目已保存；重复目录会复用现有项目。"); }
    catch (e) { error = String(e); } finally { busy = false; }
  }
</script>
<Dialog open={$addProjectOpen} title="记住一个项目" onClose={() => addProjectOpen.set(false)}>
  <form id="td-project-form" onsubmit={save}>
    <div class="td-field"><label for="td-project-path">项目目录</label><div class="td-actions"><input id="td-project-path" bind:value={path} placeholder="选择本地项目，或粘贴完整目录" required maxlength={4096} disabled={busy}/><button class="td-button" type="button" onclick={choose} disabled={busy}><FolderOpen size={15}/>选择目录</button></div><small>仅添加项目记录，不启动服务、不移动文件，也不扩大执行权限。</small></div>
    <div class="td-field"><label for="td-project-name">项目名称 <span class="td-muted">（可选）</span></label><input id="td-project-name" bind:value={name} maxlength={60} placeholder="默认使用目录名称" disabled={busy}/></div>
    {#if error}<div class="td-error" role="alert">{error}</div>{/if}
  </form>
  {#snippet footer()}<button class="td-button" onclick={() => addProjectOpen.set(false)}>取消</button><button class="td-button primary" type="submit" form="td-project-form" disabled={busy || !path.trim()}>{busy ? "保存中…" : "保存项目"}</button>{/snippet}
</Dialog>
