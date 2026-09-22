#!/usr/bin/env python3
"""Compare captured MCP tool catalogs without connecting, refreshing or changing services.

Exit: 0 complete schema match; 1 mismatch; 2 malformed input; 3 fields match
but complete schemas were not supplied. A matching snapshot is not proof that an
already-open client conversation refreshed its tool cache.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
from typing import Any

MAX_INPUT_BYTES = 4 * 1024 * 1024


def catalog(value: Any) -> tuple[dict[str, set[str]], dict[str, Any] | None]:
    if isinstance(value, dict) and isinstance(value.get("result"), dict):
        value = value["result"]
    tools = value if isinstance(value, list) else value.get("tools") if isinstance(value, dict) else None
    if isinstance(tools, list) and tools and all(isinstance(tool, dict) for tool in tools):
        fields: dict[str, set[str]] = {}
        schemas: dict[str, Any] = {}
        for tool in tools:
            name, schema = tool.get("name"), tool.get("inputSchema")
            if not isinstance(name, str) or not name or name in fields:
                raise ValueError("Tool names must be nonempty and unique")
            if not isinstance(schema, dict) or not isinstance(schema.get("properties", {}), dict):
                raise ValueError("Every tool needs an inputSchema object")
            fields[name] = set(schema.get("properties", {}))
            schemas[name] = schema
        return fields, schemas
    if not isinstance(value, dict):
        raise ValueError("Expected a tools/list response, tool array, or input_fields contract")
    contract = value.get("tool_contract", value)
    if not isinstance(contract, dict) or not isinstance(contract.get("input_fields"), dict):
        raise ValueError("A complete nonempty catalog or input_fields contract is required")
    fields = {}
    for name, names in contract["input_fields"].items():
        if not isinstance(name, str) or not name or not isinstance(names, list):
            raise ValueError("Invalid input_fields entry")
        if not all(isinstance(field, str) and field for field in names) or len(names) != len(set(names)):
            raise ValueError("Field names must be nonempty strings without duplicates")
        fields[name] = set(names)
    if not fields:
        raise ValueError("Empty catalogs cannot establish synchronization")
    return fields, None


def compare(expected: Any, observed: Any) -> dict[str, Any]:
    want, want_schema = catalog(expected)
    have, have_schema = catalog(observed)
    common = sorted(want.keys() & have.keys())
    missing_fields = {name: sorted(want[name] - have[name]) for name in common if want[name] - have[name]}
    unexpected_fields = {name: sorted(have[name] - want[name]) for name in common if have[name] - want[name]}
    complete = want_schema is not None and have_schema is not None
    changed_schemas = [name for name in common if complete and want_schema[name] != have_schema[name]]
    missing_tools, unexpected_tools = sorted(want.keys() - have.keys()), sorted(have.keys() - want.keys())
    mismatch = bool(missing_tools or unexpected_tools or missing_fields or unexpected_fields or changed_schemas)
    status = "mismatch" if mismatch else "schema_match" if complete else "fields_match_schema_unverified"
    return {
        "status": status,
        "catalog_snapshot_matches": False if mismatch else True if complete else None,
        "schema_comparison_complete": complete,
        "runtime_client_refresh_verified": False,
        "expected_tool_count": len(want), "observed_tool_count": len(have),
        "missing_tools": missing_tools, "unexpected_tools": unexpected_tools,
        "missing_fields": missing_fields, "unexpected_fields": unexpected_fields,
        "changed_schemas": changed_schemas, "read_only": True,
        "guidance": "After an official client metadata refresh, capture client-visible tools again. Never remove request_id, expected_hashes or permission checks to make a stale catalog pass."
    }


def read_json(path: Path) -> Any:
    with path.open("rb") as stream:
        data = stream.read(MAX_INPUT_BYTES + 1)
    if len(data) > MAX_INPUT_BYTES:
        raise ValueError("Catalog exceeds the 4 MiB input budget")
    return json.loads(data)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--expected", type=Path, required=True)
    parser.add_argument("--observed", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        report = compare(read_json(args.expected), read_json(args.observed))
    except (OSError, ValueError, TypeError):
        print(json.dumps({"status":"invalid_input", "read_only":True,
                          "error":"Cannot read a valid bounded tool catalog; no services or configuration changed."}))
        return 2
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return {"mismatch":1, "schema_match":0, "fields_match_schema_unverified":3}[report["status"]]


if __name__ == "__main__":
    raise SystemExit(main())
