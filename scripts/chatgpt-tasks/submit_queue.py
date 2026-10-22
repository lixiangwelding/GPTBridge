#!/usr/bin/env python3
"""Codex-safe file handoff. This CLI never starts a process or calls GPTBridge/ChatGPT."""
import argparse
import json
import os
from pathlib import Path
import sys
import uuid
from scheduler import validate_batch


def submit(plan_path, prompts, workspace, queue, batch_id):
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


def main():
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest='command',required=True)
    s=sub.add_parser('submit');s.add_argument('--plan',required=True);s.add_argument('--prompts',required=True)
    s.add_argument('--workspace',required=True);s.add_argument('--queue',required=True);s.add_argument('--batch-id',required=True)
    s=sub.add_parser('status');s.add_argument('--queue',required=True)
    a=p.parse_args()
    try:
        if a.command=='submit':
            print(json.dumps(submit(a.plan,a.prompts,a.workspace,a.queue,a.batch_id),ensure_ascii=False))
        else:
            root=Path(a.queue)
            result={'scheduler':None,'batches':[]}
            if (root/'scheduler-status.json').is_file():result['scheduler']=json.loads((root/'scheduler-status.json').read_text())
            for path in sorted((root/'states').glob('*.json')):result['batches'].append(json.loads(path.read_text()))
            print(json.dumps(result,ensure_ascii=False,indent=2))
    except Exception as e:
        print(json.dumps({'ok':False,'error':str(e)},ensure_ascii=False),file=sys.stderr);raise SystemExit(1)


if __name__=='__main__':main()
