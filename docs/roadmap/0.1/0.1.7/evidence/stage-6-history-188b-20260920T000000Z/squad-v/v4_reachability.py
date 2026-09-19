#!/usr/bin/env python3
"""V4 — the reachability ledger: every number V4.md publishes, reproduced.

Read-only over Stores already on disk and over the corpus. No lane run, no
product source touched. Every figure is diagnostic, not admission evidence.

    python3 v4_reachability.py [--ours /tmp/confirm/sample.sqlite]

Sections:
  1  identity and dbstat of both Stores
  2  the exact decomposition of the residual (residual 0)
  3  the four-way whole-file join and the tag split of the gap
  4  is the comparison target a richer pool? (v0.1.6's own commit count)
  5  the objects-row grammar: a measured counterfactual, on a copy
  6  the corpus population behind the gap
  7  the ceiling, three ways
"""
from __future__ import annotations

import collections
import hashlib
import json
import os
import shutil
import sqlite3
import struct
import sys

def _repo_root():
    """Walk up to the checkout that holds the harness, rather than counting dots."""
    here = os.path.dirname(os.path.abspath(__file__))
    while here != os.path.dirname(here):
        if os.path.isdir(os.path.join(here, "core", "benchmark", "fs-bench-pro-storage-content")):
            return here
        here = os.path.dirname(here)
    raise SystemExit("v4: the repository root was not found above %s" % __file__)


REPO = _repo_root()
HARNESS = os.path.join(REPO, "core", "benchmark", "fs-bench-pro-storage-content")
sys.path.insert(0, HARNESS)
from shared import space  # noqa: E402
V016 = os.path.join(REPO, "benchmark-results", "repository-history", "stride-10",
                    "deepseek-stride10", "host-runtime", "store.sqlite")
CORPUS = "/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data"
OURS = "/tmp/confirm/sample.sqlite"
TRACE = "/tmp/confirm/trace.jsonl"
GATE = 49_315_840
T1_TARGET = 47_048_435
SELECTION = [1 + 10 * i for i in range(16)] + [157]
MAGIC = b"LFPACK\x00\x00"


def con(path, immutable=False):
    suffix = "&immutable=1" if immutable else ""
    return sqlite3.connect("file:%s?mode=ro%s" % (path, suffix), uri=True)


# --- 1 ------------------------------------------------------------------------

def dbstat(path):
    c = con(path)
    try:
        return dict((n, b) for n, b in c.execute("SELECT name, SUM(pgsize) FROM dbstat GROUP BY name"))
    finally:
        c.close()


def identity(path, label):
    print("### %s" % label)
    print("  %s" % path)
    print("  apparent %d B  sha256 %s" % (os.path.getsize(path),
          hashlib.sha256(open(path, "rb").read()).hexdigest()[:16] + "..."))
    c = con(path)
    print("  page_size %d page_count %d freelist %d  objects %d" % (
        c.execute("PRAGMA page_size").fetchone()[0],
        c.execute("PRAGMA page_count").fetchone()[0],
        c.execute("PRAGMA freelist_count").fetchone()[0],
        c.execute("SELECT COUNT(*) FROM objects").fetchone()[0]))
    c.close()
    for name, b in sorted(dbstat(path).items(), key=lambda kv: -kv[1])[:8]:
        print("    dbstat %-34s %12d" % (name, b))


# --- 2 ------------------------------------------------------------------------

def decomposition(ours, v16):
    po, pv = space.pack_directory(ours), space.pack_directory(v16)
    do, dv = dbstat(ours), dbstat(v16)
    bo, bv = space.pack_bodies(ours), space.pack_bodies(v16)
    so, sv = os.path.getsize(ours), os.path.getsize(v16)
    print("### the decomposition of the residual")
    lanes = ["whole-file", "native", "ordinary", "pooled-metadata"]
    tot = 0
    for lane in lanes:
        a, b = po.by_lane.get(lane, 0), pv.by_lane.get(lane, 0)
        tot += a - b
        print("  %-16s %12d %12d  %+12d" % (lane, a, b, a - b))
    framing = (po.framing_bytes - pv.framing_bytes) + (do["object_packs"] - bo) - (dv["object_packs"] - bv)
    tot += framing
    print("  %-16s %12d %12d  %+12d" % ("pack framing+slack", do["object_packs"], dv["object_packs"], framing))
    obj = do["objects"] - dv["objects"]
    tot += obj
    print("  %-16s %12d %12d  %+12d" % ("objects table", do["objects"], dv["objects"], obj))
    oo, ov = so - do["object_packs"] - do["objects"], sv - dv["object_packs"] - dv["objects"]
    tot += oo - ov
    print("  %-16s %12d %12d  %+12d" % ("other non-pack", oo, ov, oo - ov))
    print("  %-16s %12d %12d  %+12d   residual %d" % ("TOTAL", so, sv, so - sv, so - sv - tot))


