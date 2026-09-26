#!/usr/bin/env python3
"""Reproducible first-party production LOC counter (repository AGENTS.md §4).

Counts nonblank, non-comment source lines of first-party LayerFS product
implementation. Excluded: tests, fixtures, mocks, examples, benchmarks,
development tooling, docs, manifests and generated artifacts. Legacy inline
test modules and `#[cfg(test)]` items are removed before counting.

Scope:
  core   core/crates/<package>/src/**/*.rs and *.sql   (replacement product)
  legacy crates/**/*.rs and *.sql                      (reference implementation)

Usage: python3 core/tools/production_loc.py [--json] [--root DIR]
"""
import argparse
import json
from pathlib import Path
import re

SKIP_DIRS = {"target", "node_modules", ".git", "tests", "benches", "examples", "benchmark"}


def strip_rust(text):
    """Remove comments and every `#[cfg(test)]`-attributed item from Rust text."""
    output = []
    index = 0
    length = len(text)
    pending_test_attribute = False
    while index < length:
        char = text[index]
        if char == "/" and index + 1 < length and text[index + 1] == "/":
            end = text.find("\n", index)
            index = length if end < 0 else end
            continue
        if char == "/" and index + 1 < length and text[index + 1] == "*":
            depth = 1
            index += 2
            while index < length and depth:
                if text.startswith("/*", index):
                    depth += 1
                    index += 2
                elif text.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            continue
        if text.startswith('#[cfg(test)]', index) or text.startswith('#[cfg(any(test', index):
            pending_test_attribute = True
            index = text.find("\n", index)
            index = length if index < 0 else index + 1
            continue
        if char == '"':
            raw = re.match(r'r(#*)"', text[index:])
            if raw:
                terminator = '"' + raw.group(1)
                end = text.find(terminator, index + raw.end())
                end = length if end < 0 else end + len(terminator)
                output.append(text[index:end])
                index = end
                continue
            index += 1
            while index < length:
                if text[index] == "\\":
                    index += 2
                    continue
                if text[index] == '"':
                    index += 1
                    break
                index += 1
            output.append('""')
            continue
        if char == "'":
            end = re.match(r"'(?:\\.|[^\\'])'", text[index:])
            if end:
                output.append("'x'")
                index += end.end()
                continue
        if pending_test_attribute:
            # Skip the attributed item: from its first '{' to the matching '}'.
            brace = text.find("{", index)
            semicolon = text.find(";", index)
            if brace < 0 or (0 <= semicolon < brace):
                index = length if semicolon < 0 else semicolon + 1
                pending_test_attribute = False
                continue
            depth = 0
            cursor = brace
            while cursor < length:
                if text[cursor] == "{":
                    depth += 1
                elif text[cursor] == "}":
                    depth -= 1
                    if depth == 0:
                        cursor += 1
                        break
                cursor += 1
            index = cursor
            pending_test_attribute = False
            continue
        output.append(char)
        index += 1
    return "".join(output)


def strip_sql(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return "\n".join(re.sub(r"--.*$", "", line) for line in text.splitlines())


def counted(path):
    body = path.read_text(errors="replace")
    body = strip_rust(body) if path.suffix == ".rs" else strip_sql(body)
    return sum(1 for line in body.splitlines() if line.strip())


def scope_files(root):
    groups = {"core": [], "legacy": []}
    for path in sorted((root / "core/crates").glob("**/*")):
        if path.suffix in (".rs", ".sql") and path.is_file():
            parts = set(path.parts)
            if parts & SKIP_DIRS or "src" not in path.parts:
                continue
            groups["core"].append(path)
    for path in sorted((root / "crates").glob("**/*")):
        if path.suffix in (".rs", ".sql") and path.is_file():
            if set(path.parts) & SKIP_DIRS:
                continue
            groups["legacy"].append(path)
    return groups


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[2]))
    arguments = parser.parse_args()
    root = Path(arguments.root).resolve()
    groups = scope_files(root)
    totals = {name: sum(counted(path) for path in paths) for name, paths in groups.items()}
    totals["combined"] = totals["core"] + totals["legacy"]
    totals["files"] = {name: len(paths) for name, paths in groups.items()}
    if arguments.json:
        print(json.dumps(totals, sort_keys=True))
    else:
        print(f"core {totals['core']} legacy {totals['legacy']} combined {totals['combined']} "
              f"files {totals['files']}")


if __name__ == "__main__":
    main()
