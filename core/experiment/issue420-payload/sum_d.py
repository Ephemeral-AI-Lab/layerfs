#!/usr/bin/env python3
"""#420 payload study (question D): lever savings over the whole fixture and over its newest slices.

usage: sum_d.py RECEIPTS_DIR
Slices are by insertion position (index.pkl order): whole fixture, newest 20 %, newest 10 %.
Per-record attribution:
  loose-blob   a3-blob-rows.jsonl: earlier WholeFile match -> rec - measured delta width (measured);
               earlier chunked-file match -> rec - 70 B (derived: mean measured delta width 69.6 B)
  loose-tree   classify rows git=tree x measured whole-fixture ratio from a3-trees.json (derived)
  chunked-loose  chunk records of zlib-class files x 1.0 (derived: all 261 files reproduce at level 1
               and match earlier content; the residual delta record is ignored)
  git-pack     chunk records of git-pack-class files x measured whole-fixture ratio
               (matched blob bytes - 40 B per matched entry) / pack stored bytes (derived)
  chunk-resemblance  b/chunk-t1-rows.jsonl minus targets of class git-pack / zlib (measured)
  wf-resemblance     b/wf-t1-rows.jsonl (measured)
"""
import glob
import json
import os
import pickle
import sys
from collections import defaultdict


def main():
    R = sys.argv[1]
    rows = pickle.load(open(os.path.join(R, "dump/index.pkl"), "rb"))
    ext = pickle.load(open(os.path.join(R, "dump/extents.pkl"), "rb"))
    n = len(rows)
    pos = {r["id"]: i for i, r in enumerate(rows)}
    cls = {}
    git = {}
    for p in glob.glob(os.path.join(R, "classify/c-*.jsonl")):
        for line in open(p):
            r = json.loads(line)
            cls[r["i"]] = r["cls"]
            if r.get("git"):
                git[r["i"]] = r

    def fcls(i):
        r = rows[i]
        if r["role"] == 2:
            return cls.get(pos[ext[bytes.fromhex(r["id"])][0].hex()], "?")
        return cls.get(i, "?")

    slices = {"all": 0, "newest20": int(n * 0.8), "newest10": int(n * 0.9)}
    sav = defaultdict(lambda: defaultdict(float))
    payload = defaultdict(float)
    for i, r in enumerate(rows):
        for s, lo in slices.items():
            if i >= lo:
                payload[s] += r["rec"]

    def add(lever, i, v):
        for s, lo in slices.items():
            if i >= lo:
                sav[lever][s] += v

    for line in open(os.path.join(R, "a3-blob-rows.jsonl")):
        b = json.loads(line)
        if b.get("match") == "earlier":
            add("loose-blob", b["i"], max(0, b["rec"] - b["dw"] if "dw" in b else b["rec"] - 70))
    tr = json.load(open(os.path.join(R, "a3-trees.json")))["c"]
    ratio_tree = tr["tree_saved"] / tr["tree_rec"]
    pk = json.load(open(os.path.join(R, "a4-chunkfiles.json")))["c"]
    ratio_pack = (pk["pack_blob_earlier_zbytes"] - 40 * pk["pack_blob_earlier_n"]) / pk["git-pack_own_rec"]
    for i, g in git.items():
        if g["git"] == "tree":
            add("loose-tree", i, g["rec"] * ratio_tree)
    for i, r in enumerate(rows):
        if r["role"] != 2:
            continue
        k = fcls(i)
        if k == "zlib":
            add("chunked-loose", i, r["rec"])
        elif k == "git-pack":
            add("git-pack", i, r["rec"] * ratio_pack)
    for line in open(os.path.join(R, "b/chunk-t1-rows.jsonl")):
        b = json.loads(line)
        if fcls(b["i"]) in ("zlib", "git-pack"):
            continue
        add("chunk-resemblance", b["i"], b["before"] - b["after"])
    for line in open(os.path.join(R, "b/wf-t1-rows.jsonl")):
        b = json.loads(line)
        add("wf-resemblance", b["i"], b["before"] - b["after"])
    print(f"{'lever':20} " + " ".join(f"{s:>12}" for s in slices))
    print(f"{'payload stored MB':20} " + " ".join(f"{payload[s]/1e6:12.2f}" for s in slices))
    tot = defaultdict(float)
    for lever, v in sav.items():
        print(f"{lever:20} " + " ".join(f"{v[s]/1e6:12.2f}" for s in slices))
        for s in slices:
            tot[s] += v[s]
    print(f"{'TOTAL':20} " + " ".join(f"{tot[s]/1e6:12.2f}" for s in slices))
    print(f"{'TOTAL % of payload':20} " + " ".join(f"{100*tot[s]/payload[s]:12.1f}" for s in slices))
    print(json.dumps({"ratio_tree": ratio_tree, "ratio_pack": ratio_pack}))


if __name__ == "__main__":
    main()