# --- 3 ------------------------------------------------------------------------

def v16_records(path):
    c = con(path)
    blobs, groups = {}, {}
    for pid, blob in c.execute("SELECT pack_id, data FROM object_packs"):
        blobs[pid] = blob
        version, count = struct.unpack_from("<II", blob, 8)
        for i in range(count):
            if version == 4:
                s = struct.unpack_from("<I", blob, 16 + 4 * i)[0]
                e = len(blob) if i + 1 == count else struct.unpack_from("<I", blob, 16 + 4 * (i + 1))[0]
            else:
                s, enc, _ = struct.unpack_from("<III", blob, 16 + 16 * i)
                e = s + enc
            groups[(pid, i)] = (version, s, e)
    out = {}
    for oid, canon, pid, g in c.execute(
            "SELECT object_id, canonical_length, pack_id, group_number FROM objects"):
        version, s, e = groups[(int(pid), int(g))]
        tag = blobs[int(pid)][s] if version == 4 else None
        base = bytes(blobs[int(pid)][s + 1:s + 33]) if tag in (1, 2) else None
        out[bytes(oid)] = dict(canon=int(canon), stored=e - s, lane=version, tag=tag, base=base)
    c.close()
    return out


def ours_records(path):
    wf = space.whole_file_records(path)
    c = con(path)
    out = {}
    for oid, role, canon, base in c.execute(
            "SELECT object_id, object_role, canonical_length, base_object_id FROM objects"):
        oid = bytes(oid)
        out[oid] = dict(canon=int(canon), role=int(role),
                        base=(bytes(base) if base is not None else None), stored=wf.get(oid))
    c.close()
    return out


def four_way(ours, v16):
    o, v = ours_records(ours), v16_records(v16)
    common = [x for x in o if o[x]["role"] == 1 and x in v and v[x]["lane"] == 4]
    print("### the four-way whole-file join (same %d objects, same canonical bytes)" % len(common))
    buckets = collections.defaultdict(lambda: [0, 0, 0, 0])
    for x in common:
        key = ("we_base" if o[x]["base"] is not None else "we_full") + " / " + \
              ("v16_base" if v[x]["tag"] in (1, 2) else "v16_full")
        b = buckets[key]
        b[0] += 1
        b[1] += o[x]["canon"]
        b[2] += o[x]["stored"]
        b[3] += v[x]["stored"]
    tot = [0, 0, 0, 0]
    for key in sorted(buckets):
        b = buckets[key]
        for i in range(4):
            tot[i] += b[i]
        print("  %-22s n=%6d canon=%12d ours=%11d v16=%11d  delta=%+11d" % (
            key, b[0], b[1], b[2], b[3], b[2] - b[3]))
    print("  %-22s n=%6d canon=%12d ours=%11d v16=%11d  delta=%+11d" % (
        "TOTAL", tot[0], tot[1], tot[2], tot[3], tot[2] - tot[3]))
    gap = [x for x in common if o[x]["base"] is None and v[x]["tag"] in (1, 2)]
    by_tag = collections.defaultdict(lambda: [0, 0, 0])
    for x in gap:
        b = by_tag[v[x]["tag"]]
        b[0] += 1
        b[1] += o[x]["stored"]
        b[2] += v[x]["stored"]
    print("  the gap bucket by v0.1.6's own record tag:")
    for t in sorted(by_tag):
        b = by_tag[t]
        print("    tag %d: n=%6d ours=%11d v16=%11d  recoverable=%+11d" % (t, b[0], b[1], b[2], b[1] - b[2]))
    return buckets


# --- 4 ------------------------------------------------------------------------

def target_pool(v16):
    c = con(v16)
    commits = c.execute("SELECT COUNT(*) FROM commits").fetchone()[0]
    nulls = c.execute("SELECT COUNT(*) FROM commits WHERE parent_commit_id IS NULL").fetchone()[0]
    objects = c.execute("SELECT COUNT(*) FROM objects").fetchone()[0]
    c.close()
    v = v16_records(v16)
    lane4 = [x for x in v if v[x]["lane"] == 4]
    print("### is the comparison target a richer pool?")
    print("  v0.1.6 commits in the stride-10 Store : %d  (NULL parent: %d)" % (commits, nulls))
    print("  v0.1.6 objects in the stride-10 Store : %d" % objects)
    print("  v0.1.6 lane-4 objects / canonical     : %d / %d" % (
        len(lane4), sum(v[x]["canon"] for x in lane4)))
    print("  the corpus's 17-state whole-file union: 44240 / 371937306 (first-run pin)")


