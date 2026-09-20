#!/usr/bin/env python3
"""What `runner.py` would publish for a `history.*` row, replayed from the retained
evidence with the runner's own functions.

No runner invocation is performed. A history performance command measured 38.1 s
(stride10) and 73.1 s (stride3) of raw wall, so a runner run of this lane cannot
fit the frozen budgets and is recorded `NOT_RUN` rather than started
(`benchmark/AGENTS.md` SS "Budgets"). What this script does instead is import
`runner.py` itself and call its real functions on the traces and phase artifacts
the retained campaign already wrote:

  * `runner.pinned_counters()`      - the frozen O3 table, as the runner reads it
  * `runner.re_derive_pins()`       - the check that returns FAIL for a row that
                                      published counters but has no pinned constant
  * `phases.compose()`              - the four declared phases, from the child's
                                      own `phases-perf.json` and the recorded wall
  * `receipt.budget()`              - the classified budgeted quantity
  * `trace.SEVERITY` and the runner's own status rules - the published status

The published status is composed in the order `runner.run_case` uses it
(`runner.py:1016-1060`): budget miss -> `NOT_RUN`; then a non-`full` verification
mode raises the row to `INCOMPLETE`, because `INCOMPLETE` is worse than `NOT_RUN`.

Everything printed here is derived from artifacts that already exist. Nothing is
re-measured, no timeout is enlarged, and no pin is written.
"""

import importlib.util
import json
import shutil
import sys
import tempfile
from pathlib import Path

EVIDENCE = Path(__file__).resolve().parent
CAMPAIGN = EVIDENCE.parent / "stage-6-history-190-read-20260920T042617Z"
REPO = Path(__file__).resolve().parents[6]
HARNESS = REPO / "core/benchmark/fs-bench-pro-storage-content"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


sys.path.insert(0, str(HARNESS / "shared"))
runner = load("h190_runner", HARNESS / "runner.py")
import phases as phases_module  # noqa: E402
import pin_expected  # noqa: E402
import receipt  # noqa: E402
import trace as trace_module  # noqa: E402

RUNS = [
    ("baseline2", "history-stride10"),
    ("candidate2", "history-stride10"),
    ("baseline2", "history-stride3"),
    ("candidate2", "history-stride3"),
]


