#!/usr/bin/env python3
import hashlib
import json
import pathlib
import sys


def sha256(path):
    digest = hashlib.sha256()
    digest.update(path.read_bytes())
    return digest.hexdigest()


def main(raw_path, output_path):
    rows = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines() if line]
    index = {(row["history_revisions"], row["sample"]): row for row in rows}
    keys = sorted(index)
    expected = [(history, sample) for history in (1, 10, 100, 1000) for sample in (1, 2)]
    assert keys == expected
    exclusions = {
        "reopen_head": {"wall_ns"},
        "head_lookup": {"wall_ns"},
        "range_read": {"wall_ns"},
        "reconstruction": {"wall_ns"},
        "materialization": {"wall_ns", "verification_ns", "cleanup_ns", "user_us", "system_us", "voluntary_switches", "involuntary_switches"},
    }
    parity = {}
    for operation, excluded in exclusions.items():
        names = sorted(set(index[keys[0]][operation]) - excluded)
        values = {name: sorted({index[key][operation][name] for key in keys}) for name in names}
        assert all(len(observed) == 1 for observed in values.values())
        parity[operation] = {name: observed[0] for name, observed in values.items()}
    edit_names = sorted(set(index[(1, 1)]["first_edit_after_reopen"]) - {"wall_ns"})
    edit_classes = {}
    for label, members in {
        "genesis_n1": [(1, 1), (1, 2)],
        "non_genesis_n10_n100_n1000": [(history, sample) for history in (10, 100, 1000) for sample in (1, 2)],
    }.items():
        values = {name: sorted({index[key]["first_edit_after_reopen"][name] for key in members}) for name in edit_names}
        assert all(len(observed) == 1 for observed in values.values())
        edit_classes[label] = {name: observed[0] for name, observed in values.items()}
    result = {
        "schema": "phase4-g5-h11-emitted-work-audit-v1",
        "status": "PASS",
        "classification": "post-terminal fresh raw audit; not a sealed H11 analyzer artifact",
        "raw_sha256": sha256(raw_path),
        "rows": len(rows),
        "all_non_timing_nested_operation_fields_checked": True,
        "parity": parity,
        "first_edit_mechanism_classes": edit_classes,
        "reopen_scope": "emitted logical counters only; preflight/open SQLite work is not completely instrumented",
    }
    output_path.write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: recompute_emitted_work.py RAW OUTPUT")
    main(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))
