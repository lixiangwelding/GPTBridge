import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const json = path => JSON.parse(read(path));
const version = '0.4.1';

test('package and Tauri branding have one name and version', () => {
  const pkg = json('package.json'), lock = json('package-lock.json'), tauri = json('src-tauri/tauri.conf.json');
  assert.equal(pkg.name, 'gptbridge');
  assert.equal(lock.name, pkg.name);
  assert.equal(lock.packages[''].name, pkg.name);
  for (const actual of [pkg.version, lock.version, lock.packages[''].version, tauri.version]) assert.equal(actual, version);
  assert.equal(tauri.productName, 'GPTBridge');
  assert.match(tauri.app.windows[0].title, /^GPTBridge/);
  assert.match(read('src-tauri/Cargo.toml'), /version = "0\.4\.1"/);
  assert.match(read('src-tauri/Cargo.lock'), /name = "coding-tools-mcp-personal"\nversion = "0\.4\.1"/);
});

test('rendered main routes and close dialog use GPTBridge', () => {
  for (const file of ['src/app.html', 'src/lib/taskdock/Shell.svelte', 'src/lib/taskdock/Workbench.svelte',
    'src/lib/taskdock/ProjectDialog.svelte', 'src/lib/taskdock/TaskInspector.svelte',
    'src/routes/projects/+page.svelte', 'src/routes/skills/+page.svelte', 'src/routes/settings/+page.svelte',
    'src/lib/components/CloseConfirmDialog.svelte']) {
    assert.ok(read(file).includes('GPTBridge'), file);
    assert.ok(!read(file).includes('TaskDock'), file);
  }
});

test('tray, app info and task handoff use the approved public name', () => {
  assert.match(read('src-tauri/src/lib.rs'), /\.tooltip\("GPTBridge/);
  const command = read('src-tauri/src/commands/taskdock.rs');
  assert.match(command, /"name":"GPTBridge"/);
  assert.match(command, /使用 GPTBridge 工具继续任务/);
});

test('rename keeps bundle identity, binary, IPC and storage compatibility', () => {
  assert.equal(json('src-tauri/tauri.conf.json').identifier, 'com.lixiangwelding.codingtools.personal');
  assert.match(read('src-tauri/Cargo.toml'), /name = "coding-tools-mcp-personal"/);
  for (const name of ['taskdock_snapshot', 'taskdock_task', 'taskdock_create', 'taskdock_job_output', 'taskdock_skill_preference']) {
    assert.ok(read('src/lib/taskdock/api.ts').includes(`"${name}"`), name);
    assert.ok(read('src-tauri/src/lib.rs').includes(name), name);
  }
  assert.match(read('src/lib/taskdock/state.ts'), /taskdock:compact/);
  assert.match(read('src/lib/taskdock/CreateTaskDialog.svelte'), /taskdock:pending-create/);
});

test('documentation leads with the current name and identifies historical references', () => {
  for (const file of ['README.md', 'README.en.md', 'PERSONAL.md']) assert.match(read(file), /^# GPTBridge/);
  assert.match(read('README.md'), /把对话，接到你的工作现场/);
  for (const file of ['README.md', 'README.en.md']) {
    assert.ok(read(file).includes('<details>'));
    assert.ok(read(file).includes('</details>'));
  }
});

test('app repository links target the owned repository rather than upstream installers', () => {
  const links = read('src/lib/app-links.ts');
  assert.match(links, /github\.com\/lixiangwelding\//);
  assert.ok(!links.includes('mybolide'));
});
