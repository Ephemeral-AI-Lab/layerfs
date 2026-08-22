#!/usr/bin/env python3
import json
import sys


SCHEDULE = [
    (1, "R01-warm-1m", "r01", 1),
    (2, "R01-warm-10m", "r01", 10),
    (3, "R01-warm-100m-primary", "r01", 100),
    (4, "R1-fresh-100m", "r1-fresh", 100),
    (5, "R1-controlled-cold-100m", "cold-unavailable", 100),
    (6, "S1-seed-read-10m", "seed-read", 10),
    (7, "S1-seed-read-100m-primary", "seed-read", 100),
    (8, "R4-returned-range-1m", "fast-guard", 100),
    (9, "M0-control-1m", "m0-control", 1),
    (10, "M0-control-10m", "m0-control", 10),
    (11, "M0-control-100m", "m0-control", 100),
    (12, "M0-candidate-1m", "m0-candidate", 1),
    (13, "M0-candidate-10m", "m0-candidate", 10),
    (14, "M0-candidate-100m-primary", "m0-candidate", 100),
    (15, "M0-candidate-controlled-cold-100m", "cold-unavailable", 100),
    (16, "S1-clone-noop-10m", "g3", 10),
    (17, "S1-clone-noop-100m", "g3", 100),
    (18, "S1-one-byte-100m", "g3", 100),
    (19, "S1-one-mib-10m", "g3", 10),
    (20, "S1-count-change-1m", "g3", 1),
    (21, "S1-count-change-100m", "g3", 100),
    (22, "S1-invalid-authority-1m", "g3", 1),
    (23, "S1-invalid-authority-100m", "g3", 100),
    (24, "S1-external-mutation-1m", "g3", 1),
    (25, "S1-symlink-1m", "g3", 1),
    (26, "S1-before-publication-1m", "g3", 1),
    (27, "S1-lost-ack-1m", "g3", 1),
    (28, "guard-full-create-100m", "fast-guard", 100),
    (29, "guard-same-count-edit-100m", "fast-guard", 100),
    (30, "guard-reopen-head-100m", "fast-guard", 100),
]

EXPECTED = {
    "matrix_record_count": 30,
    "r01_arm_observations": 9,
    "cold_administrative_records": 2,
    "seed_full_read_timed_passes": 4,
    "row_replacement_or_rerun_count": 0,
    "total_arm_observations": 50,
}


def assert_schedule():
    assert [row[0] for row in SCHEDULE] == list(range(1, 31))
    assert len({row[1] for row in SCHEDULE}) == 30
    assert sum(3 if row[2] == "r01" else 2 if row[2] in {"g3", "fast-guard"} else 0 if row[2] == "cold-unavailable" else 1 for row in SCHEDULE) == EXPECTED["total_arm_observations"]
    return {
        "schema": "phase4-g4-schedule-v1",
        "status": "PASS",
        "expected": EXPECTED,
        "schedule": [
            {"sequence": sequence, "record": record, "kind": kind, "size_mib": size}
            for sequence, record, kind, size in SCHEDULE
        ],
        "actual_rows": 0,
        "benchmark_children_invoked": 0,
        "database_copies_created": 0,
    }


if __name__ == "__main__":
    if sys.argv[1:] != ["--dry-run"]:
        raise SystemExit("usage: schedule_g4_v1.py --dry-run")
    print(json.dumps(assert_schedule(), indent=2, sort_keys=True))
