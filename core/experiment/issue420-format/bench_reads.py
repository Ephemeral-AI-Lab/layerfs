#!/usr/bin/env python3
"""#420 format study: read micro-benchmark, today's tables vs candidate encodings.

usage: bench_reads.py SIM_DIR CASE
cases:
  iv-base         20-value ordinal runs from iv-base (BETWEEN), 5,000 runs
  iv-group:NAME   same runs from iv-NAME.sqlite (group rows covering the run + zstd decompress)
  iv-ord          iv-group-colord-z3-128 runs + ordinal -> id resolution in ol-ord16 (one IN query per run)
  bh-base         bindings_of(child) on bh-base, 20,000 random children
  bh-runs         bh-runs-64: group probe + overflow-free decode (varints) + name lookups
  ol-base | ol-key8 | ol-ord16   locate 20,000 random ids
One sample, warm OS page cache (files were just written), same random keys (seed 420) per family.
Python-driven: per-op Python overhead is common to both arms; column decode in Python excluded
where noted (a Rust decode of <=128 values is ~1 us, derived).
"""
import json
import random
import sqlite3
import sys
import time
from compression import zstd


def con(path):
    c = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    c.execute("PRAGMA cache_size=-2048")  # the product's 2 MiB per connection
    return c


def main():
    d, case = sys.argv[1], sys.argv[2]
    rnd = random.Random(420)
    out = {"case": case}
    if case.startswith("iv"):
        base = con(f"{d}/iv-base.sqlite")
        ords = [r[0] for r in base.execute("SELECT ordinal FROM inode_value WHERE ordinal % 97 = 0")]
        starts = [rnd.choice(ords) for _ in range(5000)]
        t = time.perf_counter()
        n = 0
        if case == "iv-base":
            for s in starts:
                n += len(base.execute("SELECT ordinal, value FROM inode_value WHERE ordinal BETWEEN ? AND ?", (s, s + 19)).fetchall())
        else:
            name = "group-colord-z3-128" if case == "iv-ord" else case.split(":")[1]
            g = con(f"{d}/iv-{name}.sqlite")
            ol = con(f"{d}/ol-ord16.sqlite") if case == "iv-ord" else None
            for s in starts:
                rows = g.execute("SELECT first, count, body FROM inode_value_group WHERE first <= ? ORDER BY first DESC LIMIT 1", (s,)).fetchall()
                rows += g.execute("SELECT first, count, body FROM inode_value_group WHERE first > ? AND first <= ?", (s, s + 19)).fetchall()
                for first, cnt, body in rows:
                    raw = zstd.decompress(body) if "z" in name else body
                    n += 1
                if ol is not None:
                    # resolve 20 content-root ordinals (stand-ins: the run's own neighbourhood)
                    ids = [rnd.randrange(1, 1389003) for _ in range(20)]
                    ol.execute(f"SELECT ordinal, object_id FROM object_location WHERE ordinal IN ({','.join('?' * 20)})", ids).fetchall()
        dt = time.perf_counter() - t
        out.update(runs=len(starts), rows=n, us_per_run=round(dt * 1e6 / len(starts), 2))
    elif case.startswith("bh"):
        base = con(f"{d}/bh-base.sqlite")
        kids = [r[0] for r in base.execute("SELECT child FROM binding_hint WHERE child % 53 = 0")]
        keys = [rnd.choice(kids) for _ in range(20000)]
        t = time.perf_counter()
        if case == "bh-base":
            for k in keys:
                base.execute("SELECT child, parent, name FROM binding_hint WHERE child = ?", (k,)).fetchall()
        else:
            g = con(f"{d}/bh-runs-64.sqlite")
            for k in keys:
                first, last, body = g.execute("SELECT first, last, body FROM binding_group WHERE first <= ? ORDER BY first DESC LIMIT 1", (k,)).fetchone()
                # decode until child k (Python varints), collect name ids
                i, child, parent, ids = 0, first, first, []
                while i < len(body):
                    vals = []
                    for _ in range(3):
                        v = s = 0
                        while True:
                            b = body[i]; i += 1
                            v |= (b & 0x7F) << s; s += 7
                            if b < 0x80:
                                break
                        vals.append(v)
                    child += vals[0]
                    parent += (vals[1] >> 1) ^ -(vals[1] & 1)
                    if child == k:
                        ids.append(vals[2])
                    elif child > k:
                        break
                if ids:
                    g.execute(f"SELECT id, name FROM binding_name WHERE id IN ({','.join('?' * len(ids))})", ids).fetchall()
        dt = time.perf_counter() - t
        out.update(lookups=len(keys), us_per_lookup=round(dt * 1e6 / len(keys), 2))
    else:
        base = con(f"{d}/ol-base.sqlite")
        ids = [r[0] for r in base.execute("SELECT object_id FROM object_location") if rnd.random() < 0.015]
        ids = ids[:20000]
        t = time.perf_counter()
        if case == "ol-base":
            for i in ids:
                base.execute("SELECT role, canonical_length, pack_id, group_number, record_number FROM object_location WHERE object_id = ?", (i,)).fetchone()
        elif case == "ol-key8":
            c = con(f"{d}/ol-key8loc.sqlite")
            for i in ids:
                c.execute("SELECT sized, locator FROM object_location WHERE key = ?", (int.from_bytes(i[:8], "big", signed=True),)).fetchone()
        else:
            c = con(f"{d}/ol-ord16.sqlite")
            for i in ids:
                r = c.execute("SELECT ordinal, object_id, sized, locator FROM object_location WHERE substr(object_id,1,16) = ?", (i[:16],)).fetchall()
                assert any(x[1] == i for x in r)
        dt = time.perf_counter() - t
        out.update(lookups=len(ids), us_per_lookup=round(dt * 1e6 / len(ids), 2))
    print(json.dumps(out))


if __name__ == "__main__":
    main()
