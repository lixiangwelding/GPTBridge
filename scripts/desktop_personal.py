#!/usr/bin/env python3
"""Prepare/install only the personal Desktop plugin through supported Codex CLI."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
NAME = "gptbridge-plugin"
MARKET = "gptbridge-plugin"
APP = "GPTBridge.app"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_new(path: Path, data: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink():
        raise ValueError("refuse symbolic-link destination")
    if path.exists():
        if path.read_text() != data:
            raise ValueError("existing generated file changed; preserved without overwrite")
        return
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as stream:
        stream.write(data)


def plugin_files(profile: str, executable: Path, transport: str) -> dict[str, str]:
    if not profile or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in profile):
        raise ValueError("invalid profile identifier")
    if transport not in {"registered", "stdio"} or not executable.is_absolute():
        raise ValueError("invalid transport or executable")
    source = ROOT / "desktop-plugin"
    files = {str(p.relative_to(source)): p.read_text() for p in source.rglob("*") if p.is_file()}
    manifest = json.loads(files[".codex-plugin/plugin.json"])
    if transport == "stdio":
        manifest.pop("apps", None)
        files.pop(".app.json", None)
        manifest["mcpServers"] = "./.mcp.json"
        files[".mcp.json"] = json.dumps({"mcpServers": {"gptbridge": {
            "command": str(executable), "args": ["--personal-stdio", profile]}}}, indent=2) + "\n"
    files[".codex-plugin/plugin.json"] = json.dumps(manifest, ensure_ascii=False, indent=2) + "\n"
    return files


def prepare(home: Path, profile: str, transport: str) -> dict:
    app = home / "Applications" / APP
    metadata = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    if metadata.get("CFBundleIdentifier") != "com.lixiangwelding.codingtools.personal":
        raise ValueError("unexpected installed app identity")
    version = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"]
    if metadata.get("CFBundleShortVersionString") != version:
        raise ValueError("install the matching application version before plugin registration")
    name = metadata["CFBundleExecutable"]
    if not isinstance(name, str) or Path(name).name != name:
        raise ValueError("invalid bundle executable")
    executable = app / "Contents/MacOS" / name
    config = home / "Library/Application Support/coding-tools-mcp-personal/data/profiles.json"
    data = json.loads(config.read_text())
    if len([p for p in data.get("profiles", []) if p.get("id") == profile]) != 1:
        raise ValueError("profile must identify exactly one saved workspace")
    destination = home / "Library/Application Support/GPTBridge/desktop-marketplace" / (version + "-" + transport + "-" + NAME)
    files = plugin_files(profile, executable, transport)
    if json.loads(files[".codex-plugin/plugin.json"]).get("version") != version:
        raise ValueError("plugin manifest version does not match installed application")
    for relative, content in files.items():
        write_new(destination / "plugins" / NAME / relative, content)
    marketplace = {"name": MARKET, "interface": {"displayName": "GPTBridgePlugin"}, "plugins": [{
        "name": NAME, "source": {"source": "local", "path": "./plugins/" + NAME},
        "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"}, "category": "Developer Tools"}]}
    write_new(destination / ".agents/plugins/marketplace.json", json.dumps(marketplace, ensure_ascii=False, indent=2) + "\n")
    return {"version": version, "transport": transport, "marketplace": str(destination),
            "plugin": str(destination / "plugins" / NAME), "profile_id": profile,
            "executable": str(executable), "config_sha256": digest(config),
            "display_name": "GPTBridgePlugin", "desktop_restarted": False}


def run_cli(command: list[str], receipt: Path, timeout: int = 75) -> dict:
    process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    try:
        out, err = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            out, err = process.communicate(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            out, err = process.communicate()
        code = 124
    else:
        code = process.returncode
    write_new(receipt.with_suffix(".stdout"), out.decode("utf-8", "replace"))
    write_new(receipt.with_suffix(".stderr"), err.decode("utf-8", "replace"))
    return {"exit_code": code, "stdout_path": str(receipt.with_suffix(".stdout")),
            "stderr_path": str(receipt.with_suffix(".stderr"))}


def install(home: Path, profile: str, transport: str, cli: Path) -> dict:
    # Parse before any mutation; Python >=3.11 supplies a standard TOML parser.
    import tomllib
    config = home / ".codex/config.toml"
    before_bytes = config.read_bytes()
    before = tomllib.loads(before_bytes.decode())
    result = prepare(home, profile, transport)
    run = home / "Library/Application Support/GPTBridge/install-receipts" / ("install-" + str(time.time_ns()))
    run.mkdir(parents=True, mode=0o700)
    write_new(run / "codex-config-before.toml", before_bytes.decode())
    result["configuration_backup"] = str(run / "codex-config-before.toml")
    steps = [([str(cli), "plugin", "marketplace", "add", result["marketplace"], "--json"], "marketplace"),
             ([str(cli), "plugin", "add", NAME + "@" + MARKET, "--json"], "install"),
             ([str(cli), "plugin", "list", "--marketplace", MARKET, "--json"], "verify")]
    result["steps"] = {}
    for command, label in steps:
        result["steps"][label] = run_cli(command, run / label)
        if result["steps"][label]["exit_code"]:
            result["status"] = "cli_step_failed"
            break
    else:
        result["status"] = "cli_installed"
    after = tomllib.loads(config.read_text())
    # Preserve independent concurrent changes instead of restoring an old snapshot.
    for value in [before, after]:
        value.get("marketplaces", {}).pop(MARKET, None)
        value.get("plugins", {}).pop(NAME + "@" + MARKET, None)
    result["unrelated_config_equal"] = before == after
    result["composer_visibility_verified"] = False
    result["receipt"] = str(run / "result.json")
    write_new(run / "result.json", json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["prepare", "install"])
    parser.add_argument("--profile", required=True)
    parser.add_argument("--transport", choices=["registered", "stdio"], default="registered")
    parser.add_argument("--cli", type=Path, default=Path("/Applications/ChatGPT.app/Contents/Resources/codex"))
    args = parser.parse_args()
    try:
        result = install(Path.home(), args.profile, args.transport, args.cli) if args.action == "install" else prepare(Path.home(), args.profile, args.transport)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0 if result.get("status", "cli_installed") == "cli_installed" and result.get("unrelated_config_equal", True) else 1
    except (OSError, ValueError, ImportError) as error:
        print(json.dumps({"ok": False, "error_type": type(error).__name__, "message": str(error) if isinstance(error, ValueError) else "local operation failed; inspect private receipt"}))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
