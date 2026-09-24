<script lang="ts">
  import CopyButton from "$lib/components/CopyButton.svelte";
  import StatusOrb from "$lib/components/StatusOrb.svelte";
  import type { RuntimeState } from "$lib/types";

  interface Props {
    title: string;
    subtitle: string;
    status: RuntimeState;
    statusMessage?: string;
    port: number;
    portEditable?: boolean;
    busy?: boolean;
    tunnelType?: string;
    localEndpoint: string;
    publicEndpoint?: string;
    publicLabel?: string;
    onToggle: () => void | Promise<void>;
    onPortChange?: (port: number) => void | Promise<void>;
  }

  let {
    title,
    subtitle,
    status,
    statusMessage = "",
    port,
    portEditable = false,
    busy = false,
    tunnelType = "none",
    localEndpoint,
    publicEndpoint = "",
    publicLabel = "公网",
    onToggle,
    onPortChange,
  }: Props = $props();

  let draftPort = $state(0);

  $effect(() => {
    draftPort = port;
  });

  const managed = $derived(status === "external");
  const running = $derived(status === "running" || managed);
  const showError = $derived(status === "error" && Boolean(statusMessage));
  const canEditPort = $derived(portEditable && !running && status !== "starting");
  const tunnelEnabled = $derived(tunnelType === "cloudflare" || tunnelType === "frp");
  const tunnelLabel = $derived(
    tunnelType === "cloudflare" ? "Cloudflare" : tunnelType === "frp" ? "FRP" : "",
  );

  async function commitPort() {
    if (!onPortChange || draftPort === port) return;
    if (draftPort < 1024 || draftPort > 65535) {
      draftPort = port;
      return;
    }
    await onPortChange(draftPort);
  }
</script>

<article class="tx-card p-5">
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <StatusOrb state={status} />
        <h3 class="text-[15px] font-semibold tracking-tight">{title}</h3>
      </div>
      <p class="mt-1 text-sm text-[var(--color-text-muted)]">{subtitle}</p>
      {#if managed}
        <p class="mt-1 text-xs text-[var(--color-text-muted)]">后台服务独立运行，本页不控制其启停</p>
      {:else if tunnelEnabled}
        <p class="mt-1 text-xs text-[var(--color-text-muted)]">
          {tunnelLabel} 隧道随服务自动连接，停止服务时一并断开
        </p>
      {/if}
    </div>
    {#if managed}
      <span class="inline-flex shrink-0 items-center rounded-[10px] border border-[var(--color-success)] px-3 py-2 text-[13px] font-semibold text-[var(--color-success)]" role="status">后台运行</span>
    {:else}
      <button
        type="button"
        class="tx-btn-primary shrink-0"
        class:tx-btn-danger={running}
        disabled={busy || status === "starting" || status === "stopping"}
        onclick={onToggle}
      >
        {#if busy}
          处理中…
        {:else if running}
          停止
        {:else}
          启动
        {/if}
      </button>
    {/if}
  </div>

  {#if showError}
    <div class="tx-alert tx-alert--error mt-4" role="alert">
      {statusMessage}
    </div>
  {/if}
  {#if managed && statusMessage}
    <p class="mt-3 text-sm text-[var(--color-text-muted)]">{statusMessage}</p>
  {/if}

  <div class="mt-5 grid gap-3">
    <div class="tx-info-block">
      <div class="tx-info-row">
        <span class="tx-info-label">端口</span>
        {#if canEditPort}
          <input
            type="number"
            min="1024"
            max="65535"
            class="tx-input tx-input-inline"
            bind:value={draftPort}
            onchange={commitPort}
          />
        {:else}
          <span class="tx-mono text-sm">{port}</span>
        {/if}
      </div>
    </div>

    <div class="tx-info-block">
      <div class="tx-info-row">
        <span class="tx-info-label">本地地址</span>
        <CopyButton value={localEndpoint} />
      </div>
      <p class="tx-mono mt-1.5 truncate text-sm">{localEndpoint}</p>
    </div>

    {#if publicEndpoint || publicLabel}
      <div class="tx-info-block">
        <div class="tx-info-row">
          <span class="tx-info-label">{publicLabel}</span>
          {#if publicEndpoint}
            <CopyButton value={publicEndpoint} />
          {/if}
        </div>
        <p class="tx-mono mt-1.5 truncate text-sm text-[var(--color-text-secondary)]">
          {publicEndpoint || "未配置隧道"}
        </p>
      </div>
    {/if}
  </div>
</article>
