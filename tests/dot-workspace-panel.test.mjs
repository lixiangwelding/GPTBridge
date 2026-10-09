import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { compile } from "svelte/compiler";

const source = readFileSync("src/lib/dot/model.ts", "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { connectionRows, policySummary, workspaceTasks, belongsToTask, usagePrompt } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const profile = { id: "workspace-a", runtime: { permission_mode: "trusted", tool_profile: "full", gateway_workspace_ids: [] } };

test("TCP reachability never attests authentication, client catalog or dot routing", () => {
  const rows = connectionRows({ workspace_id: "workspace-a", mcp_reachable: true }, profile);
  assert.equal(rows[0].value, "端口可达");
  assert.equal(rows[2].value, "未验证");
  assert.equal(rows[3].value, "未验证");
  assert.equal(connectionRows(null, profile)[0].value, "尚未观察");
  assert.equal(connectionRows({ workspace_id: "workspace-b", mcp_reachable: true }, profile)[0].value, "尚未观察");
  assert.equal(connectionRows({ workspace_id: "workspace-a", mcp_reachable: false }, profile)[0].tone, "warning");
});

test("saved read configuration preserves single-workspace legacy scope and shared strict scope", () => {
  assert.match(policySummary(profile).configuredReadScope, /旧读取策略可读外部路径/);
  const shared = { ...profile, runtime: { ...profile.runtime, gateway_workspace_ids: ["workspace-b"], tool_profile: "read-only" } };
  assert.match(policySummary(shared).configuredReadScope, /根目录内读取/);
  assert.match(policySummary(shared).permission, /只读工具/);
});

test("task receipt and list remain bound to the selected workspace and original task", () => {
  const row = { task_id: "task-a", workspace_id: "workspace-a" };
  const detail = { task_id: "task-a", workspace_id: "workspace-a" };
  assert.equal(belongsToTask(detail, row, "workspace-a"), true);
  assert.equal(belongsToTask({ ...detail, task_id: "task-b" }, row, "workspace-a"), false);
  assert.equal(belongsToTask({ ...detail, workspace_id: "workspace-b" }, row, "workspace-a"), false);
  assert.equal(belongsToTask(detail, row, "workspace-b"), false);
  assert.equal(belongsToTask(null, row, "workspace-a"), false);
  assert.deepEqual(workspaceTasks([row, { ...row, workspace_id: "workspace-b" }], "workspace-a"), [row]);
});

test("usage prompt preserves native durable lifecycle and unknown-result recovery", () => {
  const prompt = usagePrompt(profile);
  for (const part of ["workspace-a", "expected_hashes", "request_id", "task_id", "task_open", "task_checkpoint", "policy_only", "不盲目重放", "无需外部 Agent"]) assert.ok(prompt.includes(part));
});

test("new panel compiles with Svelte and has no accessibility warnings", () => {
  const file = "src/lib/components/DotWorkspacePanel.svelte";
  const result = compile(readFileSync(file, "utf8"), { filename: file, generate: "client" });
  assert.ok(result.js.code.length > 0);
  assert.deepEqual(result.warnings.filter(warning => warning.code.startsWith("a11y_")), []);
});
