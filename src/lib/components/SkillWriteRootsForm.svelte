<script lang="ts">
  import type { SkillWriteRootConfig } from "$lib/types";
  import { confirm, open } from "@tauri-apps/plugin-dialog";

  let { roots = [], running = false, onSave }: {
    roots?: SkillWriteRootConfig[];
    running?: boolean;
    onSave: (roots: SkillWriteRootConfig[]) => Promise<void>;
  } = $props();

  let draft = $state<SkillWriteRootConfig[]>([]);
  let saving = $state(false);
  let error = $state("");

  $effect(() => {
    draft = roots.map((root) => ({ ...root }));
  });

  function labelFor(path: string) {
    const normalized = path.replaceAll("\\", "/").replace(/\/$/, "");
    return normalized.split("/").at(-1) || "Skills";
  }

  async function addRoot() {
    error = "";
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    if (draft.some((root) => root.path === selected)) {
      error = "这个目录已经在待保存列表中。";
      return;
    }
    draft = [
      ...draft,
      {
        id: crypto.randomUUID().replaceAll("-", ""),
        name: labelFor(selected),
        path: selected,
      },
    ];
  }

  function removeRoot(id: string) {
    draft = draft.filter((root) => root.id !== id);
  }

  async function save() {
    if (running || saving) return;
    error = "";
    const approved = await confirm(
      draft.length
        ? "保存后，当前工作区的 MCP 可通过专用补丁工具修改这些目录内的文件。不会授予脚本执行、网络或认证配置权限。确认保存？"
        : "这会撤销当前工作区的全部 Skill 目录写入授权。确认保存？",
      { title: "确认 Skill 写入授权", kind: "warning" },
    );
    if (!approved) return;
    saving = true;
    try {
      await onSave(draft.map((root) => ({ ...root })));
    } catch (cause) {
      error = String(cause);
    } finally {
      saving = false;
    }
  }
</script>

<section class="grid gap-3" aria-label="Skill 写入目录授权">
  <p class="tx-section-label">Skill 写入目录</p>
  <p class="text-sm text-[var(--text-secondary)]">
    只允许专用补丁工具修改下列真实目录内的相对文件；Skill 扫描结果不会自动获得写权限。
  </p>
  <p class="text-sm text-[var(--text-secondary)]">
    此授权不包含脚本执行、命令、网络、安装或认证配置。后端保存时会核对真实路径、重复目录和路径逃逸。
  </p>

  {#if draft.length === 0}
    <p class="text-sm">尚未授权任何 Skill 写入目录。</p>
  {:else}
    <div class="grid gap-2">
      {#each draft as root (root.id)}
        <div class="grid gap-2 rounded-lg border border-[var(--border)] p-3 sm:grid-cols-[minmax(10rem,0.35fr)_minmax(0,1fr)_auto] sm:items-center">
          <label class="grid gap-1 text-xs text-[var(--text-secondary)]">
            显示名称
            <input
              class="tx-input"
              bind:value={root.name}
              maxlength="80"
              disabled={running || saving}
              aria-label={`Skill 写入目录名称 ${root.path}`}
            />
          </label>
          <div class="min-w-0">
            <p class="break-all text-sm">{root.path}</p>
            <p class="break-all text-xs text-[var(--text-secondary)]">ID · {root.id}</p>
          </div>
          <button
            type="button"
            class="tx-btn-secondary"
            disabled={running || saving}
            onclick={() => removeRoot(root.id)}
          >撤销</button>
        </div>
      {/each}
    </div>
  {/if}

  {#if running}
    <p class="text-sm">MCP 入口正在运行。请先手动停止入口，再修改目录授权；保存不会自动重启服务。</p>
  {/if}
  {#if error}<p role="alert" class="break-words text-sm">{error}</p>{/if}

  <div class="flex flex-wrap items-center gap-3">
    <button type="button" class="tx-btn-secondary" disabled={running || saving} onclick={addRoot}>
      添加目录
    </button>
    <button type="button" class="tx-btn-primary" disabled={running || saving} onclick={save}>
      {saving ? "保存中…" : "保存授权"}
    </button>
    <span class="text-xs text-[var(--text-secondary)]">目录选择只发生在本机桌面设置中。</span>
  </div>
</section>
