"""Discover installed developer tools for child processes without changing the host.

GUI-launched MCP processes often inherit a minimal PATH. Preserve explicitly
configured entries, then append known installation directories that exist.
No shell profile is sourced, no tool is installed, and no credentials are read.
"""
from __future__ import annotations

import os
from pathlib import Path
import platform
import shutil
from typing import Mapping


def toolchain_environment(
    base: Mapping[str, str] | None = None,
    *,
    home: Path | None = None,
    system: str | None = None,
) -> dict[str, str]:
    env = dict(os.environ if base is None else base)
    home = Path.home() if home is None else home
    system = platform.system() if system is None else system
    candidates = [home / ".cargo" / "bin"]
    if system == "Darwin":
        candidates += [Path("/opt/homebrew/bin"), Path("/usr/local/bin")]
    elif system == "Windows":
        if env.get("ProgramFiles"):
            candidates.append(Path(env["ProgramFiles"]) / "nodejs")
        if env.get("APPDATA"):
            candidates.append(Path(env["APPDATA"]) / "npm")
    else:
        candidates += [home / ".local" / "bin", Path("/usr/local/bin")]
    paths = [p for p in env.get("PATH", os.defpath).split(os.pathsep) if p]
    for candidate in candidates:
        value = str(candidate)
        if candidate.is_dir() and value not in paths:
            paths.append(value)
    env["PATH"] = os.pathsep.join(dict.fromkeys(paths))
    return env


def toolchain_paths(env: Mapping[str, str]) -> dict[str, str | None]:
    """Only return executable paths, never the environment or credential values."""
    return {name: shutil.which(name, path=env.get("PATH"))
            for name in ("cargo", "rustc", "node", "npm", "npx", "git")}
