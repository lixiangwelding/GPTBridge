<script lang="ts">
  import { ChevronDown, History } from "@lucide/svelte";
  let expanded = $state(false);
</script>

<section class="rounded-[12px] border border-[var(--color-border)] bg-[var(--card-bg)] px-3 py-2.5 sm:px-4" aria-labelledby="task-workflow-title">
  <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between sm:gap-4">
    <div class="flex min-w-0 items-center gap-3">
      <span class="flex size-9 shrink-0 items-center justify-center rounded-[10px] bg-[var(--primary-soft)] text-[var(--primary)]" aria-hidden="true"><History size={16} /></span>
      <div class="min-w-0">
        <h3 id="task-workflow-title" class="text-sm font-semibold text-[var(--color-text)]">直接描述任务，无需复制初始化提示词</h3>
        <p class="mt-0.5 text-xs leading-5 text-[var(--color-text-muted)]">多个任务共用项目目录；按步骤保存进度，用任务编号继续。</p>
      </div>
    </div>
    <button type="button" class="tx-btn-ghost min-h-11 shrink-0 gap-1.5 px-3 py-2 text-xs" aria-expanded={expanded} aria-controls="task-workflow-help" onclick={() => (expanded = !expanded)}>
      <span>{expanded ? "收起说明" : "恢复与兼容说明"}</span>
      <ChevronDown size={14} class={`transition-transform ${expanded ? "rotate-180" : ""}`} aria-hidden="true" />
    </button>
  </div>
  {#if expanded}
    <div id="task-workflow-help" class="mt-3 border-t border-[var(--color-border)] pt-3 text-xs leading-6 text-[var(--color-text-secondary)]">
      <p>例如直接说“修复登录问题”；继续时说“继续任务”并带上上次返回的 task_id。工具会读取已完成步骤和下一步，不还原整份旧代码。</p>
      <p>运行中的命令使用原 job_id 查询，断线不重复执行。旧 history_session 档案仍可检索；只有明确传给工具的文字会被保存，不会自动读取未传入的聊天。</p>
      <p>本地技能无需复制：发送“$技能名 + 任务”后，模型通过插件搜索并读取现有 SKILL.md，返回来源位置。脚本不会自动执行；这不是输入框原生 $ 下拉，ChatGPT 原生技能选择使用 @。</p>
      <p>首次连接个人版需刷新工具列表，让客户端读取新的 task_open / task_status / task_checkpoint 说明；旧连接器和正在运行的服务不受影响。</p>
    </div>
  {/if}
</section>
