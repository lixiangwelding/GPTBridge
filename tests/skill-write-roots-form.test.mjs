import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { compile } from "svelte/compiler";

const componentFile = "src/lib/components/SkillWriteRootsForm.svelte";
const component = fs.readFileSync(componentFile, "utf8");

test("Skill write-root form compiles with the real Svelte compiler", () => {
  const result = compile(component, { filename: componentFile, generate: "client" });
  assert.ok(result.js.code.length > 0);
  assert.deepEqual(result.warnings.filter((warning) => warning.code.startsWith("a11y_")), []);
});

test("authorization stays local, explicit, and disabled while MCP is running", () => {
  assert.ok(component.includes('open({ directory: true, multiple: false })'));
  assert.ok(component.includes("确认 Skill 写入授权"));
  assert.ok(component.includes("disabled={running || saving}"));
  assert.ok(component.includes("await onSave("));
  assert.ok(component.includes("不会自动重启服务"));
  assert.ok(!component.includes("startRuntime("));
  assert.ok(!component.includes("restartRuntime("));
  assert.ok(!component.includes("exec_command"));
});

test("workspace page persists only the selected roots and never restarts the service", () => {
  const page = fs.readFileSync("src/routes/workspace/[id]/+page.svelte", "utf8");
  const start = page.indexOf("async function saveSkillWriteRoots");
  const end = page.indexOf("async function saveUpstreamMcps", start);
  const body = page.slice(start, end);
  assert.ok(start >= 0 && end > start);
  assert.ok(body.includes("skill_write_roots: roots"));
  assert.ok(body.includes("await updateWorkspace(next)"));
  assert.ok(body.includes("await load()"));
  assert.ok(!body.includes("restartRuntime("));
  assert.ok(!body.includes("startRuntime("));
  assert.ok(page.includes("<SkillWriteRootsForm"));
  assert.ok(page.includes('running={mcpStatus === "running"}'));
});

test("TypeScript runtime contract carries the optional authorization list", () => {
  const types = fs.readFileSync("src/lib/types.ts", "utf8");
  assert.ok(types.includes("skill_write_roots?: SkillWriteRootConfig[]"));
  assert.ok(types.includes("export interface SkillWriteRootConfig"));
});
