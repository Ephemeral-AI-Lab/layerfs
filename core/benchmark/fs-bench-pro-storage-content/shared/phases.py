#!/usr/bin/env python3
"""The four declared phases, and the reconciliation that keeps them honest.

`benchmark_rules.md` section 6 requires setup, performance, verification and
cleanup to use separate timing and resource scopes, and #184 requires all of them
to be **published** rather than billed to a performance budget. This module is the
reader and the composer: the child publishes one `phases-<invocation>.json` per
invocation, and everything here is derived from those files and from the product's
own byte-verbatim `timing.json`.

**What is inside each phase** is fixed per case shape by
`test_setup_and_cache_discipline.md` section 2.2, and the child cannot choose it:
the boundary is where the driver's `Timing::record` opens and closes.

**The reconciliation is a real inequality, not a tautology.** The child measures
its own invocation from its first mark to its snapshot; the runner measures the
whole process. So

    preparation + operation + verification + cleanup  <=  invocation  <=  wall

must hold, and `wall - declared` must be inside a declared tolerance. If it is
not, measured work moved outside a timer — which is exactly the failure mode
`AGENTS.md` section 1 forbids and this check exists to catch.
"""

from __future__ import annotations

import json
from pathlib import Path

SCHEMA = "layerfs-phases-v1"

# The tolerance is declared, not tuned. The slack it covers is per-invocation
# process start (fork, exec, dyld, argument parsing, the trace header) and process
# teardown, none of which is product or harness phase work. It is 250 ms plus 2% of
# the process wall: at the largest measured command in round 4c (9.952 s) that is
# 449 ms, and at the smoke lane's median (~30 ms) the absolute term dominates. A
# tolerance narrower than this reports process startup as a phase defect; a wider
# one stops catching a second of work moved into setup.
TOLERANCE_ABSOLUTE_NS = 250_000_000
TOLERANCE_FRACTION_DENOMINATOR = 50


class PhaseError(Exception):
    """The phase evidence does not reconcile."""


def tolerance_ns(wall_ns: int) -> int:
    """The declared reconciliation tolerance for one invocation's wall."""
    return TOLERANCE_ABSOLUTE_NS + wall_ns // TOLERANCE_FRACTION_DENOMINATOR


def read_invocation(path: str | Path) -> dict[str, object]:
    """Reads one `phases-<invocation>.json`, failing closed on a malformed one."""
    path = Path(path)
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PhaseError(f"{path}: {error}") from error
    if document.get("schema") != SCHEMA:
        raise PhaseError(f"{path}: schema {document.get('schema')!r} is not {SCHEMA!r}")
    invocation = document.get("invocation")
    if invocation not in {"prepare", "perf", "verify"}:
        raise PhaseError(f"{path}: invocation {invocation!r}")
    fields: dict[str, int] = {}
    for key in (
        "preparation_ns",
        "acquisition_ns",
        "operation_ns",
        "verification_ns",
        "cleanup_ns",
        "handoff_ns",
        "invocation_ns",
        "timing_json_bytes",
    ):
        value = document.get(key)
        if not isinstance(value, int) or value < 0:
            raise PhaseError(f"{path}: {key} is {value!r}")
        fields[key] = value
    if fields["acquisition_ns"] > fields["preparation_ns"]:
        raise PhaseError(
            f"{path}: acquisition {fields['acquisition_ns']} exceeds preparation "
            f"{fields['preparation_ns']}; the per-sample copy is a subset of setup"
        )
    declared = (
        fields["preparation_ns"]
        + fields["operation_ns"]
        + fields["verification_ns"]
        + fields["cleanup_ns"]
    )
    if declared > fields["invocation_ns"]:
        raise PhaseError(
            f"{path}: the declared phases sum to {declared} ns, more than the "
            f"invocation's own {fields['invocation_ns']} ns"
        )
    fields["invocation"] = invocation  # type: ignore[assignment]
    fields["declared_ns"] = declared
    fields["unaccounted_ns"] = fields["invocation_ns"] - declared
    return fields


def read_timing_root(path: str | Path) -> dict[str, object]:
    """Reads the product's own `timing.json` and returns its root reading.

    The product's tree is the authority on the operation time: it is byte-verbatim
    telemetry, never edited, and `g7.tree-complete` gates its completeness. This
    reader exists so the published `operation_ns` is checked against the raw
    artifact rather than trusted from the child's summary of it.
    """
    path = Path(path)
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PhaseError(f"{path}: {error}") from error
    if not isinstance(document, dict):
        raise PhaseError(f"{path}: not a JSON object")
    elapsed = document.get("elapsed_ns")
    if not isinstance(elapsed, int) or elapsed < 0:
        raise PhaseError(f"{path}: root elapsed_ns is {elapsed!r}")
    incomplete = bool(document.get("incomplete", False))
    return {
        "name": str(document.get("name", "")),
        "elapsed_ns": elapsed,
        "children": len(document.get("children", []) or []),
        "incomplete": incomplete,
    }


