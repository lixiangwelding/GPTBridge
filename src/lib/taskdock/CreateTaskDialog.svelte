<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { workspaces } from "$lib/stores/app";
  import { createTask } from "./api";
  import { newTaskOpen, addProjectOpen, taskCreated } from "./state";
  import { showToast } from "$lib/stores/toast";
  import Dialog from "./Dialog.svelte";
  let projectId = $state(""), goal = $state(""), error = $state(""), busy = $state(false);
  let requestId = $state<string | null>(null), submitted = $state(false);
  const storageKey = "taskdock:pending-create";
  onMount(() => {
    try {
      const saved = JSON.parse(sessionStorage.getItem(storageKey) || "null");
      if (saved && typeof saved.requestId === "string" && typeof saved.goal === "string" && typeof saved.projectId === "string") {
        requestId = saved.requestId; goal = saved.goal; projectId = saved.projectId; submitted = true;
        error = "保留了上次未核实的创建请求。重试使用同一编号，不会盲目重复新建。";
      }
    } catch { /* malformed optional session draft is not executed */ }
  });
  $effect(() => { if ($newTaskOpen && !projectId) projectId = $page.url.searchParams.get("workspace") || $workspaces[0]?.id || ""; });
  async function submit(event: SubmitEvent) {
    event.preventDefault(); if (busy || !projectId || !goal.trim()) return;
    if (new TextEncoder().encode(goal.trim()).length > 16000) { error = "目标过长，请控制在16000字节内。"; return; }
    requestId ??= crypto.randomUUID(); submitted = true; busy = true; error = "";
    try {
      // Persist identity before invoking. Retry after an uncertain result uses the same intent.
      sessionStorage.setItem(storageKey, JSON.stringify({ requestId, projectId, goal }));
      const result = await createTask(projectId, goal, requestId);
      sessionStorage.removeItem(storageKey);
      taskCreated.set({ workspaceId: result.workspace_id, taskId: result.task_id });
      newTaskOpen.set(false);
      await goto(`/?workspace=${encodeURIComponent(result.workspace_id)}&task=${encodeURIComponent(result.task_id)}`);
      goal = ""; requestId = null; submitted = false;
      showToast("目标已保存，等待 AI 客户端接续；未启动任何命令。");
    } catch (e) { error = `${String(e)}\n请求编号保留：${requestId}。请重试核验原请求。`; }
    finally { busy = false; }
  }
</script>
<Dialog open={$newTaskOpen} title="告诉工具，这次要做什么。" onClose={() => newTaskOpen.set(false)}>
  {#if !$workspaces.length}<div class="td-empty"><h2>先添加一个项目</h2><p>任务会关联这个项目的目录、技能与权限。</p><button class="td-button primary" onclick={() => {newTaskOpen.set(false);addProjectOpen.set(true);}}>添加项目</button></div>
  {:else}<form id="td-create-form" onsubmit={submit}>
    <div class="td-field"><label for="td-task-project">项目</label><select id="td-task-project" bind:value={projectId} disabled={busy || submitted} required>{#each $workspaces as p}<option value={p.id}>{p.name}</option>{/each}</select></div>
    <div class="td-field"><label for="td-task-goal">任务目标</label><textarea id="td-task-goal" bind:value={goal} required maxlength={12000} disabled={busy || submitted} placeholder="例如：检查封面生成失败的原因，先核对证据，再修复并自测。"></textarea><small>系统生成任务编号。保存后复制接续指令给已连接的 AI 客户端，不把保存目标伪装成开始执行。</small></div>
    {#if error}<div class="td-error" role="alert" style="white-space:pre-wrap">{error}</div>{/if}
  </form>{/if}
  {#snippet footer()}<button class="td-button" onclick={() => newTaskOpen.set(false)}>关闭</button><button class="td-button primary" form="td-create-form" type="submit" disabled={busy || !projectId || !goal.trim() || !$workspaces.length}>{busy ? "正在保存…" : submitted ? "核验并重试原请求" : "保存目标并生成接续指令"}</button>{/snippet}
</Dialog>