# --- 5 ------------------------------------------------------------------------

def row_width(ours):
    dst = "/tmp/v4_probe.sqlite"
    if os.path.exists(dst):
        os.remove(dst)
    shutil.copyfile(ours, dst)
    try:
        c = sqlite3.connect(dst)
        c.execute("CREATE TABLE ids (object_id BLOB PRIMARY KEY, n INTEGER) WITHOUT ROWID")
        c.execute("INSERT INTO ids SELECT object_id, row_number() OVER (ORDER BY object_id) FROM objects")
        c.execute("""CREATE TABLE objects_narrow (
            object_id BLOB NOT NULL PRIMARY KEY, object_role INTEGER NOT NULL,
            canonical_length INTEGER NOT NULL, base_ref INTEGER,
            pack_id INTEGER NOT NULL, group_number INTEGER NOT NULL,
            record_number INTEGER NOT NULL) STRICT, WITHOUT ROWID""")
        c.execute("""INSERT INTO objects_narrow SELECT o.object_id, o.object_role,
            o.canonical_length, i.n, o.pack_id, o.group_number, o.record_number
            FROM objects o LEFT JOIN ids i ON i.object_id = o.base_object_id""")
        c.execute("""CREATE TABLE objects_norole (
            object_id BLOB NOT NULL PRIMARY KEY, canonical_length INTEGER NOT NULL,
            base_ref INTEGER, pack_id INTEGER NOT NULL, group_number INTEGER NOT NULL,
            record_number INTEGER NOT NULL) STRICT, WITHOUT ROWID""")
        c.execute("""INSERT INTO objects_norole SELECT o.object_id, o.canonical_length,
            i.n, o.pack_id, o.group_number, o.record_number
            FROM objects o LEFT JOIN ids i ON i.object_id = o.base_object_id""")
        c.commit()
        rows = dict((n, b) for n, b in c.execute("SELECT name, SUM(pgsize) FROM dbstat GROUP BY name"))
        based = c.execute("SELECT COUNT(*) FROM objects WHERE base_object_id IS NOT NULL").fetchone()[0]
        total = c.execute("SELECT COUNT(*) FROM objects").fetchone()[0]
        c.close()
        print("### the objects row grammar, measured on a copy (%d rows, %d with a base)" % (total, based))
        print("  objects (32-byte BLOB base)   %12d" % rows["objects"])
        print("  objects_narrow (integer ref)  %12d   saving %+d" % (
            rows["objects_narrow"], rows["objects_narrow"] - rows["objects"]))
        print("  objects_norole (ref, no role) %12d   saving %+d" % (
            rows["objects_norole"], rows["objects_norole"] - rows["objects"]))
    finally:
        if os.path.exists(dst):
            os.remove(dst)


# --- 6 ------------------------------------------------------------------------

def corpus_population():
    doc = json.load(open(os.path.join(CORPUS, "checkpoint-manifest.json")))
    cps = doc["checkpoints"]

    def oracle(i):
        o = json.load(open(os.path.join(CORPUS, "oracles", cps[i - 1]["sha"] + ".json")))
        return dict((bytes.fromhex(k), (v[0], v[1], v[2])) for k, v in o.items() if v[2] not in ("-", ""))

    orc = dict((i, oracle(i)) for i in SELECTION)
    seen_oid, seen_path = set(), set()
    cls = collections.defaultdict(lambda: [0, 0])
    reuse = [0, 0]
    for n, i in enumerate(SELECTION):
        cur = orc[i]
        prev = orc[SELECTION[n - 1]] if n > 0 else {}
        prev_paths = set(prev)
        removed = collections.defaultdict(list)
        for p in prev_paths:
            if p not in cur:
                removed[os.path.basename(p)].append(p)
        first = collections.OrderedDict()
        for p, (mode, size, dig) in cur.items():
            if dig in seen_oid or dig in first:
                continue
            first[dig] = (p, size)
        for dig, (p, size) in first.items():
            carrying = [p2 for p2, (m2, s2, d2) in cur.items() if d2 == dig]
            if any(p2 in prev_paths for p2 in carrying):
                kind = "A modified"
            elif any(p2 in seen_path for p2 in carrying):
                kind = "B re-added"
            else:
                kind = "C new"
            cls[kind][0] += 1
            cls[kind][1] += size
            if kind == "C new" and os.path.basename(p) in removed:
                reuse[0] += 1
                reuse[1] += size
        seen_oid |= set(first)
        seen_path |= set(cur)
    print("### the corpus population behind the gap (oracle digests, 17 states)")
    for k in ("A modified", "B re-added", "C new"):
        print("  %-12s objects=%6d raw=%11d canonical~=%11d" % (k, cls[k][0], cls[k][1], cls[k][1] + 23 * cls[k][0]))
    print("  of C new, basename matches a path removed at the same step: objects=%d raw=%d" % tuple(reuse))


