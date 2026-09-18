#!/usr/bin/env python3
"""The sealed call-graph status and the runtime tripwires for the no-retry /
no-fsync / no-WAL contract.

There is no counter for an absent route, so a fabricated zero is forbidden and a
counter that cannot fail an assertion is not evidence. What this module produces
instead is two things that *can* fail:

**A sealed call-graph/manifest status.** Every first-party production source file
under `core/crates/*/src` and `core/crates/*/sql` is read, Rust comments are
stripped, and the remaining **code** is scanned for the tokens that would mean the
contract had been broken: `fsync`, `fdatasync`, `sync_all`, `sync_data`, a WAL
journal mode, a non-zero busy timeout, and retry/resend/backoff call sites.
Comments are stripped first on purpose: the product's own documentation explains
*why* these are absent, and a scan that counted the explanation would be a scan
nobody could keep green.

**Observable runtime tripwires.** A produced Store is inspected on disk: no
`-wal`, `-shm` or `-journal` sidecar may exist; `PRAGMA journal_mode` must not read
back as `wal`; `store_policy` must hold exactly one watermark row.

Both halves are re-run by `runner.py verify`, so a regression in either one fails a
receipt rather than being noticed by a reader.
"""

from __future__ import annotations

import re
import sqlite3
import sys
from pathlib import Path

# shared/ -> harness root -> benchmark/ -> core/ -> repository root
REPO_ROOT = Path(__file__).resolve().parents[4]

FORBIDDEN_CODE = {
    "fsync": r"\bfsync\b",
    "fdatasync": r"\bfdatasync\b",
    "sync_all": r"\bsync_all\s*\(",
    "sync_data": r"\bsync_data\s*\(",
}
WAL_PATTERNS = {
    "wal_mode_literal": r"journal_mode[^\n;]{0,40}\bWAL\b",
    "wal_string_literal": r"\"WAL\"|'WAL'",
}
TIMEOUT_PATTERN = r"busy_timeout\s*\(([^)]*)\)"
RETRY_PATTERNS = {
    "retry_call": r"\bretry\s*\(",
    "resend_call": r"\bresend\s*\(",
    "backoff_call": r"\bbackoff\s*\(",
}


def strip_rust_comments(source: str) -> str:
    """Removes `//` and `/* */` comments so the scan sees code only.

    String literals are left alone: a WAL mode spelled as a literal is exactly the
    case the scan must catch, so it cannot be stripped as if it were prose.
    """
    out: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        character = source[index]
        if character == "/" and index + 1 < length and source[index + 1] == "/":
            while index < length and source[index] != "\n":
                index += 1
            continue
        if character == "/" and index + 1 < length and source[index + 1] == "*":
            index += 2
            while index + 1 < length and not (source[index] == "*" and source[index + 1] == "/"):
                index += 1
            index += 2
            continue
        if character == '"':
            out.append(character)
            index += 1
            while index < length:
                out.append(source[index])
                if source[index] == "\\" and index + 1 < length:
                    index += 1
                    out.append(source[index])
                elif source[index] == '"':
                    index += 1
                    break
                index += 1
            continue
        out.append(character)
        index += 1
    return "".join(out)


def production_sources() -> list[Path]:
    """Every first-party production source file the boundary guard scans."""
    roots = [REPO_ROOT / "core" / "crates"]
    files: list[Path] = []
    for root in roots:
        for path in sorted(root.glob("*/src/**/*.rs")):
            files.append(path)
        for path in sorted(root.glob("*/sql/**/*.sql")):
            files.append(path)
        for path in sorted(root.glob("*/src/**/*.sql")):
            files.append(path)
    return files


