#!/usr/bin/env python3
"""DIAGNOSTIC micro-benchmark of the save path's SQL statements.

NOT A RECEIPT. No product code runs, no benchmark harness runs, the product
timers are not involved. Statements are issued by the Python stdlib sqlite3
module against COPIES in /tmp/squadB.

Scenario R - read-only, opened mode=ro, non-destructive.
Scenario W - DESTRUCTIVE, on a separate copy bench.sqlite, replaying the
             exact statement sequence the product issues per group seal, with
             the real pack bytes the run produced.

Timing: warm the connection, then N iterations of time.perf_counter_ns per call.
Reported: median, mean, p10, p90 of the per-call samples.
"""
import sqlite3, json, os, shutil, statistics, sys, time

SB = "/tmp/squadB"
READ_DB = "file:" + SB + "/sample.sqlite?mode=ro"
BENCH_DB = SB + "/bench.sqlite"

results = {"scenario_R": {}, "scenario_W": {}, "meta": {}}


def stats(samples):
    s = sorted(samples)
    n = len(s)
    return {
        "n": n,
        "median_ns": statistics.median(s),
        "mean_ns": statistics.mean(s),
        "p10_ns": s[int(0.10 * (n - 1))],
        "p90_ns": s[int(0.90 * (n - 1))],
        "min_ns": s[0],
        "max_ns": s[-1],
    }


def bench(fn, n, warm=32):
    for _ in range(warm):
        fn()
    samples = []
    for _ in range(n):
        t0 = time.perf_counter_ns()
        fn()
        samples.append(time.perf_counter_ns() - t0)
    return stats(samples)


