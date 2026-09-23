#!/usr/bin/env python3
"""Verify the merged candidate with source fingerprints and isolated runtime state.

This does not commit, install, restart, change authorization, or access business data.
"""
from __future__ import annotations
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[3]

def git(*args: str) -> bytes:
    return subprocess.check_output(['git', '--no-optional-locks', *args], cwd=ROOT, timeout=20)

def fingerprint() -> dict[str, str | None]:
    names = set(git('ls-files', '-z').decode().split('\0'))
    names.update(git('ls-files', '--others', '--exclude-standard', '-z').decode().split('\0'))
    roots = ('src/', 'src-tauri/src/', 'src-tauri/tests/', 'personal-runtime/', 'scripts/', 'tests/', 'desktop-plugin/')
    explicit = {'package.json', 'package-lock.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock',
                'src-tauri/build.rs', 'src-tauri/tauri.conf.json', 'vite.config.ts', 'vite.config.js',
                'svelte.config.js', 'tsconfig.json', 'README.md', 'README.en.md', 'PERSONAL.md'}
    result = {}
    for name in sorted(names):
        if name not in explicit and not name.startswith(roots):
            continue
        path = ROOT / name
        if path.is_symlink():
            raise RuntimeError('Source symlink needs explicit review: ' + name)
        if path.is_file():
            result[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        elif not path.exists():
            result[name] = None
    return result

def main() -> int:
    run = ROOT / '.artifacts' / 'continuation-193c2d12' / ('verify-' + str(time.time_ns()))
    run.mkdir(parents=True, mode=0o700)
    before = fingerprint()
    baseline = git('rev-parse', 'HEAD').decode().strip()
    (run / 'source-before.json').write_text(json.dumps(before, indent=2), encoding='utf-8')
    commands = [
        ('worker', 180, ['cargo', 'build', '--manifest-path', 'personal-runtime/Cargo.toml', '--locked', '--offline', '--bin', 'coding-tools-personal-worker']),
        ('rust-full', 300, ['cargo', 'test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', '--offline']),
        ('python-personal', 180, ['python3', '-B', '-m', 'unittest', 'discover', '-s', 'tests/personal', '-p', 'test_*.py', '-v']),
        ('ui-contracts', 120, ['node', '--test', 'tests/gptbridge-brand.test.mjs', 'tests/skill-write-roots-form.test.mjs']),
        ('svelte-check', 180, ['npm', 'run', 'check']),
        ('vite-build', 180, ['npm', 'run', 'build']),
    ]
    records = []
    for name, timeout, cmd in commands:
        if fingerprint() != before or git('rev-parse', 'HEAD').decode().strip() != baseline:
            records.append({'name': name, 'status': 'SOURCE_DRIFT_STOPPED'})
            break
        launch = [sys.executable, '-B', 'scripts/run_checked.py', '--name', 'exec34-193c-final-' + name,
                  '--timeout', str(timeout), '--', *cmd]
        result = subprocess.run(launch, cwd=ROOT, capture_output=True, text=True, timeout=timeout + 20)
        (run / (name + '.log')).write_text(result.stdout + '\n' + result.stderr, encoding='utf-8')
        record = {'name': name, 'exit_code': result.returncode, 'status': 'PASS' if result.returncode == 0 else 'FAIL'}
        lines = result.stdout.splitlines()
        if lines and lines[0].startswith('{'):
            checked = json.loads(lines[0])
            record['receipt'] = checked['receipt']
            stdout = pathlib.Path(checked['stdout']).read_text(errors='replace')
            stderr = pathlib.Path(checked['stderr']).read_text(errors='replace')
            record['rust_test_summaries'] = re.findall(r'test result: .*', stdout)
            record['python_test_summaries'] = re.findall(r'Ran \d+ tests? in .*|^OK(?: \(.*\))?$', stderr, re.MULTILINE)
            record['node_test_summaries'] = [line for line in stdout.splitlines() if any(k in line for k in ['ℹ tests ', 'ℹ pass ', 'ℹ fail ', 'ℹ skipped '])]
            record['stdout_sha256'] = hashlib.sha256(pathlib.Path(checked['stdout']).read_bytes()).hexdigest()
            record['stderr_sha256'] = hashlib.sha256(pathlib.Path(checked['stderr']).read_bytes()).hexdigest()
        records.append(record)
        print(json.dumps(record, ensure_ascii=False), flush=True)
        if result.returncode:
            break
    after = fingerprint()
    (run / 'source-after.json').write_text(json.dumps(after, indent=2), encoding='utf-8')
    drift = sorted(k for k in before.keys() | after.keys() if before.get(k) != after.get(k))
    result = {'created_at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'baseline_head': baseline,
              'source_count': len(before), 'source_drift': drift, 'commands': records,
              'business_acceptance': False, 'runtime_installation': False,
              'passed': not drift and len(records) == len(commands) and all(x['status'] == 'PASS' for x in records)}
    (run / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({'receipt': str(run / 'result.json'), **result}, ensure_ascii=False), flush=True)
    return 0 if result['passed'] else 1

if __name__ == '__main__':
    raise SystemExit(main())
