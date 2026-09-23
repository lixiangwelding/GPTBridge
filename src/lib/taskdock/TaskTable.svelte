<script lang="ts">
  import StateBadge from "./StateBadge.svelte";
  import { taskKey, stage, relativeTime, timestamp } from "./format";
  import type { TaskRow } from "./types";
  let { tasks, selected, onSelect }: { tasks: TaskRow[]; selected: string | null; onSelect: (task: TaskRow) => void } = $props();
</script>
<table class="td-table"><thead><tr><th scope="col">任务 / 编号</th><th scope="col">项目</th><th scope="col">状态</th><th scope="col">当前步骤</th><th scope="col">更新</th></tr></thead><tbody>
  {#each tasks as task (taskKey(task))}
    <tr class="td-task-row" class:selected={selected === taskKey(task)}>
      <td><button class="td-task-title" title={task.goal} aria-pressed={selected === taskKey(task)} onclick={() => onSelect(task)}>{task.goal}</button><div class="td-task-sub">{task.task_id.slice(0,8)} · {task.workspace_name}</div></td>
      <td class="td-ellipsis" title={task.workspace_name}>{task.workspace_name}</td><td><StateBadge state={task.display_state}/></td><td class="td-ellipsis" title={stage(task)}>{stage(task)}</td><td class="td-mono td-muted" title={timestamp(task.updated)}>{relativeTime(task.updated)}</td>
    </tr>
  {/each}
</tbody></table>
