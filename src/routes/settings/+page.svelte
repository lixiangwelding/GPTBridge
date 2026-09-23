<script lang="ts">
  import { ArrowUpRight } from "@lucide/svelte";
  import { workspaces } from "$lib/stores/app";
  import { appInfo, compact, setCompact, connectionProject } from "$lib/taskdock/state";
</script>
<svelte:head><title>设置 · GPTBridge</title></svelte:head>
<div class="td-page-head"><div><h1>设置</h1><p>常用设置留在这里。高级参数按需展开，不干扰日常任务。</p></div></div>
<div class="td-setting-grid"><section class="td-panel">
  <div class="td-setting-row"><div><h2>MCP 连接配置</h2><p>直接打开原工作区页面，配置公网隧道、认证、端口、权限和本地 MCP。</p></div><div class="td-actions"><a class="td-button small primary" href={$workspaces[0] ? `/workspace/${encodeURIComponent($workspaces[0].id)}` : "/projects"}>打开配置</a><button class="td-button small" onclick={() => connectionProject.set($workspaces[0]?.id || "")}>查看连接状态</button></div></div>
  <div class="td-setting-row"><div><h2>紧凑任务列表</h2><p>缩小行间留白，保留任务状态与必要操作。</p></div><label class="td-switch"><input type="checkbox" checked={$compact} onchange={event=>setCompact(event.currentTarget.checked)}/>紧凑模式</label></div>
  <div class="td-setting-row"><div><h2>权限与任务边界</h2><p>项目权限由原服务端执行。这里不提供“全部允许”或任意命令重放。</p></div><span class="td-pill">始终保留</span></div>
  <div class="td-setting-row"><div><h2>应用版本</h2><p>{$appInfo ? `${$appInfo.name} ${$appInfo.version} · 原生后端已响应` : "原生后端未连接，不能据前端页面推断运行版本。"}</p></div><span class="td-mono">{$appInfo?.version || "UNKNOWN"}</span></div>
  <details class="td-details"><summary>项目高级配置：MCP / Actions、端口、上游与隧道</summary><p>保留原配置功能。修改上游或认证前确认运行任务；不因访问设置而自动重启服务。</p><div class="td-actions">{#each $workspaces as project}<a class="td-button small" href={`/workspace/${encodeURIComponent(project.id)}`}>{project.name}<ArrowUpRight size={12}/></a>{:else}<a class="td-button small" href="/projects">先添加项目</a>{/each}</div></details>
  <details class="td-details"><summary>全局设置：通用、密钥、FRP 与工具安装</summary><p>既有数据和覆盖项保持兼容，不自动重建配置。</p><div class="td-actions"><a class="td-button small" href="/settings/general">通用与代理</a><a class="td-button small" href="/settings/keys">共享密钥</a><a class="td-button small" href="/settings/frp">FRP 配置</a><a class="td-button small" href="/settings/software">软件管理</a></div></details>
  <details class="td-details"><summary>诊断与完整审计</summary><p>工作台详情提供任务自己的作业输出。需要跨请求、访问日志或审计设置时进入完整日志页。</p><a class="td-button small" href="/settings/audit">打开运行与访问日志<ArrowUpRight size={12}/></a></details>
  <details class="td-details"><summary>兼容说明：旧连接和任务数据</summary><p>GPTBridge 是新的产品名。稳定的应用标识、协议工具名、配置目录、命令行入口和任务数据库保留兼容，避免改名后丢配置、重复创建数据库或中断旧连接。不会因为打开新版页面而替换正在运行的服务。</p></details>
</section><aside class="td-panel td-help"><div class="td-eyebrow">SET ONCE. GET BACK TO WORK.</div><h2 style="margin-top:12px">少操作，不少边界。</h2><p>任务入口负责目标与进展，AI 客户端负责继续执行。创建目标不是生成代码，命令完成也不是业务验收通过。</p><p>配置收起来，能力留下来。只有项目真的不一样时，才进入高级配置修改。</p><span class="td-pill">GPTBridge · AI 任务工作台</span></aside></div>
