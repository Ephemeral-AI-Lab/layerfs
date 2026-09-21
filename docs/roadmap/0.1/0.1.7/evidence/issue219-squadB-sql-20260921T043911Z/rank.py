#!/usr/bin/env python3
"""Ranked table: calls x ns/call for the save path's SQL statements.

calls      - the per-statement call count at this scale, with its basis
ns_per_call- DIAGNOSTIC micro-benchmark median on a COPY (scenario R read-only,
             scenario W destructive replay, scenario S supplementary)
total_ms   - calls * ns_per_call / 1e6, arithmetic only
"""
import json

mb = json.load(open("microbench.json"))
sp = json.load(open("microbench_supplement.json"))["statements"]
pw = json.load(open("page_width.json"))

def m(d, k):
    return d[k]["median_ns"]

R = mb["scenario_R"]; W = mb["scenario_W"]

# id, statement, calls, calls_basis, ns_per_call, cost_basis
ROWS = [
 ("1", "COMMIT  (sqlite/write.rs:52, issued by cas/lifecycle.rs:169 and :256)",
  17378, "EXACT: receipt counters pipeline.commits = 17378, basis SaveOutcome.commits",
  m(W, "COMMIT"), "MEASURED W median (n=1500)"),
 ("2", "UPDATE object_packs SET data = ?2 ... (sqlite/write.rs:82) + INSERT INTO object_packs ... (sqlite/write.rs:70)",
  16802, "EXACT: 16,802 groups parsed out of sample.sqlite; pack/placement.rs emits exactly one SelectedWrite per group",
  m(W, "UPDATE object_packs SET data"), "MEASURED W median; the create form is 1,250 of the 16,802 calls"),
 ("3", "SELECT o.object_id,... WHERE o.object_id IN (?) AND o.pack_id <= ?2  (sqlite/lookup.rs:76-82, called from cas/collision.rs:22)",
  25245, "EXACT: receipt pipeline.inserted = 25245 = rows written = one call per row (cas/collision.rs:21-22)",
  m(R, "L1-candidates-1id (cas/collision.rs:22)"), "MEASURED R median (n=3000)"),
 ("4", "BEGIN IMMEDIATE  (sqlite/write.rs:45)",
  17378, "DERIVED: 1 acquisition transaction + one begin_write per acknowledged commit (cas/lifecycle.rs:129-139); every COMMIT is preceded by exactly one BEGIN",
  m(W, "BEGIN IMMEDIATE"), "MEASURED W median"),
 ("5", "INSERT INTO objects (...) VALUES (?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))  (sqlite/write.rs:173-189)",
  16595, "EXACT: distinct (pack_id,group_number) in sample.sqlite = 16,595 seals = 16,595 statements (largest group is 109 rows < the 128-row chunk cap, so no group splits)",
  m(W, "INSERT INTO objects (1 row)"), "MEASURED W median (1-row form; 10,081 of 16,595 groups are 1 row)"),
 ("6", "INSERT OR REPLACE INTO content_signatures ...  (encoding/delta/candidates.rs:363-364)",
  "8192-24863", "ESTIMATE - NOT DERIVED EXACTLY: one statement per Candidates::insert call (four call sites in encoding/delta/select.rs). Lower bound EXACT: sample.sqlite holds 8,192 rows, base.sqlite held 0, so >=8192 rows were written in this run. Upper bound: the 24,863 content objects. No counter in the receipt publishes full_records.",
  m(sp, "INSERT OR REPLACE content_signatures (candidates.rs:363)"), "MEASURED S median (n=2000)"),
 ("7", "SELECT o.object_id,... WHERE o.object_id IN (? x 43) AND o.pack_id <= ?44  (sqlite/lookup.rs:76-82, called from cas/save.rs:35 and cas/dependencies.rs:66/91)",
  "~575", "ESTIMATE: PendingBatch holds <= 512 KiB (policy.rs BATCH_CANONICAL_BYTES_LIMIT); 301,171,810 content bytes / 524,288 = 574.4, so >=575 waves; each wave issues >=1 lookup::locations page (24,863 ids / 575 = 43 ids per page). The presence query per wave is NOT_MEASURED.",
  pw["43"]["median_ns"], "MEASURED page-width sweep, 43-id page (n=3000)"),
 ("8", "SELECT next_pack_id FROM store_policy WHERE id = 1  (sqlite/ownership.rs:161, called from cas/lifecycle.rs:133)",
  17377, "DERIVED: one per begin_write, i.e. one per commit except the acquisition commit",
  m(W, "SELECT next_pack_id"), "MEASURED W median"),
 ("9", "SELECT save_id,publication FROM temp.layerfs_read_scope  (cas/collision.rs:17)",
  16595, "EXACT: one per validate_candidates call = one per seal (cas/placement.rs:182)",
  m(R, "scope-read (cas/collision.rs:16)"), "MEASURED R median"),
 ("10", "UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) ...  (cas/placement.rs:238)",
  1250, "EXACT: one per pack this save created; sample.sqlite has 1,250 pack rows and base.sqlite had 0",
  m(W, "UPDATE saves SET pack_ceiling"), "MEASURED W median"),
 ("11", "UPDATE store_policy SET next_pack_id = ?1 ...  (sqlite/ownership.rs:169, advance_pack)",
  "1250", "DERIVED: written only when this transaction allocated a pack; 1,250 packs were allocated",
  m(sp, "UPDATE store_policy SET next_pack_id (advance_pack, ownership.rs:169)"), "MEASURED S median"),
 ("12", "SELECT next_ordinal ... (ownership.rs:180) + UPDATE store_policy SET next_ordinal ... (ownership.rs:189-192), reserve_ordinals",
  207, "DERIVED: one pair per pooled leaf with fresh values = the 207 catalogue rows of sample.sqlite (each holds <165 values, so one group per call)",
  m(R, "O11-next-ordinal (sqlite/ownership.rs:180)") + m(sp, "UPDATE store_policy SET next_ordinal+window (reserve_ordinals, ownership.rs:189)"), "MEASURED R + S medians"),
 ("13", "SELECT metadata_window_start FROM store_policy WHERE id = 1  (sqlite/pool.rs:183)",
  207, "DERIVED: one per write_value_groups call (cas/pool_lane.rs:336) = 207",
  m(R, "P6-window-start (sqlite/pool.rs:183)"), "MEASURED R median"),
 ("14", "INSERT INTO metadata_value_groups (...) VALUES (?1, ?2, ?3, ?4, ?5)  (sqlite/pool.rs:75-76)",
  207, "EXACT: base.sqlite had 0 rows, sample.sqlite has 207",
  m(sp, "INSERT INTO metadata_value_groups (sqlite/pool.rs:75)"), "MEASURED S median"),
 ("15", "SELECT c.stamp, c.object_id, c.signature ... ORDER BY c.stamp  (encoding/delta/candidates.rs:284)",
  1, "EXACT: Candidates::load is called once per Store::open (cas/store.rs:259) and the harness opens the Store once",
  "~1.8e6 engine (6.25e6 incl. Python row materialisation)", "MEASURED R; controls.py separates 1.55e6 ns of temp-B-tree sort and 4.2e6 ns of Python row materialisation"),
 ("16", "schema/open reads: S1 x3, S2 x1, S3 x7, S4 x6, S5-S8 x1 each, C1-C14 once per handle (sqlite/schema.rs, sqlite/connection.rs)",
  "~35", "DERIVED from the loops in schema::validate (REQUIRED_TABLES=6, REQUIRED_INDEXES=3) and connection::configure/verify_profile",
  "5e3-10e3 each", "MEASURED R medians 5.1e3-9.6e3 ns"),
 ("17", "save lifecycle singles: O3 publication, O4 live owners, O5 slot alloc, O6 INSERT INTO saves (AUTOINCREMENT), O7+O8 publish, L3 highest_pack_id",
  1, "EXACT: one save was started (saves went 1 -> 2) and Store::open ran once",
  "sum ~2.2e4", "MEASURED R medians + S INSERT INTO saves"),
 ("18", "SELECT p.data FROM object_packs ... (sqlite/lookup.rs:161, pack_bytes)",
  "NOT_MEASURED", "NOT_MEASURED: the receipt publishes no counter for acquired delta bases; the harness did not publish chain counters for this op",
  m(R, "L2-pack-bytes (sqlite/lookup.rs:161)"), "MEASURED R median (per call), call count unknown"),
 ("19", "SELECT g.first_ordinal,... ORDER BY g.first_ordinal  (sqlite/pool.rs:160-162, for_each_group)",
  1, "DERIVED: PoolIndex::sync runs once per save (cas/pool_lane.rs:221-242, pool_synced flag)",
  "~6e3 engine (1.406e5 incl. Python row materialisation for 207 rows)", "MEASURED R + controls.py"),
 ("20", "K1-K5 cleanup statements (sqlite/cleanup.rs:33,42,46,52,71)",
  0, "EXACT: the run's status is PASS and there is no failure path; abandoned saves are the only caller",
  "n/a", "NOT_EXECUTED in this run (plans recorded in explain.txt)"),
 ("21", "P1 ordinal_end / pool::next_ordinal (sqlite/pool.rs:44-67)",
  0, "EXACT: no caller in the crate - grep finds only pool.rs:44 (definition) and pool.rs:65 (the wrapper). Not on the save path.",
  "n/a", "NOT_EXECUTED"),
 ("22", "S9/S10/S11 create-path statements + cleanup of an empty store",
  0, "EXACT: Store::create was not called; the store was opened from a copy",
  "n/a", "NOT_EXECUTED"),
]