def scenario_read():
    con = sqlite3.connect(READ_DB, uri=True, isolation_level=None)
    con.execute("PRAGMA temp_store = MEMORY")
    con.execute("PRAGMA foreign_keys = ON")
    con.execute("CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)")
    con.execute("INSERT INTO layerfs_read_scope VALUES (2, 2)")

    ids = [r[0] for r in con.execute("SELECT object_id FROM objects LIMIT 128")]
    assert len(ids) == 128
    id1 = ids[0]
    pack_id = con.execute("SELECT MIN(pack_id) FROM object_packs").fetchone()[0]
    ordinal = con.execute("SELECT MAX(first_ordinal) FROM metadata_value_groups").fetchone()[0]

    def CAND(n):
        ph = ",".join("?" * n)
        sql = ("SELECT o.object_id,o.object_role,o.canonical_length,o.pack_id,o.group_number,"
               "o.record_number,o.save_id,(o.save_id = r.save_id OR s.publication <= r.publication) "
               "FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r "
               "WHERE o.object_id IN (" + ph + ") AND o.pack_id <= ?" + str(n + 1))
        def run():
            con.execute(sql, ids[:n] + [1250]).fetchall()
        return run

    stmts = {
        "L1-candidates-1id (cas/collision.rs:22)": CAND(1),
        "L1-candidates-128ids (sqlite/lookup.rs:76)": CAND(128),
        "scope-read (cas/collision.rs:16)": lambda: con.execute(
            "SELECT save_id,publication FROM temp.layerfs_read_scope").fetchall(),
        "L2-pack-bytes (sqlite/lookup.rs:161)": lambda: con.execute(
            "SELECT p.data FROM object_packs p JOIN saves s USING(save_id),temp.layerfs_read_scope r "
            "WHERE p.pack_id = ?1 AND length(p.data) BETWEEN 32 AND ?2 "
            "AND (p.save_id = r.save_id OR s.publication <= r.publication)",
            (pack_id, 16781312)).fetchall(),
        "L3-highest-pack-id (sqlite/lookup.rs:174)": lambda: con.execute(
            "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs").fetchall(),
        "O3-publication (sqlite/ownership.rs:88)": lambda: con.execute(
            "SELECT publication_sequence FROM store_policy WHERE id = 1").fetchall(),
        "O4-live-owners (sqlite/ownership.rs:107)": lambda: con.execute(
            "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL").fetchall(),
        "O5-slot-alloc (sqlite/ownership.rs:116)": lambda: con.execute(
            "WITH RECURSIVE candidate(slot) AS ( SELECT 1 UNION ALL SELECT slot + 1 FROM candidate WHERE slot < ?1 ) "
            "SELECT MIN(slot) FROM candidate WHERE NOT EXISTS (SELECT 1 FROM saves WHERE active_slot = candidate.slot)",
            (2,)).fetchall(),
        "O9-next-pack (sqlite/ownership.rs:161)": lambda: con.execute(
            "SELECT next_pack_id FROM store_policy WHERE id = 1").fetchall(),
        "O11-next-ordinal (sqlite/ownership.rs:180)": lambda: con.execute(
            "SELECT next_ordinal FROM store_policy WHERE id = 1").fetchall(),
        "P3-group-for (sqlite/pool.rs:97)": lambda: con.execute(
            "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest "
            "FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r "
            "WHERE g.first_ordinal = (SELECT MAX(first_ordinal) FROM metadata_value_groups WHERE first_ordinal <= ?1) "
            "AND (p.save_id = r.save_id OR s.publication <= r.publication)", (ordinal,)).fetchall(),
        "P4-for-each-group (sqlite/pool.rs:160)": lambda: con.execute(
            "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest "
            "FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r "
            "WHERE g.first_ordinal >= ?1 AND (p.save_id = r.save_id OR s.publication <= r.publication) "
            "ORDER BY g.first_ordinal", (1,)).fetchall(),
        "P5-group-count (sqlite/pool.rs:174)": lambda: con.execute(
            "SELECT COUNT(*) FROM metadata_value_groups").fetchall(),
        "P6-window-start (sqlite/pool.rs:183)": lambda: con.execute(
            "SELECT metadata_window_start FROM store_policy WHERE id = 1").fetchall(),
        "P1-ordinal-end (sqlite/pool.rs:46)": lambda: con.execute(
            "SELECT COALESCE(MAX(first_ordinal + count), ?1) FROM metadata_value_groups "
            "WHERE first_ordinal > (SELECT MAX(first_ordinal) FROM metadata_value_groups) - ?2",
            (1, 165)).fetchall(),
        "C3-signature-read (encoding/delta/candidates.rs:284)": lambda: con.execute(
            "SELECT c.stamp, c.object_id, c.signature FROM content_signatures c JOIN saves s USING(save_id), temp.layerfs_read_scope r "
            "WHERE c.save_id = r.save_id OR s.publication <= r.publication ORDER BY c.stamp").fetchall(),
        "S1-index-present (sqlite/schema.rs:135)": lambda: con.execute(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1 AND type = 'index'", ("packs_save",)).fetchall(),
        "S2-unexpected-table (sqlite/schema.rs:144)": lambda: con.execute(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN "
            "('store_policy','object_packs','metadata_value_groups','objects','content_signatures','saves')").fetchall(),
        "S3-table-sql (sqlite/schema.rs:203/221)": lambda: con.execute(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1", ("objects",)).fetchall(),
        "S5-load-policy (sqlite/schema.rs:252)": lambda: con.execute(
            "SELECT format_profile, small_file_threshold_bytes, whole_file_delta_max_depth, chunk_delta_max_depth, "
            "metadata_delta_max_depth FROM store_policy WHERE id = ?1", (1,)).fetchall(),
        "S6-max-concurrent-writes (sqlite/schema.rs:301)": lambda: con.execute(
            "SELECT max_concurrent_writes FROM store_policy WHERE id = 1").fetchall(),
        "S7-retained-pack-ceiling (sqlite/schema.rs:338)": lambda: con.execute(
            "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1").fetchall(),
    }
    N = 3000
    for name, fn in stmts.items():
        results["scenario_R"][name] = bench(fn, N)
    results["meta"]["P4_rows_per_call"] = con.execute(
        "SELECT COUNT(*) FROM metadata_value_groups").fetchone()[0]
    results["meta"]["C3_rows_per_call"] = con.execute(
        "SELECT COUNT(*) FROM content_signatures").fetchone()[0]
    con.close()


def scenario_write():
    if os.path.exists(BENCH_DB):
        os.remove(BENCH_DB)
    shutil.copyfile(SB + "/sample.sqlite", BENCH_DB)
    packs = json.load(open(SB + "/packs.json"))
    native = sorted([p for p in packs if p["lane"] == "Native"], key=lambda p: -p["final"])[0]

    con = sqlite3.connect(BENCH_DB, isolation_level=None)
    con.execute("PRAGMA journal_mode = MEMORY")
    con.execute("PRAGMA synchronous = OFF")
    con.execute("PRAGMA temp_store = MEMORY")
    con.execute("PRAGMA foreign_keys = ON")
    con.execute("PRAGMA busy_timeout = 0")
    con.execute("CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL)")

    con.execute("INSERT INTO saves (active_slot) VALUES (1)")
    save_id = con.execute("SELECT last_insert_rowid()").fetchone()[0]
    con.execute("INSERT INTO layerfs_read_scope VALUES (?, 9223372036854775807)", (save_id,))

    pid, data = con.execute("SELECT pack_id,data FROM object_packs WHERE pack_id = ?", (native["pack_id"],)).fetchone()
    new_pid = 900001
    gc = int.from_bytes(data[12:16], "little")
    sizes = []
    for i in range(gc):
        e = data[16 + 16 * i:16 + 16 * (i + 1)]
        sizes.append(int.from_bytes(e[4:8], "little"))
    prefixes = []
    acc = 16
    for s in sizes:
        acc += 16 + s
        prefixes.append(acc)

    def new_id(i):
        return i.to_bytes(24, "big") + b"\x00" * 8

    counter = [1_000_000]

    def one_cycle():
        i = counter[0]
        counter[0] += 1
        t = {}
        t0 = time.perf_counter_ns()
        con.execute("BEGIN IMMEDIATE")
        t["BEGIN IMMEDIATE"] = time.perf_counter_ns() - t0
        t0 = time.perf_counter_ns()
        con.execute("SELECT next_pack_id FROM store_policy WHERE id = 1").fetchone()
        t["SELECT next_pack_id"] = time.perf_counter_ns() - t0
        blob = data[:prefixes[i % len(prefixes)]]
        t0 = time.perf_counter_ns()
        if i == 1_000_000:
            # first cycle creates the pack row, exactly as write::insert_pack does
            con.execute("INSERT INTO object_packs (pack_id, data, save_id) VALUES (?, ?, "
                        "(SELECT save_id FROM temp.layerfs_read_scope))", (new_pid, blob))
            t["INSERT INTO object_packs"] = time.perf_counter_ns() - t0
        else:
            con.execute("UPDATE object_packs SET data = ?2 WHERE pack_id = ?1 AND save_id = "
                        "(SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS (SELECT 1 FROM saves "
                        "WHERE saves.save_id = object_packs.save_id AND publication IS NULL)", (new_pid, blob))
            t["UPDATE object_packs SET data"] = time.perf_counter_ns() - t0
        t0 = time.perf_counter_ns()
        con.execute("INSERT INTO objects (object_id, object_role, canonical_length, pack_id, group_number, "
                    "record_number, save_id) VALUES (?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))",
                    (new_id(counter[0] * 7 + 1), 1, 1024, new_pid, i % 256, 0))
        t["INSERT INTO objects (1 row)"] = time.perf_counter_ns() - t0
        t0 = time.perf_counter_ns()
        con.execute("UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) WHERE save_id = ?1 AND active_slot IS NOT NULL",
                    (save_id, new_pid))
        t["UPDATE saves SET pack_ceiling"] = time.perf_counter_ns() - t0
        t0 = time.perf_counter_ns()
        con.execute("COMMIT")
        t["COMMIT"] = time.perf_counter_ns() - t0
        if i % 40 == 39:
            con.execute("BEGIN IMMEDIATE")
            con.execute("DELETE FROM objects WHERE save_id = ?", (save_id,))
            con.execute("COMMIT")
        return t

    N = 1500
    con.execute("BEGIN IMMEDIATE"); con.execute("COMMIT")
    for _ in range(16):
        one_cycle()
    acc = {}
    for _ in range(N):
        for k, v in one_cycle().items():
            acc.setdefault(k, []).append(v)
    for k, v in acc.items():
        results["scenario_W"][k] = stats(v)

    cyc = []
    for _ in range(300):
        t0 = time.perf_counter_ns()
        one_cycle()
        cyc.append(time.perf_counter_ns() - t0)
    results["scenario_W"]["WHOLE SEAL CYCLE (BEGIN..COMMIT, 6 statements)"] = stats(cyc)
    results["meta"]["W_blob_bytes"] = len(data)
    results["meta"]["W_blob_written_range"] = [prefixes[0], prefixes[-1]]
    results["meta"]["W_pack_id_replayed"] = native["pack_id"]
    results["meta"]["W_bench_db_bytes"] = os.path.getsize(BENCH_DB)
    con.close()


results["meta"]["read_db"] = READ_DB
results["meta"]["bench_db"] = BENCH_DB
results["meta"]["python"] = sys.version
results["meta"]["sqlite_lib"] = sqlite3.sqlite_version
try:
    probe = sqlite3.connect(":memory:")
    results["meta"]["SQLITE_LIMIT_VARIABLE_NUMBER"] = probe.getlimit(sqlite3.SQLITE_LIMIT_VARIABLE_NUMBER)
    results["meta"]["SQLITE_LIMIT_SQL_LENGTH"] = probe.getlimit(sqlite3.SQLITE_LIMIT_SQL_LENGTH)
    probe.close()
except Exception as e:
    results["meta"]["limit_probe_error"] = repr(e)

scenario_read()
scenario_write()
json.dump(results, open(sys.argv[1], "w"), indent=2, sort_keys=False)
print(json.dumps({k: {"median_us": round(v["median_ns"] / 1000, 3), "mean_us": round(v["mean_ns"] / 1000, 3), "n": v["n"]}
                  for k, v in list(results["scenario_R"].items()) + list(results["scenario_W"].items())}, indent=2))
print(json.dumps(results["meta"], indent=2))
