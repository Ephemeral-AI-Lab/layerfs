#!/usr/bin/env python3
"""Counter-only comparison of two Phase 2 collection arms.

`CONTRACT.md` §2 makes `elapsed_ns` diagnostic-grade (P0-3 measured +17.6% on one
binary and input), so a before/after claim must be made on the work counters. This
tool strips every timing field and the arm's own output path, then reports, per
step, whether the two arms' logs are otherwise identical.

Usage:
    python3 compare_arms.py <before-arm-dir> <after-arm-dir> [--quiet]
"""
import pathlib
import re
import sys

TIMING = [
    (re.compile(r"wall_s [0-9.]+"), "wall_s <t>"),
    (re.compile(r"wall_seconds: [0-9.]+"), "wall_seconds: <t>"),
    (re.compile(r"elapsed_ns: [0-9]+"), "elapsed_ns: <t>"),
    (re.compile(r"in [0-9]+ ns"), "in <t> ns"),
    (re.compile(r"separately labelled elapsed_ns [0-9]+"), "separately labelled elapsed_ns <t>"),
    (re.compile(r"elapsed_ns [0-9]+"), "elapsed_ns <t>"),
    (re.compile(r"separately labelled elapsed_ns [0-9]+"), "separately labelled elapsed_ns <t>"),
    (re.compile(r"end-to-end ns [0-9]+"), "end-to-end ns <t>"),
    (re.compile(r"\b(ns|us|ms|s) [0-9]+(\.[0-9]+)?"), r"\1 <t>"),
    (re.compile(r"[0-9]+(\.[0-9]+)?(ns|us|ms|s)\b"), "<t>"),
    (re.compile(r"rounds/[^/]+/(before|after)"), "ARM"),
    (re.compile(r"p2-[0-9]+|v[5-7]"), "ROUND"),
]


def normalise(text: str) -> str:
    lines = []
    for line in text.split("\n"):
        if line.startswith("$ "):
            continue  # the command line names the arm's own output path
        if "exit_code" in line:
            continue
        for pattern, replacement in TIMING:
            line = pattern.sub(replacement, line)
        lines.append(line.rstrip())
    return "\n".join(lines)


# Build steps are not measurements: their logs record what cargo compiled, which
# legitimately differs between a cold and a warm target directory. The arms'
# artifact identity is `artifacts.txt` (sha256 per binary), not these logs.
BUILD_STEPS = ("B1", "B2")


def main() -> int:
    before_dir, after_dir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    quiet = "--quiet" in sys.argv
    before = {p.name: normalise(p.read_text()) for p in sorted((before_dir / "logs").glob("*.log"))
              if not p.name.startswith(BUILD_STEPS)}
    after = {p.name: normalise(p.read_text()) for p in sorted((after_dir / "logs").glob("*.log"))
             if not p.name.startswith(BUILD_STEPS)}
    moved = []
    missing = sorted(set(before) ^ set(after))
    for name in sorted(set(before) & set(after)):
        if before[name] != after[name]:
            moved.append(name)
            if not quiet:
                print(f"--- {name} differs")
                left, right = before[name].split("\n"), after[name].split("\n")
                for index in range(max(len(left), len(right))):
                    l = left[index] if index < len(left) else "<absent>"
                    r = right[index] if index < len(right) else "<absent>"
                    if l != r:
                        print(f"  before: {l}\n  after : {r}")
    print(f"steps compared: {len(set(before) & set(after))}, differing: {len(moved)}")
    for name in moved:
        print(f"  DIFF {name}")
    for name in missing:
        print(f"  MISSING IN ONE ARM: {name}")
    return 1 if moved or missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
