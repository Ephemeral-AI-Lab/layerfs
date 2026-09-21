#!/usr/bin/env python3
"""Row 2 defended: the EXACT write-size distribution of the run's 16,802 pack
writes, and the cost of 'UPDATE object_packs SET data = ?2 ...' at those sizes.

DESTRUCTIVE diagnostic on a fresh copy (bench4.sqlite). Not a receipt.
"""
import sqlite3, json, os, shutil, statistics, time, collections

SB = "/tmp/squadB"; B = SB + "/bench4.sqlite"

# ---- 1. the real write sequence, from the pack directory of sample.sqlite ----
MAGIC = b"LFPACK\x00\x00"; HEADER_LEN = 16
LANES = {1: "Ordinary", 2: "Native", 4: "WholeFile", 6: "PooledMetadata", 7: "Singleton"}
src = sqlite3.connect("file:" + SB + "/sample.sqlite?mode=ro", uri=True)
src.execute("PRAGMA temp_store = MEMORY")
writes = []          # (pack_id, lane, final_bytes, k, size)
per_lane = collections.defaultdict(lambda: {"packs": 0, "groups": 0, "written": 0, "final": 0})
for pid, data in src.execute("SELECT pack_id, data FROM object_packs ORDER BY pack_id"):
    assert data[:8] == MAGIC
    lane = LANES[int.from_bytes(data[8:12], "little")]
    gc = int.from_bytes(data[12:16], "little")
    if lane == "WholeFile":
        entry = 4
        starts = [int.from_bytes(data[16 + 4 * i: 20 + 4 * i], "little") for i in range(gc)]
        sizes = [e - s for s, e in zip(starts, starts[1:] + [len(data)])]
    else:
        entry = 16
        sizes = [int.from_bytes(data[16 + 16 * i + 4: 16 + 16 * i + 8], "little") for i in range(gc)]
    acc = HEADER_LEN
    for k, s in enumerate(sizes, start=1):
        acc += entry + s
        writes.append((pid, lane, len(data), k, acc))
    b = per_lane[lane]
    b["packs"] += 1; b["groups"] += gc; b["written"] += sum(
        HEADER_LEN + sum(entry + x for x in sizes[:j]) for j in range(1, gc + 1)); b["final"] += len(data)
src.close()

sizes_all = sorted(w[4] for w in writes)
n = len(sizes_all)
def pct(p):
    return sizes_all[int(p * (n - 1))]

summary = {
    "writes": n,
    "packs": len(set(w[0] for w in writes)),
    "bytes_written": sum(sizes_all),
    "final_bytes": sum(w[2] for w in writes if w[3] == max(x[3] for x in writes if x[0] == w[0])),
    "size_bytes": {
        "min": sizes_all[0], "p10": pct(0.10), "p25": pct(0.25), "median": pct(0.50),
        "mean": statistics.mean(sizes_all), "p75": pct(0.75), "p90": pct(0.90), "max": sizes_all[-1],
    },
    "writes_per_pack_mean": n / len(set(w[0] for w in writes)),
}
summary["amplification_measured"] = summary["bytes_written"] / 302023232
per_lane_out = {}
for lane, b in per_lane.items():
    per_lane_out[lane] = {
        "packs": b["packs"], "groups": b["groups"], "writes": b["groups"],
        "writes_per_pack": round(b["groups"] / b["packs"], 2),
        "bytes_written": b["written"], "final_bytes": b["final"],
        "amplification": round(b["written"] / b["final"], 2),
        "mean_write_bytes": round(b["written"] / b["groups"]),
    }

# ---- 2. replay exactly that size sequence against a copy ----
if os.path.exists(B):
    os.remove(B)
shutil.copyfile(SB + "/sample.sqlite", B)
con = sqlite3.connect(B, isolation_level=None)
con.execute("PRAGMA journal_mode = MEMORY"); con.execute("PRAGMA synchronous = OFF")
con.execute("PRAGMA temp_store = MEMORY"); con.execute("PRAGMA foreign_keys = ON")
con.execute("PRAGMA busy_timeout = 0")
con.execute("CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)")
con.execute("INSERT INTO saves (active_slot) VALUES (1)")
save_id = con.execute("SELECT last_insert_rowid()").fetchone()[0]
con.execute("INSERT INTO layerfs_read_scope VALUES (?, 9223372036854775807)", (save_id,))

# one synthetic pack row per real pack, in the real insertion order
real_packs = sorted(set(w[0] for w in writes))
pid_map = {rp: 900000 + i for i, rp in enumerate(real_packs)}
blob0 = b"\x00" * 38081
con.execute("BEGIN IMMEDIATE")
for i, rp in enumerate(real_packs):
    con.execute("INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, "
                "(SELECT save_id FROM temp.layerfs_read_scope))", (pid_map[rp], blob0))
con.execute("COMMIT")

UPD = ("UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND save_id = "
       "(SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS (SELECT 1 FROM saves "
       "WHERE saves.save_id = object_packs.save_id AND publication IS NULL)")
buf = bytearray(262144)
con.execute("BEGIN IMMEDIATE")
samples = []
t_start = time.perf_counter()
for (rp, lane, final_len, k, size) in writes:
    blob = bytes(memoryview(buf)[:size])
    t0 = time.perf_counter_ns()
    cur = con.execute(UPD, (pid_map[rp], blob))
    dt = time.perf_counter_ns() - t0
    if cur.rowcount != 1:
        raise SystemExit("UPDATE matched %d rows at pack %s k=%d" % (cur.rowcount, rp, k))
    samples.append((size, dt))
con.execute("ROLLBACK")
wall = time.perf_counter() - t_start
con.close()

all_ns = sorted(d for _, d in samples)
def pctl(p):
    return all_ns[int(p * (len(all_ns) - 1))]
band = collections.defaultdict(list)
for size, dt in samples:
    band[size // 32768].append(dt)
band_out = []
for b in sorted(band):
    v = sorted(band[b])
    band_out.append({"size_from": b * 32768, "size_to": (b + 1) * 32768 - 1, "n": len(v),
                     "median_ns": statistics.median(v), "mean_ns": statistics.mean(v)})

out = {
    "kind": "DIAGNOSTIC - destructive replay of the run's exact pack-write size sequence on a copy",
    "bench_db": B,
    "wall_seconds_for_the_full_replay": round(wall, 2),
    "write_size_distribution": summary,
    "per_lane": per_lane_out,
    "update_object_packs_ns": {
        "n": len(all_ns), "median": statistics.median(all_ns), "mean": statistics.mean(all_ns),
        "p10": pctl(0.10), "p25": pctl(0.25), "p75": pctl(0.75), "p90": pctl(0.90),
        "min": all_ns[0], "max": all_ns[-1],
    },
    "by_size_band_32KiB": band_out,
    "note": "every one of the 16,802 writes is replayed once, in pack order, with the size "
            "the pack actually had at that write; the median below is therefore the median "
            "of the run's own writes, not of a chosen blob",
}
json.dump(out, open("row2_defence.json", "w"), indent=2)

print(json.dumps({k: out[k] for k in ("write_size_distribution", "per_lane",
                                      "update_object_packs_ns", "wall_seconds_for_the_full_replay")}, indent=2))
print("\nby size band:")
for b in band_out:
    print("  %7d-%7d B  n=%5d  median=%9.3f us  mean=%9.3f us"
          % (b["size_from"], b["size_to"], b["n"], b["median_ns"] / 1000, b["mean_ns"] / 1000))
