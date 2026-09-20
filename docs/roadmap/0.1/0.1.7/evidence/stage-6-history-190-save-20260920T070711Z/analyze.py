#!/usr/bin/env python3
"""Re-derive this round's four runs, with the runner's own phase functions.

Three questions:

1. Do the declared phases account for each invocation? `phases.compose` is the
   runner's own, imported rather than reimplemented.
2. Does the round reproduce the retained scope result under the new harness, and are
   the two arms equivalent (state roots, content counters, saved Store bytes)?
3. **What is `storage.accept_loop` made of?** The save's own counters are published
   per state now, so the span's name can be replaced by the work that happened:
   how many objects were prepared FULL, how many prefix trials ran and what they
   read, how many values were written or reused, how many statements, commits and
   pack appends. The counters localise *work*, never time - what they cannot split
   is stated in the output rather than inferred.
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
RECORDED_STORE = {
    "history-stride10": "4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487",
    "history-stride3": "f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e",
}
SAVE = (
    ("inserted", "objects written"),
    ("save.reused", "occurrences served by an exact row"),
    ("save.delta.prepared_full", "FULL alternatives compressed"),
    ("save.delta.trials", "prefix trials"),
    ("save.delta.prefix_selected", "trials that chose PREFIX"),
    ("save.prefix_records", "PREFIX records stored"),
    ("save.full_records", "FULL records stored"),
    ("save.chain.objects", "chain objects read for trials"),
    ("save.chain.edges", "chain edges walked"),
    ("save.chain.encoded_bytes", "encoded bytes read for trials"),
    ("save.pool.new_values", "values given a new ordinal"),
    ("save.pool.reused_values", "values reusing an ordinal"),
    ("save.pool.groups", "value groups written"),
    ("save.pool.leaves", "pooled leaves admitted"),
    ("save.statements", "INSERT statements"),
    ("save.presence_queries", "presence queries"),
    ("save.packs_created", "packs created"),
    ("save.pack_appends", "pack BLOB rewrites"),
    ("save.commits", "commits"),
)
INVARIANTS = ("changed_bytes", "changed_paths", "objects", "inserted", "root")


def records(path: Path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def value(rows, key, kind=None):
    for row in rows:
        if row.get("key") == key and (kind is None or row.get("kind") == kind):
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


def subphases(raw: Path):
    tree = json.loads((raw / "timing.json").read_text())
    outer, inner = {}, {}
    for state in tree["children"]:
        for node in state.get("children", []):
            outer[node["name"]] = outer.get(node["name"], 0) + int(node["elapsed_ns"])
            if node["name"] == "filesystem":
                for child in node.get("children", []):
                    inner[child["name"]] = inner.get(child["name"], 0) + int(child["elapsed_ns"])
    return outer, inner


def main() -> int:
    report = {"schema": "h190-save-attribution-analysis-v1", "runs": {}, "save": {}, "equivalence": {}}
    lines, save_lines, equiv = [], [], []

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
            outer, _ = subphases(raw)
            report["runs"][f"{arm}-{case}"] = {
                "wall_ns": wall,
                "phases": {key: composed[key] for key in (
                    "preparation_wall_ns", "operation_ns", "verification_wall_ns", "cleanup_wall_ns")},
                "declared_ns": declared,
                "reconciliation": composed["reconciliation"],
                "outside_child_clock_ns": wall - composed["invocations"][0]["invocation_ns"],
                "budget": {"ordinary": ordinary.status, "declared_exception": exception.status},
                "corpus_resident_pages": value(rows, "history.corpus.probe.resident_pages"),
                "corpus_pages": value(rows, "history.corpus.probe.pages"),
                "subphases_ns": outer,
            }
            entry = report["runs"][f"{arm}-{case}"]
            lines.append(f"  {arm:9s} wall {wall / 1e9:7.3f} s | preparation"
                         f" {composed['preparation_wall_ns'] / 1e9:7.3f} s | OPERATION"
                         f" {composed['operation_ns'] / 1e9:7.3f} s | accept_loop"
                         f" {outer.get('storage.accept_loop', 0) / 1e9:7.3f} s | filesystem"
                         f" {outer.get('filesystem', 0) / 1e9:7.3f} s")
            lines.append(f"            reconciliation {composed['reconciliation']['status']}"
                         f" | outside the child's clock {entry['outside_child_clock_ns'] / 1e6:.1f} ms"
                         f" | budget {ordinary.status} / {exception.status}"
                         f" | corpus resident {entry['corpus_resident_pages']:,}"
                         f" of {entry['corpus_pages']:,}")
        base, cand = (report["runs"][f"{arm}-{case}"] for arm in ARMS)
        lines.append(f"  delta   operation"
                     f" {cand['phases']['operation_ns'] / 1e9 - base['phases']['operation_ns'] / 1e9:+.3f} s"
                     f" | accept_loop"
                     f" {(cand['subphases_ns'].get('storage.accept_loop', 0) - base['subphases_ns'].get('storage.accept_loop', 0)) / 1e9:+.3f} s"
                     f" | filesystem"
                     f" {(cand['subphases_ns'].get('filesystem', 0) - base['subphases_ns'].get('filesystem', 0)) / 1e9:+.3f} s")
        lines.append("")

        # ---- the save's own counters, per state -----------------------------
        base_states = per_state(records(CAMPAIGN / "runs" / f"baseline-{case}" / "raw" / "trace.jsonl"))
        cand_states = per_state(records(CAMPAIGN / "runs" / f"candidate-{case}" / "raw" / "trace.jsonl"))
        ordinals = sorted(base_states)
        half = max(ordinals) // 2
        early = [o for o in ordinals if 2 <= o <= 6]
        late = [o for o in ordinals if o > max(ordinals) - 5]

        def totals(states, key):
            return sum(states[o].get(key, 0) for o in states)

        def per_object(states, group, key):
            objects = totals({o: states[o] for o in group}, "inserted")
            return totals({o: states[o] for o in group}, key) / objects if objects else float("nan")

        save_lines.append(f"=== {case}: the save's own work, chain totals")
        save_lines.append(f"  {'counter':24s} {'baseline':>14} {'candidate':>14}  {'per inserted object (early -> late)':>36}")
        for key, label in SAVE:
            full = key
            b, c = totals(base_states, full), totals(cand_states, full)
            e, l = per_object(base_states, early, full), per_object(cand_states, late, full)
            unit = ""
            if "bytes" in key:
                b, c, unit = b / 1e6, c / 1e6, " MB"
                e, l = e / 1e6, l / 1e6
            save_lines.append(f"  {label:24s} {b:>13,.2f}{unit} {c:>13,.2f}{unit}"
                              f"  {e:>15.4f} -> {l:<15.4f}")
        save_lines.append("")
        save_lines.append(f"  accept_loop per inserted object: "
                          f"{report['runs'][f'baseline-{case}']['subphases_ns'].get('storage.accept_loop', 0) / totals(base_states, 'inserted') / 1e3:.1f} us"
                          f" (baseline, whole run); early states"
                          f" {sum(base_states[o].get('inserted', 0) for o in early):,} objects,"
                          f" late states {sum(base_states[o].get('inserted', 0) for o in late):,}")
        save_lines.append("")
        report["save"][case] = {
            "totals": {key: totals(cand_states, key) for key, _ in SAVE},
            "per_object_early": {key: per_object(cand_states, early, key) for key, _ in SAVE},
            "per_object_late": {key: per_object(cand_states, late, key) for key, _ in SAVE},
            "identical_between_arms": all(
                totals(base_states, key) == totals(cand_states, key) for key, _ in SAVE),
        }

        # ---- equivalence -----------------------------------------------------
        roots = all(base_states[o].get("root") == cand_states[o].get("root") for o in ordinals)
        invariants = {name: all(base_states[o].get(name) == cand_states[o].get(name)
                                for o in ordinals) for name in INVARIANTS}
        stores = {arm: store_sha(CAMPAIGN / "runs" / f"{arm}-{case}" / "raw" / "sample.sqlite")
                  for arm in ARMS}
        uri = f"file:{CAMPAIGN / 'runs' / f'candidate-{case}' / 'raw' / 'sample.sqlite'}?mode=ro"
        with sqlite3.connect(uri, uri=True) as connection:
            packs, pack_bytes = connection.execute(
                "SELECT COUNT(*), SUM(LENGTH(data)) FROM object_packs").fetchone()
            groups, objects = connection.execute(
                "SELECT COUNT(DISTINCT pack_id || ':' || group_number), COUNT(*) FROM objects").fetchone()
            vgroups = connection.execute("SELECT COUNT(*) FROM metadata_value_groups").fetchone()[0]
            singleton = connection.execute(
                "SELECT COUNT(*) FROM (SELECT pack_id, group_number, COUNT(*) c FROM objects"
                " GROUP BY 1,2 HAVING c = 1)").fetchone()[0]
        report["equivalence"][case] = {
            "state_roots_equal": roots, "state_counters_equal": invariants,
            "store_sha256": stores,
            "store_matches_recorded_constant": {a: stores[a] == RECORDED_STORE[case] for a in ARMS},
            "store_identical_between_arms": stores["baseline"] == stores["candidate"],
            "store": {"packs": packs, "pack_bytes": pack_bytes, "ordinary_groups": groups,
                      "objects": objects, "value_groups": vgroups, "singleton_groups": singleton},
        }
        equiv.append(f"=== {case}")
        equiv.append(f"  every state root equal          : {roots}")
        for name, ok in invariants.items():
            equiv.append(f"  state {name:14s} equal      : {ok}")
        for arm in ARMS:
            equiv.append(f"  {arm:9s} Store sha256 : {stores[arm]}"
                         f"  {'== recorded constant' if stores[arm] == RECORDED_STORE[case] else '!= RECORDED'}")
        equiv.append(f"  byte-identical between arms     : {stores['baseline'] == stores['candidate']}")
        equiv.append(f"  Store shape                     : {report['equivalence'][case]['store']}")
        equiv.append(f"  save counters identical between arms: {report['save'][case]['identical_between_arms']}")
        equiv.append("")

    (CAMPAIGN / "analysis.json").write_text(json.dumps(report, indent=2, sort_keys=False) + "\n")
    (CAMPAIGN / "analysis.txt").write_text("\n".join(lines) + "\n")
    (CAMPAIGN / "save.txt").write_text("\n".join(save_lines) + "\n")
    (CAMPAIGN / "equivalence.txt").write_text("\n".join(equiv) + "\n")
    print("\n".join(lines))
    print("\n".join(save_lines))
    print("\n".join(equiv))
    return 0


if __name__ == "__main__":
    sys.exit(main())
