#!/usr/bin/env python3
"""Run and retain the pass-only static closure for FastCDC region v2."""

import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
ROOT = REPO / "target/phase4-fastcdc-contiguous-region-kernel-20260821-v2/static-v1"


def main():
    if ROOT.exists():
        raise RuntimeError("static closure namespace already exists")
    ROOT.mkdir()
    commands = [
        ("workspace-tests", ["cargo", "test", "--workspace", "--offline", "--all-targets"]),
        ("clippy", ["cargo", "clippy", "--workspace", "--offline", "--all-targets", "--", "-D", "warnings"]),
        ("rustfmt", ["cargo", "fmt", "--all", "--", "--check"]),
        ("tracked-whitespace", ["git", "diff", "--check"]),
    ]
    ledger = []
    for label, command in commands:
        started = time.monotonic_ns()
        completed = subprocess.run(command, cwd=REPO, capture_output=True, text=True)
        elapsed = time.monotonic_ns() - started
        (ROOT / f"{label}.stdout").write_text(completed.stdout)
        (ROOT / f"{label}.stderr").write_text(completed.stderr)
        ledger.append({"label": label, "command": command, "exit": completed.returncode, "wall_ns": elapsed})
        if completed.returncode:
            raise RuntimeError(f"static command failed: {label}")

    relevant = sorted(HERE.glob("*")) + [REPO / "implementation-detail/phase-4/experiments/fastcdc-hot-loop/audit-addendum-v1.md"]
    whitespace = []
    for path in relevant:
        if not path.is_file():
            continue
        completed = subprocess.run(["git", "diff", "--no-index", "--check", "/dev/null", path],
                                   cwd=REPO, capture_output=True, text=True)
        passed = completed.returncode in (0, 1) and not completed.stdout and not completed.stderr
        whitespace.append({"path": str(path.relative_to(REPO)), "passed": passed})
        if not passed:
            raise RuntimeError(f"untracked whitespace failed: {path}")
    (ROOT / "STATIC-CLOSURE-v1.json").write_text(json.dumps({
        "status": "PASS", "commands": ledger, "relevant_untracked_whitespace": whitespace,
    }, indent=2, sort_keys=True) + "\n")
    print("PASS static closure")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"FAIL: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
