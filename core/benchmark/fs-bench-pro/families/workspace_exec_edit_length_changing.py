"""#232 Workspace Exec/FUSE length-changing family: 32 registered cases.

Twelve of the thirty-two cases are feasible with the declared POSIX size
operations (`append-tail-4k`, `truncate-tail-4k`, `zero-extend-tail-4k`). The
twenty structural cases (insert/delete/prepend/replace-grow/replace-shrink) stay
registered and visible as NOT_RUN under the frozen
`structural-shift-algorithm-unfrozen` reason: no authentic POSIX/FUSE algorithm
is frozen for them yet, their in-place window shift would move up to hundreds of
MiB through the projection, and a temporary-file-and-rename save needs up to
500 MiB against a 16 MiB `/tmp` and a 1 GiB Workspace disk budget.

The five 500 MiB `result-capped-v2` selections keep their v0.1.6 smaller input
sizes (four at 524,283,904 bytes and one at 524,285,952 bytes); no capped-v1
duplicate is added.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from shared import edit_contract as contract  # noqa: E402

FAMILY_ID = "workspace_exec_edit_length_changing"
HISTORICAL_FAMILY = "edit_length_changing"

OPERATIONS = [
    {
        "key": "insert-middle-4k",
        "algorithm": "structural-shift",
        "replacement_kind": "inline",
        "replacement_len": 4096,
        "payload_seed": 6_313_238_748_831_594_097,
        "payload_sha256": "568c5408a3f292d4a593d5ffa43736b790b6a5dac749427b0ad53c765e672616",
        "locate": lambda size: (size // 2, 0),
        "fixture_override": {"500": 524_283_904},
        "capped_labels": ("500",),
    },
    {
        "key": "delete-middle-4k",
        "algorithm": "structural-shift",
        "replacement_kind": "inline",
        "replacement_len": 0,
        "payload_seed": 14_631_710_363_380_426_233,
        "payload_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "locate": lambda size: (size // 2 - 2048, 4096),
        "fixture_override": {},
        "capped_labels": (),
    },
    {
        "key": "append-tail-4k",
        "algorithm": "positional-write",
        "replacement_kind": "inline",
        "replacement_len": 4096,
        "payload_seed": 10_524_769_729_031_953_950,
        "payload_sha256": "50495bfeedccb8983ead82f1fc3a55b7a45bb5741ecc79970b8a846616f95d22",
        "locate": lambda size: (size, 0),
        "fixture_override": {"500": 524_283_904},
        "capped_labels": ("500",),
    },
    {
        "key": "prepend-head-4k",
        "algorithm": "structural-shift",
        "replacement_kind": "inline",
        "replacement_len": 4096,
        "payload_seed": 11_539_886_650_128_519_955,
        "payload_sha256": "a675326972c1cb5168b42d324036fab260ccb91b9df982eaace85cd05682cbdb",
        "locate": lambda size: (0, 0),
        "fixture_override": {"500": 524_283_904},
        "capped_labels": ("500",),
    },
    {
        "key": "replace-grow-middle-2k-to-4k",
        "algorithm": "structural-shift",
        "replacement_kind": "inline",
        "replacement_len": 4096,
        "payload_seed": 6_297_716_278_452_303_078,
        "payload_sha256": "5d682316189ab7e945b298632c929e67f90a2b1aa13987181aef3f501421d93e",
        "locate": lambda size: (size // 2 - 1024, 2048),
        "fixture_override": {"500": 524_285_952},
        "capped_labels": ("500",),
    },
    {
        "key": "replace-shrink-middle-4k-to-2k",
        "algorithm": "structural-shift",
        "replacement_kind": "inline",
        "replacement_len": 2048,
        "payload_seed": 1_824_427_086_451_703_536,
        "payload_sha256": "5e24d5a26d23669833f39b2c5b3c9f6a3620ba16d3505c710020d31704a8744b",
        "locate": lambda size: (size // 2 - 2048, 4096),
        "fixture_override": {},
        "capped_labels": (),
    },
    {
        "key": "truncate-tail-4k",
        "algorithm": "size-truncate",
        "replacement_kind": "inline",
        "replacement_len": 0,
        "payload_seed": 9_706_727_036_258_497_900,
        "payload_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "locate": lambda size: (size - 4096, 4096),
        "fixture_override": {},
        "capped_labels": (),
    },
    {
        "key": "zero-extend-tail-4k",
        "algorithm": "size-extend",
        "replacement_kind": "zero",
        "replacement_len": 4096,
        "payload_seed": 7_852_731_424_507_589_290,
        "payload_sha256": "ad7facb2586fc6e966c004d7d1d16b024f5805ff7cb47c7a85dabd8b48892ca7",
        "locate": lambda size: (size, 0),
        "fixture_override": {"500": 524_283_904},
        "capped_labels": ("500",),
    },
]

STRUCTURAL_KEYS = (
    "insert-middle-4k",
    "delete-middle-4k",
    "prepend-head-4k",
    "replace-grow-middle-2k-to-4k",
    "replace-shrink-middle-4k-to-2k",
)

TARGETS_MS = {
    "append-tail-4k-on-1mib-ops-1": 5.76,
    "delete-middle-4k-on-1mib-ops-1": 5.06,
    "insert-middle-4k-on-1mib-ops-1": 5.30,
    "prepend-head-4k-on-1mib-ops-1": 5.12,
    "replace-grow-middle-2k-to-4k-on-1mib-ops-1": 5.75,
    "replace-shrink-middle-4k-to-2k-on-1mib-ops-1": 5.30,
    "truncate-tail-4k-on-1mib-ops-1": 5.03,
    "zero-extend-tail-4k-on-1mib-ops-1": 4.94,
    "append-tail-4k-on-10mib-ops-1": 5.71,
    "delete-middle-4k-on-10mib-ops-1": 5.75,
    "insert-middle-4k-on-10mib-ops-1": 6.40,
    "prepend-head-4k-on-10mib-ops-1": 5.65,
    "replace-grow-middle-2k-to-4k-on-10mib-ops-1": 6.81,
    "replace-shrink-middle-4k-to-2k-on-10mib-ops-1": 5.89,
    "truncate-tail-4k-on-10mib-ops-1": 9.14,
    "zero-extend-tail-4k-on-10mib-ops-1": 5.52,
    "append-tail-4k-on-100mib-ops-1": 5.60,
    "delete-middle-4k-on-100mib-ops-1": 6.41,
    "insert-middle-4k-on-100mib-ops-1": 6.25,
    "prepend-head-4k-on-100mib-ops-1": 6.54,
    "replace-grow-middle-2k-to-4k-on-100mib-ops-1": 8.44,
    "replace-shrink-middle-4k-to-2k-on-100mib-ops-1": 7.13,
    "truncate-tail-4k-on-100mib-ops-1": 5.44,
    "zero-extend-tail-4k-on-100mib-ops-1": 5.76,
    "append-tail-4k-on-500mib-result-capped-v2-ops-1": 6.40,
    "delete-middle-4k-on-500mib-ops-1": 12.16,
    "insert-middle-4k-on-500mib-result-capped-v2-ops-1": 7.76,
    "prepend-head-4k-on-500mib-result-capped-v2-ops-1": 6.73,
    "replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1": 8.63,
    "replace-shrink-middle-4k-to-2k-on-500mib-ops-1": 7.35,
    "truncate-tail-4k-on-500mib-ops-1": 7.19,
    "zero-extend-tail-4k-on-500mib-result-capped-v2-ops-1": 6.50,
}


def registry():
    return contract.family_registry(FAMILY_ID, OPERATIONS, TARGETS_MS, STRUCTURAL_KEYS)
