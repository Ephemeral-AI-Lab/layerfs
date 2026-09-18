#!/usr/bin/env python3
"""Freeze the pinned expectation constants the frozen oracle's O1 and O3 need.

`gates_and_oracles.md` section 4 defines O1 as an expected root *"from a frozen
constant"* and O3 as counts *"pinned as constants"*. This tool is how the constants
are made frozen, and it is deliberately a two-source tool:

* **counters** come from a named baseline run — the round-4c full lane at
  `90bbb617d`, the last tree on which all 217 admission rows passed with 0 `FAIL`.
  Pinning *that* run's numbers is what turns "every counter identical to round 4c"
  from a comparison a reader performs into a gate the row must pass.
* **identity digests** were first published in round 5 (round 4c recorded no roots),
  so they come from a bootstrap run of the same tree. The closure run then has to
  reproduce them, which is the only thing that makes them evidence rather than a
  transcription.

A key that the baseline does not carry is **not** pinned: the table is a statement
about the numbers that existed, never about numbers invented to fill it.

Usage:
  pin_expected.py counters --run <baseline-run> [--run <another>] --out expected.tsv
  pin_expected.py digests  --run <bootstrap-run>  --out expected.tsv
  pin_expected.py merge --counters a.tsv --digests b.tsv --out expected.tsv
  pin_expected.py --self-check
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

FORMAT = "# fs-bench-expected-v1"

# Counters that describe the harness's own instrumentation rather than the product,
# and which therefore may change when the harness publishes more of itself.
INSTRUMENTATION_KEYS = {"timing_json_bytes"}


def counters_of(run_dir: str | Path) -> dict[tuple[str, str], int]:
    """Every pinned counter, from the receipts of a run."""
    run_dir = Path(run_dir)
    pinned: dict[tuple[str, str], int] = {}
    for case_dir in sorted(entry for entry in run_dir.iterdir() if entry.is_dir()):
        path = case_dir / "receipt.json"
        if not path.exists():
            continue
        document = json.loads(path.read_text(encoding="utf-8"))
        if document.get("status") != "PASS":
            # A row that did not pass has no baseline number to pin: pinning a
            # `NOT_RUN` row's absent counter as zero would invent a constant.
            continue
        case_id = str(document.get("case_id", case_dir.name))
        for key, value in sorted((document.get("counters") or {}).items()):
            if key in INSTRUMENTATION_KEYS:
                continue
            if not isinstance(value, int):
                continue
            pinned[(case_id, f"counter:{key}")] = value
    return pinned


def digests_of(run_dir: str | Path) -> dict[tuple[str, str], str]:
    """Every identity digest a run published, read from the raw traces."""
    run_dir = Path(run_dir)
    pinned: dict[tuple[str, str], str] = {}
    for case_dir in sorted(entry for entry in run_dir.iterdir() if entry.is_dir()):
        trace_path = case_dir / "trace.jsonl"
        if not trace_path.exists():
            continue
        for line in trace_path.read_text(encoding="utf-8", errors="replace").splitlines():
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                continue
            if record.get("kind") != "oracle":
                continue
            key = str(record.get("key", ""))
            if not key.startswith("identity."):
                continue
            value = str(record.get("value", ""))
            if len(value) != 64:
                continue
            name = key[len("identity.") :]
            existing = pinned.get((case_dir.name, f"digest:{name}"))
            if existing is not None and existing != value:
                raise SystemExit(
                    f"{case_dir.name} {name}: two different digests in one run "
                    f"({existing} and {value}); the identity is not deterministic"
                )
            pinned[(case_dir.name, f"digest:{name}")] = value
    return pinned


def render(pinned: dict[tuple[str, str], object]) -> str:
    """The table, sorted so a diff is readable."""
    lines = [FORMAT]
    lines.append(
        "# case_id\tlabel\tvalue. Pinned by round 5 from the round-4c full lane"
    )
    lines.append("# (counters) and a round-5 bootstrap run of the same tree (identity digests).")
    lines.append("# Regenerate with shared/pin_expected.py; never hand-edit a value.")
    for (case_id, label), value in sorted(pinned.items()):
        lines.append(f"{case_id}\t{label}\t{value}")
    return "\n".join(lines) + "\n"


def parse(path: str | Path) -> dict[tuple[str, str], object]:
    """Reads a table back, so merge is a set operation rather than a rewrite."""
    out: dict[tuple[str, str], object] = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        case_id, label, value = line.split("\t")
        out[(case_id, label)] = int(value) if label.startswith("counter:") else value
    return out


def self_check() -> list[str]:
    """Proves the tool refuses to invent a constant and detects a split identity."""
    import tempfile

    failures: list[str] = []
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        good = root / "passing"
        (good / "a").mkdir(parents=True)
        (good / "a" / "receipt.json").write_text(
            json.dumps(
                {
                    "case_id": "a",
                    "status": "PASS",
                    "counters": {"k": 1, "timing_json_bytes": 60},
                }
            )
        )
        (good / "a" / "trace.jsonl").write_text(
            json.dumps(
                {
                    "schema": "layerfs-trace-v1",
                    "seq": 0,
                    "kind": "oracle",
                    "key": "identity.root",
                    "value": "ab" * 32,
                }
            )
            + "\n"
        )
        counters = counters_of(good)
        if counters != {("a", "counter:k"): 1}:
            failures.append(f"instrumentation was not excluded: {counters}")
        if digests_of(good) != {("a", "digest:root"): "ab" * 32}:
            failures.append("a published identity digest was not harvested")
        failed = root / "failing"
        (failed / "b").mkdir(parents=True)
        (failed / "b" / "receipt.json").write_text(
            json.dumps({"case_id": "b", "status": "NOT_RUN", "counters": {"k": 0}})
        )
        if counters_of(failed):
            failures.append("a constant was invented for a row that did not pass")
        split = root / "split"
        (split / "c").mkdir(parents=True)
        (split / "c" / "trace.jsonl").write_text(
            "\n".join(
                json.dumps(
                    {
                        "schema": "layerfs-trace-v1",
                        "seq": index,
                        "kind": "oracle",
                        "key": "identity.root",
                        "value": value * 32,
                    }
                )
                for index, value in enumerate(("ab", "cd"))
            )
            + "\n"
        )
        try:
            digests_of(split)
            failures.append("two different digests in one run were accepted")
        except SystemExit:
            pass
        rendered = render({("a", "counter:k"): 1, ("a", "digest:root"): "ab" * 32})
        path = root / "expected.tsv"
        path.write_text(rendered)
        if parse(path) != {("a", "counter:k"): 1, ("a", "digest:root"): "ab" * 32}:
            failures.append("the rendered table does not round-trip")
    return failures


def main() -> int:
    arguments = sys.argv[1:]
    if arguments == ["--self-check"]:
        failures = self_check()
        for failure in failures:
            print(f"pin_expected: FAIL: {failure}")
        if failures:
            return 1
        print("pin_expected: PASS (no invented constant, split identity refused, table round-trips)")
        return 0
    mode = arguments[0] if arguments else ""
    runs: list[str] = []
    out: str | None = None
    counters_path: str | None = None
    digests_path: str | None = None
    index = 1
    while index < len(arguments):
        match arguments[index]:
            case "--run":
                runs.append(arguments[index + 1])
                index += 2
            case "--out":
                out = arguments[index + 1]
                index += 2
            case "--counters":
                counters_path = arguments[index + 1]
                index += 2
            case "--digests":
                digests_path = arguments[index + 1]
                index += 2
            case other:
                print(f"unknown argument {other!r}", file=sys.stderr)
                return 2
    if mode == "counters":
        pinned: dict[tuple[str, str], object] = {}
        for run in runs:
            pinned.update(counters_of(run))
    elif mode == "digests":
        pinned = {}
        for run in runs:
            pinned.update(digests_of(run))
    elif mode == "merge":
        if not counters_path or not digests_path or not out:
            print("merge needs --counters, --digests and --out", file=sys.stderr)
            return 2
        pinned = parse(counters_path)
        pinned.update(parse(digests_path))
    else:
        print(__doc__)
        return 2
    if not out:
        print("--out is required", file=sys.stderr)
        return 2
    Path(out).write_text(render(pinned), encoding="utf-8")
    counters = sum(1 for _, label in pinned if label.startswith("counter:"))
    digests = sum(1 for _, label in pinned if label.startswith("digest:"))
    print(f"pinned {counters} counter(s) and {digests} identity digest(s) into {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
