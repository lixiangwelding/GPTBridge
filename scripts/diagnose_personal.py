#!/usr/bin/env python3
"""Bounded, read-only MCP audit summary. Never exports inputs, outputs or secrets."""
from __future__ import annotations
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import sqlite3
import time

ROOT = Path(__file__).resolve().parents[1]


def percentile(values: list[int], fraction: float) -> int | None:
    if not values:
        return None
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]


def error_category(code: str | None) -> str:
    if code in {"RESOURCE_BUSY", "QUEUE_FULL"}:
        return "contention_or_capacity"
    if code in {"POLICY_REJECTED", "EXECUTABLE_OUTSIDE_WORKSPACE", "ABSOLUTE_PATH_DENIED", "PATH_OUTSIDE_WORKSPACE"}:
        return "policy_boundary_not_automatically_a_bug"
    if code in {"STALE_FILE", "PATCH_FAILED", "STALE_CHECKPOINT", "IDEMPOTENCY_CONFLICT", "GOAL_CONFLICT"}:
        return "conflicting_or_stale_input"
    if code in {"NOT_FOUND", "JOB_NOT_FOUND", "TASK_NOT_FOUND"}:
        return "missing_or_wrong_scope"
    if code is None:
        return "command_outcome_requires_inspection"
    return "contract_or_runtime_error"


def audit_summary(path: Path, *, hours: int = 48, limit: int = 20_000,
                  now_ms: int | None = None) -> dict:
    if not 1 <= hours <= 168 or not 1 <= limit <= 50_000:
        raise ValueError("hours must be 1..168 and limit 1..50000")
    observed = int(time.time() * 1000) if now_ms is None else now_ms
    since = observed - hours * 3_600_000
    deadline = time.monotonic() + 10
    # mode=ro fails on absent files rather than silently creating a new database.
    connection = sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True, timeout=2)
    try:
        connection.execute("PRAGMA query_only=ON")
        connection.set_progress_handler(lambda: int(time.monotonic() > deadline), 1000)
        rows = connection.execute(
            "SELECT tool_name,error_code,is_error,duration_ms,started_at_ms,status "
            "FROM audit_records WHERE record_type='tool' AND started_at_ms>=? "
            "AND started_at_ms<=? ORDER BY started_at_ms DESC LIMIT ?",
            (since, observed, limit + 1)).fetchall()
    finally:
        connection.close()
    truncated = len(rows) > limit
    rows = rows[:limit]
    timings: dict[str, list[int]] = defaultdict(list)
    failures: Counter = Counter()
    failed = 0
    for tool, code, is_error, duration, _started, status in rows:
        tool = str(tool or "unknown")
        if isinstance(duration, int) and duration >= 0:
            timings[tool].append(duration)
        if is_error:
            failed += 1
            failures[(tool, code, status)] += 1
    latency = [{"tool": tool, "count": len(values), "p50_ms": percentile(values, .50),
                "p95_ms": percentile(values, .95), "p99_ms": percentile(values, .99),
                "max_ms": max(values)} for tool, values in timings.items()]
    latency.sort(key=lambda row: (-row["p95_ms"], row["tool"]))
    errors = [{"tool": tool, "error_code": code, "status": status, "count": count,
               "category": error_category(code)}
              for (tool, code, status), count in failures.most_common(40)]
    return {"generated_at": datetime.fromtimestamp(observed / 1000, timezone.utc).isoformat(),
            "window_hours": hours, "requested_since_ms": since, "observed_at_ms": observed,
            "sampled_calls": len(rows), "sampled_error_outcomes": failed,
            "latest_calls_limit": limit, "truncated": truncated,
            "earliest_sample_ms": min((row[4] for row in rows), default=None),
            "latency": latency, "errors": errors,
            "state_modified": False, "raw_content_exported": False,
            "caveats": ["Only retained tool audit records are counted, not all HTTP traffic.",
                        "Tool latency includes requested waits; it is not command lifetime.",
                        "Errors and nonzero command outcomes are not automatically MCP defects.",
                        "When truncated, statistics describe only the newest bounded sample."]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audit", type=Path, default=Path.home() / "Library/Application Support/coding-tools-mcp-personal/audit/audit.sqlite")
    parser.add_argument("--hours", type=int, default=48)
    parser.add_argument("--limit", type=int, default=20_000)
    parser.add_argument("--output", type=Path, help="Optional new JSON file inside this repository's .artifacts directory")
    args = parser.parse_args()
    try:
        report = audit_summary(args.audit, hours=args.hours, limit=args.limit)
        encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
        if args.output:
            output = args.output.resolve()
            if not output.is_relative_to((ROOT / ".artifacts").resolve()):
                raise ValueError("output must be inside this repository's .artifacts directory")
            output.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            # Never overwrite another run's receipt or any user configuration.
            import os
            descriptor = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
                stream.write(encoded)
            print(json.dumps({"report": str(output), "sampled_calls": report["sampled_calls"], "truncated": report["truncated"]}))
        else:
            print(encoded, end="")
        return 0
    except (OSError, ValueError, sqlite3.Error) as error:
        # Database errors can contain SQL or paths; do not export private details.
        print(json.dumps({"available": False, "error_type": type(error).__name__, "state_modified": False}))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
