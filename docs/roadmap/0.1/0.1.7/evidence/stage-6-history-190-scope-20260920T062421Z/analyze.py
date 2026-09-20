#!/usr/bin/env python3
"""Re-derive the four diagnostic runs, with the runner's own phase functions.

Three questions, answered from this round's artifacts and nothing else:

1. Do the declared phases account for each invocation? `phases.compose` is the
   runner's own, imported rather than reimplemented, and a row that does not
   reconcile is reported as INCOMPLETE - never rounded into a pass.
2. What did each arm cost: complete command, preparation, operation?
3. Are the arms equivalent: every state root, every state's content counters, the
   canonical object count, the value-group inventory, and the saved Store byte for
   byte against the constants the retained campaign recorded?

The cache-state asymmetry is published beside the numbers rather than assumed away:
the corpus probe reports, per run, how many of the corpus pages the chain reads were
already resident when it first asked for them. Preparation differences are a
consequence of that and are never credited to the operation.
"""
import hashlib
import json
import sqlite3
import sys
from pathlib import Path

CAMPAIGN = Path(__file__).resolve().parent
REPO = Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope")
sys.path.insert(0, str(REPO / "core/benchmark/fs-bench-pro-storage-content/shared"))
import phases as phases_module  # noqa: E402
import receipt  # noqa: E402

ARMS = ("baseline", "candidate")
CASES = ("history-stride10", "history-stride3")
# Recorded by the retained campaign (L42/L47) for the same two selections, so the
# equivalence gate is checked against a constant and not against a new sample.
RECORDED_STORE = {
    "history-stride10": "4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487",
    "history-stride3": "f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e",
}
P = "filesystem.provider.pooled."
COUNTERS = ("pack_fetches", "pack_bytes", "value_group_decodes", "physical_record_calls",
            "physical_group_decodes", "physical_group_cache_hits", "chain_edges", "leaf_requests")
# Per-state content counters that must not move: they are what the operation
# *did*, not what it cost.
INVARIANTS = ("changed_bytes", "changed_paths", "objects", "inserted", "root")


def records(path: Path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def value(rows, key, kind):
    for row in rows:
        if row.get("kind") == kind and row.get("key") == key:
            return row.get("value")
    return None


def per_state(rows):
    out = {}
    for row in rows:
        key = str(row.get("key", ""))
        if not key.startswith("history.state.") or not isinstance(row.get("value"), (int, str)):
            continue
        parts = key.split(".")
        if parts[2].isdigit():
            out.setdefault(int(parts[2]), {})[".".join(parts[3:])] = row["value"]
    return out


def store_sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def inventory(path: Path):
    uri = f"file:{path}?mode=ro"
    with sqlite3.connect(uri, uri=True) as connection:
        objects = connection.execute("SELECT COUNT(*) FROM objects").fetchone()[0]
        packs, pack_bytes = connection.execute(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(data)), 0) FROM object_packs").fetchone()
        groups, group_values = connection.execute(
            "SELECT COUNT(*), COALESCE(SUM(count), 0) FROM metadata_value_groups").fetchone()
    return {"objects": objects, "packs": packs, "pack_bytes": pack_bytes,
            "groups": groups, "group_values": group_values}


