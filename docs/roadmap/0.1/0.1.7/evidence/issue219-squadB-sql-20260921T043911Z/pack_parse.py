#!/usr/bin/env python3
"""Parse sample.sqlite pack bodies to (a) count pack writes exactly and
(b) sum the bytes those writes put through SQLite.

Pack format read from core/crates/layerfs-storage/src/pack/layout.rs:
  HEADER_LEN = 16            magic[8] "LFPACK\\0\\0", version u32 LE, group_count u32 LE
  DIRECTORY_ENTRY_LEN = 16   body_start u32 LE, encoded u32 LE, decoded u32 LE, flags u32 LE
  WHOLE_FILE_ENTRY_LEN = 4   whole-file lane directory is "starts only"
  WholeFile body_size = bytes.len() - WHOLE_FILE_COMPACT_DROP(8); others = encoded
  assembled(k groups) = HEADER_LEN + k*entry_len + sum(body_size of those groups)
"""
import sqlite3, sys, collections, json

MAGIC = b"LFPACK\x00\x00"
HEADER_LEN = 16
DIR = 16
WF_DIR = 4
WF_DROP = 8
LANES = {1: "Ordinary", 2: "Native", 4: "WholeFile", 6: "PooledMetadata", 7: "Singleton"}

con = sqlite3.connect("file:/tmp/squadB/sample.sqlite?mode=ro", uri=True)
con.execute("PRAGMA temp_store = MEMORY")

per_pack = []
bad = []
for pack_id, data in con.execute("SELECT pack_id, data FROM object_packs ORDER BY pack_id"):
    if data[:8] != MAGIC:
        bad.append((pack_id, "magic")); continue
    version = int.from_bytes(data[8:12], "little")
    gc = int.from_bytes(data[12:16], "little")
    lane = LANES.get(version, "v%d" % version)
    if lane == "WholeFile":
        entry = WF_DIR
        sizes = []
        off = HEADER_LEN + WF_DIR * gc
        dir_end = off
        for i in range(gc):
            declared = int.from_bytes(data[HEADER_LEN + WF_DIR*i: HEADER_LEN + WF_DIR*(i+1)], "little")
            sizes.append(declared - WF_DROP)
    elif lane in ("Ordinary", "Native", "PooledMetadata", "Singleton"):
        entry = DIR
        sizes = []
        for i in range(gc):
            e = data[HEADER_LEN + DIR*i: HEADER_LEN + DIR*(i+1)]
            encoded = int.from_bytes(e[4:8], "little")
            sizes.append(encoded)
    else:
        bad.append((pack_id, "version %d" % version)); continue
    prefix = []
    acc = HEADER_LEN
    for s in sizes:
        acc += entry + s
        prefix.append(acc)
    assert acc == len(data), (pack_id, acc, len(data))
    per_pack.append({
        "pack_id": pack_id, "lane": lane, "groups": gc, "final": len(data),
        "writes": gc,                       # one group per select_many call (see README)
        "bytes_written": sum(prefix),       # sum of assembled length at each write
        "max_prefix": max(prefix) if prefix else 0,
    })

tot = {
    "packs": len(per_pack),
    "groups": sum(p["groups"] for p in per_pack),
    "writes": sum(p["writes"] for p in per_pack),
    "pack_bytes_written": sum(p["bytes_written"] for p in per_pack),
    "final_bytes": sum(p["final"] for p in per_pack),
}
by_lane = collections.defaultdict(lambda: {"packs":0,"groups":0,"writes":0,"bytes_written":0,"final":0})
for p in per_pack:
    b = by_lane[p["lane"]]
    b["packs"] += 1; b["groups"] += p["groups"]; b["writes"] += p["writes"]
    b["bytes_written"] += p["bytes_written"]; b["final"] += p["final"]
print(json.dumps({"total": tot, "by_lane": by_lane, "bad": bad, "writes_per_pack_hist": dict(collections.Counter(p["writes"] for p in per_pack))}, indent=2))
json.dump(per_pack, open(sys.argv[1], "w"), indent=0)
