#!/usr/bin/env python3
"""Replace only the personal macOS app; preserve live data and never stop services.

Default: read-only plan. --apply stages and verifies the app, backs up data and
moves the previous app aside for rollback. No legacy-app migration, process
termination, listener startup, permission change or credential output occurs.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import sys
import time
from typing import Callable
try:
    import fcntl
except ImportError:
    fcntl = None

ROOT = Path(__file__).resolve().parents[1]
BUNDLE_ID = "com.lixiangwelding.codingtools.personal"
APP_NAME = "Coding Tools MCP Personal.app"


def sha(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def manifest(root: Path, *, internal_links: bool = False) -> dict[str, str]:
    if root.is_symlink() or not root.is_dir():
        raise ValueError("selected directory is absent or a symlink")
    canonical = root.resolve()
    found: dict[str, str] = {}
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            path = Path(directory) / name
            relative = str(path.relative_to(root))
            if path.is_symlink():
                if not internal_links or not path.resolve(strict=True).is_relative_to(canonical):
                    raise ValueError("unsafe symbolic link in selected tree")
                found[relative] = "link:" + os.readlink(path)
            elif path.is_file():
                if not stat.S_ISREG(path.stat().st_mode):
                    raise ValueError("non-regular file in selected tree")
                found[relative] = sha(path)
            elif not path.is_dir():
                raise ValueError("unsupported entry in selected tree")
    return found


def tree_digest(value: dict[str, str]) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def metadata(app: Path, version: str | None = None) -> dict:
    path = app / "Contents/Info.plist"
    if path.is_symlink() or path.stat().st_size > 1024 * 1024:
        raise ValueError("invalid application metadata")
    data = plistlib.loads(path.read_bytes())
    if data.get("CFBundleIdentifier") != BUNDLE_ID:
        raise ValueError("unexpected application identity")
    if version is not None and data.get("CFBundleShortVersionString") != version:
        raise ValueError("build version does not match the source manifest")
    executable = data.get("CFBundleExecutable")
    if not isinstance(executable, str) or executable in {"", ".", ".."} or any(c in executable for c in "/\\"):
        raise ValueError("invalid bundle executable")
    return data


def save(path: Path, value: dict) -> None:
    temp = path.with_suffix(".tmp")
    with temp.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temp, path)


def native_check(app: Path, home: Path, version: str) -> dict:
    info = metadata(app, version)
    executable = app / "Contents/MacOS" / info["CFBundleExecutable"]
    env = os.environ.copy()
    env.update(CODING_TOOLS_PERSONAL_HOME=str(home), CODING_TOOLS_PERSONAL_IMPORT="off")
    reports = []
    for flag in ["--personal-check-config", "--personal-tool-contract"]:
        process = subprocess.run([str(executable), flag], env=env, capture_output=True, timeout=60, check=False)
        if process.returncode or len(process.stdout) > 2 * 1024 * 1024:
            raise ValueError("native validation failed; private output withheld")
        report = json.loads(process.stdout)
        if report.get("services_started") is not False or report.get("configuration_written") is not False:
            raise ValueError("native validation did not confirm read-only operation")
        reports.append(report)
    config, contract = reports
    fields = set(contract.get("input_fields", {}).get("apply_patch", []))
    if not config.get("ok") or not config.get("native_deserialization"):
        raise ValueError("native configuration parsing failed")
    if contract.get("version") != version or contract.get("tool_count") != 33 or not {"request_id", "expected_hashes"}.issubset(fields):
        raise ValueError("native tool contract does not include the managed-write fields")
    return {"profiles": config["profiles"], "tool_count": contract["tool_count"],
            "schema_sha256": contract["schema_sha256"], "services_started": False,
            "configuration_written": False, "native_deserialization": True}


def _upgrade_locked(source: Path, target: Path, home: Path, backups: Path, version: str,
            *, apply: bool = False, validator: Callable = native_check) -> dict:
    for path in [source, target, home, target.parent, backups]:
        if path.is_symlink():
            raise ValueError("upgrade paths must not be symbolic links")
    roots = [source.resolve(), target.resolve(), home.resolve()]
    if any(a == b or a.is_relative_to(b) or b.is_relative_to(a) for i, a in enumerate(roots) for b in roots[i + 1:]):
        raise ValueError("source, target and configuration paths must be disjoint")
    if any(backups.resolve().is_relative_to(path) for path in roots):
        raise ValueError("backup path must not be inside an input directory")
    new_info, old_info = metadata(source, version), metadata(target)
    source_files = manifest(source, internal_links=True)
    old_files = manifest(target, internal_links=True)
    data = home / "data"
    data_files = manifest(data)
    if "profiles.json" not in data_files:
        raise ValueError("personal configuration is missing")
    report = {"version": version, "previous_version": old_info["CFBundleShortVersionString"],
        "target_app": str(target), "configuration": str(data / "profiles.json"),
        "configuration_sha256": data_files["profiles.json"],
        "source_app_digest": tree_digest(source_files), "previous_app_digest": tree_digest(old_files),
        "legacy_modified": False, "configuration_written": False, "services_stopped": False,
        "services_started": False, "running_process_restarted": False,
        "client_catalog_synchronized": None, "live_service_version_verified": False}
    if not apply:
        return {**report, "status": "plan", "app_files_will_change": source_files != old_files}
    if source_files == old_files:
        checked = validator(target, home, version)
        # A native check can overlap another build or a live configuration edit.
        # No replacement does not mean that the pre-check snapshot is still true.
        # Refuse stale success without restoring or moving another writer's data.
        executable_sha = sha(target / "Contents/MacOS" / new_info["CFBundleExecutable"])
        if manifest(data) != data_files or manifest(target, internal_links=True) != old_files or manifest(source, internal_links=True) != source_files:
            raise ValueError("input changed during already-installed validation; verification stopped")
        return {**report, "status": "already_installed", "native_check": checked,
                "exact_configuration_preserved": True, "executable_sha256": executable_sha}
    identifier = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + "-" + str(time.time_ns())
    backup = backups / identifier
    backup.mkdir(parents=True, mode=0o700)
    backup.chmod(0o700)
    stage = target.parent / (".personal-upgrade-" + identifier + ".app")
    previous = backup / "previous.app"
    receipt = backup / "upgrade.json"
    report.update(status="preparing", backup=str(backup), receipt=str(receipt), staging_app=str(stage))
    save(receipt, report)
    moved_old = installed_new = False
    try:
        shutil.copytree(data, backup / "personal-data", copy_function=shutil.copy2)
        for path in (backup / "personal-data").rglob("*"):
            if path.is_file():
                path.chmod(0o600)
        if manifest(backup / "personal-data") != data_files:
            raise ValueError("configuration backup verification failed")
        shutil.copytree(source, stage, copy_function=shutil.copy2, symlinks=True)
        if manifest(stage, internal_links=True) != source_files:
            raise ValueError("staged application differs from source")
        checked = validator(stage, home, version)
        # Refuse to replace a concurrently changed app/configuration. Never
        # restore live data from an older backup: this upgrader does not own it.
        if manifest(data) != data_files or manifest(target, internal_links=True) != old_files or manifest(source, internal_links=True) != source_files:
            raise ValueError("input changed before replacement; upgrade stopped")
        target.rename(previous)
        moved_old = True
        report["status"] = "previous_app_backed_up"
        save(receipt, report)
        stage.rename(target)
        installed_new = True
        checked = validator(target, home, version)
        if manifest(target, internal_links=True) != source_files or manifest(data) != data_files:
            raise ValueError("post-replacement verification failed")
        report.update(status="installed_app_files", exact_configuration_preserved=True,
            native_check=checked, executable_sha256=sha(target / "Contents/MacOS" / new_info["CFBundleExecutable"]))
        save(receipt, report)
        return report
    except BaseException as error:
        rollback_complete = not moved_old
        if installed_new and target.exists():
            # Do not move an app that another writer replaced after this attempt.
            if manifest(target, internal_links=True) == source_files:
                target.rename(backup / "failed-candidate.app")
                installed_new = False
        if moved_old and not target.exists():
            previous.rename(target)
            rollback_complete = True
        report.update(status="failed", error_type=type(error).__name__, rollback_complete=rollback_complete,
                      live_data_restored=False)
        save(receipt, report)
        raise


def upgrade(source: Path, target: Path, home: Path, backups: Path, version: str,
            *, apply: bool = False, validator: Callable = native_check) -> dict:
    # Validate paths/identity before creating any host-side lock or backup.
    plan = _upgrade_locked(source, target, home, backups, version, apply=False, validator=validator)
    if not apply:
        return plan
    if fcntl is None:
        raise ValueError("this installer requires POSIX advisory locks")
    backups.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd = os.open(backups / ".upgrade.lock", os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0), 0o600)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError("another personal-app upgrade is in progress") from error
        # Re-read all inputs after acquiring the inter-process lock.
        return _upgrade_locked(source, target, home, backups, version, apply=True, validator=validator)
    finally:
        os.close(fd)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this entry point targets the user's personal macOS installation")
    os.umask(0o077)
    support = Path.home() / "Library/Application Support"
    version = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"]
    try:
        report = upgrade(ROOT / ".artifacts/cargo/debug/bundle/macos" / APP_NAME,
                         Path.home() / "Applications" / APP_NAME,
                         support / "coding-tools-mcp-personal", support / "coding-tools-mcp-personal-backups", version,
                         apply=args.apply)
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Upgrade stopped ({type(error).__name__}); inspect the local receipt before retrying.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
