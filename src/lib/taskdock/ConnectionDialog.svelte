<script lang="ts">
  import { workspaces } from "$lib/stores/app";
  import { mcpLocalEndpoint, actionsLocalEndpoint } from "$lib/types";
  import { showToast } from "$lib/stores/toast";
  import { connectionProject, addProjectOpen } from "./state";
  import { getConnections, startService, copyText } from "./api";
  import { timestamp } from "./format";
  import type { Connections } from "./types";
  import Dialog from "./Dialog.svelte";
  let data = $state<Connections | null>(null), error = $state(""), busy = $state(false), starting = $state("");
  let generation = 0;
  const selected = $derived($workspaces.find(p => p.id === $connectionProject));
  $effect(() => { const id = $connectionProject; data = null; error = ""; generation++; if (id) void refresh(id); });
  async function refresh(id = $connectionProject) {
    if (!id) return; const token = ++generation; busy = true;
    try { const result = await getConnections(id); if (token === generation && $connectionProject === id) {data = result; error = "";} }
    catch (e) { if (token === generation) error = String(e); }
    finally { if (token === generation) busy = false; }
  }
  async function start(service: "mcp" | "actions") {
    const id = $connectionProject; if (!id || starting) return; starting = service;
    try { const result = await startService(id, service); if (result.state !== "running") throw new Error(result.localMessage || "服务尚未启动，请查看诊断。"); await refresh(id); showToast(`${service === "mcp" ? "MCP" : "Actions"} 已由当前实例启动。`); }
    catch (e) { if ($connectionProject === id) error = String(e); }
    finally { starting = ""; }
  }
  async function copy(text: string) { try {await copyText(text);showToast("地址已复制；复制不代表客户端已连接。");}catch(e){error=String(e);} }
</script>
<Dialog open={$connectionProject !== null} title="连接你的 AI 工具" onClose={() => connectionProject.set(null)}>
  {#if !$workspaces.length}<div class="td-empty"><h2>先选择项目</h2><p>连接地址与权限跟随已保存的项目。</p><button class="td-button" onclick={() => {connectionProject.set(null);addProjectOpen.set(true);}}>添加项目</button></div>
  {:else}
    <div class="td-field"><label for="td-connect-project">项目</label><select id="td-connect-project" value={$connectionProject || ""} onchange={event => connectionProject.set(event.currentTarget.value)}><option value="" disabled>选择项目</option>{#each $workspaces as p}<option value={p.id}>{p.name}</option>{/each}</select></div>
    <p>优先使用 MCP 原生连接；Actions 是兼容入口。检测只访问本地端口，不会接管其他运行实例。</p>
    {#if busy}<p role="status"><span class="td-loading-dot"></span>读取服务状态并检查端口…</p>{/if}
    {#if error}<div class="td-error" role="alert">{error}</div>{/if}
    {#if data && selected}
      {#each ["mcp", "actions"] as raw}
        {@const service = raw as "mcp" | "actions"}
        {@const status = data[service]}
        {@const reachable = service === "mcp" ? data.mcp_reachable : data.actions_reachable}
        {@const endpoint = status.publicEndpoint || status.localEndpoint || (service === "mcp" ? mcpLocalEndpoint(selected.runtime.local_port) : actionsLocalEndpoint(selected.actions?.local_port || 8787))}
        <section class="td-connect-row"><h3>{service === "mcp" ? "MCP 原生连接" : "Actions 兼容连接"} {#if service === "mcp"}<span class="td-pill">推荐</span>{/if}</h3>
          <p><strong>{status.state === "running" ? "当前实例运行中" : reachable ? "端口被占用 · 服务身份未核验" : status.state === "error" ? "服务异常" : "本地端口未连接"}</strong></p>
          <p>{status.localMessage || "暂无运行消息"}</p><div class="td-code">{endpoint}</div>
          <div class="td-actions"><button class="td-button small" onclick={() => copy(endpoint)}>复制连接地址</button><button class="td-button small primary" disabled={busy || !!starting || reachable || status.state === "running" || status.state === "starting"} onclick={() => start(service)}>{starting === service ? "启动中…" : "启动已配置服务"}</button></div>
        </section>
      {/each}
      <div class="td-note">客户端握手状态：未核验。端口可达不等于已经完成登录或工具发现。启动会使用当前项目保存的认证、上游和隧道设置，不重启其他服务。</div>
      <p class="td-mono td-muted">检查时间 {timestamp(data.checked_at)}</p>
      <div class="td-actions"><a href={`/workspace/${encodeURIComponent(selected.id)}`} class="td-button small primary" onclick={() => connectionProject.set(null)}>打开 MCP 连接配置</a></div>
      <details class="td-details"><summary>连接说明</summary><p>原工作区页面可配置端口、认证、MCP/Actions 与隧道；旧连接和任务数据继续兼容。</p></details>
    {/if}
  {/if}
  {#snippet footer()}<button class="td-button" disabled={busy || !$connectionProject} onclick={() => refresh()}>重新检测</button><button class="td-button primary" onclick={() => connectionProject.set(null)}>完成</button>{/snippet}
</Dialog>
