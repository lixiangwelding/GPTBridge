<script lang="ts">
  import { labels } from "./format";
  import type { TaskState } from "./types";
  let { state }: { state: TaskState } = $props();
</script>
<span class="td-state {state}"><span aria-hidden="true">●</span>{labels[state] ?? state}</span>
