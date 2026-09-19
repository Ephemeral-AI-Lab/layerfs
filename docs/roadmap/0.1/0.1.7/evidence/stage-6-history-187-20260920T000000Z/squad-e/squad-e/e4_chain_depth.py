#!/usr/bin/env python3
"""E4 instrument: walk the persisted base_object_id links of a Store and report
(a) the true chain-length histogram per role, (b) every object whose base sits at
true depth >= the Store's persisted whole_file_delta_max_depth.

Read-only. Fail-closed: a base that names a row the Store does not hold is counted
as 'dangling' and never silently treated as a root.

Grammar read from core/crates/layerfs-storage/src/sqlite/schema.rs (objects table:
object_id, object_role, canonical_length, base_object_id, pack_id, group_number,
record_number) and the depth rules from encoding/delta/select.rs (eligible:
depth < depth_cap) and encoding/delta/read.rs (chain walk).
"""
import collections
import sqlite3
import sys


def analyse(path):
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    rows = con.execute(
        "SELECT object_id, object_role, base_object_id, canonical_length"
        " FROM objects"
    ).fetchall()
    policy = con.execute(
        "SELECT whole_file_delta_max_depth, chunk_delta_max_depth,"
        " metadata_delta_max_depth FROM store_policy"
    ).fetchone()
    base = {r[0]: r[2] for r in rows}
    role = {r[0]: r[1] for r in rows}
    length = {r[0]: r[3] for r in rows}
    whole_cap = policy[0]

    memo = {}

    def edges(oid):
        stack = []
        cur = oid
        while True:
            if cur in memo:
                d = memo[cur]
                break
            if cur not in base:
                d = None
                break
            b = base[cur]
            if b is None:
                d = 0
                break
            stack.append(cur)
            cur = b
            if len(stack) > 200:
                raise SystemExit("runaway chain")
        if d is None:
            for s in stack:
                memo[s] = None
            memo[oid] = None
            return None
        for s in reversed(stack):
            d += 1
            memo[s] = d
        memo[oid] = d
        return d

    hist = collections.Counter()
    role_hist = collections.defaultdict(collections.Counter)
    dangling = 0
    for oid in role:
        d = edges(oid)
        if d is None:
            dangling += 1
            continue
        hist[d] += 1
        role_hist[role[oid]][d] += 1

    over = []
    for oid, r in role.items():
        b = base.get(oid)
        if b is None:
            continue
        bd = memo.get(b)
        if bd is not None and bd >= whole_cap:
            over.append((oid, r, memo[oid], bd, length[oid]))

    print(f"--- {path}")
    print(f"    objects={len(rows)}  policy whole/chunk/metadata depth={policy}"
          f"  dangling={dangling}")
    print("    all edges:", " ".join(f"{k}:{hist[k]}" for k in sorted(hist) if k))
    for r in sorted(role_hist):
        h = role_hist[r]
        withbase = sum(v for k, v in h.items() if k > 0)
        print(f"    role {r}: with-base={withbase}  chain records (edges+1):",
              " ".join(f"{k + 1}:{h[k]}" for k in sorted(h) if k))
    print(f"    objects whose base has true depth >= {whole_cap}: {len(over)}")
    for oid, r, cd, bd, ln in over:
        print(f"      child={oid.hex()[:16]} role={r} child_edges={cd}"
              f" base_edges={bd} child_canonical={ln}")
    con.close()


for arg in sys.argv[1:]:
    analyse(arg)
