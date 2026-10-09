import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
async function module(file) {
  const js = ts.transpileModule(readFileSync(file, "utf8"), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(js).toString("base64")}`);
}
const { outputPage } = await module("src/lib/dot/output.ts");
const { receiptPageMatches, receiptSource, receiptTiming, receiptHash } = await module("src/lib/dot/receipts.ts");
const identity = { workspace_id: "workspace-a", task_id: "task-a", job_id: "job-a", stream: "stdout" };

test("output metadata uses actual response, including zero retained bytes and explicit non-truncation", () => {
  const page = outputPage({ ...identity, content: "result", retained_bytes: 0, may_be_truncated: false, job_status: "exited", next_offset: 25 }, identity, 20);
  assert.deepEqual(page, { content: "result", offset: 20, next: 25, retainedBytes: 0, truncated: false, status: "exited", identityVerified: true });
});
test("output rejects any mismatched or partial new response identity", () => {
  for (const [field, wrong] of [["workspace_id", "workspace-b"], ["task_id", "task-b"], ["job_id", "job-b"], ["stream", "stderr"]]) {
    assert.throws(() => outputPage({ ...identity, [field]: wrong, content: "foreign" }, identity, 0), /归属/);
  }
  assert.throws(() => outputPage({ workspace_id: "workspace-a", content: "partial" }, identity, 0), /归属/);
});
test("old response gaps remain unknown rather than inheriting prior output metadata", () => {
  const page = outputPage({ content: "", poll_offset: 12 }, identity, 12);
  assert.equal(page.next, 12);
  assert.equal(page.retainedBytes, null);
  assert.equal(page.truncated, null);
  assert.equal(page.status, null);
  assert.equal(page.identityVerified, false);
  assert.throws(() => outputPage({ content: "", next_offset: 3 }, identity, 12), /分页/);
});
test("receipt query identity is checked without inventing the original profile identity", () => {
  const receipt = { source: "durable_receipt", workspace_id: null, task_id: "task-a" };
  const page = { workspace_id: "workspace-a", task_id: "task-a", items: [receipt] };
  assert.equal(receiptPageMatches(page, "workspace-a", "task-a"), true);
  assert.equal(receiptPageMatches(page, "workspace-b", "task-a"), false);
  assert.equal(receiptPageMatches({ ...page, items: [{ ...receipt, task_id: "task-b" }] }, "workspace-a", "task-a"), false);
  assert.match(receiptSource(receipt), /原工作区 ID 未记录/);
});
test("dispatcher time cannot masquerade as worker lifetime, and absent-file hashes retain their meaning", () => {
  assert.match(receiptTiming({ timing_source: "dispatcher_audit" }), /不是命令进程全生命周期/);
  assert.match(receiptTiming({ timing_source: "task_event_end_only" }), /开始时间未记录/);
  assert.match(receiptTiming({ timing_source: "not_recorded" }), /没有保存/);
  assert.equal(receiptHash(null, false), "该阶段文件不存在");
  assert.equal(receiptHash(null, null), "未记录");
  assert.equal(receiptHash("original-sha256", true), "original-sha256");
});
