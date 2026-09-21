#!/usr/bin/env python3
"""Supplementary DIAGNOSTIC: per-call cost of the remaining save-path writes.
DESTRUCTIVE on a fresh third copy (bench3.sqlite). Statements are issued inside
one open transaction, which is the product's shape, and rolled back at the end."""
import sqlite3, json, os, shutil, statistics, time
SB="/tmp/squadB"; B=SB+"/bench3.sqlite"
if os.path.exists(B): os.remove(B)
shutil.copyfile(SB+"/sample.sqlite", B)
con=sqlite3.connect(B, isolation_level=None)
con.execute("PRAGMA journal_mode = MEMORY"); con.execute("PRAGMA synchronous = OFF")
con.execute("PRAGMA temp_store = MEMORY"); con.execute("PRAGMA foreign_keys = ON")
con.execute("CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)")
con.execute("INSERT INTO layerfs_read_scope VALUES (2, 2)")

def bench(fn, n=2000, warm=50):
    for _ in range(warm): fn()
    s=[]
    for _ in range(n):
        t0=time.perf_counter_ns(); fn(); s.append(time.perf_counter_ns()-t0)
    s.sort()
    return {"n":n,"median_ns":statistics.median(s),"mean_ns":statistics.mean(s),
            "p10_ns":s[int(0.10*(n-1))],"p90_ns":s[int(0.90*(n-1))]}

blob = con.execute("SELECT data FROM object_packs ORDER BY length(data) DESC LIMIT 1").fetchone()[0]
out={}
con.execute("BEGIN IMMEDIATE")
out["UPDATE store_policy SET next_pack_id (advance_pack, ownership.rs:169)"] = bench(
    lambda: con.execute("UPDATE store_policy SET next_pack_id = ?1 WHERE id = 1 AND next_pack_id <= ?1", (1251,)))
out["UPDATE store_policy SET next_ordinal+window (reserve_ordinals, ownership.rs:189)"] = bench(
    lambda: con.execute("UPDATE store_policy SET next_ordinal = ?1, metadata_window_start = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?3 ELSE metadata_window_start END, metadata_window_values = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?2 ELSE metadata_window_values + ?2 END WHERE id = 1", (10164, 49, 10115)))
out["UPDATE store_policy SET publication_sequence (publish, ownership.rs:140)"] = bench(
    lambda: con.execute("UPDATE store_policy SET publication_sequence = publication_sequence + 1, retained_pack_ceiling = MAX(retained_pack_ceiling, (SELECT pack_ceiling FROM saves WHERE save_id = ?1)) WHERE id = 1 AND publication_sequence < 9223372036854775807", (2,)))
out["UPDATE saves SET active_slot=NULL,publication (publish, ownership.rs:148)"] = bench(
    lambda: con.execute("UPDATE saves SET active_slot = NULL, publication = (SELECT publication_sequence FROM store_policy WHERE id = 1) WHERE save_id = ?1 AND active_slot IS NOT NULL AND publication IS NULL", (2,)))
# the product writes the pack row first (write_pack), then the catalogue row
con.execute("INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, (SELECT save_id FROM temp.layerfs_read_scope))",
            (900002, b"\x00" * 4096))
ctr=[0]
def ins_mvg():
    ctr[0]+=1
    con.execute("INSERT INTO metadata_value_groups (first_ordinal, count, pack_id, group_number, digest) VALUES (?1, ?2, ?3, ?4, ?5)",
                (2_000_000+ctr[0], 49, 900002, ctr[0]%250, b"\x00"*32))
# the catalogue table allows only 256 group ordinals per pack, so this one is
# sampled over a shorter run than the others
out["INSERT INTO metadata_value_groups (sqlite/pool.rs:75)"] = bench(ins_mvg, n=200, warm=10)
def ins_sig():
    ctr[0]+=1
    con.execute("INSERT OR REPLACE INTO content_signatures (slot, stamp, object_id, signature, save_id) VALUES (?1, ?2, ?3, ?4, (SELECT save_id FROM temp.layerfs_read_scope))",
                (ctr[0]%8192, 900000+ctr[0], b"\x11"*32, b"\x22"*32))
out["INSERT OR REPLACE content_signatures (candidates.rs:363)"] = bench(ins_sig)
# Any INSERT into an AUTOINCREMENT table exercises the sqlite_sequence update.
# The shipped CHECK admits exactly one of (active_slot, publication), so this
# probe inserts a distinct publication instead of a slot; the AUTOINCREMENT
# work is the same statement-level work the product's slot insert does.
sctr=[0]
def ins_save():
    sctr[0]+=1
    con.execute("INSERT INTO saves (publication) VALUES (?1)", (10_000_000+sctr[0],))
out["INSERT INTO saves (ownership.rs:131, incl. AUTOINCREMENT)"] = bench(ins_save, n=200, warm=10)
out["UPDATE objects (nothing) - control"] = bench(lambda: con.execute("UPDATE store_policy SET id = 1 WHERE id = 1"))
con.execute("ROLLBACK")
con.close()
json.dump({"bench_db": B, "statements": out}, open("microbench_supplement.json","w"), indent=2)
for k,v in out.items():
    print("%-72s median=%8.3f us mean=%8.3f us" % (k, v["median_ns"]/1000, v["mean_ns"]/1000))
