#!/usr/bin/env python3
"""#420 format study: delta chain shapes of InodeLeaf (6) and InodeBranch (9) records.

usage: chains.py DB OUT_JSON
Decodes every ordinary pack group holding a role-6/9 object, reads the record tag and base id,
and reports per role: depth histogram, full/delta counts and mean stored record bytes by kind,
the number of full records whose own successor chain reached the cap (cap-forced estimate:
full records that are bases of nothing, or roots of chains whose deepest node is at the cap).
"""
import json
import sqlite3
import struct
import sys
from collections import Counter, defaultdict
from compression import zstd


def main():
    db, outp = sys.argv[1], sys.argv[2]
    con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    want = defaultdict(dict)
    for oid, role, p, g, r in con.execute(
            "SELECT object_id, role, pack_id, group_number, record_number FROM object_location WHERE role IN (6,9)"):
        want[p][(g, r)] = (oid, role)
    info = {}
    for p in sorted(want):
        body = con.execute("SELECT body FROM pack WHERE pack_id=?", (p,)).fetchone()[0]
        gc = struct.unpack_from("<I", body, 12)[0]
        groups = {}
        for (g, r), (oid, role) in want[p].items():
            if g not in groups:
                s, enc, dec, fl = struct.unpack_from("<IIII", body, 24 + 16 * g)
                raw = body[s:s + enc]
                groups[g] = (zstd.decompress(raw) if fl & 0xff == 1 else raw, enc / dec)
            gb, scale = groups[g]
            n = struct.unpack_from("<I", gb, 0)[0]
            ends = struct.unpack_from(f"<{n}I", gb, 4)
            area = 4 + 4 * n
            start = ends[r - 1] if r else 0
            rec = gb[area + start: area + ends[r]]
            base = rec[1:33] if rec[0] == 1 else None
            info[oid] = (role, base, len(rec), len(rec) * scale)
    depth = {}

    def d(o):
        stack = []
        while o not in depth:
            role, base, _, _ = info[o]
            if base is None or base not in info:
                depth[o] = 0 if base is None else 1
                break
            stack.append(o)
            o = base
        v = depth[o]
        for x in reversed(stack):
            v += 1
            depth[x] = v
        return depth[stack[0]] if stack else depth[o]

    res = {}
    children = Counter(b for _, b, _, _ in info.values() if b is not None)
    for role in (6, 9):
        objs = [o for o, v in info.items() if v[0] == role]
        hist = Counter(d(o) for o in objs)
        full = [o for o in objs if info[o][1] is None]
        delta = [o for o in objs if info[o][1] is not None]
        res[str(role)] = {
            "objects": len(objs), "full": len(full), "delta": len(delta),
            "depth_hist": dict(sorted(hist.items())),
            "full_alloc_mean": sum(info[o][3] for o in full) / max(1, len(full)),
            "delta_alloc_mean": sum(info[o][3] for o in delta) / max(1, len(delta)),
            "full_alloc_total": sum(info[o][3] for o in full),
            "delta_alloc_total": sum(info[o][3] for o in delta),
            "full_without_delta_children": sum(1 for o in full if children[o] == 0),
            "at_cap_nodes": hist.get(8, 0),
        }
    json.dump(res, open(outp, "w"), indent=1)
    print(json.dumps(res, indent=1))


if __name__ == "__main__":
    main()
