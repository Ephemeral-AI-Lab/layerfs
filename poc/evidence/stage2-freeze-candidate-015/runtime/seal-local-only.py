#!/usr/bin/env python3
"""Regenerate and verify candidate 015's local-only SHA-256 custody manifest."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import sys
from pathlib import Path

VERIFY_RECEIPTS = {
    "SHA256SUMS.verify.stdout",
    "SHA256SUMS.verify.stderr",
    "SHA256SUMS.verify.exit",
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def files(root: Path) -> list[Path]:
    manifest = root / "SHA256SUMS"
    paths = sorted(
        path
        for path in root.rglob("*")
        if path.is_file()
        and path != manifest
        and str(path.relative_to(root)) not in VERIFY_RECEIPTS
    )
    if any(path.is_symlink() for path in paths):
        raise SystemExit("evidence seal refuses symlinks")
    return paths


def generate(root: Path) -> None:
    verifier = root / "runtime/verify-local-comparison.py"
    output = root / "local-comparison.json"
    result = subprocess.run(
        [sys.executable, str(verifier), str(root), "--output", str(output)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    (root / "local-comparison.verify.stdout").write_bytes(result.stdout)
    (root / "local-comparison.verify.stderr").write_bytes(result.stderr)
    (root / "local-comparison.verify.exit").write_text(f"{result.returncode}\n")
    if result.returncode or result.stdout != output.read_bytes() or result.stderr:
        raise SystemExit("local comparison verifier did not pass exactly")
    lines = [f"{digest(path)}  ./{path.relative_to(root)}\n" for path in files(root)]
    (root / "SHA256SUMS").write_text("".join(lines))
    print(f"GENERATED {len(lines)}")


def verify_count(root: Path) -> int:
    manifest = root / "SHA256SUMS"
    entries: dict[str, str] = {}
    for line in manifest.read_text().splitlines():
        expected, name = line.split("  ./", 1)
        if not re_full_sha256(expected) or name in entries or "\n" in name:
            raise SystemExit(f"invalid manifest entry: {line!r}")
        entries[name] = expected
    actual = {str(path.relative_to(root)): digest(path) for path in files(root)}
    if entries != actual:
        missing = sorted(set(actual) - set(entries))
        extra = sorted(set(entries) - set(actual))
        changed = sorted(name for name in set(entries) & set(actual) if entries[name] != actual[name])
        raise SystemExit(f"SHA256SUMS mismatch: missing={missing} extra={extra} changed={changed}")
    return len(entries)


def verify(root: Path) -> None:
    print(f"VERIFIED {verify_count(root)}")


def record_verification(root: Path) -> None:
    output = f"VERIFIED {verify_count(root)}\n"
    (root / "SHA256SUMS.verify.stdout").write_text(output)
    (root / "SHA256SUMS.verify.stderr").write_bytes(b"")
    (root / "SHA256SUMS.verify.exit").write_text("0\n")
    print(output, end="")


def re_full_sha256(value: str) -> bool:
    return len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("generate", "verify", "record-verify"))
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    if args.action == "generate":
        generate(root)
    elif args.action == "record-verify":
        record_verification(root)
    else:
        verify(root)


if __name__ == "__main__":
    main()
