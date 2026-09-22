#!/usr/bin/env python3
"""Run one bounded verification in isolated personal state; retain real logs and exit code."""
from __future__ import annotations
import argparse, datetime, json, os, pathlib, signal, subprocess, sys, time
from personal_env import toolchain_environment

def verification_environment(root: pathlib.Path, run: pathlib.Path) -> dict[str, str]:
    env = toolchain_environment()
    worker = root.resolve() / '.artifacts' / 'cargo' / 'debug' / (
        'coding-tools-personal-worker.exe' if os.name == 'nt' else 'coding-tools-personal-worker')
    # Integration-test libraries are compiled without cfg(test). Their current_exe
    # is a test harness, not the app's worker entry. Use the same explicit worker
    # as selftest_personal; missing builds then fail clearly instead of staying queued.
    env.update(CODING_TOOLS_PERSONAL_HOME=str(run / 'home'), CODING_TOOLS_PERSONAL_IMPORT='off',
               CODING_TOOLS_PERSONAL_WORKER=str(worker), CARGO_TARGET_DIR=str(root / '.artifacts' / 'cargo'),
               CARGO_BUILD_JOBS='2', RUST_TEST_THREADS='2')
    return env

def main() -> int:
    parser=argparse.ArgumentParser()
    parser.add_argument('--name',required=True)
    parser.add_argument('--timeout',type=int,default=300)
    parser.add_argument('command',nargs=argparse.REMAINDER)
    args=parser.parse_args()
    command=args.command[1:] if args.command[:1]==['--'] else args.command
    if not command or not args.name.replace('-','').replace('_','').isalnum():
        parser.error('a safe run name and command are required')
    root=pathlib.Path(__file__).resolve().parents[1]
    run=root/'.artifacts'/'checks'/f'{args.name}-{time.time_ns()}'
    run.mkdir(parents=True,mode=0o700)
    env=verification_environment(root, run)
    record={'name':args.name,'command':command,'status':'running','exit_code':None,'started':datetime.datetime.now(datetime.timezone.utc).isoformat(),'runtime_state':str(run/'home'),'stdout':str(run/'stdout.log'),'stderr':str(run/'stderr.log')}
    receipt=run/'result.json'
    receipt.write_text(json.dumps(record,indent=2))
    started=time.monotonic(); proc=None
    try:
        with (run/'stdout.log').open('wb') as out,(run/'stderr.log').open('wb') as err:
            proc=subprocess.Popen(command,cwd=root,env=env,stdout=out,stderr=err,start_new_session=os.name!='nt')
            try:
                code=proc.wait(timeout=args.timeout)
                record.update(status='passed' if code==0 else 'failed',exit_code=code)
            except subprocess.TimeoutExpired:
                if os.name!='nt': os.killpg(proc.pid,signal.SIGKILL)
                else: proc.kill()
                proc.wait(); record.update(status='timeout',exit_code=124)
    except OSError as exc:
        record.update(status='spawn_failed',exit_code=127,error=str(exc))
    finally:
        record['duration_seconds']=round(time.monotonic()-started,3)
        record['finished']=datetime.datetime.now(datetime.timezone.utc).isoformat()
        receipt.write_text(json.dumps(record,indent=2))
        print(json.dumps({'receipt':str(receipt),**record},ensure_ascii=False))
        for name in ['stdout.log','stderr.log']:
            path=run/name
            if path.exists():
                print(f'--- {name} tail ---')
                print('\n'.join(path.read_text(errors='replace').splitlines()[-35:]))
    return int(record['exit_code'])

if __name__=='__main__': raise SystemExit(main())
