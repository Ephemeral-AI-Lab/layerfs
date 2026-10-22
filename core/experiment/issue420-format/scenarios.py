#!/usr/bin/env python3
"""#420 format study: candidate savings and combined format v-next scenarios.

All inputs are receipts of this study (sim-*.jsonl, codec-summary.txt, payload-summary.txt,
chains.json, pg-sizes.txt) or the size study's per-table PG receipts; derived items are labelled.
SQLite saving of a table candidate = fixture table bytes x (1 - candidate / fresh copy of today's
table), so fill differences between the live file and a fresh build cancel.
"""
MB = 1e6
FIX = 1830789120
# fixture dbstat bytes (dbstat-by-table.txt)
fx = {"inode_value": 369922048, "binding_hint": 113577984, "object_location": 77307904,
      "commit_family": 24879104 + 4575232 + 6422528 + 6205440 + 36864}
# fresh SQLite builds (sim-*.jsonl)
fresh = {"iv_base": 367996928, "iv_colz3_128": 160731136, "iv_colord_128": 33275904,
         "bh_base": 117186560, "bh_runs64": 30560256, "ol_base": 78495744, "ol_key16": 53309440,
         "ol_key8loc": 36470784, "ol_ord16": 109293568, "cm_base": 37081088, "cm_slim": 19296256}
BH_OVERFLOW = 3.5 * MB  # derived: re-bindings in an overflow row table + per-Commit group rows


def sq(table, cand, base, extra=0.0):
    return fx[table] * (1 - (fresh[cand] + extra) / fresh[base])


C = {  # SQLite savings in bytes (positive = smaller)
    "iv grouped columnar zstd-3": sq("inode_value", "iv_colz3_128", "iv_base"),
    "iv grouped + ordinal content roots": sq("inode_value", "iv_colord_128", "iv_base"),
    "bh child runs + interned names": sq("binding_hint", "bh_runs64", "bh_base", BH_OVERFLOW),
    "ol 16-byte key": sq("object_location", "ol_key16", "ol_base"),
    "ol 8-byte integer key + packed locator": sq("object_location", "ol_key8loc", "ol_base"),
    "ol ordinal table + 16-byte id index": sq("object_location", "ol_ord16", "ol_base"),
    "commit integer refs + lineage folded": sq("commit_family", "cm_slim", "cm_base"),
    "payload full records zstd 9": 30.3 * MB,       # payload-summary.txt extrapolated (10% sample)
    "stored-probe: whole-record trial": 5.9 * MB,   # payload-summary.txt extrapolated
    "metadata groups 16 KiB dictionary": 154360816 - 146374158,  # codec-summary.txt
    "inode-branch rebasing within cap 8": 14.5 * MB,  # derived from chains.json
    "file_digest 16-byte digest + ordinal root": 4.0 * MB,  # derived
}
for k, v in C.items():
    print(f"SQLite {k:44} {v / MB:8.1f} MB")

scen = {
    "conservative (key16)": ["iv grouped columnar zstd-3", "bh child runs + interned names", "ol 16-byte key",
                             "commit integer refs + lineage folded", "payload full records zstd 9",
                             "metadata groups 16 KiB dictionary", "inode-branch rebasing within cap 8"],
}
scen["conservative (key8)"] = [x if x != "ol 16-byte key" else "ol 8-byte integer key + packed locator" for x in scen["conservative (key16)"]]
scen["maximal within budget"] = ["iv grouped + ordinal content roots", "ol ordinal table + 16-byte id index",
                                 "bh child runs + interned names", "commit integer refs + lineage folded",
                                 "payload full records zstd 9", "stored-probe: whole-record trial",
                                 "metadata groups 16 KiB dictionary", "inode-branch rebasing within cap 8",
                                 "file_digest 16-byte digest + ordinal root"]
for name, items in scen.items():
    s = sum(C[i] for i in items)
    print(f"SQLite scenario {name:24} saves {s / MB:7.1f} MB -> {(FIX - s) / MB:8.1f} MB ({-100 * s / FIX:5.1f}%)")

# Postgres (MB): base as ported and study layouts per table (size study receipts), candidates measured here
pg_base = {"iv": 600.6, "bh": 410.6, "ol": 234.0, "cm": 53.4, "fd": 16.3}
pg_rec = {"iv": 218.7, "bh": 104.3, "ol": 80.3, "cm": 53.4, "fd": 15.3}   # iv_b32z, bh_b2, ol_b18 (reconstructed)
pg_small = {"iv": 218.7, "bh": 58.0, "ol": 70.4, "cm": 53.4, "fd": 15.3}  # iv_b32z, bh_b3, ol_b16 (reconstructed)
pack_c = (30.3 + 8.0 + 14.5)
pack_m = pack_c + 5.9
rows = {  # candidate PG sizes, row-oriented formats (no study bucketing) and bucketed formats
    "C16": {"iv": 177.6, "bh": 40.3, "ol": 145.9, "cm": 28.5, "fd": 16.3, "pack": pack_c},
    "C8": {"iv": 177.6, "bh": 40.3, "ol": 101.0, "cm": 28.5, "fd": 16.3, "pack": pack_c},
    "M": {"iv": 33.2, "bh": 40.3, "ol": 208.7, "cm": 28.5, "fd": 11.3, "pack": pack_m},
}
buck = {
    "C16": {"iv": 177.6, "bh": 40.3, "ol": 46.0, "cm": 28.5, "fd": 15.3, "pack": pack_c},  # ol key16 bucket derived
    "C8": {"iv": 177.6, "bh": 40.3, "ol": 37.9, "cm": 28.5, "fd": 15.3, "pack": pack_c},
    "M": {"iv": 33.2, "bh": 40.3, "ol": 110.2, "cm": 28.5, "fd": 10.3, "pack": pack_m},
}
for name, start, ref, cand in (("as ported", 2572.4, pg_base, rows), ("study recommended", 1735.4, pg_rec, buck),
                               ("study smallest", 1646.5, pg_small, buck)):
    for k, c in cand.items():
        d = sum(ref[t] - c[t] for t in ("iv", "bh", "ol", "cm", "fd")) + c["pack"]
        print(f"PG from {name:18} {k:4}: saves {d:7.1f} MB -> {start - d:7.1f} MB")
