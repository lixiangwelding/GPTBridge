"""Isolated native HTTP transport, fixture ownership and cleanup for release checks.

These helpers only operate on the newly created fixture's profiles, processes
and canonical-workspace state. They never control the user's installed service.
"""
from __future__ import annotations

import hashlib
import http.client
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha256(value):
    return hashlib.sha256(value).hexdigest()


def file_sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def reserve_port():
    # The native listener binds itself. A race fails closed; occupied ports are
    # never reclaimed and no process on an unrelated port is stopped.
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class Rpc:
    def __init__(self, port, token):
        self.port, self.token, self.sequence = port, token, 0

    def frame(self, method, params=None):
        self.sequence += 1
        return {"jsonrpc": "2.0", "id": self.sequence, "method": method,
                "params": params or {}}

    def request(self, frame, authenticated=True, method="POST", accept="application/json"):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=15)
        headers = {"Content-Type": "application/json", "Accept": accept}
        if authenticated:
            headers["Authorization"] = "Bearer " + self.token
        try:
            connection.request(method, "/mcp", json.dumps(frame).encode(), headers)
            response = connection.getresponse()
            raw = response.read(4 * 1024 * 1024 + 1)
            require(len(raw) <= 4 * 1024 * 1024, "HTTP response exceeds fixture budget")
            return response.status, json.loads(raw) if raw and response.status == 200 else None
        finally:
            connection.close()

    def call(self, method, params=None, allow_error=False):
        frame = self.frame(method, params)
        status, response = self.request(frame)
        require(status == 200, f"{method}: HTTP {status}")
        require(isinstance(response, dict) and response.get("id") == frame["id"],
                f"{method}: response identity mismatch")
        if "error" in response:
            require(allow_error, f"{method}: JSON-RPC error")
            return {"jsonrpc_error": response["error"]}
        return response["result"]

    def tool(self, name, arguments, expect_error=False):
        result = self.call("tools/call", {"name": name, "arguments": arguments}, allow_error=expect_error)
        if "jsonrpc_error" in result:
            error = result["jsonrpc_error"]
            return {"ok": False, "error": {"code": error.get("data", {}).get("reason"),
                "rpc_code": error.get("code")}}
        value = result.get("structuredContent")
        require(isinstance(value, dict), f"{name}: structured result missing")
        failed = result.get("isError") is True or value.get("ok") is False
        require(failed == expect_error, f"{name}: unexpected tool outcome {value.get('error', {}).get('code')}")
        return value

    def drop_response(self, name, arguments, started_marker):
        frame = self.frame("tools/call", {"name": name, "arguments": arguments})
        body = json.dumps(frame).encode()
        header = (f"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{self.port}\r\n"
                  "Content-Type: application/json\r\nAccept: application/json\r\n"
                  f"Authorization: Bearer {self.token}\r\nContent-Length: {len(body)}\r\n"
                  "Connection: close\r\n\r\n").encode()
        with socket.create_connection(("127.0.0.1", self.port), timeout=10) as sock:
            sock.sendall(header + body)
            # No response bytes are received. Recovery must inspect the original
            # logical request, rather than submit a second command identity.
            # Keep the socket open until the real command begins: closing before
            # HTTP admission can legitimately cancel an unexecuted request.
            deadline = time.monotonic() + 10
            while not started_marker.exists():
                require(time.monotonic() < deadline, "unread-response command did not start")
                time.sleep(0.05)
        return {"rpc_id": frame["id"], "response_received": False,
                "request_sha256": sha256(body), "request_bytes": len(body)}


