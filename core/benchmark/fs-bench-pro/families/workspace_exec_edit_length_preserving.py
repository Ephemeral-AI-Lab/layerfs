"""#232 Workspace Exec/FUSE length-preserving family: 12 registered cases.

The semantic shape (overwrite head/middle/tail with a 4 KiB payload on
1/10/100/500 MiB inputs) and every payload, offset and historical G2 target come
from the v0.1.6 `edit_length_preserving` family. The scenarios are separately
registered `-exec-v1` Workspace Exec/FUSE cases; they inherit no direct-SDK
`edit_*` status.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from shared import edit_contract as contract  # noqa: E402

FAMILY_ID = "workspace_exec_edit_length_preserving"
HISTORICAL_FAMILY = "edit_length_preserving"

OPERATIONS = [
    {
        "key": "overwrite-head-4k",
        "algorithm": "positional-write",
        "replacement_kind": "inline",
        "replacement_len": 4096,
        "payload_seed": 3_686_764_519_212_284_394,
        "payload_sha256": "faca857b3e7f8b1b8f46c19b1625a1b9995248ae9ac85eae672b85fbc9932375",
        "locate": lambda size: (0, 4096),
        "fixture_override": {},
        "capped_labels": (),
    },
    {
        "key": "overwrite-middle-4k",
        "algorithm": "positional-write",
        "replacement_kind": "inline",
        "replacement_len": 4096,
        "payload_seed": 10_800_757_348_883_211_881,
        "payload_sha256": "f2ebe7d18fdbdd17c8aaff760ad8eeb0dfd82dadf874c41dad175ff916b2a6c5",
        "locate": lambda size: (size // 2 - 2048, 4096),
        "fixture_override": {},
        "capped_labels": (),
    },
    {
        "key": "overwrite-tail-4k",
        "algorithm": "positional-write",
        "replacement_kind": "inline",
        "replacement_len": 4096,
        "payload_seed": 3_866_307_116_232_060_780,
        "payload_sha256": "37cf5837649769db2eccb504f2af96c69b9e514304374ed7d74c3b1299c2f385",
        "locate": lambda size: (size - 4096, 4096),
        "fixture_override": {},
        "capped_labels": (),
    },
]

# Frozen per-case engineering target: the same historical selection's G2
# candidate `edit_commit_ns` from the #152 final report, at 0.01 ms precision.
TARGETS_MS = {
    "overwrite-head-4k-on-1mib-ops-1": 6.31,
    "overwrite-middle-4k-on-1mib-ops-1": 5.63,
    "overwrite-tail-4k-on-1mib-ops-1": 4.72,
    "overwrite-head-4k-on-10mib-ops-1": 5.90,
    "overwrite-middle-4k-on-10mib-ops-1": 5.70,
    "overwrite-tail-4k-on-10mib-ops-1": 6.06,
    "overwrite-head-4k-on-100mib-ops-1": 5.81,
    "overwrite-middle-4k-on-100mib-ops-1": 6.43,
    "overwrite-tail-4k-on-100mib-ops-1": 5.82,
    "overwrite-head-4k-on-500mib-ops-1": 7.34,
    "overwrite-middle-4k-on-500mib-ops-1": 10.01,
    "overwrite-tail-4k-on-500mib-ops-1": 7.37,
}


def registry():
    return contract.family_registry(FAMILY_ID, OPERATIONS, TARGETS_MS)
