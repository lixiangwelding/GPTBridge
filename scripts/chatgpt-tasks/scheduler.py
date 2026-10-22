#!/usr/bin/env python3
"""Independent Windows-owned queue consumer. Importing this module never dispatches.
Codex may submit/read queue files; only the user enables this scheduler in GPTBridge.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time
import uuid

ID = re.compile(r'^[A-Za-z0-9_-]{1,80}$')


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + '.' + uuid.uuid4().hex + '.tmp')
    try:
        with tmp.open('x', encoding='utf-8') as f:
            json.dump(value, f, ensure_ascii=False, indent=2)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
    finally:
        tmp.unlink(missing_ok=True)


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def inside(root, relative):
    if not isinstance(relative, str) or not relative or '\\' in relative:
        raise ValueError('expected workspace-relative POSIX path')
    p = Path(relative)
    if p.is_absolute() or '..' in p.parts:
        raise ValueError('path escapes workspace')
    root = Path(root).resolve()
    resolved = (root / p).resolve()
    if not resolved.is_relative_to(root):
        raise ValueError('symlink escapes workspace')
    return resolved


def validate_batch(batch):
    if batch.get('schemaVersion') != 1 or batch.get('connector') != 'GPTBridge Windows':
        raise ValueError('Windows queue schema mismatch')
    if not ID.fullmatch(batch.get('batchId', '')):
        raise ValueError('invalid batch id')
    tasks = batch.get('tasks')
    if not isinstance(tasks, list) or not 1 <= len(tasks) <= 200:
        raise ValueError('expected 1-200 tasks')
    ids = set()
    for t in tasks:
        if not ID.fullmatch(t.get('id', '')) or t['id'] in ids:
            raise ValueError('invalid or duplicate task id')
        ids.add(t['id'])
        if not isinstance(t.get('prompt'), str) or not t['prompt'].strip() or len(t['prompt'].encode()) > 100000:
            raise ValueError('invalid task prompt')
        if not isinstance(t.get('dependsOn'), list) or not all(isinstance(x, str) for x in t['dependsOn']):
            raise ValueError('invalid dependency list')
    visiting, visited = set(), set()
    by_id = {t['id']: t for t in tasks}
    def visit(key):
        if key not in ids or key in visiting:
            raise ValueError('missing dependency or cycle')
        if key in visited:
            return
        visiting.add(key)
        for dep in by_id[key]['dependsOn']:
            visit(dep)
        visiting.remove(key)
        visited.add(key)
    for key in ids:
        visit(key)
    return batch


def verify_receipt(workspace, receipt, batch_id, task_id, nonce):
    if receipt.get('schemaVersion') != 1 or receipt.get('batchId') != batch_id or receipt.get('taskId') != task_id or receipt.get('nonce') != nonce:
        raise ValueError('receipt identity mismatch')
    if receipt.get('status') != 'completed':
        raise ValueError('task did not complete: ' + str(receipt.get('status')))
    if not str(receipt.get('summary', '')).strip():
        raise ValueError('missing result summary')
    artifacts, checks = receipt.get('artifacts'), receipt.get('checks')
    if not isinstance(artifacts, list) or not artifacts or not isinstance(checks, list) or not checks:
        raise ValueError('completion needs artifacts and validation evidence')
    # This verifies submitted evidence identity, never infers game acceptance or executes receipt commands.
    for a in artifacts:
        path = inside(workspace, a.get('path'))
        if not path.is_file() or digest(path) != a.get('sha256'):
            raise ValueError('artifact missing or hash mismatch')
    for c in checks:
        if c.get('exitCode') != 0 or isinstance(c.get('exitCode'), bool) or not str(c.get('command', '')).strip():
            raise ValueError('validation failed or missing command record')
        path = inside(workspace, c.get('logPath'))
        if not path.is_file() or digest(path) != c.get('logSha256'):
            raise ValueError('validation log missing or hash mismatch')
    return True


def receipt_instructions(batch_id, task_id, nonce, relative):
    template = dict(schemaVersion=1, batchId=batch_id, taskId=task_id, nonce=nonce,
                    status='completed', summary='实际完成范围、未验层级和遗留问题',
                    artifacts=[dict(path='本工作区内实际产物的相对路径', sha256='实际SHA256')],
                    checks=[dict(command='实际执行的验证命令，仅记录，不由调度器执行', exitCode=0,
                                 logPath='实际验证日志相对路径', logSha256='实际日志SHA256')])
    return ('\n\n独立 Windows 调度器的本任务结果合同：\n'
            '只在 ChatGPT 中调用 GPTBridge Windows 执行；Codex 仅提交了此任务队列。\n'
            '完成实际工作并核对产物后，使用 GPTBridge Windows 将以下 JSON 按实际值写入工作区相对路径：'
            + relative + '\n' + json.dumps(template, ensure_ascii=False, indent=2)
            + '\n保留本次 nonce，不使用旧回执；不得编造完成或验证日志。未通过请写 status=blocked/failed 和 summary，保留遗留项。'
            '不要把仅编译/组件/目录通过升级为原端、游戏或奖励验收。只有 completed 且产物与日志 SHA 独立回读一致，调度器才推进依赖任务。')


class Engine:
    def __init__(self, queue, workspace, adapter, options):
        self.queue, self.workspace = Path(queue), Path(workspace).resolve()
        self.adapter, self.options = adapter, options
        self.queue.mkdir(parents=True, exist_ok=True)

    def send_prepared(self, state_path, state, row, request):
        row['status'] = 'unknown'
        atomic_json(state_path, state)  # durable before exactly one send
        sent = self.adapter(request)
        row.update(status='sent' if sent.get('status') == 'sent' else 'unknown', detail=sent.get('detail', '查询原对话'))
        row.pop('preparedRequest', None)
        atomic_json(state_path, state)
        return row['status']

    def tick(self):
        if (self.queue / 'stop').exists():
            return 'stopped'
        if (self.queue / 'pause').exists():
            return 'paused'
        for incoming in sorted((self.queue / 'inbox').glob('*.json')):
            if incoming.stat().st_size > 2 * 1024 * 1024:
                raise ValueError('batch file exceeds size limit')
            batch = validate_batch(json.loads(incoming.read_text(encoding='utf-8-sig')))
            if Path(batch.get('workspace', '')).resolve() != self.workspace:
                raise ValueError('batch workspace differs from user-enabled workspace')
            batch_id = batch['batchId']
            state_path = self.queue / 'states' / (batch_id + '.json')
            if state_path.exists():
                state = json.loads(state_path.read_text(encoding='utf-8'))
                if state['inputSha256'] != digest(incoming):
                    raise ValueError('submitted batch changed after dispatch')
            else:
                state = dict(batchId=batch_id, inputSha256=digest(incoming), status='queued', tasks={})
            if state['status'] in ('completed', 'blocked', 'attention'):
                continue
            for task in batch['tasks']:
                row = state['tasks'].get(task['id'])
                if row and row['status'] in ('completed', 'blocked'):
                    continue
                if row and row['status'] == 'prepared':
                    return self.send_prepared(state_path, state, row, row['preparedRequest'])
                if row and row['status'] in ('preparing', 'unknown', 'sent'):
                    receipt_path = inside(self.workspace, row['receiptPath'])
                    if receipt_path.exists():
                        if receipt_path.stat().st_size > 2 * 1024 * 1024:
                            raise ValueError('receipt exceeds size limit')
                        result = json.loads(receipt_path.read_text(encoding='utf-8-sig'))
                        if result.get('status') in ('blocked','failed'):
                            if result.get('schemaVersion')!=1 or result.get('batchId')!=batch_id or result.get('taskId')!=task['id'] or result.get('nonce')!=row['nonce'] or not str(result.get('summary','')).strip():
                                raise ValueError('blocked receipt identity/summary mismatch')
                            row.update(status='blocked',detail=result['summary'],receiptSha256=digest(receipt_path))
                            state['status']='running'
                            atomic_json(state_path,state)
                            continue  # only dependent tasks wait; independent ready tasks may proceed
                        # A partially written JSON is rejected until the writer finishes atomically.
                        verify_receipt(self.workspace, result, batch_id, task['id'], row['nonce'])
                        row.update(status='completed', receiptSha256=digest(receipt_path), completedAt=time.time())
                        state['status'] = 'running'
                        atomic_json(state_path, state)
                        continue
                    if time.time() - row['startedAt'] > self.options.get('timeoutSeconds', 7200):
                        state.update(status='attention', detail='结果超时，查询原请求；没有重发。')
                        atomic_json(state_path, state)
                    return 'waiting_receipt'
                if any(state['tasks'].get(dep, {}).get('status') != 'completed' for dep in task['dependsOn']):
                    continue
                if row and row['status']=='waiting_client' and time.time()-row['startedAt']>self.options.get('timeoutSeconds',7200):
                    state.update(status='attention',detail='客户端长时间忙碌，未发送当前任务。');atomic_json(state_path,state);return 'attention'
                nonce = row['nonce'] if row and row['status']=='waiting_client' else uuid.uuid4().hex
                started_at = row['startedAt'] if row and row['status']=='waiting_client' else time.time()
                relative = f'output/gptbridge-chatgpt-results/{batch_id}/{task["id"]}.result.json'
                inside(self.workspace, relative).parent.mkdir(parents=True, exist_ok=True)
                prompt = task['prompt'] + receipt_instructions(batch_id, task['id'], nonce, relative)
                row = dict(status='preparing', nonce=nonce, requestId=nonce,
                           receiptPath=relative, startedAt=started_at, client=self.options['client'])
                state['tasks'][task['id']] = row
                state['status'] = 'running'
                atomic_json(state_path, state)  # durable before touching the client
                request = dict(action='stage', client=row['client'], taskId=task['id'], requestId=nonce,
                               windowRef='', model=self.options['model'], effort=self.options['effort'],
                               prompt=prompt, newChat=True)
                prepared = self.adapter(request)
                # Only known-unsent preparation may fall back to the web; never after send.
                if prepared.get('status') == 'blocked' and row['client'] == 'desktop' and self.options.get('webFallback'):
                    request['client'] = 'web'
                    row['client'] = 'web'
                    atomic_json(state_path, state)
                    prepared = self.adapter(request)
                if prepared.get('status') == 'busy':
                    row.update(status='waiting_client',detail=prepared.get('detail','client busy; no action performed'))
                    atomic_json(state_path,state)
                    return 'waiting_client'
                if prepared.get('status') != 'prepared':
                    row.update(status='blocked', detail=prepared.get('detail', '无法准备客户端'))
                    state.update(status='blocked', detail=row['detail'])
                    atomic_json(state_path, state)
                    return 'blocked'
                row['windowRef'] = prepared.get('windowRef', '')
                request.update(action='send', windowRef=row['windowRef'], newChat=False)
                row.update(status='prepared', preparedRequest=request)
                atomic_json(state_path, state)
                if (self.queue/'stop').exists() or (self.queue/'pause').exists():
                    return 'paused_before_send'
                return self.send_prepared(state_path, state, row, request)
            if all(state['tasks'].get(t['id'], {}).get('status') == 'completed' for t in batch['tasks']):
                state['status'] = 'completed'
                atomic_json(state_path, state)
                return 'completed'
            if any(r.get('status')=='blocked' for r in state['tasks'].values()):
                state.update(status='blocked',detail='阻塞项及其依赖保留；当前没有可独立推进的就绪任务。')
                atomic_json(state_path,state)
                return 'blocked'
        return 'idle'


class WindowsAdapter:
    def __init__(self, queue, adapter_path):
        self.queue, self.adapter_path = Path(queue), adapter_path

    def __call__(self, request):
        attempt = self.queue / 'attempts' / (request['requestId'] + '.' + request['action'] + '.' + request['client'] + '.json')
        if request['action'] == 'send' and attempt.exists():
            return dict(status='unknown', detail='本请求已有发送尝试；没有重放。')
        atomic_json(attempt, dict(status='unknown', requestId=request['requestId'], action=request['action']))
        path = attempt.with_suffix('.input.json')
        atomic_json(path, request)
        try:
            win_path = subprocess.check_output(['wslpath', '-w', str(path)], text=True).strip()
            p = subprocess.run(['/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe',
                                '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
                                self.adapter_path, '-RequestPath', win_path], capture_output=True, timeout=50)
            result = json.loads(p.stdout.decode('utf-8-sig')) if p.returncode == 0 else dict(status='unknown', detail='适配器退出异常')
            atomic_json(attempt, result)
            return result
        except Exception:
            return dict(status='unknown', detail='适配器异常；查询原对话，禁止重发。')
        finally:
            path.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True)
    args = parser.parse_args()
    config = json.loads(Path(args.config).read_text(encoding='utf-8-sig'))
    if config.get('ownerEnabled') is not True:
        parser.error('Windows owner must enable the scheduler in GPTBridge; Codex only submits queue files')
    queue = Path(config['queue'])
    queue.mkdir(parents=True, exist_ok=True)
    (queue / 'inbox').mkdir(exist_ok=True)
    import fcntl  # Windows host explicitly starts this worker in its existing WSL Python runtime.
    with (queue / 'owner.lock').open('a+') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit('a scheduler already owns this queue')
        atomic_json(queue/'auto-enabled',dict(pid=os.getpid(),owner='Windows GPTBridge'))
        engine = Engine(queue, config['workspace'], WindowsAdapter(queue, config['adapterWindowsPath']), config)
        heartbeat = queue / 'scheduler-status.json'
        try:
            while True:
                try:
                    state = engine.tick()
                    atomic_json(heartbeat, dict(status=state, pid=os.getpid(), updatedAt=time.time(), detail=''))
                except Exception as e:
                    # Bad/partial/mismatched receipts never cause redispatch. Preserve all task state.
                    atomic_json(heartbeat, dict(status='attention', pid=os.getpid(), updatedAt=time.time(), detail=str(e)))
                    state = 'attention'
                if state == 'stopped':
                    return
                time.sleep(2)
        finally:
            (queue/'auto-enabled').unlink(missing_ok=True)
            atomic_json(heartbeat, dict(status='stopped', pid=os.getpid(), updatedAt=time.time()))


if __name__ == '__main__':
    main()