# --- 7 ------------------------------------------------------------------------

def ceiling(ours, v16):
    o, v = ours_records(ours), v16_records(v16)
    common = [x for x in o if o[x]["role"] == 1 and x in v and v[x]["lane"] == 4]
    # Per **bucket**, not per object: a Store picks one selection rule, so it
    # cannot hold our base choice and v0.1.6's at the same time. The four buckets
    # are the four-way join above, re-derived here so the ceiling is checkable.
    buckets = collections.defaultdict(lambda: [0, 0])
    for x in common:
        key = ("we_base" if o[x]["base"] is not None else "we_full") + "/" + \
              ("v16_base" if v[x]["tag"] in (1, 2) else "v16_full")
        buckets[key][0] += o[x]["stored"]
        buckets[key][1] += v[x]["stored"]
    best = sum(min(a, b) for a, b in buckets.values())
    po, pv = space.pack_directory(ours), space.pack_directory(v16)
    do, dv = dbstat(ours), dbstat(v16)
    bo, bv = space.pack_bodies(ours), space.pack_bodies(v16)
    so, sv = os.path.getsize(ours), os.path.getsize(v16)
    lines = [
        ("whole-file lane", po.by_lane["whole-file"], pv.by_lane["whole-file"], best),
        ("native lane", po.by_lane["native"], pv.by_lane["native"], None),
        ("ordinary lane", po.by_lane["ordinary"], pv.by_lane["ordinary"], None),
        ("pooled lane", po.by_lane["pooled-metadata"], pv.by_lane["pooled-metadata"], None),
        ("pack framing", po.framing_bytes, pv.framing_bytes, None),
        ("pack page slack", do["object_packs"] - bo, dv["object_packs"] - bv, None),
        ("objects table", do["objects"], dv["objects"], None),
        ("other non-pack", so - do["object_packs"] - do["objects"],
         sv - dv["object_packs"] - dv["objects"], None),
    ]
    total = 0
    print("### the ceiling, line by line")
    print("  %-16s %12s %12s %12s %12s" % ("line", "ours", "v0.1.6", "best-of-both", "saving"))
    for name, a, b, forced in lines:
        pick = forced if forced is not None else min(a, b)
        total += pick
        print("  %-16s %12d %12d %12d %+12d" % (name, a, b, pick, a - pick))
    print("  %-16s %12s %12s %12d %+12d" % ("TOTAL", so, sv, total, so - total))
    print("    vs the gate  %d  ->  %+d" % (GATE, total - GATE))
    print("    vs the T1 target %d  ->  %+d  (positive = target NOT reachable)" % (T1_TARGET, total - T1_TARGET))
    savings = ((po.by_lane["whole-file"] - best)
               + (po.by_lane["native"] - pv.by_lane["native"])
               + 1_236_992)
    assert best == 37_498_612, best
    print("  identical-object-set levers only (whole-file best-of-both + native + measured row narrowing)")
    print("    %d - %d = %d   vs the gate %+d" % (so, savings, so - savings, so - savings - GATE))
    print("  the costed path (L1+L2+L4+L5): %d   vs the gate %+d" % (
        so - (5_569_648 + 645_974 + 1_737_621 + 1_236_992),
        so - (5_569_648 + 645_974 + 1_737_621 + 1_236_992) - GATE))


def main():
    ours = OURS
    if "--ours" in sys.argv:
        ours = sys.argv[sys.argv.index("--ours") + 1]
    for path, label in ((ours, "the T1 best Store"), (V016, "v0.1.6's retained stride-10 Store")):
        identity(path, label)
    print()
    decomposition(ours, V016)
    print()
    four_way(ours, V016)
    print()
    target_pool(V016)
    print()
    row_width(ours)
    print()
    corpus_population()
    print()
    ceiling(ours, V016)


if __name__ == "__main__":
    main()
