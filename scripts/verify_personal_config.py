#!/usr/bin/env python3
"""Verify copy-only profile inheritance without printing secrets or starting services."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import sys
import time

from personal import ROOT, source_candidates

AUTO_KEYS = {"auto_start", "autostart", "autoStart", "start_on_launch", "startOnLaunch", "launch_at_login"}


def expected_copy(source: dict) -> dict:
    result = copy.deepcopy(source)

    def disable(value):
        if isinstance(value, dict):
            for key in value:
                if key in AUTO_KEYS:
                    value[key] = False
                else:
                    disable(value[key])
        elif isinstance(value, list):
            for item in value:
                disable(item)

    disable(result)
    profiles = result.get("profiles")
    if not isinstance(profiles, list) or any(not isinstance(p, dict) for p in profiles):
        raise ValueError("unsupported profiles structure")
    used = set()
    for profile in profiles:
        for key in ("runtime", "actions"):
            section = profile.get(key)
            if isinstance(section, dict) and isinstance(section.get("local_port"), int):
                used.add(section["local_port"])
    port = 38766
    for profile in profiles:
        for key in ("runtime", "actions"):
            if not isinstance(profile.get(key), dict):
                profile[key] = {}
            while port in used:
                port += 1
            if port > 65000:
                raise ValueError("port range exhausted")
            profile[key].update(local_port=port, runtime_command="")
            used.add(port)
            port += 1
        if not isinstance(profile.get("tunnel"), dict):
            profile["tunnel"] = {}
        profile["tunnel"].update(type="none", public_url="")
        profile["actions"].update(tunnel_type="none", public_url="")
        upstreams = profile["runtime"].get("upstream_mcps")
        if isinstance(upstreams, list):
            for upstream in upstreams:
                upstream["enabled"] = False
    return result


def verify(source: Path, destination: Path) -> dict:
    for path in (source, destination):
        if not path.is_file() or path.is_symlink() or path.stat().st_size > 16 * 1024 * 1024:
            raise ValueError("configuration must be a bounded regular file")
    if source.resolve() == destination.resolve():
        raise ValueError("source and destination must differ")
    original, copied = source.read_bytes(), destination.read_bytes()
    source_value = json.loads(original)
    destination_value = json.loads(copied)
    # First personal release inserted null for absent upstream_mcps. Native loading
    # now accepts that old representation; new imports preserve field absence.
    comparable = copy.deepcopy(destination_value)
    if isinstance(source_value, dict) and isinstance(comparable, dict):
        for original_profile, copied_profile in zip(source_value.get("profiles", []), comparable.get("profiles", [])):
            original_runtime = original_profile.get("runtime", {})
            copied_runtime = copied_profile.get("runtime", {})
            if isinstance(original_runtime, dict) and isinstance(copied_runtime, dict) and "upstream_mcps" not in original_runtime and copied_runtime.get("upstream_mcps") is None:
                copied_runtime.pop("upstream_mcps", None)
    if not isinstance(source_value, dict) or expected_copy(source_value) != comparable:
        raise ValueError("copied configuration differs from the documented safe transformation")
    if source.read_bytes() != original or destination.read_bytes() != copied:
        raise ValueError("configuration changed during verification")
    profiles = destination_value["profiles"]
    return {
        "verified": True,
        "source_sha256": hashlib.sha256(original).hexdigest(),
        "destination_sha256": hashlib.sha256(copied).hexdigest(),
        "profiles": len(profiles),
        "local_ports": [p[key]["local_port"] for p in profiles for key in ("runtime", "actions")],
        "other_settings_and_credentials_match": True,
        "source_stable_during_verification": True,
        "source_written_by_verifier": False,
        "services_started": False,
        "tunnels_enabled": False,
        "upstreams_enabled": False,
        "note": "Ports are not reserved. This compares the current source snapshot, not an unrecorded earlier snapshot.",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path)
    args = parser.parse_args()
    source = args.source or next((path for path in source_candidates() if path.is_file()), None)
    if source is None:
        print("No supported source configuration found.", file=sys.stderr)
        return 2
    try:
        result = verify(source, ROOT / ".personal-home" / "data" / "profiles.json")
    except (OSError, ValueError, TypeError, KeyError) as error:
        # JSON parse errors and path errors can include private contents: never print them.
        print(f"Configuration verification failed ({type(error).__name__}); no source was modified.", file=sys.stderr)
        return 1
    directory = ROOT / ".artifacts" / "config-verification"
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    report = directory / f"{time.time_ns()}.json"
    report.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({**result, "report": str(report)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
