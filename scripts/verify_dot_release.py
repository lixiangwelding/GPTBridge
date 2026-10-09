#!/usr/bin/env python3
"""Verify the installed macOS App over real, isolated loopback HTTP/MCP.

Only a newly created fixture/profile home and its canonical-workspace state are
used. No production profile, launch service, tunnel or external client is used.
The installed App itself runs both the listener and its durable command worker.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import plistlib
import subprocess
import time

from dot_release_support import Fixture, file_sha, require, sha256


def verify(fixture, expected_version, record):
    workspace = fixture.workspace
    (workspace / "dot-owned.txt").write_text("before\n")
    stdout = "verified\n" + "".join(f"输出 {n:03d} native release receipt\n" for n in range(48))
    stderr = "isolated release stderr\n"
    script = ("import pathlib, sys, time\n"
        "root = pathlib.Path(__file__).resolve().parent\n"
        "assert (root / 'dot-owned.txt').read_text() == 'verified\\n'\n"
        "with (root / 'run-count.txt').open('a') as f: f.write('once\\n')\n"
        "(root / 'command-started.txt').write_text('started\\n')\n"
        "time.sleep(0.5)\n"
        f"sys.stdout.write({stdout!r})\nsys.stderr.write({stderr!r})\n")
    (workspace / "dot-command.py").write_text(script)
    client = fixture.start(fixture.core)
    status, _ = client.request(client.frame("initialize"), authenticated=False)
    require(status == 401, "Bearer authentication rejection failed")
    record("authentication_rejection", http_status=status, token="redacted_ephemeral_bearer")
    initialized = client.call("initialize", {"protocolVersion": "2025-03-26",
        "capabilities": {}, "clientInfo": {"name": "installed-release-fixture", "version": "1"}})
    require(initialized["serverInfo"]["version"] == expected_version, "installed runtime version mismatch")
    require(initialized["capabilities"]["tools"]["listChanged"] is False, "catalog notification contract drift")
    require("no external Agent is required" in initialized.get("instructions", ""), "native instructions missing")
    record("initialize", version=expected_version,
        list_changed=False, native_instructions_present="no external Agent is required" in initialized.get("instructions", ""))
    status, _ = client.request({}, method="GET", accept="text/event-stream")
    require(status == 405, "GET /mcp SSE availability contract drift")
    record("GET_SSE", http_status=status)
    catalog = client.call("tools/list")
    tools = {tool["name"]: tool for tool in catalog["tools"]}
    require(len(tools) == len(catalog["tools"]) == 39, "core tool catalog count mismatch")
    for name, fields in {"apply_patch": ["request_id", "expected_hashes", "task_id"],
                         "exec_command": ["request_id", "task_id", "mode", "durable"],
                         "read_output": ["output_ref", "offset", "limit"]}.items():
        require(all(field in tools[name]["inputSchema"]["properties"] for field in fields),
                "native tool preconditions missing from catalog: " + name)
    record("tools_list", count=len(tools), names=list(tools),
        catalog_sha256=sha256(json.dumps(catalog["tools"], sort_keys=True).encode()),
        mutation_preconditions_present=True)
    info = client.tool("server_info", {})
    direct = info["direct_workspace"]
    require(info["version"] == expected_version and info["auth_type"] == "bearer", "server metadata mismatch")
    require(direct["execution_path"] == "native_tools" and direct["external_agent_required"] is False,
            "native metadata mismatch")
    require(direct["security"]["execution_isolation"] == "policy_only"
            and direct["security"]["sandbox_enforced"] is False
            and direct["security"]["read_scope"] == "explicit_external_paths_allowed", "actual policy mismatch")
    record("server_info", version=info["version"], auth_type=info["auth_type"], direct_workspace=direct,
        actual_tool_profile=info["tool_profile"], actual_permission_mode=info["permission_mode"])
    task = client.tool("task_open", {"goal": "Verify installed release in an isolated fixture",
        "request_id": "release-open"})
    task_id = task["task_id"]
    record("task_open", task_id=task_id, revision=task["revision"])
    read = client.tool("read_file", {"path": "dot-owned.txt"})
    before_hash = sha256(b"before\n")
    require(read["file_sha256"] == before_hash, "read_file hash does not match actual fixture")
    record("read_file_hash", file_sha256=read["file_sha256"], path="dot-owned.txt")
    patch = {"task_id": task_id, "request_id": "release-patch", "confirm": True,
        "expected_hashes": {"dot-owned.txt": before_hash},
        "patch": "*** Begin Patch\n*** Update File: dot-owned.txt\n@@\n-before\n+verified\n*** End Patch"}
    applied = client.tool("apply_patch", patch)
    require((workspace / "dot-owned.txt").read_bytes() == b"verified\n", "patch file readback failed")
    require(applied["before_hashes"]["dot-owned.txt"] == before_hash
            and applied["after_hashes"]["dot-owned.txt"] == sha256(b"verified\n"), "patch receipt hashes mismatch")
    record("hash_protected_patch", task_id=task_id, request_id=patch["request_id"],
        change_id=applied["change_id"], before_hashes=applied["before_hashes"], after_hashes=applied["after_hashes"])
    replay = client.tool("apply_patch", patch)
    require(replay["deduplicated"] is True and replay["change_id"] == applied["change_id"], "patch recovery repeated mutation")
    record("patch_replay", original_change_id=replay["change_id"], deduplicated=True)
    stale = dict(patch, request_id="release-stale-patch",
        patch="*** Begin Patch\n*** Update File: dot-owned.txt\n@@\n-verified\n+bad\n*** End Patch")
    rejected = client.tool("apply_patch", stale, expect_error=True)
    require(rejected["error"]["code"] == "STALE_FILE" and (workspace / "dot-owned.txt").read_bytes() == b"verified\n",
            "stale hash was not safely rejected")
    record("stale_hash_rejection", error_code=rejected["error"]["code"], file_unchanged=True)
    command = {"task_id": task_id, "request_id": "release-exec-once", "cmd": "python3 dot-command.py",
        "workdir": ".", "mode": "write", "confirm": True, "durable": True,
        "timeout_ms": 15000, "yield_time_ms": 0, "max_output_bytes": 1024}
    dropped = client.drop_response("exec_command", command, workspace / "command-started.txt")
    view = client.tool("task_status", {"task_id": task_id})
    original = [job for job in view["jobs"]["jobs"] if job["request_id"] == command["request_id"]]
    require(len(original) == 1, "dropped request did not have exactly one durable job")
    job_id, session = original[0]["job_id"], original[0]["session_id"]
    fixture.known_jobs.add(job_id)
    dropped.update(task_id=task_id, request_id=command["request_id"], job_id=job_id,
        original_identity_observed_before_replay=True)
    record("disconnect_after_request", **dropped)
    recovered = client.tool("exec_command", command)
    require(recovered["deduplicated"] is True and recovered["job_id"] == job_id
            and recovered["session_id"] == session, "command recovery created a different job")
    require(recovered["execution_mode"] == "durable_worker"
            and recovered["sandbox_enforced"] is False, "durable worker boundary mismatch")
    record("original_request_recovery", job_id=job_id, session_id=session,
        request_id=command["request_id"], deduplicated=True, execution_mode=recovered["execution_mode"])
    deadline, polls = time.monotonic() + 20, 0
    while True:
        done = client.tool("write_stdin", {"session_id": session, "chars": "",
            "yield_time_ms": 100, "max_output_bytes": 1024})
        polls += 1
        require(done["job_id"] == job_id and done["request_id"] == command["request_id"], "poll changed original identity")
        if done["status"] not in {"queued", "running"}:
            break
        require(time.monotonic() < deadline, "original durable job did not finish")
    require(done["status"] == "exited" and done["exit_code"] == 0
            and done["command_ok"] is True, "original command did not exit successfully")
    record("original_session_terminal", job_id=job_id, session_id=session, polls=polls,
        job_status=done["status"], exit_code=done["exit_code"], command_ok=done["command_ok"])
    for stream, expected in [("stdout", stdout), ("stderr", stderr)]:
        reference, offset, pages, chunks = done["output_refs"][stream], 0, [], []
        while True:
            page = client.tool("read_output", {"output_ref": reference, "offset": offset, "limit": 128})
            require(page["job_id"] == job_id and page["task_id"] == task_id
                    and page["request_id"] == command["request_id"] and page["job_status"] == "exited",
                    "paged output owner/status mismatch")
            require(page["offset"] == offset and page["retained_bytes"] == len(expected.encode())
                    and page["may_be_truncated"] is False, "paged output byte metadata mismatch")
            chunks.append(page["content"])
            pages.append({key: page[key] for key in ["offset", "next_offset", "poll_offset",
                "bytes_read", "retained_bytes", "may_be_truncated", "job_status"]})
            next_offset = page["next_offset"]
            if next_offset is None:
                break
            require(next_offset > offset and len(pages) < 100, "paged output did not progress")
            offset = next_offset
        joined = "".join(chunks)
        require(joined == expected, "original paged output did not match actual command output")
        if stream == "stdout":
            require(len(pages) >= 3, "stdout pagination was not exercised")
        record("original_" + stream + "_pages", output_ref=reference, pages=pages,
            content_sha256=sha256(joined.encode()), content_bytes=len(joined.encode()))
    require((workspace / "run-count.txt").read_bytes() == b"once\n", "command side effect ran more than once")
    after = client.tool("exec_command", command)
    require(after["deduplicated"] is True and after["job_id"] == job_id
            and (workspace / "run-count.txt").read_bytes() == b"once\n", "terminal recovery repeated command")
    record("exec_replay_side_effect", job_id=job_id, deduplicated=True, actual_side_effect_count=1)
    checkpoint = {"task_id": task_id, "request_id": "release-checkpoint",
        "expected_revision": task["revision"], "state": "completed",
        "checkpoint": {"summary": "Installed release: real hash patch, command and original output verified",
            "steps": {"installed-native-flow": {"state": "passed", "evidence": [done["output_refs"]["stdout"]]}}}}
    saved = client.tool("task_checkpoint", checkpoint)
    require(saved["state"] == "completed", "completed checkpoint not persisted")
    checkpoint_replay = client.tool("task_checkpoint", checkpoint)
    require(checkpoint_replay["revision"] == saved["revision"], "checkpoint replay advanced revision")
    restored = client.tool("task_open", {"task_id": task_id})
    require(restored["state"] == "completed" and restored["revision"] == saved["revision"], "task reopen lost checkpoint")
    record("checkpoint_and_reopen", task_id=task_id, state=restored["state"],
        revision=restored["revision"], checkpoint_replay_same_revision=True)
    readonly = fixture.start(fixture.readonly)
    readonly_catalog = readonly.call("tools/list")["tools"]
    readonly_names = {tool["name"] for tool in readonly_catalog}
    require(len(readonly_names) == 26 and not {"apply_patch", "exec_command", "task_open", "task_checkpoint"} & readonly_names,
            "read-only exposure mismatch")
    before_files = {path.name: file_sha(path) for path in workspace.iterdir() if path.is_file()}
    denied_codes = {}
    for name, arguments in [("apply_patch", patch), ("exec_command", command)]:
        denied = readonly.tool(name, arguments, expect_error=True)
        denied_codes[name] = denied["error"]["code"]
        require(denied_codes[name] == "unknown_tool", "read-only write was not denied by actual exposure")
    require({path.name: file_sha(path) for path in workspace.iterdir() if path.is_file()} == before_files,
            "read-only rejection changed fixture files")
    record("read_only_write_denial", catalog_count=len(readonly_names), error_codes=denied_codes,
        fixture_files_unchanged=True)
    fixture.assert_configuration()
    record("isolated_profile_preserved", byte_sha256=fixture.config_hash, unchanged=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True, help="actual installed .app path")
    parser.add_argument("--expect-version", required=True)
    parser.add_argument("--expect-binary-sha256", required=True)
    parser.add_argument("--source-commit", required=True, help="source commit associated with installed artifact")
    parser.add_argument("--output", type=Path, required=True, help="new result path; never overwrites an old receipt")
    parser.add_argument("--keep-evidence", action="store_true", help="retain private fixture/state for subsequent real App GUI evidence")
    args = parser.parse_args()
    args.output = args.output.resolve()
    require(not args.output.exists(), "result path already exists; use a new receipt path")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    report = {"status": "RUNNING", "started_utc": datetime.now(timezone.utc).isoformat(),
        "transport": "real_loopback_HTTP_MCP", "source_commit": args.source_commit,
        "installed_release": True, "external_dot_client_acceptance": "NOT_VERIFIED",
        "legacy_GPTBridge_client_acceptance": "NOT_VERIFIED", "steps": []}
    fixture = None
    artifact_verified = False
    def record(name, **value):
        report["steps"].append(dict(name=name, status="PASS", **value))
        print(json.dumps({"step": name, "status": "PASS"}), flush=True)
    try:
        app = args.app.resolve(strict=True)
        require(app.suffix == ".app" and app.is_dir(), "expected an installed macOS App directory")
        with (app / "Contents/Info.plist").open("rb") as stream:
            metadata = plistlib.load(stream)
        binary = app / "Contents/MacOS" / metadata["CFBundleExecutable"]
        require(binary.is_file() and not binary.is_symlink() and os.access(binary, os.X_OK), "App executable is invalid")
        require(metadata["CFBundleShortVersionString"] == args.expect_version, "installed App bundle version mismatch")
        require(file_sha(binary) == args.expect_binary_sha256, "installed App binary SHA mismatch")
        signature = subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)],
            capture_output=True, timeout=30)
        require(signature.returncode == 0, "installed App strict signature verification failed")
        report["artifact"] = {"app_path": str(app), "binary_path": str(binary),
            "version": args.expect_version, "binary_sha256": args.expect_binary_sha256,
            "strict_signature_verified": True, "app_is_adhoc_or_developer_signed": "not_a_notarization_assertion"}
        artifact_verified = True
        fixture = Fixture(binary, report)
        verify(fixture, args.expect_version, record)
        require(file_sha(binary) == args.expect_binary_sha256, "installed App changed during acceptance")
        report["status"] = "PASS"
    except Exception as error:
        report["status"] = "FAIL"
        message = str(error)
        if fixture is not None:
            message = message.replace(fixture.token, "<redacted>")
        report["failure"] = {"type": type(error).__name__, "message": message}
    finally:
        if fixture is not None:
            try:
                if not fixture.close(args.keep_evidence):
                    report["status"] = "FAIL"
            except Exception as error:
                report["cleanup"] = {"status": "FAIL", "type": type(error).__name__,
                    "message": str(error).replace(fixture.token, "<redacted>"), "fixture_preserved": True}
                report["status"] = "FAIL"
        if artifact_verified:
            try:
                report["artifact"]["binary_sha256_after"] = file_sha(binary)
                require(report["artifact"]["binary_sha256_after"] == args.expect_binary_sha256,
                        "installed App changed during final readback")
            except Exception as error:
                report["artifact"]["final_readback_error"] = type(error).__name__
                report["status"] = "FAIL"
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
        with args.output.open("x", encoding="utf-8") as stream:
            json.dump(report, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
    print(json.dumps({"status": report["status"], "output": str(args.output),
        "passed_steps": len(report["steps"]), "fixture_kept": args.keep_evidence}, ensure_ascii=False))
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
