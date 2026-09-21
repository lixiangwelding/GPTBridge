<script lang="ts">
  import type { WorkspaceProfile } from "$lib/types";
  let { profiles, hostId, selectedIds = [], running = false, onSave }: {
    profiles: WorkspaceProfile[];
    hostId: string;
    selectedIds?: string[];
    running?: boolean;
    onSave: (ids: string[]) => Promise<void>;
  } = $props();
  let selected = $state<string[]>([]);
  let saving = $state(false);
  let error = $state("");
  $effect(() => { selected = [...selectedIds]; });
  const candidates = $derived(profiles.filter(p => p.id !== hostId));
  function toggle(id: string, checked: boolean) {
    selected = checked ? [...new Set([...selected, id])] : selected.filter(x => x !== id);
  }
  async function save() {
    saving = true;
    error = "";
    try { await onSave([...selected]); }
    catch (e) { error = String(e); }
    finally { saving = false; }
  }
</script>

<section class="grid gap-3" aria-label="一个入口共享多个仓库">
  <p class="tx-section-label">一个入口 · 多个仓库</p>
  <p class="text-sm text-[var(--text-secondary)]">
    本仓库作为入口。勾选其他仓库后，只启动这里的 MCP 和一条 FRP 隧道；其他仓库不需要单独启动服务。
  </p>
  <p class="text-sm text-[var(--text-secondary)]">
    当前入口的登录凭据将能访问全部所选仓库；每次操作仍明确指定仓库，分别使用各自的文件边界和执行策略。
  </p>
  {#if candidates.length === 0}
    <p class="text-sm">先通过左侧“添加工作区”添加其他仓库。</p>
  {:else}
    <div class="grid gap-2">
      {#each candidates as candidate (candidate.id)}
        <label class="flex min-h-11 items-start gap-3 rounded-lg border border-[var(--border)] p-3">
          <input type="checkbox" class="mt-1" checked={selected.includes(candidate.id)} disabled={running || saving}
            onchange={e => toggle(candidate.id, e.currentTarget.checked)} />
          <span class="min-w-0"><span class="block text-sm font-medium">{candidate.name}</span>
            <span class="block break-all text-xs text-[var(--text-secondary)]">{candidate.path}</span></span>
        </label>
      {/each}
    </div>
  {/if}
  {#if running}<p class="text-sm">入口正在运行。请先手动停止个人版入口再修改成员；保存不会自动重启服务。</p>{/if}
  {#if selected.length > 31}<p role="alert" class="text-sm">一个入口最多包含自身及另外 31 个仓库。</p>{/if}
  {#if error}<p role="alert" class="break-words text-sm">{error}</p>{/if}
  <div class="flex flex-wrap items-center gap-3">
    <button type="button" class="tx-btn-primary" disabled={running || saving || selected.length > 31} onclick={save}>
      {saving ? "保存中…" : "保存共享入口"}
    </button>
    <span class="text-xs text-[var(--text-secondary)]">不勾选其他仓库时，保持单仓库模式。</span>
  </div>
</section>
