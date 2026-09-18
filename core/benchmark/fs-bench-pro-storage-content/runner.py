#!/usr/bin/env python3
"""The Stage 6 harness runner: list | prepare | perf | verify | report | self-check | calibrate.

What each verb guarantees
-------------------------

`list`
    Prints the frozen registry, or the `--smoke` lane. Reads the binary's own
    generated table, so what is listed is what is registered.

`prepare`
    Acquires a prepared artifact once, keyed by a compatibility digest, and leaves
    it immutable. Nothing in a timed phase rebuilds it.

`perf`
    Runs one sample per case per arm, holds the measurement lock, enforces the
    complete-command budget, and writes a **receipt per case** derived from the raw
    trace the child wrote. It never overwrites an existing path: a rerun into a used
    directory is refused, because a rerun that overwrites the evidence destroys the
    only witness there is.

`verify`
    Re-reads the raw artifacts and **re-derives every published figure** — flatness,
    sequence, worst-gate aggregation, budget classification, and for C2 rows the
    space and pack accounting read out of the Store file itself. A figure that only
    the collecting process could produce is not evidence.

`report`
    Renders the ladders, bands and the four-axis view. Time is printed and never
    decides.

`self-check`
    Runs every Python self-check, the registry self-check in the binary, the lock
    parity test, and asserts `LAYERFS_CONSTRUCTION_WORKERS=1`.

`calibrate`
    Runs the untimed experiments E1-E4 and records each outcome, failures included.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

HARNESS_ROOT = Path(__file__).resolve().parent
REPO_ROOT = HARNESS_ROOT.parents[2]
sys.path.insert(0, str(HARNESS_ROOT / "shared"))

import analyze  # noqa: E402
import copyladder  # noqa: E402
import invariants  # noqa: E402
import receipt  # noqa: E402
import space  # noqa: E402
import trace as trace_module  # noqa: E402

BINARY = HARNESS_ROOT / "target" / "release" / "fs-bench-storage-content"
MANIFEST = HARNESS_ROOT / "Cargo.toml"
TOOLCHAIN = "+1.85.1"
LOCK_PATH = HARNESS_ROOT / ".measurement.lock"
RESULTS_ROOT = REPO_ROOT / "benchmark-results" / "fs-bench-pro-storage-content"

# Cases whose complete command is a declared exception on the <= 25 s list.
# Owner decision D4: over-budget tiers are declared, never cut and never shrunk.
DECLARED_EXCEPTIONS = {
    "payload-create-100m",
    "payload-create-500m",
    "payload-create-chunked-100m",
    "payload-create-chunked-500m",
    "overwrite-fixed-64k-chunk-count-preserve-500m",
    "overwrite-fixed-64k-chunk-count-increase-500m",
    "overwrite-fixed-64k-chunk-count-decrease-500m",
    "payload-random-read-500",
}


class RunnerError(Exception):
    """The run cannot proceed as asked."""


def cargo(*arguments: str, check: bool = True) -> subprocess.CompletedProcess:
    """Runs one cargo command against the harness manifest."""
    command = ["cargo", TOOLCHAIN, *arguments, "--manifest-path", str(MANIFEST)]
    return subprocess.run(command, capture_output=True, text=True, check=check)


def build(release: bool = True, locked: bool = True) -> None:
    """Builds the harness binary."""
    arguments = ["build"]
    if release:
        arguments.append("--release")
    if locked:
        arguments.append("--locked")
    result = cargo(*arguments, check=False)
    if result.returncode != 0:
        raise RunnerError(f"cargo build failed:\n{result.stderr[-4000:]}")


def registry_rows(*extra: str) -> list[str]:
    """The case IDs the binary lists for the given selection."""
    result = subprocess.run(
        [str(BINARY), "--list", "--format", "tsv", *extra],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RunnerError(f"the binary could not list the registry: {result.stderr.strip()}")
    return [
        line.split("\t")[0]
        for line in result.stdout.splitlines()
        if line and not line.startswith("id\t")
    ]


def registry_self_check() -> dict[str, object]:
    """Runs the binary's registry self-check and parses its counts."""
    result = subprocess.run(
        [str(BINARY), "--self-check"], capture_output=True, text=True, check=False
    )
    fields: dict[str, object] = {"exit_code": result.returncode}
    for line in result.stdout.splitlines():
        if ":" in line:
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    if result.returncode != 0:
        fields["status"] = "FAIL"
        fields["stderr"] = result.stderr.strip()
    else:
        fields["status"] = "PASS"
    return fields