def main() -> int:
    report = {"schema": "h190-pooled-scope-analysis-v1", "runs": {}, "equivalence": {}}
    lines, counter_lines, equiv_lines = [], [], []

    for case in CASES:
        lines.append(f"=== {case}")
        for arm in ARMS:
            run = CAMPAIGN / "runs" / f"{arm}-{case}"
            raw = run / "raw"
            wall = int(json.loads((run / "perf-receipt.json").read_text())["wall_ns"])
            rows = records(raw / "trace.jsonl")
            composed = phases_module.compose(raw, {"perf": wall}, expects_operation=True)
            declared = sum(int(composed.get(f, 0) or 0) for f in (
                "preparation_wall_ns", "operation_ns", "verification_wall_ns", "cleanup_wall_ns"))
            ordinary = receipt.budget(wall, False, declared_ns=declared)
            exception = receipt.budget(wall, True, declared_ns=declared)
            invocation = composed["invocations"][0]
            diagnostics = {key: value(rows, key, "resource") for key in (
                "history.corpus_read_ns", "history.corpus.probe.files", "history.corpus.probe.bytes",
                "history.corpus.probe.pages", "history.corpus.probe.resident_pages",
                "history.operation.disk_read_bytes")}
            verification = {}
            verify_receipt = run / "verify-receipt.json"
            if verify_receipt.exists():
                vr = json.loads(verify_receipt.read_text())
                verification = {
                    "wall_ns": vr["wall_ns"],
                    "store_sha256": vr["store_sha256"],
                    "trace_sha256_before": vr["trace_sha256_before"],
                    "trace_sha256_after": vr["trace_sha256_after"],
                    "compared": value(rows, "verify.compared", "counter"),
                    "path_states": value(rows, "verify.path_states", "counter"),
                    "mismatches": value(rows, "verify.mismatches", "counter"),
                    "missing": value(rows, "verify.missing", "counter"),
                    "unexpected": value(rows, "verify.unexpected", "counter"),
                    "files_read": value(rows, "verify.files_read", "counter"),
                    "file_bytes": value(rows, "verify.file_bytes", "counter"),
                    "sampled": value(rows, "verify.sampled", "counter"),
                }
            report["runs"][f"{arm}-{case}"] = {
                "verification": verification,
                "invocations": [{"invocation": item["invocation"],
                                 "invocation_ns": item["invocation_ns"],
                                 "declared_ns": item["declared_ns"],
                                 "reconciles": item["reconciles"]}
                                for item in composed["invocations"]],
                "wall_ns": wall,
                "declared_ns": declared,
                "phases": {key: composed[key] for key in (
                    "preparation_wall_ns", "operation_ns", "verification_wall_ns",
                    "cleanup_wall_ns", "handoff_ns")},
                "reconciliation": composed["reconciliation"],
                "child_invocation_ns": invocation["invocation_ns"],
                "child_unaccounted_ns": invocation["unaccounted_ns"],
                "outside_child_clock_ns": wall - invocation["invocation_ns"],
                "budget": {"ordinary": ordinary.status, "declared_exception": exception.status},
                "diagnostics": diagnostics,
                "root_ns": value(rows, "history.root_ns", "counter"),
                "children_ns": value(rows, "history.children_ns", "counter"),
            }
            entry = report["runs"][f"{arm}-{case}"]
            lines.append(f"  {arm:9s} wall {wall / 1e9:7.3f} s | preparation"
                         f" {composed['preparation_wall_ns'] / 1e9:7.3f} s | OPERATION"
                         f" {composed['operation_ns'] / 1e9:7.3f} s | verification"
                         f" {composed['verification_wall_ns'] / 1e9:6.3f} s | declared"
                         f" {declared / 1e9:7.3f} s")
            lines.append(f"            child invocation {invocation['invocation_ns'] / 1e9:.3f} s,"
                         f" unaccounted inside the child {invocation['unaccounted_ns'] / 1e6:.1f} ms,"
                         f" outside the child's clock {entry['outside_child_clock_ns'] / 1e6:.1f} ms")
            lines.append(f"            reconciliation {composed['reconciliation']['status']}"
                         f" | budget ordinary {ordinary.status} / declared exception {exception.status}")
            lines.append(f"            corpus {diagnostics['history.corpus_read_ns'] / 1e9:.3f} s,"
                         f" resident before first read"
                         f" {diagnostics['history.corpus.probe.resident_pages']:,} of"
                         f" {diagnostics['history.corpus.probe.pages']:,} pages"
                         f" ({diagnostics['history.corpus.probe.resident_pages'] / diagnostics['history.corpus.probe.pages']:.2%})")
        for arm in ARMS:
            entry = report["runs"][f"{arm}-{case}"]
            if entry["verification"]:
                v = entry["verification"]
                lines.append(f"            verification {arm:9s} {v['wall_ns'] / 1e9:6.3f} s,"
                             f" {v['compared']:,} of {v['path_states']:,} path-states sampled,"
                             f" mismatches {v['mismatches']}, missing {v['missing']},"
                             f" unexpected {v['unexpected']}, {v['files_read']:,} files read")
        base, cand = (report["runs"][f"{arm}-{case}"] for arm in ARMS)
        lines.append(f"  delta   wall {cand['wall_ns'] / 1e9 - base['wall_ns'] / 1e9:+.3f} s |"
                     f" preparation {cand['phases']['preparation_wall_ns'] / 1e9 - base['phases']['preparation_wall_ns'] / 1e9:+.3f} s |"
                     f" OPERATION {cand['phases']['operation_ns'] / 1e9 - base['phases']['operation_ns'] / 1e9:+.3f} s")
        lines.append("")

        # ---- per-state counters: the mechanism test
        base_states = per_state(records(CAMPAIGN / "runs" / f"baseline-{case}" / "raw" / "trace.jsonl"))
        cand_states = per_state(records(CAMPAIGN / "runs" / f"candidate-{case}" / "raw" / "trace.jsonl"))
        counter_lines.append(f"=== {case}: pooled read work per state, baseline -> candidate")
        counter_lines.append(f"  {'st':>3} {'changed MB':>10}" + "".join(f" {name[:14]:>15}" for name in COUNTERS))
        totals = {arm: {name: 0 for name in COUNTERS} for arm in ARMS}
        for ordinal in sorted(base_states):
            b, c = base_states[ordinal], cand_states[ordinal]
            cells = []
            for name in COUNTERS:
                bv = b.get(P + name, 0)
                cv = c.get(P + name, 0)
                totals["baseline"][name] += bv
                totals["candidate"][name] += cv
                cells.append(f" {bv:>7,}->{cv:<7,}")
            counter_lines.append(f"  {ordinal:>3} {b.get('changed_bytes', 0) / 1e6:>10.2f}" + "".join(cells))
        counter_lines.append("  totals")
        for name in COUNTERS:
            bv, cv = totals["baseline"][name], totals["candidate"][name]
            ratio = f"{cv / bv:.3f}x" if bv else "n/a"
            counter_lines.append(f"    {name:22s} {bv:>12,} -> {cv:>12,}  {ratio}")
        report.setdefault("counters", {})[case] = totals
        counter_lines.append("")

        # ---- equivalence
        roots_equal = all(base_states[o].get("root") == cand_states[o].get("root")
                          for o in base_states)
        invariants = {}
        for name in INVARIANTS:
            key = "root" if name == "root" else name
            invariants[name] = all(base_states[o].get(key) == cand_states[o].get(key)
                                   for o in base_states)
        stores, inventories = {}, {}
        for arm in ARMS:
            path = CAMPAIGN / "runs" / f"{arm}-{case}" / "raw" / "sample.sqlite"
            stores[arm] = store_sha(path)
            inventories[arm] = inventory(path)
        entry = {
            "state_roots_equal": roots_equal,
            "state_counters_equal": invariants,
            "store_sha256": stores,
            "store_matches_recorded_constant": {
                arm: stores[arm] == RECORDED_STORE[case] for arm in ARMS},
            "store_identical_between_arms": stores["baseline"] == stores["candidate"],
            "inventory": inventories,
            "inventory_equal": inventories["baseline"] == inventories["candidate"],
        }
        report["equivalence"][case] = entry
        equiv_lines.append(f"=== {case}")
        equiv_lines.append(f"  every state root equal            : {roots_equal}")
        for name, equal in invariants.items():
            equiv_lines.append(f"  state {name:14s} equal        : {equal}")
        for arm in ARMS:
            equiv_lines.append(f"  {arm:9s} Store sha256 : {stores[arm]}"
                               f"  {'== recorded constant' if entry['store_matches_recorded_constant'][arm] else '!= RECORDED CONSTANT'}")
        equiv_lines.append(f"  byte-identical between arms       : {entry['store_identical_between_arms']}")
        verification = {arm: report["runs"][f"{arm}-{case}"]["verification"] for arm in ARMS}
        entry["verification_equal_between_arms"] = verification["baseline"] == verification["candidate"]
        entry["verification_sampled"] = True
        equiv_lines.append(f"  verification (sampled, INCOMPLETE) : "
                           f"{verification['baseline']['compared']:,} of"
                           f" {verification['baseline']['path_states']:,} path-states,"
                           f" mismatches {verification['baseline']['mismatches']},"
                           f" missing {verification['baseline']['missing']},"
                           f" unexpected {verification['baseline']['unexpected']}")
        equiv_lines.append(f"  verification equal between arms   : {entry['verification_equal_between_arms']}"
                           f" (walls {verification['baseline']['wall_ns'] / 1e9:.3f} s vs"
                           f" {verification['candidate']['wall_ns'] / 1e9:.3f} s)")
        equiv_lines.append(f"  canonical/value-group inventory   : {inventories['baseline']}")
        equiv_lines.append(f"  inventory equal between arms      : {entry['inventory_equal']}")
        equiv_lines.append("")

    (CAMPAIGN / "analysis.json").write_text(json.dumps(report, indent=2, sort_keys=False) + "\n")
    (CAMPAIGN / "analysis.txt").write_text("\n".join(lines) + "\n")
    (CAMPAIGN / "counters.txt").write_text("\n".join(counter_lines) + "\n")
    (CAMPAIGN / "equivalence.txt").write_text("\n".join(equiv_lines) + "\n")
    print("\n".join(lines))
    print("\n".join(equiv_lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
