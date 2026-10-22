import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { spawnSync } from 'node:child_process';
import { validatePlan, promptFor, nextReady, transition, isConversationUrl } from '../src/lib/chatgpt-tasks/model.ts';
const seed = JSON.parse(fs.readFileSync(new URL('../src/lib/chatgpt-tasks/ddt-plan.json',import.meta.url)));
test('20 tasks are valid, versioned, and dependency ordered',()=>{
 const p=validatePlan(seed);assert.equal(p.tasks.length,20);assert.equal(nextReady(p).id,'T01');
 for(const t of p.tasks){assert.ok(t.steps.length>=3);assert.ok(t.acceptance.length>=2);assert.ok(promptFor(p,t).includes('Mac 连接不要调用或修改'));}
});
test('dependency cannot be bypassed by manual sending',()=>{
 const p=validatePlan(seed);assert.throws(()=>transition(p,'T02','sent','','https://chatgpt.com/c/abc'),/依赖/);
});
test('sending does not imply business acceptance',()=>{
 let p=transition(seed,'T01','sent','','https://chatgpt.com/c/abc');assert.equal(nextReady(p),undefined);
 assert.throws(()=>transition(p,'T01','done'),/验收/);
 p=transition(p,'T01','done','source manifest verified');assert.equal(nextReady(p).id,'T02');
});
test('UNKNOWN never becomes a draft automatically',()=>{
 const p=transition(seed,'T01','unknown','query original request');assert.throws(()=>transition(p,'T01','draft'),/重发/);
 assert.throws(()=>transition(p,'T01','sent','','https://chatgpt.com/c/abc'),/草稿/);
});
test('reject cycles, missing deps, duplicate ids and cross connector imports',()=>{
 for(const mutate of [p=>p.tasks[0].dependsOn=['T20'],p=>p.tasks[0].dependsOn=['missing'],p=>p.tasks[1].id='T01',p=>p.connector='Mac']){
  const p=structuredClone(seed);mutate(p);assert.throws(()=>validatePlan(p));
 }
});
test('link validation excludes other hosts, queries, credentials and javascript',()=>{
 assert.equal(isConversationUrl('https://chatgpt.com/c/abc-123'),true);
 for(const x of ['https://chatgpt.com.evil/c/abc','javascript:alert(1)','https://user:pass@chatgpt.com/c/abc','https://chatgpt.com/c/abc?token=secret'])assert.equal(isConversationUrl(x),false);
});
test('imported completion must include evidence and validation returns isolated copy',()=>{
 const p=structuredClone(seed);p.tasks[0].status='done';assert.throws(()=>validatePlan(p),/回执/);
 const v=validatePlan(seed);v.tasks[0].title='changed';assert.notEqual(seed.tasks[0].title,'changed');
});
test('native dispatcher is not an MCP tool and uses unique send attempt barrier',()=>{
 const rs=fs.readFileSync(new URL('../src-tauri/src/commands/chatgpt_tasks.rs',import.meta.url),'utf8');
 const ps=fs.readFileSync(new URL('../src-tauri/scripts/chatgpt-dispatch.ps1',import.meta.url),'utf8');
 assert.match(rs,/create_new\(true\)/);assert.match(rs,/UNKNOWN/);assert.match(rs,/kill_on_drop\(true\)/);
 assert.match(ps,/Remove GPTBridge Windows/);assert.match(ps,/ValuePattern/);assert.match(ps,/Result 'unknown'/);
 const registry=fs.readFileSync(new URL('../src-tauri/src/tools/registry.rs',import.meta.url),'utf8');assert.ok(!registry.includes('chatgpt_task_dispatch'));
});

test('Windows desktop image check rejects Codex embedded ChatGPT before UI actions',{skip:process.platform!=='win32'},()=>{
 const ps=fs.readFileSync(new URL('../src-tauri/scripts/chatgpt-dispatch.ps1',import.meta.url),'utf8');
 const helper=ps.match(/function Test-IndependentChatGPTImage\([^]*?\r?\n}/)?.[0];assert.ok(helper);
 const cases=[
  [String.raw`C:\Program Files\WindowsApps\OpenAI.Codex_26.1013_x64__id\app\ChatGPT.exe`,false],
  [String.raw`C:\Users\me\AppData\Local\Programs\Codex\ChatGPT.exe`,false],
  [String.raw`C:\Program Files\WindowsApps\OpenAI.ChatGPT_1_x64__id\ChatGPT.exe`,true],
  ['',false], [String.raw`C:\Apps\Other.exe`,false]
 ];
 const body=helper+'\n'+cases.map(([path,expected])=>`if((Test-IndependentChatGPTImage '${path.replaceAll("'","''")}') -ne $${expected}){throw 'wrong image classification'}`).join('\n');
 const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(body,'utf16le').toString('base64')],{encoding:'utf8'});
 assert.equal(result.status,0,result.stderr);
});
