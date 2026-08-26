#!/usr/bin/env python3
"""Derive the admitted offline scenario bodies directly from upstream fs-bench.sh."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path


EXPECTED_SOURCE_SHA256 = "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
ADMITTED = (
    "create 1000 files",
    "stat 1000 files",
    "rm 1000 files",
    "mkdir tree (10x10x10)",
    "find tree",
    "write 64 MiB",
    "copy 64 MiB",
    "read 64 MiB",
    "pure read 64 MiB",
    "pure copy 64 MiB",
    "overwrite 64 MiB",
    "git init + commit 100 files",
)
CALL = re.compile(
    r'run_scenario\s+"(?P<name>[^"]+)"\s+\'(?P<command>.*?)\''
    r"(?:\s+\'(?P<prep>.*?)\')?",
    re.DOTALL,
)


def clean(body: str | None) -> str:
    if body is None:
        return ""
    lines = body.splitlines()
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    return "\n".join(line.rstrip() for line in lines)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: derive-scenario-map.py FS_BENCH_SH")
    source = Path(sys.argv[1]).resolve()
    raw = source.read_bytes()
    source_sha256 = hashlib.sha256(raw).hexdigest()
    if source_sha256 != EXPECTED_SOURCE_SHA256:
        raise SystemExit(f"unexpected fs-bench SHA-256: {source_sha256}")
    text = raw.decode()
    calls = {}
    all_names = []
    for match in CALL.finditer(text):
        name = match.group("name")
        all_names.append(name)
        if name in ADMITTED:
            calls[name] = {
                "name": name,
                "source_line": text.count("\n", 0, match.start()) + 1,
                "command": clean(match.group("command")),
                "prep": clean(match.group("prep")),
            }
    if tuple(calls) != ADMITTED:
        raise SystemExit(f"admitted order/body extraction mismatch: {tuple(calls)}")
    rows = [calls[name] for name in ADMITTED]
    canonical = json.dumps(rows, ensure_ascii=True, separators=(",", ":")).encode()
    excluded = [name for name in all_names if name not in ADMITTED]
    receipt = {
        "schema": "layerfs-stage2-upstream-scenario-map-v1",
        "status": "PASS",
        "source_path": "containers/layerfs-fuse/fs-bench.sh",
        "source_sha256": source_sha256,
        "admitted_count": len(rows),
        "admitted_mapping_sha256": hashlib.sha256(canonical).hexdigest(),
        "network_scenarios_admitted": 0,
        "excluded_scenarios": excluded,
        "scenarios": rows,
    }
    print(json.dumps(receipt, indent=2, ensure_ascii=True))


if __name__ == "__main__":
    main()
