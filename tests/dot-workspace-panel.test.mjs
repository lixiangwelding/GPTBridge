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
  assert.equal(rows[2].value, "尚未检查");
  assert.equal(rows[3].value, "尚未检查");
  assert.equal(rows[5].value, "本地服务无法确认");
  assert.equal(rows[6].value, "本地服务无法确认");
  assert.equal(connectionRows(null, profile)[0].value, "尚未观察");
  assert.equal(connectionRows({ workspace_id: "workspace-b", mcp_reachable: true }, profile)[0].value, "尚未观察");
  assert.equal(connectionRows({ workspace_id: "workspace-a", mcp_reachable: false }, profile)[0].tone, "warning");
});

const probe = (id = "workspace-a") => ({ workspace_id: id, auth: { status: "passed" }, handshake: { status: "passed" },
  catalog: { status: "passed", count: 8, sha256: "catalog" }, runtime_policy: { status: "passed", permission_mode: "safe", tool_profile: "read-only", direct_workspace: { security: { read_scope: "workspace_root", execution_isolation: "policy_only", sandbox_enforced: false } } } });
test("protocol evidence is workspace-bound and never attests a client cache or dot route", () => {
  const rows = connectionRows(null, profile, probe());
  assert.equal(rows[0].value, "尚未观察");
  assert.equal(rows[2].value, "已验证");
  assert.equal(rows[3].value, "已验证");
  assert.equal(rows[4].value, "8 个工具 · 已读取");
  assert.equal(rows[5].value, "本地服务无法确认");
  assert.equal(rows[6].value, "本地服务无法确认");
  assert.equal(connectionRows(null, profile, probe("workspace-b"))[2].value, "尚未检查");
  const failed = { ...probe(), auth: { status: "failed" }, handshake: { status: "unavailable" } };
  assert.equal(connectionRows(null, profile, failed)[2].value, "检查失败");
  assert.equal(connectionRows(null, profile, failed)[3].value, "无法检查");
});

test("saved configuration and observed runtime policy remain separate and failed evidence is unavailable", () => {
  const policy = policySummary(profile, probe());
  assert.equal(policy.permission, "trusted");
  assert.equal(policy.runtime.permission, "safe");
  assert.equal(policy.runtime.sandbox, false);
  assert.equal(policy.runtime.readScope, "workspace_root");
  assert.equal(policySummary(profile, probe("workspace-b")).runtime, null);
  assert.equal(policySummary(profile, { ...probe(), runtime_policy: { status: "failed" } }).runtime, null);
  assert.equal(policySummary(profile, null).runtime, null);
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

test("panel and output dialog compile with Svelte and have no accessibility warnings", () => {
  for (const file of ["src/lib/components/DotWorkspacePanel.svelte", "src/lib/taskdock/JobLogDialog.svelte"]) {
    const result = compile(readFileSync(file, "utf8"), { filename: file, generate: "client" });
    assert.ok(result.js.code.length > 0);
    assert.deepEqual(result.warnings.filter(warning => warning.code.startsWith("a11y_")), []);
  }
});