def ns(v):
    return v if isinstance(v, (int, float)) else None

out = []
for sid, stmt, calls, basis, nspc, costbasis in ROWS:
    if isinstance(calls, int) and isinstance(nspc, (int, float)):
        total_ms = calls * nspc / 1e6
    else:
        total_ms = None
    out.append((sid, stmt, calls, basis, nspc, total_ms, costbasis))

with open("ranked_table.tsv", "w") as fh:
    fh.write("rank\tstatement\tcalls\tcalls_basis\tns_per_call\ttotal_ms\tcost_basis\n")
    for sid, stmt, calls, basis, nspc, total_ms, costbasis in out:
        fh.write("%s\t%s\t%s\t%s\t%s\t%s\t%s\n" % (
            sid, stmt, calls, basis,
            ("%.0f" % nspc) if isinstance(nspc, (int, float)) else nspc,
            ("%.1f" % total_ms) if total_ms is not None else "NOT_COMPUTED", costbasis))

print("%-4s %-58s %10s %12s %14s" % ("rank", "statement", "calls", "ns/call", "total ms"))
print("-" * 106)
for sid, stmt, calls, basis, nspc, total_ms, costbasis in out:
    print("%-4s %-58s %10s %12s %14s" % (
        sid, stmt[:58], calls,
        ("%.0f" % nspc) if isinstance(nspc, (int, float)) else "n/a",
        ("%.1f" % total_ms) if total_ms is not None else "n/a"))
ranked = [o for o in out if o[5] is not None and isinstance(o[2], int)]
ranked.sort(key=lambda o: -o[5])
print()
print("EXACT-call-count rows only, by total cost:")
for sid, stmt, calls, basis, nspc, total_ms, costbasis in ranked:
    print("  %-4s %-56s %8d x %10.0f ns = %8.1f ms" % (sid, stmt[:56], calls, nspc, total_ms))
exact_sum = sum(o[5] for o in ranked)
print("  sum of the %d rows with an EXACT call count and a MEASURED ns/call: %.1f ms" % (len(ranked), exact_sum))
print("  operation_ns = 3585.8 ms  ->  %.1f %%" % (100 * exact_sum / 3585.8))