def reconcile_invocation(reading: dict[str, object], wall_ns: int, tolerance: int) -> dict[str, object]:
    """Checks one invocation's declared phases against the process wall it ran in."""
    declared = int(reading["declared_ns"])
    invocation = int(reading["invocation_ns"])
    record = dict(reading)
    record["wall_ns"] = wall_ns
    record["tolerance_ns"] = tolerance
    record["slack_ns"] = wall_ns - declared
    if declared > invocation:
        record["reconciles"] = False
        record["reason"] = (
            f"declared phases {declared} ns exceed the child's own invocation {invocation} ns"
        )
    elif invocation > wall_ns:
        record["reconciles"] = False
        record["reason"] = (
            f"the child's invocation {invocation} ns exceeds the process wall {wall_ns} ns"
        )
    elif wall_ns - declared > tolerance:
        record["reconciles"] = False
        record["reason"] = (
            f"{wall_ns - declared} ns of the {wall_ns} ns wall is outside every declared "
            f"phase, more than the declared {tolerance} ns tolerance"
        )
    else:
        record["reconciles"] = True
        record["reason"] = (
            f"declared {declared} ns inside a {wall_ns} ns wall; "
            f"{wall_ns - declared} ns unaccounted, within {tolerance} ns"
        )
    return record


def compose(
    case_dir: str | Path,
    invocation_walls: dict[str, int],
    expects_operation: bool = True,
) -> dict[str, object]:
    """Composes the six published phase fields for one case.

    `expects_operation` is false for a row that ran no driver at all — a `NOT_RUN`
    row with a driver the harness has not implemented. Such a row measured nothing
    because there was nothing to measure, and reporting that as an unreconciled phase
    would turn a declared gap into a verdict change. Every row that did run a driver
    is required to publish an operation.

    `invocation_walls` maps an invocation name to the runner's own measured process
    wall for it. The performance invocation is the one whose wall is budgeted, so
    `complete_command_ns` is that wall and nothing else — the deferred verification
    invocation keeps its own 60 s budget, as `benchmark_rules.md` section 11
    requires.
    """
    case_dir = Path(case_dir)
    invocations: list[dict[str, object]] = []
    readings: dict[str, dict[str, object]] = {}
    problems: list[str] = []
    for invocation, wall_ns in invocation_walls.items():
        path = case_dir / f"phases-{invocation}.json"
        if not path.exists():
            problems.append(f"{invocation}: no {path.name}; the invocation published no phases")
            continue
        try:
            reading = read_invocation(path)
        except PhaseError as error:
            problems.append(str(error))
            continue
        readings[invocation] = reading
        invocations.append(
            reconcile_invocation(reading, wall_ns, tolerance_ns(wall_ns))
        )
    totals = {
        "preparation_wall_ns": sum(int(r["preparation_ns"]) for r in readings.values()),
        "acquisition_wall_ns": sum(int(r["acquisition_ns"]) for r in readings.values()),
        "operation_ns": sum(int(r["operation_ns"]) for r in readings.values()),
        "verification_wall_ns": sum(int(r["verification_ns"]) for r in readings.values()),
        "cleanup_wall_ns": sum(int(r["cleanup_ns"]) for r in readings.values()),
        "handoff_ns": sum(int(r["handoff_ns"]) for r in readings.values()),
        "complete_command_ns": invocation_walls.get("perf", 0),
        "verification_invocation_ns": invocation_walls.get("verify", 0),
    }
    # The published operation time is checked against the product's own artifact,
    # not against the child's summary of it. Two sources, one number.
    timing_path = case_dir / "timing.json"
    if readings.get("perf") is not None and int(readings["perf"]["operation_ns"]) > 0:
        if not timing_path.exists():
            problems.append("timing.json is absent but the performance invocation measured an operation")
        else:
            try:
                root = read_timing_root(timing_path)
            except PhaseError as error:
                problems.append(str(error))
            else:
                totals["operation_root_elapsed_ns"] = root["elapsed_ns"]
                totals["operation_root_name"] = root["name"]
                totals["operation_root_incomplete"] = root["incomplete"]
                if root["elapsed_ns"] != totals["operation_ns"]:
                    problems.append(
                        f"operation_ns {totals['operation_ns']} does not match the product's "
                        f"own timing root {root['elapsed_ns']}"
                    )
    elif readings.get("perf") is not None and expects_operation:
        problems.append("the performance invocation measured no operation")
    failed = [record for record in invocations if not record.get("reconciles")]
    for record in failed:
        problems.append(f"{record['invocation']}: {record['reason']}")
    totals["invocations"] = invocations
    totals["reconciliation"] = {
        "status": "PASS" if not problems else "INCOMPLETE",
        "reason": "; ".join(problems) if problems else "every declared phase reconciles with its wall",
        "problems": problems,
        "tolerance_absolute_ns": TOLERANCE_ABSOLUTE_NS,
        "tolerance_fraction": f"1/{TOLERANCE_FRACTION_DENOMINATOR} of the wall",
    }
    return totals