def main() -> int:
    lines: list[str] = []
    report: dict[str, object] = {"campaign": CAMPAIGN.name, "runs": {}}

    pinned = runner.pinned_counters()
    history_pins = {k: v for k, v in pinned.items()
                    if k.startswith("history-") or k.startswith("history.")}
    lines.append("=== the frozen O3 table as the runner reads it")
    lines.append(f"  case ids carrying at least one pinned counter : {len(pinned)}")
    lines.append(f"  of those, history.* rows                      : {len(history_pins)}")
    lines.append(f"  history lanes the runner offers               : {list(runner.HISTORY_LANES)}")
    lines.append(f"  verification mode defaulted for those lanes    : "
                 f"{runner.default_verification_mode('history-stride10', None)!r} "
                 f"(explicit --case: {runner.default_verification_mode('history-stride10', ['x'])!r}; "
                 f"lane full: {runner.default_verification_mode('full', None)!r})")
    lines.append(f"  mode a caller can still declare explicitly     : "
                 f"{list(runner.VERIFICATION_MODES)} via --verify")
    lines.append(f"  history lanes in DECLARED_EXCEPTIONS           : "
                 f"{sorted(set(runner.HISTORY_LANES) & runner.DECLARED_EXCEPTIONS)}")
    lines.append(f"  ordinary limit / exception limit / lifecycle   : "
                 f"{receipt.COMPLETE_COMMAND_LIMIT_NS / 1e9:.0f} s / "
                 f"{receipt.DECLARED_EXCEPTION_LIMIT_NS / 1e9:.0f} s / "
                 f"{receipt.LIFECYCLE_ALLOWANCE_NS / 1e6:.0f} ms")
    report["history_pins"] = history_pins
    report["pinned_case_ids"] = len(pinned)

    for arm, case in RUNS:
        run_dir = CAMPAIGN / "runs" / f"{arm}-{case}"
        raw = run_dir / "raw"
        wall_ns = int(json.loads((run_dir / "perf-receipt.json").read_text())["wall_ns"])
        parsed = trace_module.read(raw / "trace.jsonl")
        trace_status = parsed.status()
        counters = parsed.counters()
        composed = phases_module.compose(raw, {"perf": wall_ns}, expects_operation=True)
        declared_ns = sum(int(composed.get(field, 0) or 0) for field in (
            "preparation_wall_ns", "operation_ns", "verification_wall_ns", "cleanup_wall_ns"))
        # A runner-shaped case directory: `trace.jsonl` as `runner.run_case` writes
        # it, and the receipt counters the runner would compose from that trace.
        document = {"case_id": case, "counters": counters}
        pins = runner.re_derive_pins(raw, document)

        matrix = {}
        for exception in (False, True):
            budget = receipt.budget(wall_ns, exception, declared_ns=declared_ns)
            for mode in ("sample", "full"):
                status = trace_status
                if budget.status != "PASS":
                    status = "NOT_RUN"
                if mode != "full" and trace_module.SEVERITY["INCOMPLETE"] > trace_module.SEVERITY.get(status, 5):
                    status = "INCOMPLETE"
                matrix[f"{'exception' if exception else 'ordinary'}/{mode}"] = {
                    "budgeted_ns": budget.budgeted_ns if hasattr(budget, "budgeted_ns") else None,
                    "budget_status": budget.status,
                    "budget_limit_s": budget.limit_ns / 1e9,
                    "published_status": status,
                }
        entry = {
            "wall_ns": wall_ns,
            "declared_phases_ns": declared_ns,
            "budgeted_ns": declared_ns + receipt.LIFECYCLE_ALLOWANCE_NS,
            "phase_reconciliation": composed.get("reconciliation", {}).get("status"),
            "trace_status": trace_status,
            "published_numeric_counters": len(counters),
            "pin_re_derivation": pins,
            "status_matrix": matrix,
        }
        report["runs"][f"{arm}-{case}"] = entry

        lines.append("")
        lines.append(f"=== {arm} {case}")
        lines.append(f"  recorded wall (complete command)          : {wall_ns / 1e9:.3f} s")
        lines.append(f"  declared phases (prep+op+verif+cleanup)   : {declared_ns / 1e9:.3f} s")
        lines.append(f"  budgeted quantity (+0.250 s lifecycle)    : {(declared_ns + receipt.LIFECYCLE_ALLOWANCE_NS) / 1e9:.3f} s")
        lines.append(f"  phase reconciliation                      : {entry['phase_reconciliation']}")
        lines.append(f"  trace status (driver's own gates)         : {trace_status}")
        lines.append(f"  published numeric counters (whole trace)  : {len(counters)}")
        lines.append(f"  reconciliation problems                   : "
                     f"{composed.get('reconciliation', {}).get('problems')}")
        lines.append(f"  runner pin re-derivation                  : {pins['status']}: {pins['reason']}")
        for option, values in matrix.items():
            lines.append(f"    {option:20s} -> budget {values['budget_status']:6s} "
                         f"limit {values['budget_limit_s']:.0f} s, published status "
                         f"{values['published_status']}")

    # The bootstrap question: can the pin generator pin a row whose runner receipt
    # is not PASS? `pin_expected.counters_of` skips those receipts, so replay it on
    # the status the runner would publish above.
    lines.append("")
    lines.append("=== can the pin generator bootstrap a new row?")
    # Scratch receipts live outside the evidence tree: they are synthesized, and
    # a synthesized receipt must never be mistaken for retained evidence.
    scratch = Path(tempfile.mkdtemp(prefix="h190-verdict-"))
    for arm, case in RUNS:
        run_dir = CAMPAIGN / "runs" / f"{arm}-{case}"
        parsed = trace_module.read(run_dir / "raw" / "trace.jsonl")
        # One isolated root per case and status: `counters_of` walks every case
        # directory under the root it is given, so a shared root would let one
        # case's receipt answer for another.
        case_dir = scratch / f"{arm}-{case}" / "root" / case
        case_dir.mkdir(parents=True, exist_ok=True)
        (case_dir / "trace.jsonl").write_text((run_dir / "raw" / "trace.jsonl").read_text())
        for status in ("INCOMPLETE", "NOT_RUN", "PASS"):
            (case_dir / "receipt.json").write_text(json.dumps(
                {"case_id": case, "status": status, "counters": parsed.counters()}))
            found = pin_expected.counters_of(case_dir.parent)
            lines.append(f"  {case:18s} receipt status {status:11s} -> "
                         f"{len(found)} pinned constant(s)")
    lines.append("  (a synthesized runner-shaped receipt, replayed through the real"
                 " `pin_expected.counters_of`; the status column is the value the")
    lines.append("   runner's own rules compose above, not a measurement)")

    shutil.rmtree(scratch, ignore_errors=True)
    (EVIDENCE / "runner-verdict.json").write_text(
        json.dumps(report, indent=2, sort_keys=False) + "\n", encoding="utf-8")
    (EVIDENCE / "runner-verdict.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
