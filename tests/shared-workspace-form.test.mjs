import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { compile } from "svelte/compiler";

const file="src/lib/components/SharedWorkspaceForm.svelte";
const source=fs.readFileSync(file,"utf8");

test("shared-repository form compiles through the real Svelte compiler",()=>{
  const result=compile(source,{filename:file,generate:"client"});
  assert.ok(result.js.code.length>0);
  assert.deepEqual(result.warnings.filter(x=>x.code.startsWith("a11y_")),[]);
});

test("form requires explicit membership and disables edits for a running entry",()=>{
  assert.ok(source.includes("selected.includes(candidate.id)"));
  assert.ok(source.includes("disabled={running || saving}"));
  assert.ok(source.includes("onSave([...selected])"));
  assert.ok(!source.includes("startRuntime("));
  assert.ok(!source.includes("restartTunnel("));
});

test("page persists the selected member IDs without automatic restart",()=>{
  const page=fs.readFileSync("src/routes/workspace/[id]/+page.svelte","utf8");
  const start=page.indexOf("async function saveSharedWorkspaces");
  const end=page.indexOf("async function saveUpstreamMcps",start);
  const body=page.slice(start,end);
  assert.ok(body.includes("gateway_workspace_ids: ids"));
  assert.ok(body.includes("await confirm("));
  assert.ok(!body.includes("restartRuntime("));
  assert.ok(!body.includes("startRuntime("));
});