def self_check() -> list[str]:
    """Proves the composer fails closed on each way the reconciliation can break."""
    import tempfile

    failures: list[str] = []

    def write(directory: Path, name: str, document: object) -> Path:
        path = directory / name
        path.write_text(json.dumps(document), encoding="utf-8")
        return path

    def phases(**overrides: object) -> dict[str, object]:
        base: dict[str, object] = {
            "schema": SCHEMA,
            "invocation": "perf",
            "preparation_ns": 100,
            "acquisition_ns": 10,
            "operation_ns": 1000,
            "verification_ns": 200,
            "cleanup_ns": 50,
            "handoff_ns": 5,
            "invocation_ns": 1400,
            "timing_json_bytes": 60,
        }
        base.update(overrides)
        return base

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        write(root, "phases-perf.json", phases())
        write(root, "timing.json", {"name": "c2.delta", "elapsed_ns": 1000, "children": []})
        composed = compose(root, {"perf": 1500})
        if composed["reconciliation"]["status"] != "PASS":
            failures.append(f"a reconciling row was rejected: {composed['reconciliation']['reason']}")
        if composed["operation_ns"] != 1000 or composed["preparation_wall_ns"] != 100:
            failures.append("the composed fields are not the published ones")

        # A wall much larger than the declared phases is work outside every timer.
        wide = compose(root, {"perf": 1500 + 2 * TOLERANCE_ABSOLUTE_NS})
        if wide["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("work outside every declared phase was accepted")

        # The product's own artifact is the authority on the operation time.
        write(root, "timing.json", {"name": "c2.delta", "elapsed_ns": 999, "children": []})
        mismatched = compose(root, {"perf": 1500})
        if mismatched["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("a published operation time that disagrees with timing.json was accepted")
        write(root, "timing.json", {"name": "c2.delta", "elapsed_ns": 1000, "children": []})

        # Phases that sum past the child's own invocation cannot be right.
        write(root, "phases-perf.json", phases(invocation_ns=100))
        if compose(root, {"perf": 1500})["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("phases summing past the child's own invocation were accepted")
        write(root, "phases-perf.json", phases(invocation_ns=900))
        if compose(root, {"perf": 1500})["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("an invocation shorter than its declared phases was accepted")
        write(root, "phases-perf.json", phases(invocation_ns=1400))

        # An acquisition larger than the preparation that contains it is a defect.
        write(root, "phases-perf.json", phases(acquisition_ns=101))
        over = compose(root, {"perf": 1500})
        if over["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("an acquisition larger than its preparation was accepted")
        write(root, "phases-perf.json", phases())

        # A row that ran no driver is not required to publish an operation.
        write(root, "phases-perf.json", phases(operation_ns=0, timing_json_bytes=0))
        (root / "timing.json").unlink()
        if compose(root, {"perf": 1500}, expects_operation=False)["reconciliation"]["status"] != "PASS":
            failures.append("a row that ran no driver was required to publish an operation")
        if compose(root, {"perf": 1500}, expects_operation=True)["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("a row that claims a measurement was allowed to publish none")
        write(root, "phases-perf.json", phases())
        write(root, "timing.json", {"name": "c2.delta", "elapsed_ns": 1000, "children": []})

        # A missing phases file is never quietly composed from nothing.
        (root / "phases-perf.json").unlink()
        missing = compose(root, {"perf": 1500})
        if missing["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("a row with no published phases was accepted")

        # A malformed file fails closed rather than being skipped.
        write(root, "phases-perf.json", phases(schema="something-else"))
        if compose(root, {"perf": 1500})["reconciliation"]["status"] != "INCOMPLETE":
            failures.append("a phases file with the wrong schema was accepted")

    if tolerance_ns(0) != TOLERANCE_ABSOLUTE_NS:
        failures.append("the declared tolerance is not the absolute term at a zero wall")
    return failures


def main() -> int:
    failures = self_check()
    for failure in failures:
        print(f"phases: FAIL: {failure}")
    if failures:
        return 1
    print("phases: PASS (four phases composed, reconciliation fails closed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
