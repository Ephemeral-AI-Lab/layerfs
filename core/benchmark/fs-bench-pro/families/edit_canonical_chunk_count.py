"""#232 Workspace Exec/FUSE canonical-chunk-count family: 12 registered cases.

Module name follows the owner's v0.1.6 family style; the Exec/FUSE identity is
carried by the route, operation-contract, entrypoint and scenario-version fields
of every row and receipt.

The declared edit is one 64 KiB positional overwrite at offset 147,456 on
1/10/100/500 MiB inputs, chosen in v0.1.6 so that the resulting canonical chunk
count decreases, increases or is preserved. The historical G2 table pins the
resulting canonical file root and chunk count for each case; because canonical
content addressing is a function of the final bytes, those pinned roots and
counts remain valid oracles for a different write route and are declared as the
expected values here.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from shared import edit_contract as contract  # noqa: E402

FAMILY_ID = "edit_canonical_chunk_count"
HISTORICAL_FAMILY = "edit_canonical_chunk_count"
START = 147_456
LEN = 65_536
PAYLOAD_DOMAIN = 0x4348554E4B434E54


def payload_seed(value):
    return value


OPERATIONS = [
    {
        "key": "overwrite-fixed-64k-chunk-count-preserve",
        "algorithm": "positional-write",
        "replacement_kind": "inline",
        "replacement_len": LEN,
        "payload_seed": PAYLOAD_DOMAIN ^ 4,
        "payload_sha256": "6403e9f46c8e5034759add37d4d64ecffbeee1f26b719809d4f04a1e02864978",
        "locate": lambda size: (START, LEN),
        "fixture_override": {},
        "capped_labels": (),
    },
    {
        "key": "overwrite-fixed-64k-chunk-count-increase",
        "algorithm": "positional-write",
        "replacement_kind": "inline",
        "replacement_len": LEN,
        "payload_seed": PAYLOAD_DOMAIN ^ 2,
        "payload_sha256": "ba71e3adc4ce9f1645d8f622f6c2600ca8236146f1d6817fce183f80170dade0",
        "locate": lambda size: (START, LEN),
        "fixture_override": {},
        "capped_labels": (),
    },
    {
        "key": "overwrite-fixed-64k-chunk-count-decrease",
        "algorithm": "positional-write",
        "replacement_kind": "zero",
        "replacement_len": LEN,
        "payload_seed": 0,
        "payload_sha256": "de2f256064a0af797747c2b97505dc0b9f3df0de4f489eac731c23ae9ca9cc31",
        "locate": lambda size: (START, LEN),
        "fixture_override": {},
        "capped_labels": (),
    },
]

TARGETS_MS = {
    "overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1": 6.02,
    "overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1": 6.84,
    "overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1": 6.20,
    "overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1": 6.63,
    "overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1": 6.90,
    "overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1": 7.24,
    "overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1": 13.54,
    "overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1": 8.01,
    "overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1": 7.46,
    "overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1": 7.98,
    "overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1": 9.27,
    "overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1": 8.13,
}

# Canonical result oracle pinned from the v0.1.6 family definition and the G2
# report: (initial_count, final_count, final_sha256, canonical_file_root).
EXPECTED = {
    ("decrease", 1_048_576): (54, 53, "dab7df0938e609ac80dddfc7fb6c0ed0a3e2643d5eb9181197cdd0e185920ed6",
                              "d0b2112fb3ce304634515ec0126759d4d54ce41ddd6ec6772646286445af35dc"),
    ("decrease", 10_485_760): (544, 543, "94d1d712c610775c38ae1be46233ff351e9104ce6af67543d4be3b6c5b8f0d4d",
                               "3a36a0494a2ad58411826e27d75bfaaa1970594efc06bc165496323256f71552"),
    ("decrease", 104_857_600): (5_394, 5_393, "0865e1e1cdef049bfb49b5888808fdf87b1add29c45c7da55ff0c5d1f43db961",
                                "e7405b58a6e108d5cb4f7949766cb43a25b755a71233fb39e5694331950d41f3"),
    ("decrease", 524_288_000): (26_995, 26_994, "af536c25d5ee02671afa6eb0194534973d7f1595b6a84cf82acbf5305e7a145d",
                                "b3c43de62a637318f417a091cc82fe3b9c5bccf9d61d33127207182614e14e2e"),
    ("increase", 1_048_576): (54, 55, "b320ec162166c71532c93f1013e42b6d0beb9f194c467e043c07057f55101055",
                              "3756ef696b53388b234cc2c5877240c82a2ed660e9a462e86168bf4ebd9c4fcd"),
    ("increase", 10_485_760): (544, 545, "a5afa52cc6527313281971d1bb816d593cb173abf024a09689406a9b7afe01b5",
                               "e24afbe82dcb11d8d6084b77d519cf342127d29c4cad7db74fd275ae6ca3fc4d"),
    ("increase", 104_857_600): (5_394, 5_395, "b95945fead470121e03fa4d1e640582a5d2a2ae63d66ff66b537ce407e408c7d",
                                "1b5c0ee5c643aaea649b7a76f1b325093f4887ce0e6fddfce1e221f2d5826567"),
    ("increase", 524_288_000): (26_995, 26_996, "3accd704e6596ce90622d134a35591efe89f6ccb6e84a3d88b5e4aea6378bfd4",
                                "95fc3c70f8ea88cb3bee08ed9aa9c0fc4aae28eeac480f9d323c0665ddd9dd67"),
    ("preserve", 1_048_576): (54, 54, "a3374b0be7c654cf87f2b8d411d657e170c821837dcce8661759aa5fe1fc7070",
                              "644ab1b651adc897f95da15461e32e587565b7e2789b377524c8e88aaf03e4a6"),
    ("preserve", 10_485_760): (544, 544, "231b62f873d1c1b498809d40bd92235a5cdf08150abaf802422e109fee490fcc",
                               "feef5c7528ffb92220caca77fdd89d2d1cce257e977cc204cef1e676ade6e493"),
    ("preserve", 104_857_600): (5_394, 5_394, "f8e906873405662688d8c8add82abef06155c084f84957f0043103fc55d909f3",
                                "edfd0588251dddcb7fbbd4993a18d9d10e96d43c2ba07f3fa9c11389cd2e88cc"),
    ("preserve", 524_288_000): (26_995, 26_995, "5d8205919a2abe3f7c51f1592ceed0977b39fa2c7e6a4568b541e6fa9ed51437",
                                "728adcbeb98753983afe97b0c2fde4d92251e044e6330d7919012879bc9a1c39"),
}


def registry():
    rows = contract.family_registry(FAMILY_ID, OPERATIONS, TARGETS_MS)
    for row in rows:
        shape = row["operation_key"].rsplit("-", 1)[-1]
        initial, final, sha256, root = EXPECTED[(shape, row["fixture_bytes"])]
        row["initial_count"] = initial
        row["canonical_count_expected"] = final
        row["canonical_root_expected"] = root
        if row["final_sha256"] != sha256:
            raise ValueError(f"{row['scenario_id']}: canonical final digest")
    return rows
