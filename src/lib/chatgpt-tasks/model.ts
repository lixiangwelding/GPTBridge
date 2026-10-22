export type TaskStatus = 'draft' | 'sent' | 'unknown' | 'done' | 'blocked';
export interface ChatTask {
  id: string; title: string; version: string; goal: string; inputs: string[];
  steps: string[]; acceptance: string[]; dependsOn: string[];
  status: TaskStatus; conversationUrl: string; receipt: string; customPrompt: string;
}
export interface TaskPlan {
  schemaVersion: 1; title: string; workspace: string; connector: 'GPTBridge Windows';
  requestedModel: string; requestedEffort: string; tasks: ChatTask[];
}
export const STORAGE_KEY = 'gptbridge:chatgpt-task-workbench:v1';
const statuses = ['draft','sent','unknown','done','blocked'];
export function validatePlan(value: unknown): TaskPlan {
  if (!value || typeof value !== 'object') throw new Error('任务包必须是 JSON 对象。');
  const p = value as TaskPlan;
  if (p.schemaVersion !== 1 || p.connector !== 'GPTBridge Windows') throw new Error('只接受版本 1 的 GPTBridge Windows 任务包。');
  if (![p.title,p.workspace,p.requestedModel,p.requestedEffort].every(s => typeof s === 'string' && s.trim())) throw new Error('任务包缺少名称、工作目录或模型偏好。');
  if (!Array.isArray(p.tasks) || !p.tasks.length || p.tasks.length > 200) throw new Error('任务包需要包含 1–200 个任务。');
  const ids = new Set<string>();
  for (const t of p.tasks) {
    if (!t || typeof t !== 'object' || !/^[a-zA-Z0-9_-]{1,64}$/.test(t.id) || ids.has(t.id)) throw new Error('任务编号无效或重复。');
    ids.add(t.id);
    if (![t.title,t.version,t.goal,t.conversationUrl,t.receipt,t.customPrompt].every(s => typeof s === 'string')) throw new Error(`任务 ${t.id} 文本字段不完整。`);
    if (!t.title.trim() || !t.goal.trim() || !statuses.includes(t.status)) throw new Error(`任务 ${t.id} 名称、目标或状态无效。`);
    for (const a of [t.inputs,t.steps,t.acceptance,t.dependsOn]) if (!Array.isArray(a) || !a.every(s=>typeof s==='string')) throw new Error(`任务 ${t.id} 列表无效。`);
    if (t.status === 'done' && !t.receipt.trim()) throw new Error(`任务 ${t.id} 完成状态缺少验收回执。`);
    if (t.conversationUrl && !isConversationUrl(t.conversationUrl)) throw new Error(`任务 ${t.id} 的 ChatGPT 链接无效。`);
  }
  const visited = new Set<string>(), visiting = new Set<string>();
  const byId = new Map(p.tasks.map(t=>[t.id,t]));
  function visit(id: string) {
    if (visited.has(id)) return;
    if (visiting.has(id)) throw new Error('任务依赖存在循环。');
    visiting.add(id);
    for (const dep of byId.get(id)!.dependsOn) {
      if (!ids.has(dep)) throw new Error(`任务 ${id} 依赖不存在的任务 ${dep}。`);
      visit(dep);
    }
    visiting.delete(id);visited.add(id);
  }
  p.tasks.forEach(t=>visit(t.id));
  // Svelte $state values are proxies, which structuredClone rejects. Project the
  // validated JSON contract into plain values so persistence and transitions work.
  const out: TaskPlan = {
    schemaVersion: 1, title: p.title, workspace: p.workspace, connector: p.connector,
    requestedModel: p.requestedModel, requestedEffort: p.requestedEffort,
    tasks: p.tasks.map(t => ({
      id: t.id, title: t.title, version: t.version, goal: t.goal,
      inputs: [...t.inputs], steps: [...t.steps], acceptance: [...t.acceptance], dependsOn: [...t.dependsOn],
      status: t.status, conversationUrl: t.conversationUrl, receipt: t.receipt, customPrompt: t.customPrompt,
    })),
  };
  // Migrate the original candidate's swapped Pro/effort defaults; task identities and states stay intact.
  if(out.requestedModel==='Pro' && out.requestedEffort==='深入思考'){out.requestedModel='GPT-6';out.requestedEffort='Pro';}
  return out;
}
export function isConversationUrl(value: string): boolean {
  try { const u = new URL(value); return u.origin === 'https://chatgpt.com' && /^\/c\/[a-zA-Z0-9-]+$/.test(u.pathname) && !u.username && !u.password && !u.search && !u.hash; } catch { return false; }
}
export function waitingFor(plan: TaskPlan, task: ChatTask): string[] {
  return task.dependsOn.filter(id=>plan.tasks.find(t=>t.id===id)?.status !== 'done');
}
export function nextReady(plan: TaskPlan): ChatTask | undefined {
  return plan.tasks.find(t=>t.status==='draft' && waitingFor(plan,t).length===0);
}
export function transition(plan: TaskPlan, id: string, status: TaskStatus, receipt = '', conversationUrl = ''): TaskPlan {
  const out = validatePlan(plan), task = out.tasks.find(t=>t.id===id);
  if (!task) throw new Error('任务不存在。');
  if (status === 'sent' && (task.status !== 'draft' || waitingFor(out,task).length)) throw new Error('只有依赖完成的草稿可以登记为已发送。');
  if (task.status === 'unknown' && status === 'draft') throw new Error('结果未知时保留原任务；先查询原对话，不能直接重发。');
  if (status === 'done' && !receipt.trim()) throw new Error('请填写可核对的验收结果，不能只凭发送成功标记完成。');
  if (status === 'sent' && !isConversationUrl(conversationUrl)) throw new Error('请先填写实际 ChatGPT 对话链接。');
  if (conversationUrl && !isConversationUrl(conversationUrl)) throw new Error('只接受 https://chatgpt.com/c/… 对话链接。');
  if (task.status === 'done' && status !== 'done' && out.tasks.some(t=>t.dependsOn.includes(id) && t.status!=='draft')) throw new Error('已有后继任务执行，不能直接撤销此依赖。');
  task.status=status;task.receipt=receipt.trim();task.conversationUrl=conversationUrl;
  return validatePlan(out);
}
export function promptFor(plan: TaskPlan, task: ChatTask): string {
  if (task.customPrompt.trim()) return task.customPrompt;
  return `我现在直接在 ChatGPT 聊天中明确选择并要求使用 @GPTBridge Windows，仅操作 Windows / WSL 工作区，Mac 连接不要调用或修改。\n\n任务 ${task.id}：${task.title}\n版本：${task.version}\n项目：${plan.workspace}\n目标：${task.goal}\n\n先核实际工作区，再读取项目 AGENTS、适用 Skill、同版知识入口与 current 来源；读取下列正式交接文件并核清单及成员 SHA。不要把 thread:// 链接当作你已获得的聊天正文。若实际执行宿主是 Codex，立即停止插件调用，使用原生工具；不得嵌套 Codex 转调 GPTBridge。\n\n输入：\n${task.inputs.map(x=>'- '+x).join('\n')}\n\n执行：\n${task.steps.map((x,i)=>`${i+1}. ${x}`).join('\n')}\n\n验收：\n${task.acceptance.map(x=>'- '+x).join('\n')}\n\n依赖：${task.dependsOn.join('、') || '无'}。依赖未通过时只完成独立准备，保留缺口。保留其他聊天改动，不接管活动进程；修复资料沿用 ddt-repairs/<版本>/ 原主题，业务知识写同版正式正文。只在独占本地开发环境验证，不连接生产、不部署、不改 Mac、不 commit/push；私密配置只在受管私密位置读取，不回显或导出。Maven 使用 flock /tmp/cv-mvn.lock，前端构建和测试使用 flock /tmp/cv-fe.lock。记录源码/制品/运行身份、实际测试和未验项；UNKNOWN 保留原 requestId，先查询，不重发。结尾给修改路径、验证回执、遗留问题与下一任务的具体输入。不要将组件通过、旧验收或目录存在写成逐关通关和奖励到账。`;
}