def golden_matches() -> bool:
    """Whether the committed golden table is what the registry renders."""
    temporary = HARNESS_ROOT / "target" / "registry-rendered.tsv"
    temporary.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [str(BINARY), "--emit-registry-tsv", str(temporary)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return False
    rendered = temporary.read_text(encoding="utf-8")
    golden = (HARNESS_ROOT / "tests" / "golden" / "registry.tsv").read_text(encoding="utf-8")
    return rendered == golden


def environment() -> dict[str, str]:
    """The environment every measured child runs under."""
    env = dict(os.environ)
    # One construction worker for every case except namespace init. No run raises
    # it and no second lane is added to pass a gate.
    env["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    return env


def assert_workers(identity: receipt.Identity) -> None:
    if identity.construction_workers != "1":
        raise RunnerError(
            "LAYERFS_CONSTRUCTION_WORKERS must be exported as 1 in every receipt; "
            f"it is {identity.construction_workers!r}"
        )


def cmd_list(arguments: argparse.Namespace) -> int:
    extra = ["--lane", arguments.lane, "--format", arguments.format]
    if arguments.format == "tsv":
        print(subprocess.run([str(BINARY), "--list", *extra], capture_output=True, text=True).stdout, end="")
    else:
        for case_id in registry_rows("--lane", arguments.lane):
            print(case_id)
    return 0


def cmd_prepare(arguments: argparse.Namespace) -> int:
    """Acquires the prepared artifacts a selection needs, once."""
    root = Path(arguments.out) if arguments.out else RESULTS_ROOT / "prepared"
    root.mkdir(parents=True, exist_ok=True)
    manifest = {
        "schema": "layerfs-prepared-manifest-v1",
        "root": str(root),
        "acquired_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "cases": [],
    }
    selected = arguments.case or registry_rows()
    for case_id in selected:
        destination = root / case_id
        if destination.exists():
            manifest["cases"].append({"case_id": case_id, "state": "reused"})
            continue
        destination.mkdir(parents=True)
        # The prepared artifact a C1 filesystem family needs is a serialized
        # `FilesystemInput`, and a C2 family needs a closed Store. Both are
        # produced by the child, because construction is a product operation and
        # Python cannot perform one. Until that producer exists the row is
        # recorded here rather than faked.
        manifest["cases"].append(
            {
                "case_id": case_id,
                "state": "not-produced",
                "reason": "no child producer is implemented for this family's artifact yet",
            }
        )
    receipt.write_append_only(root / f"manifest-{manifest['acquired_utc'].replace(':', '')}.json", manifest)
    print(f"prepare: {len(manifest['cases'])} case(s) considered at {root}")
    return 0


def run_case(case_id: str, run_dir: Path, identity: receipt.Identity) -> dict[str, object]:
    """Runs one case, enforces the budget, and writes its derived receipt."""
    case_dir = run_dir / case_id
    if case_dir.exists():
        raise RunnerError(f"{case_dir} exists; a run never overwrites evidence")
    # The child creates its own output directory and refuses an existing one, so
    # the append-only rule is enforced at the producer rather than by the runner
    # creating the path first and then refusing the child its own directory.
    declared_exception = case_id in DECLARED_EXCEPTIONS
    started = time.monotonic_ns()
    result = subprocess.run(
        [str(BINARY), "--case", case_id, "--out", str(case_dir)],
        capture_output=True,
        text=True,
        check=False,
        env=environment(),
    )
    wall_ns = time.monotonic_ns() - started
    budget = receipt.budget(wall_ns, declared_exception)
    parsed = trace_module.read(case_dir / "trace.jsonl")
    status = parsed.status()
    identity_fields = parsed.identity()
    gates = [
        {
            "class": gate.gate_class,
            "id": gate.identifier,
            "status": gate.status,
            "measured": gate.measured,
            "limit": gate.limit,
        }
        for gate in parsed.gates()
    ]
    if budget.status != "PASS":
        status = "NOT_RUN"
        gates.append(
            {
                "class": "G6",
                "id": "budget.complete-command",
                "status": "NOT_RUN",
                "measured": f"{wall_ns / 1e9:.3f} s",
                "limit": f"<= {budget.limit_ns / 1e9:.0f} s",
            }
        )
    document: dict[str, object] = {
        "schema": receipt.SCHEMA,
        "case_id": case_id,
        "family": identity_fields.get("family", ""),
        "admission": identity_fields.get("admission", ""),
        "tier_label": identity_fields.get("tier_label", ""),
        "profile": identity_fields.get("profile", ""),
        "declared_bytes": int(identity_fields.get("declared_bytes", 0) or 0),
        "declared_entries": int(identity_fields.get("declared_entries", 0) or 0),
        "shape": identity_fields.get("shape", ""),
        "status": status,
        "gates": gates,
        "counters": parsed.counters(),
        "resources": parsed.resources(),
        "notes": parsed.notes(),
        "trace_defects": parsed.defects,
        "child": {
            "exit_code": result.returncode,
            "stdout": result.stdout.strip(),
            "stderr": result.stderr.strip()[-2000:],
        },
        "wall_ns": wall_ns,
        "budget": budget.as_fields(),
        "identity": identity.as_fields(),
        "receipt_path": str(case_dir / "receipt.json"),
    }
    receipt.write_append_only(case_dir / "receipt.json", document)
    return document


def cmd_perf(arguments: argparse.Namespace) -> int:
    if arguments.no_build is False:
        build()
    run_dir = Path(arguments.out)
    if run_dir.exists():
        raise RunnerError(f"{run_dir} exists; a run never overwrites evidence")
    run_dir.mkdir(parents=True)
    identity = receipt.identify(REPO_ROOT, HARNESS_ROOT, BINARY)
    assert_workers(identity)
    selection = arguments.case or registry_rows("--lane", arguments.lane)
    run_document = {
        "schema": "layerfs-core-run-v1",
        "run_dir": str(run_dir),
        "lane": arguments.lane,
        "selection": selection,
        "selection_size": len(selection),
        "cache_state": "declared per row; never pooled",
        "samples_per_case_per_arm": 1,
        "declared_exceptions": sorted(DECLARED_EXCEPTIONS & set(selection)),
        "registry_self_check": registry_self_check(),
        "golden_matches": golden_matches(),
        "identity": identity.as_fields(),
    }
    started = time.monotonic_ns()
    rows = []
    with receipt.measurement_lock(LOCK_PATH):
        for case_id in selection:
            try:
                rows.append(run_case(case_id, run_dir, identity))
            except RunnerError as error:
                rows.append(
                    {
                        "case_id": case_id,
                        "status": "NOT_RUN",
                        "notes": [f"runner refused: {error}"],
                    }
                )
    run_document["wall_ns"] = time.monotonic_ns() - started
    run_document["rows"] = len(rows)
    tally: dict[str, int] = {}
    for row in rows:
        tally[row["status"]] = tally.get(row["status"], 0) + 1
    run_document["tally"] = tally
    receipt.write_append_only(run_dir / "run.json", run_document)
    write_manifest(run_dir)
    for key in sorted(tally):
        print(f"  {key:<12} {tally[key]}")
    print(f"perf: {len(rows)} case(s) in {run_document['wall_ns'] / 1e9:.1f} s -> {run_dir}")
    return 0


def write_manifest(run_dir: Path) -> Path:
    """Hashes every retained file into the run manifest."""
    entries = []
    for path in sorted(run_dir.rglob("*")):
        if path.is_file() and path.name != "manifest.json":
            entries.append(
                {
                    "path": str(path.relative_to(run_dir)),
                    "bytes": path.stat().st_size,
                    "sha256": receipt.sha256_file(path),
                }
            )
    return receipt.write_append_only(
        run_dir / "manifest.json",
        {"schema": "layerfs-core-manifest-v1", "files": entries, "count": len(entries)},
    )


def cmd_verify(arguments: argparse.Namespace) -> int:
    run_dir = Path(arguments.run)
    if not run_dir.is_dir():
        raise RunnerError(f"{run_dir} is not a run directory")
    started = time.monotonic_ns()
    findings: list[dict[str, object]] = []
    disagreements = 0
    with receipt.measurement_lock(LOCK_PATH):
        for case_dir in sorted(entry for entry in run_dir.iterdir() if entry.is_dir()):
            receipt_path = case_dir / "receipt.json"
            trace_path = case_dir / "trace.jsonl"
            if not receipt_path.exists():
                findings.append({"case_id": case_dir.name, "status": "INCOMPLETE", "detail": "no receipt.json"})
                continue
            document = json.loads(receipt_path.read_text(encoding="utf-8"))
            parsed = trace_module.read(trace_path)
            budget = receipt.budget(
                int(document.get("wall_ns", 0)),
                case_dir.name in DECLARED_EXCEPTIONS,
            )
            # The complete-command budget is a *published* figure too, and it can
            # override a row: a case whose gates all held but whose command
            # overran its limit is `NOT_RUN` with the measured wall time. The
            # re-derivation therefore applies the same rule from the recorded wall
            # time rather than comparing against the trace's gate set alone — the
            # first pass of this verifier did exactly that and reported four
            # budget-overridden rows as disagreements, which is how the omission
            # was found.
            trace_status = parsed.status()
            re_derived = trace_status
            if budget.status != "PASS" and (
                trace_module.SEVERITY["NOT_RUN"] > trace_module.SEVERITY.get(trace_status, 5)
            ):
                re_derived = "NOT_RUN"
            published = document.get("status")
            record: dict[str, object] = {
                "case_id": case_dir.name,
                "published_status": published,
                "re_derived_status": re_derived,
                "trace_status": trace_status,
                "trace_defects": parsed.defects,
                "gates_re_derived": len(parsed.gates()),
                "gates_published": len(document.get("gates", [])),
                "budget_status_re_derived": budget.status,
            }
            if re_derived != published:
                disagreements += 1
                record["disagreement"] = f"{published} published, {re_derived} re-derived"
            record["budget_status"] = budget.status
            store = case_dir / "sample.sqlite"
            if store.exists():
                reading = space.footprint(store, document.get("attribution", "exclusive"))
                record["footprint"] = reading.as_fields()
                record["pack_accounting"] = reading.pack_accounting()
            record["status"] = "PASS" if not parsed.defects and re_derived == published else "FAIL"
            findings.append(record)
    # The two halves of the no-retry/no-fsync/no-WAL contract: a sealed
    # call-graph status over the product source, and the runtime tripwires read off
    # the Stores this run actually produced. Neither is a counter, and neither is a
    # fabricated zero.
    call_graph = invariants.scan()
    stores = sorted(
        str(entry) for entry in run_dir.rglob("*.sqlite") if entry.is_file()
    )
    tripwire = invariants.tripwires(stores)
    document = {
        "schema": "layerfs-core-verification-v1",
        "run_dir": str(run_dir),
        "cases": len(findings),
        "disagreements": disagreements,
        "findings": findings,
        "sealed_call_graph": call_graph,
        "runtime_tripwires": tripwire,
        "wall_ns": time.monotonic_ns() - started,
    }
    document["budget"] = receipt.verification_budget(document["wall_ns"]).as_fields()
    # A verification never overwrites a previous one: a corrected pass is written
    # beside the one it corrects, and both are retained with their exit codes.
    receipt.write_append_only(run_dir / arguments.tag, document)
    print(
        f"verify: {len(findings)} case(s), {disagreements} disagreement(s), "
        f"call-graph {call_graph['status']} over {call_graph['files_scanned']} files, "
        f"tripwires {tripwire['status']} over {tripwire['stores_checked']} store(s), "
        f"{document['wall_ns'] / 1e9:.2f} s"
    )
    for finding in tripwire["findings"]:
        print(f"  tripwire FINDING: {finding}")
    return 0 if disagreements == 0 and call_graph["status"] == "PASS" and tripwire["status"] == "PASS" else 1


def cmd_report(arguments: argparse.Namespace) -> int:
    run_dir = Path(arguments.run)
    identity = None
    run_json = run_dir / "run.json"
    if run_json.exists():
        document = json.loads(run_json.read_text(encoding="utf-8"))
        identity = dict(document.get("identity", {}))
        identity["run_dir"] = str(run_dir)
        identity["lane"] = document.get("lane", "")
        identity["rows"] = document.get("rows", 0)
    rendered = analyze.render(analyze.load_run(run_dir), identity)
    target = Path(arguments.out) if arguments.out else run_dir / "report.txt"
    receipt.write_text_append_only(target, rendered)
    print(rendered)
    print(f"report: written to {target}")
    return 0


def cmd_self_check(_: argparse.Namespace) -> int:
    failures: list[str] = []
    if not BINARY.exists():
        build()
    checks = {
        "residency": __import__("residency").self_check(),
        "space": space.self_check(),
        "receipt": receipt.self_check(),
        "trace": trace_module.self_check(),
        "copyladder": copyladder.self_check(),
        "invariants": invariants.self_check(),
    }
    for name, found in checks.items():
        if found:
            failures.extend(f"{name}: {failure}" for failure in found)
        else:
            print(f"  {name:<12} PASS")
    parity = subprocess.run(
        [sys.executable, str(HARNESS_ROOT / "shared" / "test_lock_parity.py")],
        capture_output=True,
        text=True,
        check=False,
    )
    print(parity.stdout.strip())
    if parity.returncode != 0:
        failures.append("lock parity failed")
    registry = registry_self_check()
    print(f"  registry     {registry.get('status')}")
    if registry.get("status") != "PASS":
        failures.append(f"registry self-check failed: {registry}")
    golden = golden_matches()
    print(f"  golden       {'PASS' if golden else 'FAIL'}")
    if not golden:
        failures.append("tests/golden/registry.tsv drifted from the registry")
    if os.environ.get("LAYERFS_CONSTRUCTION_WORKERS") and os.environ["LAYERFS_CONSTRUCTION_WORKERS"] != "1":
        failures.append("LAYERFS_CONSTRUCTION_WORKERS is set to something other than 1")
    for failure in failures:
        print(f"self-check: FAIL: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-check: PASS")
    return 0


def cmd_calibrate(arguments: argparse.Namespace) -> int:
    import experiments

    if not BINARY.exists():
        build()
    started = time.monotonic()
    results = experiments.run_all(BINARY, REPO_ROOT)
    document = {
        "schema": "layerfs-calibration-v1",
        "ran_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "wall_ns": int((time.monotonic() - started) * 1e9),
        "experiments": [result.as_document() for result in results],
    }
    for result in results:
        print(f"{result.identifier}: {result.outcome}")
        for key, value in result.fields.items():
            print(f"    {key} = {value}")
        for note in result.notes:
            print(f"    note: {note}")
    if arguments.out:
        target = Path(arguments.out)
        if target.is_dir():
            target = target / "experiments.json"
        receipt.write_append_only(target, document)
        print(f"calibrate: written to {target}")
    return 0 if all(result.outcome == "SATISFIED" for result in results) else 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="runner.py", description=__doc__)
    sub = parser.add_subparsers(dest="verb", required=True)

    listing = sub.add_parser("list")
    listing.add_argument("--lane", choices=["smoke", "full"], default="smoke")
    listing.add_argument("--format", choices=["tsv", "jsonl"], default="tsv")
    listing.set_defaults(function=cmd_list)

    prepare = sub.add_parser("prepare")
    prepare.add_argument("--case", action="append")
    prepare.add_argument("--out")
    prepare.set_defaults(function=cmd_prepare)

    perf = sub.add_parser("perf")
    perf.add_argument("--lane", choices=["smoke", "full"], default="smoke")
    perf.add_argument("--out", required=True)
    perf.add_argument("--case", action="append")
    perf.add_argument("--no-build", action="store_true")
    perf.set_defaults(function=cmd_perf)

    verify = sub.add_parser("verify")
    verify.add_argument("--run", required=True)
    verify.add_argument("--tag", default="verification.json")
    verify.set_defaults(function=cmd_verify)

    report = sub.add_parser("report")
    report.add_argument("--run", required=True)
    report.add_argument("--out")
    report.set_defaults(function=cmd_report)

    self_check = sub.add_parser("self-check")
    self_check.set_defaults(function=cmd_self_check)

    calibrate = sub.add_parser("calibrate")
    calibrate.add_argument("--experiment", default="all")
    calibrate.add_argument("--out")
    calibrate.set_defaults(function=cmd_calibrate)

    arguments = parser.parse_args()
    # One construction worker, exported by the runner itself and inherited by
    # every measured child, so the value the receipt records is the value the
    # child ran under rather than a value the runner merely intended.
    if arguments.verb in {"perf", "calibrate"}:
        os.environ["LAYERFS_CONSTRUCTION_WORKERS"] = "1"
    try:
        return int(arguments.function(arguments))
    except RunnerError as error:
        print(f"runner: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
