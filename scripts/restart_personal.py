#!/usr/bin/env python3
"""Explicitly reload one installed personal LaunchAgent after idle verification.

Uses launchd's management interface for its own registered label. It neither
queries business-service ports nor uses a wrapper to execute rejected lsof calls.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import socket
import subprocess
import time
import uuid
from service_personal import foreign_jobs, health, launch_definition, rpc_idle, sha, wait_health

ROOT = Path(__file__).resolve().parents[1]


def parse_service(text: str, executable: Path, profile: str) -> int:
    program = re.findall(r'^\s*program = (.+)$', text, re.MULTILINE)
    pids = re.findall(r'^\s*pid = ([0-9]+)$', text, re.MULTILINE)
    args = re.search(r'^\s*arguments = \{\n(.*?)^\s*\}', text, re.MULTILINE | re.DOTALL)
    expected = [str(executable), '--personal-serve', profile]
    entries = [line.strip() for line in args.group(1).splitlines()] if args else []
    if (program != [str(executable)] or len(pids) != 1 or int(pids[0]) <= 1
            or entries[:3] != expected or len(entries) != 4 or not entries[3].isdigit()):
        raise ValueError('launchd service identity is not the exact personal server')
    return int(pids[0])


def guarded_restart(expected_pid, read_pid, is_idle, restart):
    """Two idle samples and a final identity check; no forced lock recovery."""
    for _ in range(2):
        if not is_idle() or read_pid() != expected_pid:
            raise ValueError('new work or service identity change; listener preserved')
        time.sleep(.25)
    if not is_idle() or read_pid() != expected_pid:
        raise ValueError('new work arrived before reload; listener preserved')
    restart()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', required=True)
    parser.add_argument('--task-id', required=True)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    uuid.UUID(args.task_id)
    if not re.fullmatch(r'[A-Za-z0-9_-]+', args.profile):
        parser.error('invalid profile')
    home = Path.home()
    config = home / 'Library/Application Support/coding-tools-mcp-personal/data/profiles.json'
    before = sha(config)
    selected = [p for p in json.loads(config.read_text())['profiles'] if p['id'] == args.profile]
    if len(selected) != 1:
        raise ValueError('exact saved profile required')
    selected = selected[0]
    app = home / 'Applications/Coding Tools MCP Personal.app'
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    version = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())['version']
    if (info.get('CFBundleIdentifier') != 'com.lixiangwelding.codingtools.personal'
            or info.get('CFBundleShortVersionString') != version):
        raise ValueError('install the tested personal application first')
    name = info['CFBundleExecutable']
    if not isinstance(name, str) or Path(name).name != name:
        raise ValueError('invalid bundle executable')
    executable = app / 'Contents/MacOS' / name
    binary_sha = sha(executable)
    definition = launch_definition(executable, home, args.profile, selected['runtime']['local_port'])
    plist = home / 'Library/LaunchAgents' / (definition['Label'] + '.plist')
    if plist.is_symlink() or plistlib.loads(plist.read_bytes()) != definition:
        raise ValueError('existing LaunchAgent differs; preserved')
    plist_sha = sha(plist)
    target = 'gui/%d/%s' % (os.getuid(), definition['Label'])
    port = selected['runtime']['local_port']
    log = home / 'Library/Application Support/coding-tools-mcp-personal/logs' / args.profile / 'mcp-requests.log'

    def read_pid():
        result = subprocess.run(['/bin/launchctl', 'print', target], capture_output=True, text=True, timeout=5)
        if result.returncode:
            raise ValueError('registered personal LaunchAgent is unavailable')
        return parse_service(result.stdout, executable, args.profile)

    def idle():
        return (sha(config) == before and sha(plist) == plist_sha and sha(executable) == binary_sha
                and not foreign_jobs(home, Path(selected['path']), args.task_id) and rpc_idle(log))

    old_pid = read_pid()
    report = {'profile': args.profile, 'version': version, 'oldPid': old_pid,
              'configurationSha256': before, 'executableSha256': binary_sha,
              'target': target, 'restartRequested': False, 'foreignJobsCancelled': False}
    if not args.apply:
        report.update(status='plan', health=health(port))
        print(json.dumps(report, ensure_ascii=False)); return 0
    out = ROOT / '.artifacts/restart-personal' / str(time.time_ns())
    out.mkdir(parents=True, mode=0o700)
    report['receipt'] = str(out / 'receipt.json')
    try:
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0)); probe_port = reservation.getsockname()[1]
        env = {**os.environ, **definition['EnvironmentVariables']}
        with open(os.devnull, 'wb') as sink:
            probe = subprocess.Popen([str(executable), '--personal-serve', args.profile, str(probe_port)],
                stdin=subprocess.DEVNULL, stdout=sink, stderr=sink, env=env, start_new_session=True)
            try:
                report['candidateHealth'] = wait_health(probe_port, version)
                if probe.poll() is not None:
                    raise ValueError('candidate exited during warmup')
            finally:
                probe.terminate()
                try: probe.wait(timeout=5)
                except subprocess.TimeoutExpired: probe.kill(); probe.wait(timeout=5)
        # The administrative tool RPC may still be completing. Wait only for
        # bounded idle samples; never cancel any other task to manufacture idle.
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline and not idle():
            time.sleep(.3)

        def restart():
            report['restartRequested'] = True
            result = subprocess.run(['/bin/launchctl', 'kickstart', '-k', target], capture_output=True, timeout=10)
            if result.returncode:
                raise ValueError('launchd rejected the selected service reload')

        guarded_restart(old_pid, read_pid, idle, restart)
        report['health'] = wait_health(port, version)
        report['newPid'] = read_pid()
        report['configurationUnchanged'] = sha(config) == before and sha(plist) == plist_sha
        if not report['configurationUnchanged'] or report['newPid'] == old_pid or sha(executable) != binary_sha:
            raise ValueError('service reload identity or configuration readback failed')
        report['status'] = 'running'
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        report.update(status='not_verified', error=str(error)[:400])
    finally:
        (out / 'receipt.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
        print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report['status'] == 'running' else 1


if __name__ == '__main__':
    raise SystemExit(main())
