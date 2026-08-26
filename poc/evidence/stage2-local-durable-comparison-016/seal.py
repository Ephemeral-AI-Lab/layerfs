#!/usr/bin/env python3
"""Generate and verify the non-self-referential comparison evidence seal."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


EXCLUDED = {
    "SHA256SUMS",
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
    result = sorted(
        path
        for path in root.rglob("*")
        if path.is_file() and str(path.relative_to(root)) not in EXCLUDED
    )
    if any(path.is_symlink() for path in result):
        raise SystemExit("seal refuses symlinks")
    return result


def verify_count(root: Path) -> int:
    entries = {}
    for line in (root / "SHA256SUMS").read_text().splitlines():
        expected, name = line.split("  ./", 1)
        if len(expected) != 64 or name in entries:
            raise SystemExit(f"invalid manifest entry: {line!r}")
        entries[name] = expected
    actual = {str(path.relative_to(root)): digest(path) for path in files(root)}
    if entries != actual:
        raise SystemExit("SHA256SUMS does not match the evidence tree")
    return len(entries)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("generate", "verify", "record-verify"))
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    if args.action == "generate":
        lines = [f"{digest(path)}  ./{path.relative_to(root)}\n" for path in files(root)]
        (root / "SHA256SUMS").write_text("".join(lines))
        print(f"GENERATED {len(lines)}")
    elif args.action == "record-verify":
        output = f"VERIFIED {verify_count(root)}\n"
        (root / "SHA256SUMS.verify.stdout").write_text(output)
        (root / "SHA256SUMS.verify.stderr").write_bytes(b"")
        (root / "SHA256SUMS.verify.exit").write_text("0\n")
        print(output, end="")
    else:
        print(f"VERIFIED {verify_count(root)}")


if __name__ == "__main__":
    main()
