#!/usr/bin/env python3
"""Freeze real native MCP catalogs and compare approved descriptive changes.

Uses isolated, disposable profiles; never changes the installed configuration.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

PROFILES = ("core", "read-only", "advanced", "compat-readonly-all")
DESCRIPTION_ALLOWLIST = {
    "server_info", "read_file", "apply_patch", "exec_command", "write_stdin", "read_output"
}


def snapshot(binary):
    catalogs = {}
    with tempfile.TemporaryDirectory(prefix="gptbridge-dot-contract-") as tmp:
        root = Path(tmp)
        workspace = root / "workspace"
        workspace.mkdir()
        home = root / "home"
        (home / "data").mkdir(parents=True)
        profiles = [{"id": name, "name": name, "path": str(workspace),
                     "tunnel": {"type": "none"},
                     "auth": {"type": "noauth", "use_shared_secrets": False},
                     "runtime": {"tool_profile": name, "permission_mode": "trusted"}}
                    for name in PROFILES]
        (home / "data/profiles.json").write_text(json.dumps({"profiles": profiles}))
        # User-skill discovery is environment-dependent and is not part of this
        # source compatibility contract. Disable it in both disposable fixtures.
        env = dict(os.environ, CODING_TOOLS_PERSONAL_HOME=str(home), CODING_TOOLS_PERSONAL_IMPORT="off",
                   CODING_TOOLS_PERSONAL_SKILLS="off")
        requests = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        ]
        for name in PROFILES:
            result = subprocess.run([str(binary), "--personal-stdio", name],
                                    input="".join(json.dumps(r) + "\n" for r in requests),
                                    text=True, capture_output=True, env=env, timeout=30, check=True)
            responses = [json.loads(line) for line in result.stdout.splitlines()]
            initialize = next(r["result"] for r in responses if r.get("id") == 1)
            listing = next(r["result"] for r in responses if r.get("id") == 2)
            assert initialize["capabilities"]["tools"]["listChanged"] is False
            tools = listing["tools"]
            assert len({t["name"] for t in tools}) == len(tools)
            catalogs[name] = {"version": initialize["serverInfo"]["version"],
                              "capabilities": initialize["capabilities"],
                              "instructions": initialize.get("instructions"),
                              "tools": sorted(tools, key=lambda t: t["name"])}
    return {"profiles": catalogs, "configuration_written": False,
            "fixture": "disposable native stdio profiles", "client_refresh_verified": False}


def compare(before, after):
    changes = []
    violations = []
    for name in PROFILES:
        old, new = before["profiles"][name], after["profiles"][name]
        if old["capabilities"] != new["capabilities"]:
            violations.append(f"{name}: capability drift")
        a = {t["name"]: t for t in old["tools"]}
        b = {t["name"]: t for t in new["tools"]}
        if a.keys() != b.keys():
            violations.append(f"{name}: tool-name drift")
        for tool in sorted(a.keys() & b.keys()):
            x, y = dict(a[tool]), dict(b[tool])
            dx, dy = x.pop("description", None), y.pop("description", None)
            if x != y:
                violations.append(f"{name}/{tool}: schema, annotations or other contract drift")
            if dx != dy:
                changes.append(f"{name}/{tool}")
                if tool not in DESCRIPTION_ALLOWLIST:
                    violations.append(f"{name}/{tool}: unapproved description")
    return {"status": "PASS" if not violations else "FAIL", "violations": violations,
            "description_changes": changes,
            "profile_tool_counts": {p: len(after["profiles"][p]["tools"]) for p in PROFILES},
            "full_input_schema_and_annotations_compared": True,
            "client_refresh_verified": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    snap = sub.add_parser("snapshot")
    snap.add_argument("--binary", type=Path, required=True)
    snap.add_argument("--output", type=Path, required=True)
    diff = sub.add_parser("compare")
    diff.add_argument("--before", type=Path, required=True)
    diff.add_argument("--after", type=Path, required=True)
    diff.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "snapshot":
        report = snapshot(args.binary.resolve())
    else:
        report = compare(json.loads(args.before.read_text()), json.loads(args.after.read_text()))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"output": str(args.output.resolve()), "status": report.get("status", "SNAPSHOT"),
                      "profile_tool_counts": report.get("profile_tool_counts")}, ensure_ascii=False))
    return 1 if report.get("status") == "FAIL" else 0


if __name__ == "__main__":
    raise SystemExit(main())
