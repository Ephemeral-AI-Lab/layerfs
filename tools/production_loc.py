#!/usr/bin/env python3
"""Count first-party production source lines for the per-commit LOC rule.

Production code is nonblank, non-comment first-party product implementation:
the replacement product under core/crates/*/src and the reference product under
crates/*/src, plus the reference SQL that ships with the store. Tests, examples,
benches, fixtures, benchmark harnesses, development tools, docs, manifests and
generated artifacts are excluded. Legacy inline #[cfg(test)] items are removed
before counting, because the reference tree keeps tests inside src/.

Method: comments are blanked by a Rust-aware scanner (line/nested block comments,
normal/raw/byte strings, char literals and lifetimes) and a line counts once when
it still holds non-whitespace. See AGENTS.md, "Production LOC comparison for
every commit".
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

SKIP_DIRS = {"target", "tests", "examples", "benches", "node_modules"}
CODE_SUFFIXES = (".rs",)


def blank_rust(source: str) -> str:
    """Blank comments, keeping every newline so line numbering is unchanged."""
    out = list(source)
    index = 0
    length = len(source)
    block_depth = 0
    while index < length:
        char = source[index]
        if block_depth:
            if source.startswith("/*", index):
                block_depth += 1
                out[index] = out[index + 1] = " "
                index += 2
                continue
            if source.startswith("*/", index):
                block_depth -= 1
                out[index] = out[index + 1] = " "
                index += 2
                continue
            if char != "\n":
                out[index] = " "
            index += 1
            continue
        if source.startswith("//", index):
            while index < length and source[index] != "\n":
                out[index] = " "
                index += 1
            continue
        if source.startswith("/*", index):
            block_depth = 1
            out[index] = out[index + 1] = " "
            index += 2
            continue
        if char == "r" and is_raw_string_start(source, index):
            index = blank_raw_string(source, out, index)
            continue
        if char == '"':
            index = blank_quoted(source, out, index, '"')
            continue
        if char == "'":
            index = skip_char_literal_or_lifetime(source, out, index)
            continue
        index += 1
    return "".join(out)


def is_raw_string_start(source: str, index: int) -> bool:
    if index > 0 and (source[index - 1].isalnum() or source[index - 1] == "_"):
        return False
    cursor = index + 1
    while cursor < len(source) and source[cursor] == "#":
        cursor += 1
    return cursor < len(source) and source[cursor] == '"'


def blank_quoted(source: str, out: list, index: int, quote: str) -> int:
    """Keep literal text, skip escapes, and return the index after the quote."""
    cursor = index + 1
    while cursor < len(source):
        char = source[cursor]
        if char == "\\":
            cursor += 2
            continue
        if char == quote:
            return cursor + 1
        cursor += 1
    return cursor


def blank_raw_string(source: str, out: list, index: int) -> int:
    cursor = index + 1
    hashes = 0
    while source[cursor] == "#":
        hashes += 1
        cursor += 1
    closing = '"' + "#" * hashes
    end = source.find(closing, cursor + 1)
    return len(source) if end < 0 else end + len(closing)


def skip_char_literal_or_lifetime(source: str, out: list, index: int) -> int:
    if source.startswith("'\\", index):
        return blank_quoted(source, out, index, "'")
    if index + 2 < len(source) and source[index + 2] == "'" and source[index + 1] != "'":
        return index + 3
    return index + 1


def blank_inline_tests(code: str) -> str:
    """Blank #[cfg(test)] items and their bodies from legacy product source."""
    out = list(code)
    cursor = 0
    while True:
        found = code.find("#[cfg(", cursor)
        if found < 0:
            break
        end = code.find(")]", found)
        if end < 0:
            break
        attribute = code[found : end + 2]
        cursor = end + 2
        if "test" not in attribute.replace(" ", ""):
            continue
        body = skip_attributes(code, cursor)
        stop = item_end(code, body)
        for position in range(found, stop):
            if out[position] != "\n":
                out[position] = " "
        cursor = stop
    return "".join(out)


def skip_attributes(code: str, cursor: int) -> int:
    while True:
        while cursor < len(code) and code[cursor].isspace():
            cursor += 1
        if not code.startswith("#[", cursor):
            return cursor
        end = code.find("]", cursor)
        if end < 0:
            return cursor
        cursor = end + 1


def item_end(code: str, start: int) -> int:
    depth = 0
    for position in range(start, len(code)):
        char = code[position]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth <= 0:
                return position + 1
        elif char == ";" and depth == 0:
            return position + 1
    return len(code)


def blank_sql(source: str) -> str:
    """Blank -- line comments and block comments, keeping line numbering."""
    lines = []
    for line in source.splitlines():
        stripped = line.lstrip()
        lines.append("" if stripped.startswith("--") else line)
    return "\n".join(lines)


def counted_lines(path: Path) -> int:
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".rs":
        text = blank_inline_tests(blank_rust(text))
    elif path.suffix == ".sql":
        text = blank_sql(text)
    return sum(1 for line in text.splitlines() if line.strip())


def scope_files(root: Path, scope: str) -> list:
    if scope == "core":
        base = root / "core" / "crates"
    else:
        base = root / "crates"
    if not base.is_dir():
        return []
    files = []
    for path in sorted(base.rglob("*")):
        if not path.is_file():
            continue
        parts = path.relative_to(base).parts
        if any(part in SKIP_DIRS for part in parts):
            continue
        if "src" in parts and path.suffix in CODE_SUFFIXES:
            files.append(path)
        elif scope == "reference" and "sql" in parts and path.suffix == ".sql":
            # Runtime SQL is shipped implementation, included from src/.
            files.append(path)
    return files


def scan(root: Path) -> dict:
    result = {"root": str(root), "scopes": {}, "combined": 0}
    for scope in ("core", "reference"):
        per_crate = {}
        total = 0
        for path in scope_files(root, scope):
            lines = counted_lines(path)
            total += lines
            parts = path.relative_to(root).parts
            crate = parts[2] if scope == "core" else parts[1]
            per_crate[crate] = per_crate.get(crate, 0) + lines
        result["scopes"][scope] = {
            "files": len(scope_files(root, scope)),
            "lines": total,
            "crates": dict(sorted(per_crate.items())),
        }
        result["combined"] += total
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".", help="tree to count (default: current directory)")
    parser.add_argument("--detail", action="store_true", help="print per-crate totals")
    parser.add_argument("--json", action="store_true", help="print the full result as JSON")
    args = parser.parse_args()
    result = scan(Path(args.root).resolve())
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    for scope in ("core", "reference"):
        entry = result["scopes"][scope]
        print(f"{scope:9s} {entry['lines']:7d} production lines in {entry['files']:4d} files")
        if args.detail:
            for crate, lines in entry["crates"].items():
                print(f"          {crate:48s} {lines:7d}")
    print(f"{'combined':9s} {result['combined']:7d} production lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
