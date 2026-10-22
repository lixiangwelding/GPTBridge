#!/usr/bin/env python3
"""Codex-safe file handoff. This CLI never starts a process or calls GPTBridge/ChatGPT."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import sys
import uuid
from scheduler import ID, atomic_json, digest, inside, validate_batch


def submit(plan_path, prompts, workspace, queue, batch_id, supersedes=None):
    plan = json.loads(Path(plan_path).read_text(encoding='utf-8-sig'))
    if plan.get('connector') != 'GPTBridge Windows':
        raise ValueError('only GPTBridge Windows is allowed')
    tasks = []
    for task in plan['tasks']:
        if task.get('status', 'draft') != 'draft':
            raise ValueError('submit only a fresh draft plan; do not replay sent/unknown tasks')
        task_id = task['id']
        if not isinstance(task_id, str) or not task_id.replace('-', '').replace('_', '').isalnum() or len(task_id)>80:
            raise ValueError('invalid task id')
        prompt = task.get('customPrompt', '').strip() or (Path(prompts) / (task_id + '.md')).read_text(encoding='utf-8')
        tasks.append(dict(id=task_id, title=task['title'], dependsOn=task['dependsOn'], prompt=prompt))
    batch = validate_batch(dict(schemaVersion=1, connector='GPTBridge Windows', batchId=batch_id,
                                workspace=str(Path(workspace).resolve()), title=plan['title'], tasks=tasks))
    if supersedes is not None:
        with supersede_unsent(queue, workspace, supersedes, batch):
            result = publish_batch(batch, queue)
        return dict(result, supersedes=supersedes, oldBatchArchived=True)
    return publish_batch(batch, queue)


def publish_batch(batch, queue):
    batch_id = batch['batchId']
    inbox = Path(queue) / 'inbox'
    inbox.mkdir(parents=True, exist_ok=True)
    target = inbox / (batch_id + '.json')
    if target.exists():
        if json.loads(target.read_text(encoding='utf-8')) == batch:
            return dict(submitted=True, duplicate=True, batchId=batch_id, path=str(target), dispatched=False)
        raise ValueError('batch id already exists with different content')
    tmp = inbox / ('.' + uuid.uuid4().hex + '.tmp')
    try:
        with tmp.open('x', encoding='utf-8') as f:
            json.dump(batch, f, ensure_ascii=False, indent=2)
            f.flush()
            os.fsync(f.fileno())
        os.link(tmp, target)  # atomic no-replace publication, same filesystem
    finally:
        tmp.unlink(missing_ok=True)
    return dict(submitted=True, duplicate=False, batchId=batch_id, path=str(target), dispatched=False)


@contextlib.contextmanager
def supersede_unsent(queue, workspace, old_id, replacement):
    """File handoff only: archive proven-unsent input under the worker's owner lock.

    Archive before publishing the replacement, so a crash cannot expose both inputs.
    Original states, request IDs, attempts and receipts are never rewritten/deleted.
    """
    import fcntl  # The Codex-safe queue handoff runs in the existing WSL runtime.
    new_id = replacement['batchId']
    if not ID.fullmatch(old_id) or not ID.fullmatch(new_id) or old_id == new_id:
        raise ValueError('invalid supersession identity')
    root = Path(queue)
    with (root / 'owner.lock').open('a+') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('scheduler owns queue; cannot supersede') from None
        heartbeat = json.loads((root / 'scheduler-status.json').read_text())
        if heartbeat.get('status') != 'stopped' or (root / 'auto-enabled').exists():
            raise ValueError('scheduler must already be stopped')
        incoming = root / 'inbox' / (old_id + '.json')
        archive = root / 'superseded' / old_id
        saved = archive / 'batch.json'
        source = incoming if incoming.exists() else saved
        batch = validate_batch(json.loads(source.read_text()))
        state_path = root / 'states' / (old_id + '.json')
        state = json.loads(state_path.read_text())
        if (batch['batchId'] != old_id or state['batchId'] != old_id
                or state['inputSha256'] != digest(source)
                or Path(batch['workspace']).resolve() != Path(workspace).resolve()):
            raise ValueError('old batch identity changed')
        for row in state['tasks'].values():
            request_id = row.get('requestId', '')
            if (row.get('status') not in ('waiting_client', 'blocked') or not ID.fullmatch(request_id)
                    or row.get('nonce') != request_id or row.get('preparedRequest')):
                raise ValueError('old request is sent, prepared or uncertain; preserve it')
            if (list((root / 'attempts').glob(request_id + '.send.*'))
                    or inside(workspace, row['receiptPath']).exists()):
                raise ValueError('old request has send evidence or a receipt')
            audits = list((root / 'attempts').glob(request_id + '.stage.*.json'))
            if not audits:
                raise ValueError('old request has no preparation audit')
            for audit in audits:
                result = json.loads(audit.read_text(encoding='utf-8-sig'))
                if result.get('requestId') != request_id or result.get('status') not in ('blocked', 'busy'):
                    raise ValueError('preparation does not prove request unsent')
        # Bind recoverable interrupted handoffs to the same replacement contents.
        fingerprint = hashlib.sha256(json.dumps(replacement, ensure_ascii=False, sort_keys=True).encode()).hexdigest()
        metadata = dict(oldBatchId=old_id, replacementBatchId=new_id, oldInputSha256=digest(source),
                        oldStateSha256=digest(state_path), replacementSha256=fingerprint)
        archive.mkdir(parents=True, exist_ok=True)
        record = archive / 'supersession.json'
        if record.exists() and json.loads(record.read_text()) != metadata:
            raise ValueError('supersession belongs to different inputs')
        if saved.exists() and digest(saved) != digest(source):
            raise ValueError('archive differs from original input')
        atomic_json(record, metadata)
        if incoming.exists():
            if not saved.exists():
                os.link(incoming, saved)
            incoming.unlink()
        try:
            yield
        except BaseException:
            # The owner lock prevents dispatch while an invalid replacement rolls back.
            target = root / 'inbox' / (new_id + '.json')
            published = target.exists() and json.loads(target.read_text()) == replacement
            if not incoming.exists() and not published:
                os.link(saved, incoming)
            raise


def main():
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest='command',required=True)
    s=sub.add_parser('submit');s.add_argument('--plan',required=True);s.add_argument('--prompts',required=True)
    s.add_argument('--workspace',required=True);s.add_argument('--queue',required=True);s.add_argument('--batch-id',required=True)
    s.add_argument('--supersedes', help='Archive a stopped, proven-unsent batch; preserve all request audits')
    s=sub.add_parser('status');s.add_argument('--queue',required=True)
    a=p.parse_args()
    try:
        if a.command=='submit':
            print(json.dumps(submit(a.plan,a.prompts,a.workspace,a.queue,a.batch_id,a.supersedes),ensure_ascii=False))
        else:
            root=Path(a.queue)
            result={'scheduler':None,'batches':[],'pending':[],'superseded':[]}
            if (root/'scheduler-status.json').is_file():result['scheduler']=json.loads((root/'scheduler-status.json').read_text(encoding='utf-8'))
            for path in sorted((root/'states').glob('*.json')):result['batches'].append(json.loads(path.read_text(encoding='utf-8')))
            for path in sorted((root/'inbox').glob('*.json')):
                batch=validate_batch(json.loads(path.read_text(encoding='utf-8-sig')))
                result['pending'].append(dict(batchId=batch['batchId'],title=batch.get('title',''),taskIds=[t['id'] for t in batch['tasks']],inputSha256=digest(path)))
            for path in sorted((root/'superseded').glob('*/supersession.json')):
                result['superseded'].append(json.loads(path.read_text(encoding='utf-8')))
            print(json.dumps(result,ensure_ascii=False,indent=2))
    except Exception as e:
        print(json.dumps({'ok':False,'error':str(e)},ensure_ascii=False),file=sys.stderr);raise SystemExit(1)


if __name__=='__main__':main()
