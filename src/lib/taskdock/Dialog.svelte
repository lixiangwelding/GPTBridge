<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  let { open, title, onClose, children, footer, wide = false }: { open: boolean; title: string; onClose: () => void; children: Snippet; footer?: Snippet; wide?: boolean } = $props();
  let dialog: HTMLDialogElement;
  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>
<dialog bind:this={dialog} class:td-wide={wide} class="td-dialog" aria-label={title} onclose={onClose} oncancel={onClose}>
  <header class="td-dialog-head"><h2>{title}</h2><button type="button" class="td-icon-button" aria-label="关闭对话框" onclick={onClose}><X size={17} /></button></header>
  <div class="td-dialog-body">{@render children()}</div>
  {#if footer}<footer class="td-dialog-foot">{@render footer()}</footer>{/if}
</dialog>
