#!/usr/bin/env python3
"""Explicit, user-level launchd activation; never force-unlocks or kills workers."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
from collections import Counter
import signal
import socket
import sqlite3
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def health(port: int) -> dict:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open("http://127.0.0.1:%d/mcp" % port, timeout=1) as response:
        return json.loads(response.read(65536))


def pids(port: int, established: bool = False) -> list[int]:
    query = ["/usr/sbin/lsof", "-nP", "-iTCP:%d" % port,
             "-sTCP:" + ("ESTABLISHED" if established else "LISTEN"), "-Fp"]
    result = subprocess.run(query, capture_output=True, text=True, timeout=5)
    if result.returncode not in (0, 1) or result.stderr.strip():
        raise ValueError("cannot inspect listening connections")
    return sorted({int(line[1:]) for line in result.stdout.splitlines() if line.startswith("p")})


def launch_definition(executable: Path, home: Path, profile: str, port: int) -> dict:
    if not executable.is_absolute() or not profile or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in profile):
        raise ValueError("invalid application path or profile")
    if not 1024 <= port <= 65535:
        raise ValueError("invalid unprivileged port")
    label = "com.lixiangwelding.codingtools.personal." + profile
    logs = home / "Library/Application Support/coding-tools-mcp-personal/headless" / profile
    return {"Label": label, "ProgramArguments": [str(executable), "--personal-serve", profile, str(port)],
            "RunAtLoad": True, "KeepAlive": True, "ThrottleInterval": 5,
            "EnvironmentVariables": {"CODING_TOOLS_PERSONAL_HOME": str(home / "Library/Application Support/coding-tools-mcp-personal"), "CODING_TOOLS_PERSONAL_IMPORT": "off"},
            "StandardOutPath": str(logs / "stdout.log"), "StandardErrorPath": str(logs / "stderr.log")}


def foreign_jobs(home: Path, workspace: Path, own_task: str) -> list[str]:
    key = hashlib.sha256(str(workspace.resolve()).encode()).hexdigest()
    database = home / "Library/Application Support/coding-tools-mcp/harness/personal-runtime" / key / "runtime.sqlite3"
    connection = sqlite3.connect(database.as_uri() + "?mode=ro", uri=True, timeout=2)
    try:
        return [row[0] for row in connection.execute("SELECT id FROM jobs WHERE state IN ('queued','running') AND (task_id IS NULL OR task_id<>?) LIMIT 33", (own_task,))]
    finally:
        connection.close()


def wait_health(port: int, version: str, seconds: int = 15) -> dict:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            result = health(port)
            if result.get("version") == version:
                return result
        except (OSError, ValueError):
            pass
        time.sleep(.15)
    raise ValueError("new listener did not pass the version check")


def rpc_idle(path: Path) -> bool:
    # Old append logging can concatenate whole records without a newline.
    # Reassemble only complete known records; ambiguity fails closed.
    try:
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 32 * 1024 * 1024:
            return False
        with path.open("rb") as stream:
            raw = stream.read(32 * 1024 * 1024 + 1)
        if len(raw) > 32 * 1024 * 1024:
            return False
        text = raw.decode("utf-8", "strict")
    except (OSError, UnicodeError):
        return False
    pattern = r"\[rpc\] (request|completed|worker_failed) id=(.*?) method=([A-Za-z0-9_/]+) tool=([A-Za-z0-9_]*)"
    records = re.findall(pattern, text)
    markers = sum(text.count("[rpc] " + kind + " ") for kind in ["request", "completed", "worker_failed"])
    if not records or len(records) != markers:
        return False
    counts: Counter = Counter()
    for kind, identity, method, tool in records:
        if method.startswith("notifications/"):
            continue
        counts[(identity, method, tool)] += 1 if kind == "request" else -1
    return all(count == 0 for count in counts.values())


def verify_managed_service(definition: dict, home: Path, pid: int, arguments: str) -> None:
    expected = definition["ProgramArguments"]
    # The executable has spaces; ps renders argv as text. Compare the complete
    # known command instead of accepting a substring/profile supplied by callers.
    if arguments.strip() != " ".join(expected):
        raise ValueError("managed listener arguments differ from the saved profile")
    plist = home / "Library/LaunchAgents" / (definition["Label"] + ".plist")
    if plist.is_symlink() or not plist.is_file() or plistlib.loads(plist.read_bytes()) != definition:
        raise ValueError("registered LaunchAgent definition differs")
    state = subprocess.run(["/bin/launchctl", "print", "gui/%d/%s" % (os.getuid(), definition["Label"])],
                           capture_output=True, text=True, timeout=5)
    if state.returncode or not re.search(r"(?m)^\s*pid = %d\s*$" % pid, state.stdout):
        raise ValueError("running PID is not owned by the expected LaunchAgent")


def activate(profile: str, own_task: str, handover_pid: int | None = None, restart_service: bool = False) -> dict:
    if sys.platform != "darwin":
        raise ValueError("this activation entry supports the personal macOS installation")
    import uuid
    uuid.UUID(own_task)
    home = Path.home()
    app = home / "Applications/Coding Tools MCP Personal.app"
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    if info.get("CFBundleIdentifier") != "com.lixiangwelding.codingtools.personal":
        raise ValueError("unexpected application identity")
    version = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"]
    if info.get("CFBundleShortVersionString") != version:
        raise ValueError("application install version differs from the release")
    name = info["CFBundleExecutable"]
    if not isinstance(name, str) or Path(name).name != name:
        raise ValueError("invalid executable in application metadata")
    executable = app / "Contents/MacOS" / name
    config = home / "Library/Application Support/coding-tools-mcp-personal/data/profiles.json"
    before = sha(config)
    data = json.loads(config.read_text())
    matches = [p for p in data["profiles"] if p["id"] == profile]
    if len(matches) != 1:
        raise ValueError("select one saved profile")
    selected = matches[0]
    port = selected["runtime"]["local_port"]
    definition = launch_definition(executable, home, profile, port)
    report = ROOT / ".artifacts/desktop036" / ("activation-" + str(time.time_ns()) + ".json")
    report.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    result = {"version": version, "profile_id": profile, "port": port, "configuration_sha256": before,
              "executable_sha256": sha(executable), "receipt": str(report), "old_process_stopped": False,
              "foreign_jobs_cancelled": False, "tunnels_stopped": False}
    try:
        current = pids(port)
        if current:
            existing = health(port)
            if existing.get("version") == version:
                result.update(status="already_running", health=existing, process_ids=current)
                return result
            if handover_pid is None or current != [handover_pid] or handover_pid <= 1:
                raise ValueError("old listener remains active; explicit verified GUI handover PID required")
            command = subprocess.run(["/bin/ps", "-p", str(handover_pid), "-o", "comm="], capture_output=True, text=True, timeout=5).stdout.strip()
            arguments = subprocess.run(["/bin/ps", "-ww", "-p", str(handover_pid), "-o", "command="], capture_output=True, text=True, timeout=5).stdout
            if command != str(executable) or "--personal-job-worker" in arguments:
                raise ValueError("handover target is not the exact personal application")
            if "--personal-serve" in arguments:
                if not restart_service:
                    raise ValueError("managed service restart requires --restart-service")
                verify_managed_service(definition, home, handover_pid, arguments)
            elif restart_service:
                raise ValueError("--restart-service requires the registered headless service")
        # Verify the same installed executable and saved HTTP auth on an unused port.
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0)); probe_port = reservation.getsockname()[1]
        env = os.environ.copy(); env.update(definition["EnvironmentVariables"])
        with open(os.devnull, "wb") as null:
            probe = subprocess.Popen([str(executable), "--personal-serve", profile, str(probe_port)],
                                     stdin=subprocess.DEVNULL, stdout=null, stderr=null, env=env, start_new_session=True)
            try:
                result["preflight_health"] = wait_health(probe_port, version)
                if pids(probe_port) != [probe.pid]:
                    raise ValueError("preflight listener ownership mismatch")
            finally:
                probe.terminate()
                try: probe.wait(timeout=5)
                except subprocess.TimeoutExpired: probe.kill(); probe.wait()
        foreign = foreign_jobs(home, Path(selected["path"]), own_task)
        if foreign:
            result["blocking_job_ids"] = foreign
            raise ValueError("other jobs are active or queued; existing listener preserved")
        if current:
            # Allow this administrative RPC to return before checking idle state.
            # Do not stop a busy connection or claim lock-free hot swapping.
            deadline = time.monotonic() + 8
            idle_samples = 0
            while time.monotonic() < deadline and idle_samples < 2:
                if foreign_jobs(home, Path(selected["path"]), own_task):
                    raise ValueError("another job arrived; existing listener preserved")
                log = home / "Library/Application Support/coding-tools-mcp-personal/logs" / profile / "mcp-requests.log"
                # A disconnected HTTP client can leave a blocking tool running;
                # absence of sockets alone is not sufficient evidence of idleness.
                idle = rpc_idle(log)
                idle_samples = idle_samples + 1 if idle else 0
                time.sleep(.4)
            if idle_samples < 2:
                raise ValueError("HTTP connections remain; existing listener preserved")
            result["idle_rpc_verified"] = True
            result["idle_keepalive_connections_may_reconnect"] = bool(pids(port, established=True))
        if sha(config) != before or pids(port) != current:
            raise ValueError("configuration or listener changed during preflight")
        plist = home / "Library/LaunchAgents" / (definition["Label"] + ".plist")
        plist.parent.mkdir(parents=True, exist_ok=True)
        if plist.is_symlink() or (plist.exists() and plistlib.loads(plist.read_bytes()) != definition):
            raise ValueError("existing LaunchAgent differs and was preserved")
        logs = Path(definition["StandardOutPath"]).parent
        logs.mkdir(parents=True, exist_ok=True, mode=0o700)
        if not plist.exists():
            descriptor = os.open(plist, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, "wb") as stream: stream.write(plistlib.dumps(definition))
        # Never signal workers or a process group. A verified idle GUI is replaced
        # only after the candidate was healthy; FRP and durable jobs are untouched.
        if current:
            if foreign_jobs(home, Path(selected["path"]), own_task) or not rpc_idle(log):
                raise ValueError("new work arrived before handover; existing listener preserved")
            if restart_service:
                verify_managed_service(definition, home, handover_pid, arguments)
            os.kill(handover_pid, signal.SIGTERM)
            result["old_process_stopped"] = True
            if restart_service:
                # Existing KeepAlive LaunchAgent starts the replaced executable.
                # Never unload it or signal its worker children/process group.
                result["health"] = wait_health(port, version, seconds=20)
                result.update(status="running", process_ids=pids(port), launch_agent=str(plist),
                              configuration_unchanged=sha(config)==before, managed_restart=True)
                if not result["configuration_unchanged"]:
                    raise ValueError("configuration drifted during service restart")
                return result
            deadline = time.monotonic() + 5
            while pids(port) and time.monotonic() < deadline: time.sleep(.1)
            if pids(port): raise ValueError("old GUI has not released the port; no forced termination")
        bootstrap = subprocess.run(["/bin/launchctl", "bootstrap", "gui/%d" % os.getuid(), str(plist)], capture_output=True, timeout=10)
        result["bootstrap_exit_code"] = bootstrap.returncode
        result["health"] = wait_health(port, version)
        result.update(status="running", process_ids=pids(port), launch_agent=str(plist), configuration_unchanged=sha(config)==before)
        if not result["configuration_unchanged"]: raise ValueError("configuration drifted during activation")
        return result
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        result.update(status="blocked" if not result["old_process_stopped"] else "activation_failed", error=str(error))
        # A launchd problem must not strand the verified port after handover.
        # Start only the already-tested personal binary, never replay user jobs.
        if result["old_process_stopped"]:
            try:
                if not pids(port):
                    with open(definition["StandardOutPath"], "ab") as out, open(definition["StandardErrorPath"], "ab") as err:
                        fallback = subprocess.Popen(definition["ProgramArguments"], stdin=subprocess.DEVNULL,
                            stdout=out, stderr=err, env=env, start_new_session=True)
                    result["fallback_pid"] = fallback.pid
                result["health"] = wait_health(port, version)
                result.update(status="running_fallback", configuration_unchanged=sha(config)==before)
            except (OSError, ValueError, subprocess.SubprocessError):
                result["fallback_verified"] = False
        return result
    finally:
        descriptor = os.open(report, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream: json.dump(result, stream, ensure_ascii=False, indent=2)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--task-id", required=True)
    parser.add_argument("--handover-pid", type=int)
    parser.add_argument("--restart-service", action="store_true", help="Explicit idle restart of the verified personal LaunchAgent only")
    args = parser.parse_args()
    try:
        result = activate(args.profile, args.task_id, args.handover_pid, args.restart_service)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0 if result["status"] in {"running", "already_running", "running_fallback"} else 1
    except (OSError, ValueError) as error:
        print(json.dumps({"status":"blocked","error_type":type(error).__name__,"old_process_stopped":False}))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