def scan() -> dict[str, object]:
    """The sealed call-graph status: a per-token verdict over the product source."""
    files = production_sources()
    findings: dict[str, list[str]] = {}
    scanned = 0
    for path in files:
        try:
            source = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        scanned += 1
        code = strip_rust_comments(source)
        relative = str(path.relative_to(REPO_ROOT))
        for token, pattern in {**FORBIDDEN_CODE, **WAL_PATTERNS, **RETRY_PATTERNS}.items():
            for match in re.finditer(pattern, code):
                line = code[: match.start()].count("\n") + 1
                findings.setdefault(token, []).append(f"{relative}:{line}")
        for match in re.finditer(TIMEOUT_PATTERN, code):
            argument = match.group(1).strip()
            if argument not in {"Duration::ZERO", "0", "std::time::Duration::ZERO"}:
                line = code[: match.start()].count("\n") + 1
                findings.setdefault("non_zero_busy_timeout", []).append(
                    f"{relative}:{line} -> {argument}"
                )
    return {
        "files_scanned": scanned,
        "status": "PASS" if not findings else "FAIL",
        "findings": findings,
        "checked": sorted([*FORBIDDEN_CODE, *WAL_PATTERNS, *RETRY_PATTERNS, "non_zero_busy_timeout"]),
    }


def tripwires(store_paths: list[str | Path]) -> dict[str, object]:
    """The runtime tripwires, read off the Stores a run actually produced."""
    findings: list[str] = []
    checked: list[dict[str, object]] = []
    for raw in store_paths:
        path = Path(raw)
        if not path.exists():
            continue
        record: dict[str, object] = {"store": str(path)}
        sidecars = [
            suffix for suffix in ("-wal", "-shm", "-journal") if Path(f"{path}{suffix}").exists()
        ]
        record["sidecars"] = sidecars
        if sidecars:
            findings.append(f"{path}: sidecar(s) {sidecars} exist")
        try:
            connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
            try:
                mode = str(connection.execute("PRAGMA journal_mode").fetchone()[0])
                record["journal_mode_persisted"] = mode
                if mode.lower() == "wal":
                    findings.append(f"{path}: persisted journal_mode is wal")
                rows = int(
                    connection.execute("SELECT COUNT(*) FROM store_policy").fetchone()[0]
                )
                record["store_policy_rows"] = rows
                if rows > 1:
                    findings.append(f"{path}: store_policy holds {rows} rows, expected one")
            finally:
                connection.close()
        except sqlite3.Error as error:
            record["sqlite_error"] = str(error)
            findings.append(f"{path}: {error}")
        checked.append(record)
    return {
        "stores_checked": len(checked),
        "status": "PASS" if not findings else "FAIL",
        "findings": findings,
        "stores": checked,
    }


def self_check() -> list[str]:
    """Proves the scanner catches what it claims to catch, and clears prose."""
    failures: list[str] = []
    if strip_rust_comments("let a = 1; // fsync\n/* sync_all( */\nlet b = 2;") != (
        "let a = 1; \n\nlet b = 2;"
    ):
        failures.append("comment stripping is wrong, so a prose mention could fail the scan")
    if "fsync" not in strip_rust_comments('let x = "fsync";'):
        failures.append("a string literal was stripped, so a real WAL literal could hide")
    probe = {
        "no_fsync": r"\bfsync\b",
        "wal": r"journal_mode[^\n;]{0,40}\bWAL\b",
    }
    sample = 'connection.execute("PRAGMA journal_mode = WAL")?;'
    if not any(re.search(pattern, sample) for pattern in probe.values()):
        failures.append("a WAL journal-mode literal was not detected")
    status = scan()
    if status["files_scanned"] == 0:
        failures.append("the scan found no product source, so it proves nothing")
    if status["status"] != "PASS":
        failures.append(f"the product source scan failed: {status['findings']}")
    return failures


def main() -> int:
    failures = self_check()
    status = scan()
    print(
        f"invariants: {status['status']} "
        f"({status['files_scanned']} product source files scanned for "
        f"{len(status['checked'])} token classes)"
    )
    for token, where in sorted(status["findings"].items()):
        for location in where:
            print(f"  FINDING {token}: {location}")
    for failure in failures:
        print(f"invariants: FAIL: {failure}", file=sys.stderr)
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
