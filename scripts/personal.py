#!/usr/bin/env python3
"""Local personal-build entry point. Never stops, rewrites or starts the legacy app."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from personal_env import toolchain_environment

ROOT = Path(__file__).resolve().parents[1]

def environment(root: Path = ROOT) -> dict[str, str]:
    env = toolchain_environment()
    env.update({
        "CODING_TOOLS_PERSONAL_HOME": str(root / ".personal-home"),
        "CODING_TOOLS_PERSONAL_IMPORT": "off",
        "CARGO_TARGET_DIR": str(root / ".artifacts" / "cargo"),
        "CARGO_BUILD_JOBS": "2",
    })
    return env

def executable(root: Path = ROOT) -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    return root / ".artifacts" / "cargo" / "debug" / ("coding-tools-mcp-personal" + suffix)

def source_candidates() -> list[Path]:
    home = Path.home()
    if platform.system() == "Darwin":
        base = home / "Library" / "Application Support"
    elif os.name == "nt":
        base = Path(os.environ.get("APPDATA", home / "AppData" / "Roaming"))
    else:
        base = Path(os.environ.get("XDG_CONFIG_HOME", home / ".config"))
    return [base / "coding-tools-mcp-desktop" / "data" / "profiles.json",
            home / ".coding-tools-mcp-desktop" / "profiles.json"]

def run(command: list[str], timeout: int = 600) -> int:
    try:
        return subprocess.run(command, cwd=ROOT, env=environment(), check=False, timeout=timeout).returncode
    except FileNotFoundError as error:
        print(f"Missing executable: {error.filename}", file=sys.stderr)
        return 2
    except subprocess.TimeoutExpired:
        print("Command timeout; inspect its result before retrying.", file=sys.stderr)
        return 124

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("build", help="build into the private target directory; do not launch")
    imp = commands.add_parser("import-config", help="copy legacy profiles into the new personal home only")
    imp.add_argument("--source", type=Path)
    commands.add_parser("start", help="explicitly launch the personal app; never stop the old one")
    commands.add_parser("info", help="show paths without reading credential values")
    args = parser.parse_args(argv)
    if args.command == "info":
        print(json.dumps({"source":str(ROOT),"executable":str(executable()),
            "personal_home":environment()["CODING_TOOLS_PERSONAL_HOME"],"legacy_modified":False},ensure_ascii=False,indent=2))
        return 0
    if args.command == "build":
        for command in (["cargo","build","--manifest-path","personal-runtime/Cargo.toml"],
                        ["npm","run","build"],
                        ["cargo","build","--manifest-path","src-tauri/Cargo.toml"]):
            code = run(command)
            if code: return code
        return 0
    binary = executable()
    if not binary.is_file():
        print("Run `python3 scripts/personal.py build` first.", file=sys.stderr)
        return 2
    if args.command == "import-config":
        source = args.source
        if source is None:
            source = next((p for p in source_candidates() if p.is_file()), None)
        if source is None:
            print("No supported legacy profiles.json found; provide --source with its absolute path.", file=sys.stderr)
            return 2
        if not source.is_absolute():
            print("--source must be an absolute path.", file=sys.stderr)
            return 2
        # The native import CLI exits before Tauri initialization or listener startup.
        return run([str(binary),"--personal-import-config",str(source)],60)
    # Launch is deliberately never a side effect of build, import or tests.
    process = subprocess.Popen([str(binary)], cwd=ROOT, env=environment())
    print(json.dumps({"personal_pid":process.pid,"old_service_stopped":False}))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