class Fixture:
    def __init__(self, binary, report):
        self.binary, self.report = binary, report
        self.root = Path(tempfile.mkdtemp(prefix="gptbridge-dot-release-")).resolve()
        self.workspace, self.home = self.root / "workspace", self.root / "home"
        self.workspace.mkdir(mode=0o700)
        (self.home / "data").mkdir(parents=True, mode=0o700)
        os.chmod(self.home, 0o700)
        self.core, self.readonly = uuid.uuid4().hex, uuid.uuid4().hex
        self.ports = {self.core: reserve_port(), self.readonly: reserve_port()}
        while self.ports[self.readonly] == self.ports[self.core]:
            self.ports[self.readonly] = reserve_port()
        self.token = secrets.token_urlsafe(48)
        profile_rows = []
        for identity, tool_profile in [(self.core, "core"), (self.readonly, "read-only")]:
            profile_rows.append({"id": identity, "name": "dot release " + tool_profile,
                "path": str(self.workspace), "tunnel": {"type": "none", "use_proxy": False},
                "auth": {"type": "bearer", "use_shared_secrets": False},
                "runtime": {"local_port": self.ports[identity], "tool_profile": tool_profile,
                    "permission_mode": "dangerous", "allowed_commands": "python3",
                    "upstream_mcps": [], "gateway_workspace_ids": []}})
        self.configuration = self.home / "data/profiles.json"
        self.configuration.write_text(json.dumps({"profiles": profile_rows,
            "last_workspace_id": self.core,
            "workspace_secrets": {identity: {"bearer_token": self.token}
                                  for identity in [self.core, self.readonly]}}))
        os.chmod(self.configuration, 0o600)
        self.config_hash = file_sha(self.configuration)
        self.workspace_hash = sha256(str(self.workspace).encode())
        require(sys.platform == "darwin", "this installed-App verifier requires macOS")
        harness = Path.home() / "Library/Application Support/coding-tools-mcp/harness"
        self.runtime = harness / "personal-runtime" / self.workspace_hash
        self.legacy = harness / "workspaces" / self.workspace_hash[:32]
        self.canonical_lock = harness / "personal-runtime/shared-write-roots/locks" / (
            sha256(("write-root:" + str(self.workspace)).encode()) + ".lock")
        self.external_paths = [self.runtime, self.legacy, self.canonical_lock]
        require(all(not path.exists() and not path.is_symlink() for path in self.external_paths),
                "fixture canonical state must not preexist")
        self.env = dict(os.environ, CODING_TOOLS_PERSONAL_HOME=str(self.home),
            CODING_TOOLS_PERSONAL_IMPORT="off", CODING_TOOLS_PERSONAL_SKILLS="off",
            PYTHONUTF8="1", PYTHONIOENCODING="utf-8")
        self.env.pop("CODING_TOOLS_PERSONAL_WORKER", None)
        self.processes, self.streams, self.clients, self.known_jobs = [], [], {}, set()
        report["fixture"] = {"root": str(self.root), "workspace": str(self.workspace),
            "profile_home": str(self.home), "core_profile_id": self.core,
            "readonly_profile_id": self.readonly, "ports": self.ports,
            "runtime_directory": str(self.runtime), "legacy_directory": str(self.legacy),
            "canonical_lock": str(self.canonical_lock), "temporary_profiles_only": True,
            "root_profile_accessed": False, "home_environment_repurposed": False,
            "worker_override": False, "upstreams": [], "tunnels": []}

    def start(self, identity):
        stream = (self.root / f"listener-{identity}.log").open("xb")
        os.chmod(stream.name, 0o600)
        self.streams.append(stream)
        command = [str(self.binary), "--personal-serve", identity, str(self.ports[identity])]
        process = subprocess.Popen(command, env=self.env, cwd=self.root,
            stdout=stream, stderr=stream, start_new_session=True)
        self.processes.append(process)
        process_record = {"pid": process.pid, "role": "isolated_listener", "profile_id": identity,
                          "arguments": command, "started": True, "ready": False}
        self.report.setdefault("processes", []).append(process_record)
        client = self.clients[identity] = Rpc(self.ports[identity], self.token)
        started = time.monotonic()
        while time.monotonic() - started < 15:
            require(process.poll() is None, f"isolated listener exited {process.returncode}")
            try:
                status, _ = client.request(client.frame("initialize"), authenticated=False)
                if status == 401:
                    process_record["ready"] = True
                    return client
            except (OSError, http.client.HTTPException, ValueError):
                pass
            time.sleep(0.1)
        raise RuntimeError("isolated listener did not become ready")

    def assert_configuration(self):
        require(file_sha(self.configuration) == self.config_hash,
                "isolated profile bytes unexpectedly changed")

    def worker_records(self, errors=None):
        records = []
        if self.runtime.is_dir():
            for path in sorted((self.runtime / "jobs").glob("*/worker.json")):
                try:
                    value = json.loads(path.read_text())
                    require(value.get("job_id") == path.parent.name
                            and isinstance(value.get("worker_pid"), int)
                            and value["worker_pid"] > 0, "worker metadata identity invalid")
                    self.known_jobs.add(value["job_id"])
                    records.append(value)
                except Exception as error:
                    if errors is None:
                        raise
                    errors.append("worker metadata unavailable: " + str(path) + ": " + type(error).__name__)
        return records

    def close(self, keep):
        errors, removed = [], []
        # Cancel only jobs in this previously absent, fixture-owned runtime.
        core = self.clients.get(self.core)
        workers = self.worker_records(errors)
        for worker in workers:
            job = worker["job_id"]
            if core is not None:
                try:
                    state = core.tool("task_status", {"job_id": job})
                    if state.get("status") in {"queued", "running", "unknown"}:
                        core.tool("kill_session", {"session_id": "job-" + job, "wait_ms": 5000})
                except Exception as error:
                    errors.append(f"isolated job cleanup {job}: {type(error).__name__}")
        for process in self.processes:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            for record in self.report.get("processes", []):
                if record["pid"] == process.pid:
                    record.update(stopped=True, returncode=process.returncode)
        for stream in self.streams:
            stream.close()
        self.report["listener_log_tail"] = {
            Path(stream.name).name: Path(stream.name).read_text(errors="replace")[-4000:].replace(self.token, "<redacted>")
            for stream in self.streams}
        # A terminal receipt precedes worker process exit by a small interval.
        # Verify only the exact workspace/job worker identity; never kill a PID
        # because a historical numeric PID happens to be present.
        for worker in workers:
            pid, job = worker.get("worker_pid"), worker["job_id"]
            expected = f"{self.binary} --personal-job-worker {self.runtime} {job}"
            deadline = time.monotonic() + 5
            while True:
                result = subprocess.run(["/bin/ps", "-ww", "-p", str(pid), "-o", "command="],
                    text=True, capture_output=True, timeout=5)
                command = result.stdout.strip()
                if result.returncode == 1 and not command and not result.stderr.strip():
                    break
                if result.returncode != 0 or result.stderr.strip():
                    errors.append(f"worker process observation unavailable: {pid}/{job}")
                    break
                if command != expected:
                    # A different identity is never terminated by this script.
                    break
                if time.monotonic() >= deadline:
                    errors.append(f"fixture worker still active: {pid}/{job}")
                    break
                time.sleep(0.1)
        self.report["workers"] = workers
        if not keep and not errors:
            cleanup_paths = [*self.external_paths, self.root]
            for path in cleanup_paths:
                if path.exists():
                    require(not path.is_symlink(), "fixture cleanup path became a symlink")
                    # lsof exit 1 with empty output means no references. Other
                    # failures preserve the fixture instead of risking deletion.
                    command = ["/usr/sbin/lsof", "+D", str(path)] if path.is_dir() else ["/usr/sbin/lsof", str(path)]
                    opened = subprocess.run(command, text=True, capture_output=True, timeout=15)
                    if opened.returncode != 1 or opened.stdout.strip() or opened.stderr.strip():
                        errors.append("open-file check unavailable or active: " + str(path))
            for path in cleanup_paths if not errors else []:
                if path.exists():
                    if path.is_dir():
                        shutil.rmtree(path)
                    else:
                        path.unlink()
                    require(not path.exists() and not path.is_symlink(), "fixture cleanup readback failed")
                    removed.append(str(path))
        self.report["cleanup"] = {"status": "PASS" if not errors else "FAIL",
            "kept_for_ui_evidence": keep, "removed": removed, "errors": errors,
            "production_processes_stopped": False, "exact_fixture_paths_only": True}
        return not errors
