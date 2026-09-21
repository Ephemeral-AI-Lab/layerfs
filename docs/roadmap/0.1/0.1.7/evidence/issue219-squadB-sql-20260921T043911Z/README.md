# #219 Squad B — the SAVE PATH's SQL at 10,000-file scale: a ranked statement table

> Status: **Research; diagnostic evidence, not a receipt.** No product code ran, no
> benchmark ran, no build ran, no Rust source was touched. Every ns/call number
> below is a **DIAGNOSTIC micro-benchmark issued by the Python stdlib `sqlite3`
> module against byte copies in `/tmp`**, not a harness measurement. The one
> harness figure quoted is the campaign receipt this squad was pointed at.
> Admission, budget classes and gates are unchanged by this document.

Worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch
`codex/219-ns10000`, HEAD `9c46930b846600e5f3c6ca4a4c4cbcf44ecdc356` (clean at
the start of this work). The measured row is
`core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-pinned2-20260921T031259Z/pipeline-namespace-10000/`.

---

## 0. What the precedent established — reported before anything else

Source: `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-rca-20260920T191016Z/README.md`
(read in full, lines 1–284; treatment commit `63fa15c49`).

That round investigated a **different lane** — the retained-history / multi-writer
stride10 regression, +16.756 s (16.360 s → 33.116 s). It established:

1. **One SQL clause dominated it.** The publication-scoped locator query gained
   `ORDER BY o.object_id,o.save_id LIMIT ?`. On this machine, against that run's
   own `sample.sqlite`, same pragma profile, 30,000 calls each:

   | form | µs/call |
   | --- | ---: |
   | previous model, `WHERE object_id IN (?)` | 4.559 |
   | new model, publication-scoped, **no** ORDER/LIMIT | 3.488 |
   | new model **+ `ORDER BY o.object_id,o.save_id LIMIT ?`** | **15.543** |
   | new model + `ORDER BY` only | 3.485 |
   | new model + `LIMIT` only | 13.840 |

   The `LIMIT` was the cost — 4.46× the clause it was added to. **The plan was
   identical with and without it**: `SEARCH o USING PRIMARY KEY (object_id=?)`,
   `SEARCH s USING INTEGER PRIMARY KEY (rowid=?)`, `SCAN r`. The scoping itself was
   1.07× *faster* than the previous model's query.
2. **It was paid 380,380 times inside the step**, so no commit-cadence change could
   avoid it. Removing the clause recovered **6.65 s of the 16.76 s** on the clean arm
   pair (33.116 → 26.467 s) and 7.49 s on the strictly matched instrumented pair.
3. **The second cost was the commit cadence**: 48,446 commits, of which the `COMMIT`
   statement alone was 2.809 s = **58.0 µs per commit**, 96 % of the `commit_ns`
   bucket, on the declared profile `journal_mode = MEMORY`, `synchronous = OFF`,
   `busy_timeout = 0` — no fsync in it; what it writes is the dirty page set, large
   because every pack append rewrites a whole pack body (255 packs, 45,298,203 bytes,
   177,640 average, 262,112 maximum → ≈8.1 GB of BLOB rewritten across 45,794 appends).
4. **The cadence is a contract, not a tuning choice.** Widening the step
   (`LAYERFS_STORAGE_COMMIT_EVERY`) made the *second writer fail*, not wait:
   `CleanupFailed { original: OwnershipUnavailable, cleanup: OwnershipUnavailable }`
   at every value above 1. Quote from `cas/lifecycle.rs::maybe_commit`: "A write
   transaction therefore never outlives the step that opened it under the arbitration
   lock, so every step commits before that lock is released. Batching stays inside a
   step; it cannot span steps."
5. The two effects accounted for 15.17 s of the 16.76 s; **1.59 s was unexplained**.
   `filesystem` growth (3.439 → 6.788 s) was declared outside that RCA's scope.

**Squad B's brief is the save path at 10,000-file scale, not that lane.** The
relevant carry-over is the *method* (per-statement calls × per-call cost, measured
against the run's own store, with `EXPLAIN QUERY PLAN` recorded verbatim) and one
concrete question: does that 15.5 µs clause exist here? (§7.4: **no**.)

---

## 1. Custody — what was and was not done

| | |
| --- | --- |
| Rust source modified | **none** (`git status --porcelain` empty before this squad's directory was created) |
| Builds run | **none** |
| Benchmarks run | **none** |
| Originals touched | **no.** `base.sqlite` sha256 `135af7c8a4581ae47525152043c36d8e1599897c1352a4c5bfe78252eea9e390`, mtime 1789964795; `sample.sqlite` sha256 `03918d61a9f004b292deafeadbea6992f870b8bdd96b3a631d4bc3309408db29`, mtime 1789964798 — both re-read after every copy and unchanged |
| Directories created | exactly one, this one |
| Copies | `/tmp/squadB/{base,sample}.sqlite` (byte-identical sha256, read-only in scenario R); `bench.sqlite`, `bench3.sqlite` (destructive diagnostics, scenario W and S) |
| Tools | `sqlite3` CLI 3.51.0; Python 3.14.3 stdlib `sqlite3` linked against SQLite **3.51.2**. The product links `libsqlite3-sys` 0.38.2 (bundled SQLite). **The version difference is declared and not corrected for.** |
| Statistics | **no `sqlite_stat1` in either database** — `ANALYZE` has never run, so every plan recorded in `explain.txt` was chosen with the planner's default (statistics-free) assumptions |

Engine limits read back from the measuring SQLite (not the product's), recorded for
the chunk-width derivation only: `SQLITE_LIMIT_VARIABLE_NUMBER = 250000`,
`SQLITE_LIMIT_SQL_LENGTH = 1000000000`.

---

## 2. The store at each end of the run

Both read directly from the copies (`explain.txt` PREAMBLE section, and the
inspection block reproduced in `db_state.txt`).

| | `base.sqlite` (pre-write) | `sample.sqlite` (post-run) |
| --- | ---: | ---: |
| file bytes | 65,536 | 307,879,936 |
| `page_count` / `page_size` / `freelist_count` | 16 / 4096 / 0 | 75,166 / 4096 / 27 |
| `objects` | 0 | **25,245** |
| `saves` | 1 | **2** |
| `object_packs` | 0 | **1,250** |
| `metadata_value_groups` | 0 | **207** |
| `content_signatures` | 0 | **8,192** |
| `store_policy.publication_sequence` | 1 | 2 |
| `store_policy.next_pack_id` / `next_ordinal` | 1 / 1 | 1,251 / 10,164 |
| `store_policy.metadata_window_values` | 0 | 10,163 |
| `sqlite_stat1` present | no | **no** |
| `sqlite_sequence` present | **yes** (`saves → 1`) | **yes** (`saves → 2`) |

Indexes present in both: `objects_save`, `packs_save`, `signatures_save`, plus the
implicit `sqlite_autoindex_saves_1/2` (`active_slot`, `publication` UNIQUE) and
`sqlite_autoindex_metadata_value_groups_1` (`UNIQUE(pack_id, group_number)`).
There is no index on `objects.pack_id`, `objects.object_id` alone, or
`content_signatures.stamp`.

### Derived run structure (used for the call counts)

egin{verbatim}
pipeline.inserted            25,245   receipt
pipeline.commits             17,378   receipt   -> 1.452 objects per commit
distinct (pack_id,group_number) in objects   16,595  -> 1.521 rows per
                                                        INSERT statement
object groups of exactly 1 row               10,081  (60.7 % of all groups)
pack rows (all created this run)              1,250
pack groups across both tables               16,802  = 16,595 + 207
pack writes                                   16,802  = 1,250 creates + 15,552 appends
pack bytes written through those writes 2,292,865,337  (2.14 GiB)
final pack bytes                          302,023,232  (288 MiB)  = write amplification 7.59x
                                              (matches receipt pack_bodies_bytes exactly)
end{verbatim}

The pack-write count is **DERIVED-EXACT** — a database count plus a source
invariant, not a receipt counter (§12(a) states it in full and names its falsifiers):
`pack/placement.rs::select_many`
emits one `SelectedWrite` per pack that receives a group **in that call**, and every
call in this run carries exactly one group — `cas/placement.rs::seal_group` passes
`vec![group]` (line 137), and `cas/pool_lane.rs::write_value_groups` passes all
groups of one leaf (line 295) while the catalogue shows every group holds ≤ 97 of
the 165 values a leaf's ≤100 rows can produce, so no call carries two. The byte
total is the sum over packs of the assembled length at each of its writes, read out
of the pack directory (`pack_parse.py`; format from `pack/layout.rs`).

---

## 3. The statement inventory

62 statements, every one quoted verbatim from the file:line it is attributed to and
**machine-checked against the source**: 60 match with folded whitespace, 2 (the two
`format!`-templated ones, `L1` and `S2`) match with all whitespace removed. See
`statements.tsv` and `sql_verbatim_check.txt`.

Sources named in the brief (`sqlite/{lookup,write,pool,ownership,schema,connection,cleanup}.rs`,
`cas/{lifecycle,placement,pool_lane,membership,owner,selection,store}.rs`) plus the
SQL that lives **outside** those files but is on the same path and is therefore
included rather than omitted: `cas/collision.rs` (the per-row collision probe),
`encoding/delta/candidates.rs` (the content-signature ring), and
`core/crates/layerfs-storage/sql/schema.sql` (the shipped schema, which carries
`AUTOINCREMENT`). `cas/membership.rs`, `cas/selection.rs`, `cas/owner.rs` and
`cas/store.rs` contain **no SQL of their own** — they call the `sqlite::` and
`encoding::` functions listed here; that was verified by grep, not assumed.

```
$ grep -rn -E "(SELECT|INSERT|UPDATE|DELETE|PRAGMA|BEGIN|COMMIT|ROLLBACK|CREATE TABLE|CREATE INDEX|WITH RECURSIVE|ANALYZE)" sqlite/ cas/
$ grep -rn -E "(pool::(group_for|ordinal_end|for_each_group|group_count|window_start|next_ordinal|insert_group)|lookup::(...)|crate::sqlite)" encoding/
```

---

## 4. The ranked table

Full machine-readable form: `ranked_table.tsv`. Every row carries its call-count
basis; `EXACT` means a value read out of the receipt or out of the two databases,
`DERIVED` means the source's own control flow fixes it, `ESTIMATE` means it does
not and the number is labelled as an estimate.

| # | statement (file:line) | calls | basis | ns/call | total ms |
| ---: | --- | ---: | --- | ---: | ---: |
| 1 | `COMMIT` — `sqlite/write.rs:52` (issued at `cas/lifecycle.rs:169` and `:256`) | 17,378 | **EXACT** receipt `pipeline.commits` | 63,583 | **1,104.9** |
| 2 | `UPDATE object_packs SET data = ?2 …` `sqlite/write.rs:82` + `INSERT INTO object_packs …` `sqlite/write.rs:70` | 16,802 | **DERIVED-EXACT** — a database group count plus a source invariant, **not** a receipt counter; derivation and its falsifiers in §12(a) | 27,208 | **457.1** |
| 3 | `SELECT o.object_id,… WHERE o.object_id IN (?) AND o.pack_id <= ?2` — `sqlite/lookup.rs:76-82`, called **once per row** from `cas/collision.rs:22` | 25,245 | **EXACT** = `pipeline.inserted` | 7,083 | **178.8** |
| 4 | `BEGIN IMMEDIATE` — `sqlite/write.rs:45` | 17,378 | **DERIVED** one acquisition + one per commit | 5,375 | **93.4** |
| 5 | `INSERT INTO objects (…) VALUES (?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope))` — `sqlite/write.rs:173-189` | 16,595 | **EXACT** = distinct `(pack_id,group_number)` in `sample.sqlite` | 4,250 | **70.5** |
| 6 | `INSERT OR REPLACE INTO content_signatures …` — `encoding/delta/candidates.rs:363-364` | 8,192 – 24,863 | **ESTIMATE** (lower bound **EXACT**: `sample` 8,192 rows, `base` 0) | 3,750 | 30.7 – 93.2 |
| 7 | `SELECT o.object_id,… IN (? × 43) …` — `sqlite/lookup.rs:76-82`, from `cas/save.rs:35` / `cas/dependencies.rs:66,91` | ~575 | **ESTIMATE** ≥575 waves by the 512 KiB batch bound | 69,709 | ~40.1 |
| 8 | `SELECT next_pack_id FROM store_policy WHERE id = 1` — `sqlite/ownership.rs:161` (from `cas/lifecycle.rs:133`) | 17,377 | **DERIVED** one per `begin_write` | 1,500 | **26.1** |
| 9 | `SELECT save_id,publication FROM temp.layerfs_read_scope` — `cas/collision.rs:17` | 16,595 | **EXACT** one per seal | 1,292 | **21.4** |
| 10 | `UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) …` — `cas/placement.rs:238` | 1,250 | **EXACT** one per created pack | 1,250 | 1.6 |
| 11 | `UPDATE store_policy SET next_pack_id = ?1 …` — `sqlite/ownership.rs:169` | ~1,250 | **DERIVED** one per allocated pack | 1,000 | ~1.3 |
| 12 | `SELECT next_ordinal …` `sqlite/ownership.rs:180` + `UPDATE store_policy SET next_ordinal …` `:189-192` | 207 each | **DERIVED** one pair per pooled leaf with fresh values = the 207 catalogue rows | 5,167 + 1,125 | 1.3 |
| 13 | `SELECT metadata_window_start FROM store_policy WHERE id = 1` — `sqlite/pool.rs:183` | 207 | **DERIVED** one per `write_value_groups` | 5,166 | 1.1 |
| 14 | `INSERT INTO metadata_value_groups …` — `sqlite/pool.rs:75-76` | 207 | **EXACT** `base` 0 → `sample` 207 | 2,000 | 0.4 |
| 15 | `SELECT c.stamp, c.object_id, c.signature … ORDER BY c.stamp` — `encoding/delta/candidates.rs:284` | 1 | **EXACT** once per `Store::open` (`cas/store.rs:259`) | ≈1.8×10⁶ engine | ~1.8 |
| 16 | schema/open reads — `sqlite/schema.rs` S1 ×3, S2 ×1, S3 ×7, S4 ×6, S5–S8 ×1; `sqlite/connection.rs` C1–C14 | ~35 | **DERIVED** from the `REQUIRED_TABLES` (6) / `REQUIRED_INDEXES` (3) loops and `configure`/`verify_profile` | 5×10³–10×10³ each | <0.3 |
| 17 | save-lifecycle singles: `O3`, `O4`, `O5`, `O6`, `O7`, `O8`, `L3` | 1 each | **EXACT** one save was started (`saves` 1 → 2), one `Store::open` | 5×10³–22×10³ | ≈0.05 |
| 18 | `SELECT p.data FROM object_packs …` — `sqlite/lookup.rs:161` | **NOT_MEASURED** | the receipt publishes no counter for acquired delta bases and this op published no chain counters | 49,417 | **NOT_COMPUTED** |
| 19 | `SELECT g.first_ordinal,… ORDER BY g.first_ordinal` — `sqlite/pool.rs:160-162` | 1 | **DERIVED** `PoolIndex::sync` runs once per save (`pool_synced`) | ≈6×10³ engine | ≈0.006 |
| 20 | `K1`–`K5` cleanup — `sqlite/cleanup.rs:33,42,46,52,71` | **0** | **EXACT** no failure path on a PASS run | n/a | 0 |
| 21 | `P1` `ordinal_end` / `pool::next_ordinal` — `sqlite/pool.rs:44-67` | **0** | **EXACT** no caller in the crate | n/a | 0 |
| 22 | `S9`/`S10`/`S11` and cleanup of an empty store — create path | **0** | **EXACT** `Store::create` was not called | n/a | 0 |

**Sum of the 11 rows with an EXACT call count and a MEASURED ns/call: 1,956.7 ms of
the 3,585.8 ms `operation_ns` = 54.6 %.** Rows 6, 7 and 18 are excluded from that sum
precisely because they are not exact; with the estimated values of rows 6 (8,192) and
7 (~575) the same eleven-plus-two sum is ≈2,027 ms ≈ 56.5 %, and row 18 remains
unquantified.

**The single largest cost is row 1: the `COMMIT` statement, 17,378 calls ×
63,583 ns = 1,104.9 ms = 30.8 % of `operation_ns` on its own**, and 1,562.0 ms
(43.6 %) together with row 2, the pack-body rewrite that makes each commit dirty a
whole pack. This is the same statement the precedent found dominant in the other
lane, at almost exactly the same per-call price (58.0 µs there, 63.6 µs here).

---

## 5. How each call count was derived, plainly

* **17,378 commits — exact.** The receipt's `pipeline.commits`, whose own basis is
  `SaveOutcome.commits` (`trace.jsonl` seq 23). `OutcomeCounters::commits` is
  incremented in exactly three places (`cas/lifecycle.rs:97` initial 1 for the
  acquisition transaction, `:175` in `maybe_commit`, `:258` in `finish_inner`).
* **17,378 `BEGIN IMMEDIATE`, 17,377 `SELECT next_pack_id` — derived.** Every
  `COMMIT` happens with `transaction_open` true, and `transaction_open` is only set
  by `begin_write` (`cas/lifecycle.rs:129-139`), which issues `BEGIN IMMEDIATE` and
  then `ownership::next_pack`. Every commit except the acquisition's is therefore
  preceded by exactly one `begin_write`. The acquisition's own `BEGIN IMMEDIATE`
  (`sqlite/ownership.rs:104`) issues no `next_pack`.
* **16,595 `INSERT INTO objects` and 16,595 seals — exact.** `SELECT COUNT(*) FROM
  (SELECT pack_id,group_number FROM objects GROUP BY 1,2)` = 16,595. Each seal calls
  `write::insert_objects` once (`cas/placement.rs:179`), and `insert_objects` chunks
  at `min(SQLITE_LIMIT_VARIABLE_NUMBER/6, SQLITE_LIMIT_SQL_LENGTH/58,
  OBJECT_INSERT_CHUNK_CAP=128)` (`sqlite/write.rs:115-129`). The largest group in
  the store holds **109** rows, so **no group splits and the statement count equals
  the group count whatever the engine's limits are above 109 rows.** The claim "the
  batching constant is 128" therefore does not have to be trusted for this number.
* **25,245 per-row membership queries — exact.** `cas/collision.rs:21-22` loops over
  the group's `rows` and calls `lookup::candidates(&[row.object_id])` once per row.
  `validate_candidates` is called once per seal (`cas/placement.rs:182`), so the
  total is the sum of the group sizes = every inserted object = `pipeline.inserted`
  = 25,245. **This is the N+1 query shape, and it is per row, not per group.**
* **16,802 pack writes — DERIVED-EXACT, and this is the one number the campaign
  has made load-bearing, so its basis is stated in full in §12(a).** It is a database
  count (16,802 `(pack_id, group_number)` groups) combined with the
  `pack/placement.rs::select_many` invariant that a one-group call produces exactly
  one write. It is **not** from a receipt counter, a trace counter or a loop bound —
  **the receipt publishes no `packs_created` and no `pack_appends` at all** (§12(c)).
  The 1,250 pack rows are `SELECT COUNT(*) FROM object_packs` on the sample store,
  with `base.sqlite` at 0 (§12(b)).
* **207 `INSERT INTO metadata_value_groups`, 207 `reserve_ordinals` pairs, 207
  `window_start` reads — exact/derived.** `base` had 0 catalogue rows and
  `sample` has 207; each leaf contributes `ceil(fresh/165)` groups and its ≤100
  rows make that 1, so calls = groups = 207.
* **`INSERT OR REPLACE content_signatures` — NOT derivable, bounded only.** One
  statement per `Candidates::insert` call (four call sites, `encoding/delta/select.rs:301,
  335, 359, 396`), flushed in `Candidates::flush` (`:367-378`). The receipt publishes
  no `full_records`, and the harness's `pipeline.statements` counter is written on a
  different op path than this row and is absent from this receipt's `counters` and
  `trace.jsonl`. **All 8,192 rows in `sample.sqlite` were written during this run**
  (`base` had 0, and the table is 0 % occupied before), so 8,192 is a hard lower
  bound; 24,863 content objects is the hard upper bound. **I cannot derive it and I
  do not present an estimate as a measurement.**
* **~575 wave-membership lookups — an estimate.** `PendingBatch` is bounded by
  `policy.rs::BATCH_CANONICAL_BYTES_LIMIT` = 512 KiB; the row's content is
  301,171,810 bytes, so ≥574 waves, and 24,863 content objects over 575 waves is 43
  ids per page. The *presence* lookup per wave (`cas/dependencies.rs::seed` →
  `lookup::present`) is **NOT_MEASURED**: the number of distinct references is not
  published anywhere in the receipt.
* **Row 18 (`pack_bytes`) — NOT_MEASURED call count.** Its per-call cost is measured
  (49.4 µs, a 241 KB BLOB by primary key) but nothing in the receipt says how many
  delta bases were acquired, so it is **not** placed in the ranking.

---

## 6. ns/call, and what the harness inflates

Method: warm the connection (32 warm-up calls), then N iterations of
`time.perf_counter_ns`; median and mean of the per-call samples
(`microbench.py`, `microbench.json`). Read-only statements ran against
`file:/tmp/squadB/sample.sqlite?mode=ro` with the run's read scope
`(save_id = 2, publication = 2)` in a TEMP table. Write statements ran against a
**third copy** `bench.sqlite` under the declared pragma profile
(`journal_mode=MEMORY`, `synchronous=OFF`, `temp_store=MEMORY`, `foreign_keys=ON`,
`busy_timeout=0`), replaying the real per-seal statement sequence with the real
262,112-byte pack body of pack 365 (the largest Native pack).

### Read-only (scenario R, n = 3,000 each)

| statement | median µs | mean µs |
| --- | ---: | ---: |
| `L1` candidates, 1 id (`cas/collision.rs:22`) | 7.083 | 7.215 |
| `L1` candidates, 128 ids (`sqlite/lookup.rs:76`) | 192.542 | 195.486 |
| scope read (`cas/collision.rs:17`) | 1.292 | 1.330 |
| `L2` pack_bytes (`sqlite/lookup.rs:161`) | 49.417 | 51.067 |
| `L3` highest_pack_id (`sqlite/lookup.rs:174`) | 5.333 | 5.414 |
| `O3` publication | 5.166 | 5.253 |
| `O4` live owners | 5.125 | 5.266 |
| `O5` slot alloc (recursive CTE) | 7.500 | 7.636 |
| `O9` next_pack | 5.166 | 5.268 |
| `O11` next_ordinal | 5.167 | 5.310 |
| `P3` group_for | 7.167 | 7.338 |
| `P4` for_each_group (207 rows) | 140.625 | 142.835 |
| `P5` group_count | 5.125 | 5.279 |
| `P6` window_start | 5.166 | 5.217 |
| `P1` ordinal_end | 5.959 | 6.130 |
| `C3` signature read (8,192 rows, ORDER BY) | 6,057.521 | 6,114.977 |
| S1 / S2 / S3 / S5 / S6 / S7 (schema, policy) | 5.166–9.583 | 5.279–9.843 |

### Page-width sweep of the membership query (`page_width.txt`)

| ids in the `IN (…)` | median µs | ns per id |
| ---: | ---: | ---: |
| 1 | 6.875 | 6,875 |
| 8 | 18.375 | 2,297 |
| 16 | 30.105 | 1,882 |
| 32 | 53.584 | 1,675 |
| 43 | 69.709 | 1,621 |
| 64 | 99.750 | 1,559 |
| 128 | 193.875 | 1,515 |

Fixed cost ≈ 5.4 µs, marginal ≈ 1.47 µs per id. **One id per call costs 4.5× what a
128-id page costs per id.** `cas/collision.rs` has the batch API available
(`lookup::candidates` pages its own identifiers) and calls it with one id, 25,245
times.

### The Python harness inflates every multi-row statement — measured, not asserted

`controls.py` / `control.txt` decompose the two multi-row statements:

| control | median µs | what it isolates |
| --- | ---: | --- |
| `SELECT COUNT(*) FROM content_signatures` | 5.8 | parse + plan + count |
| `SELECT stamp,object_id,signature FROM content_signatures` (8,192 rows, no ORDER BY) | 4,207.9 | + Python row materialisation |
| … `ORDER BY stamp` | 5,760.7 | + the temp B-tree: **1,552.8 µs** |
| full `C3` (join + scope + ORDER BY) | 6,245.7 | the whole statement |
| `SELECT c.stamp FROM content_signatures WHERE 0` | 4.7 | floor |
| `P4` without `ORDER BY` (207 rows) | 139.8 | 647 ns/row of Python overhead |
| `P4` with `ORDER BY g.first_ordinal` | 140.4 | **+0.7 µs — the sort is free** |

So: **513–647 ns per returned row is this harness, not SQLite.** `C3`'s true engine
cost is ≈1.6–1.8 ms of which **1.55 ms is the temp B-tree**, and `P4`'s true engine
cost is ≈6 µs. Rows 15 and 19 of the ranked table carry the engine figure; the raw
Python figure is in `microbench.json` for anyone who wants it.

### Write statements (scenario W, n = 1,500; S, n = 2,000)

| statement | median µs | mean µs |
| --- | ---: | ---: |
| `COMMIT` | **63.583** | 62.283 |
| `UPDATE object_packs SET data` (blob 38,081–262,112 B) | 27.208 | 27.368 |
| `BEGIN IMMEDIATE` | 5.375 | 5.751 |
| `INSERT INTO objects` (1 row) | 4.250 | 4.788 |
| `UPDATE saves SET pack_ceiling` | 1.250 | 1.381 |
| `SELECT next_pack_id` | 1.500 | 1.678 |
| **whole seal cycle, BEGIN…COMMIT, 6 statements** | **112.312** | 106.105 |
| `UPDATE store_policy SET next_pack_id` | 1.000 | 1.029 |
| `UPDATE store_policy SET next_ordinal, window` | 1.125 | 1.139 |
| `UPDATE store_policy SET publication_sequence` | 1.166 | 1.184 |
| `UPDATE saves SET active_slot=NULL, publication` | 0.833 | 0.845 |
| `INSERT INTO metadata_value_groups` | 2.000 | 2.106 |
| `INSERT OR REPLACE INTO content_signatures` | 3.750 | 3.902 |
| `INSERT INTO saves` (incl. AUTOINCREMENT) | 1.916 | 1.936 |
| single-row UPDATE control | 1.084 | 1.121 |

**112.3 µs per seal cycle × 17,378 commits ≈ 1.95 s.** That is an independent
corroboration of the ranked table's exact-row sum (1,956.7 ms) from a different
measurement: replaying the cycle as a unit rather than multiplying its parts.

---

## 7. Plans — full scans, temp B-trees, ORDER BYs, and the historical clause

Verbatim plans for all 45 statements, on both databases, are in `explain.txt`;
the condensed diff is `plan_table.txt`. **Every plan is identical on
`base.sqlite` and `sample.sqlite`** — with no `sqlite_stat1`, the planner has no
row counts to change its mind with.

### 7.1 Full scans

| statement | plan | when it runs | cost |
| --- | --- | --- | --- |
| `C3` `candidates.rs:284` | `SCAN c` (8,192 rows) `\| SEARCH s \| SCAN r \| USE TEMP B-TREE FOR ORDER BY` | **once per `Store::open`** | 1.55 ms of sort, measured |
| `S1`/`S2`/`S3` `schema.rs:135/144/203` | `SCAN sqlite_master` (13 objects) | once per open, ~11 calls | <0.1 ms |
| `X5` `schema.rs:354` | `SCAN objects USING COVERING INDEX objects_save` | **not executed** (create path) | — |
| `O5` slot alloc | `SCAN candidate` (≤ budget = 2 rows) + `SEARCH saves USING COVERING INDEX sqlite_autoindex_saves_1 (active_slot=?)` | once per save | 7.5 µs |
| every membership/pack query | `SCAN r` over `temp.layerfs_read_scope` — a **1-row** temp table, scanned once per outer row | per call | in the 5.4 µs fixed cost |

**There is no full scan of `objects` (25,245 rows), `object_packs` (1,250 × ~241 KB)
or `metadata_value_groups` (207 rows) anywhere on the save path.**

### 7.2 Temp B-trees

**Exactly one, and only one: `C3`'s `USE TEMP B-TREE FOR ORDER BY`**, sorting the
8,192-row `content_signatures` full scan. `P4`'s visually similar
`ORDER BY g.first_ordinal` does **not** produce one — the plan is
`SEARCH g USING INTEGER PRIMARY KEY (rowid>?)`, which already walks
`first_ordinal` in order; the control measures the clause at **+0.7 µs**, i.e. free.

### 7.3 An ORDER BY the tree already knows how to avoid

`C3` orders by `c.stamp`, which is not the primary key, so the sort is real. But
for a store that has never wrapped its ring the imposed order **is** the primary-key
order: with `stamp ≤ SLOTS`, `slot = (stamp - 1) % SLOTS = stamp - 1`. The planner
confirms ordering by the primary key needs no sort:

```
EXPLAIN QUERY PLAN SELECT c.stamp, c.object_id, c.signature FROM content_signatures c ORDER BY c.slot;
  QUERY PLAN
  `--SCAN c                     <- no "USE TEMP B-TREE"
EXPLAIN QUERY PLAN SELECT c.stamp, c.object_id, c.signature FROM content_signatures c ORDER BY c.stamp;
  QUERY PLAN
  `--SCAN c
  `--USE TEMP B-TREE FOR ORDER BY
```

**This is an observation, not a treatment: it was not implemented, not tested, and
no saving is claimed for it.** It is also worth only ~1.55 ms once per operation —
0.04 % of the row — so it is recorded for completeness, not as a candidate.

### 7.4 Does the historical 15.5 µs clause still exist in this path? — **No.**

`grep -rn -E "ORDER BY|LIMIT [0-9?]" core/crates/layerfs-storage/src` finds four
surviving `ORDER BY`s and no `ORDER BY o.object_id,o.save_id` anywhere in the tree:

* `encoding/delta/candidates.rs:284` — `ORDER BY c.stamp` (§7.2, 1.55 ms, once);
* `sqlite/pool.rs:162` — `ORDER BY g.first_ordinal` (**free**, §7.2);
* `sqlite/cleanup.rs:42,46,52` — `ORDER BY … LIMIT`, **failure path only**, zero calls
  on this run.

`sqlite/lookup.rs:76-82` — the save path's membership/locator query — carries
**neither an `ORDER BY` nor a `LIMIT`**, and its plan is the *same optimal plan* the
precedent recorded for the clause-free form:

```
|--SEARCH o USING PRIMARY KEY (object_id=?)
|--SEARCH s USING INTEGER PRIMARY KEY (rowid=?)
`--SCAN r
```

**The fix that closed the other lane's 6.65 s is present in this tree and this path
does not re-introduce the clause.** Its per-id cost here is 6.875 µs at page width 1
and 1.515 µs at page width 128 — nowhere near 15.5 µs.

### 7.5 A statement-shape defect that is not a plan defect

`cas/collision.rs:22` calls a **batch API with one id, once per row** — 25,245
times, 178.8 ms, the third-largest row in the table. The plan is optimal; the *call
shape* is 4.5× more expensive per id than the paged form the same module already
exposes (`sqlite/lookup.rs:39-41`, `LOOKUP_PAGE_IDS = 128`). The same file's
`save.rs:35` and `dependencies.rs:66,91` do use the paged form.

### 7.6 The structural finding the ranking names

One `COMMIT` per **1.45** inserted objects, and one whole-pack-body rewrite per
**1.52** inserted objects, because **10,081 of 16,595 groups (60.7 %) hold exactly
one record** — the WholeFile lane seals after every single member
(`cas/selection.rs:57-58` and `:97-102`), and every seal ends in
`maybe_commit()` (`cas/placement.rs:199`). The consequence is measured, not
inferred: **2,292,865,337 bytes of pack BLOB written for 302,023,232 bytes of final
pack data (7.59×)**, i.e. ≈132 KB of dirty pages per commit
(2,292,865,337 / 17,378), which is what makes a `COMMIT` cost 63.6 µs on a profile
with no fsync in it. The commit cadence itself is the multi-writer contract quoted
in §0, so this is a statement about *how many objects a seal carries*, not a
proposal to batch across steps.

---

## 8. `AUTOINCREMENT` and `sqlite_sequence` — is it on the per-commit path?

**No. Plainly: `sqlite_sequence` handling runs once in this run, not 17,378 times.**

* `sql/schema.sql:31-32` declares `saves (save_id INTEGER PRIMARY KEY AUTOINCREMENT, …)`,
  so SQLite maintains the `sqlite_sequence` table for it — and both databases have
  one: `base.sqlite` `sqlite_sequence` = `saves → 1`, `sample.sqlite` = `saves → 2`.
* The **only** `INSERT INTO saves` on the save path is `sqlite/ownership.rs:131`,
  inside `ownership::acquire`, which `cas/lifecycle.rs:39` calls **once per save**.
  It is not called again by any commit.
* The databases prove the count: `saves` went **1 → 2** across the whole measured
  run while `pipeline.commits` recorded **17,378** commits. **17,377 of the 17,378
  commits wrote no `saves` row and therefore performed no `sqlite_sequence`
  update.**
* SQLite behaviour relied on: for a table declared `AUTOINCREMENT`, an `INSERT`
  makes SQLite read and update the single `sqlite_sequence` row holding that table's
  largest-ever ROWID, as part of executing that statement. The work is therefore
  **per `INSERT INTO saves`**, not per transaction or per commit — which is why the
  17,378 commits do not pay it and the one save does.
* Cost: measured at **1.916 µs** for one `INSERT INTO saves` in the store's own
  schema (scenario S, n = 200, on the copy). Once per run that is 0.0002 % of
  `operation_ns`. **The `AUTOINCREMENT` clause is not a cost driver here and the
  ranking does not implicate it.**
* The table itself is one page of the 75,166, and it is not the reason
  `sqlite_stat1` is absent — `ANALYZE` was simply never run, which is why §7 has no
  statistics to explain.

---

## 9. What was NOT examined, and why

| not examined | reason |
| --- | --- |
| `lookup::pack_bytes` (`sqlite/lookup.rs:161`) **call count** | **NOT_MEASURED.** Nothing in the receipt or `trace.jsonl` publishes acquired-delta-base counts for this op (`SaveOutcome.chain` exists in the product but this harness row did not publish it). Its per-call cost *is* measured (49.4 µs). |
| The presence lookup's identifier volume (`cas/dependencies.rs:66,91`) | **NOT_MEASURED** — the harness published no `presence_queries` counter for this row. The wave count is bounded, the reference count is not. |
| `full_records` / `prefix_records` | Not in the receipt; needed for row 6's exact count. |
| Write amplification for **reused** saves | This row had `pipeline.reused = 0`, so every object was written. Nothing here generalises to a reuse-heavy save. |
| The read path (`cas/read.rs`, `cas/provider.rs`, `Store::read_batch`) | Out of scope by the brief (save path). Statements there are enumerated only where they share a function with the save path. |
| `sqlite/schema.rs` create path (`S9`–`S11`) and `cleanup.rs` | Plans recorded in `explain.txt` for completeness; **zero calls** in this run (the store was opened from a copy and the row passed). |
| `cas/membership.rs`, `cas/selection.rs`, `cas/owner.rs`, `cas/store.rs` SQL | **They contain none.** Verified by grep; they delegate to the `sqlite::` functions listed here. |
| Compression, codec and chain costs | Not SQL; outside this squad's brief. The 1,956.7 ms accounted for here leaves ≈1.6 s of `operation_ns` in non-SQL work (filesystem build, pack assembly, hashing, codecs) plus the three unquantified rows. |
| A product-side measurement of any kind | Forbidden by the brief. Every number is either from the receipt, from the two databases, or from the Python diagnostic. |
| `ANALYZE`-informed plans | `sqlite_stat1` does not exist and was deliberately not created; this report describes the planner's behaviour **without** statistics, which is the state the row ran in. |

---

## 10. Reproduce

```sh
cd docs/roadmap/0.1/0.1.7/evidence/issue219-squadB-sql-20260921T043911Z

# copies (never the originals)
mkdir -p /tmp/squadB
cp <receipt-dir>/base.sqlite   /tmp/squadB/base.sqlite
cp <receipt-dir>/sample.sqlite /tmp/squadB/sample.sqlite

python3 eqp.py > /tmp/squadB/eqp-body.sql      # 45 statements, verbatim from source
python3 plan_table.py explain.txt              # condensed plan diff
python3 statements.py statements.tsv           # 62 statements, verbatim-checked
python3 pack_parse.py pack_stats.json          # pack write amplification
python3 microbench.py microbench.json          # scenario R (ro) + W (destructive)
python3 microbench_supplement.py               # the remaining writes
python3 controls.py                            # Python-overhead decomposition
python3 rank.py                                # the ranked table
```

`explain.txt` is produced by concatenating a PREAMBLE (temp scope table, pragma
profile, version) with `eqp-body.sql` and piping it into
`sqlite3 "file:/tmp/squadB/<db>.sqlite?mode=ro"`.

## 11. Files

| file | what it is |
| --- | --- |
| `README.md` | this report |
| `explain.txt` | verbatim `EXPLAIN QUERY PLAN` for all 45 statements, on both databases |
| `plan_table.txt` | condensed plan-per-statement, with the base/sample diff and flags |
| `statements.tsv` | the 62-statement inventory, exact SQL + file:line |
| `sql_verbatim_check.txt` | the machine check that every quoted SQL exists in its source |
| `db_state.txt` | database inspection: pages, objects, tables, `sqlite_sequence`, `sqlite_stat1` |
| `pack_stats.json` | pack-by-pack groups/writes/bytes-written, from the pack directory |
| `pack_parse.py` | the parser that produced it |
| `microbench.json` | raw per-statement samples, scenarios R and W |
| `microbench.py` | the harness |
| `microbench_supplement.json`, `microbench_supplement.txt` | the remaining write statements |
| `microbench_supplement.py` | its harness |
| `controls.json`, `control.txt`, `controls.py` | the Python-overhead decomposition |
| `page_width.json`, `page_width.txt` | the membership query's cost against page width |
| `ranked_table.tsv`, `ranked_table.txt`, `rank.py` | the ranked table and its generator |
| `row2_defence.json`, `row2_defence.txt` | the run's exact 16,802-write size distribution and the replayed `UPDATE object_packs` cost at those sizes (§12(d)) |
| `row2_defence.py` | its harness |
| `eqp.py`, `plan_table.py`, `statements.py`, `rank.py` | generators for the files above |

**Not a receipt. Not release admission. No budget class is claimed.**
# Part II — row 2 defended, and the closing summary

## 12. Answers to the coordinator's four questions on row 2

Row 2 was escalated to "load-bearing". Its **number is unchanged (16,802 calls,
27,208 ns/call, 457.1 ms)**; its **label is downgraded** from `EXACT` to
`DERIVED-EXACT (database count + source invariant)`, because `EXACT` in §4 was
defined as "read out of the receipt or out of the two databases", and this count is
neither alone — it is a database count times a source invariant. Two corrections to
Squad A §13.1 are also owed (§12(e)).

### 12(a) The exact derivation of 16,802 — and what it is not

**It is not** a receipt counter. **It is not** a trace counter. **It is not** a loop
bound read off a line of source. There is no `packs_created`/`pack_appends` value
on disk for this row at all (§12(c)).

**It is two facts multiplied together.**

**Fact 1 — how many groups exist. Database count, exact.**
```
SELECT COUNT(*) FROM (SELECT pack_id,group_number FROM objects
                      UNION
                      SELECT pack_id,group_number FROM metadata_value_groups);
  -> 16802        (16,595 from objects + 207 from metadata_value_groups)
```
Raw output: `db_state.txt` and `pack_stats.json`. `objects` holds 25,245 rows in
16,595 distinct groups and `metadata_value_groups` holds 207 rows, each already
unique on `(pack_id, group_number)` by its own schema constraint
(`sql/schema.sql:56`). A group is written by exactly one seal
(`cas/placement.rs:161` `write_pack`, or `cas/pool_lane.rs:300` `write_pack` inside
the value-group loop), and no group's rows were removed: `objects` = 25,245 =
`pipeline.inserted` exactly, the freelist is 27 pages, and no cleanup statement ran
(§4 row 20).

**Fact 2 — one call, one write. Source invariant, quoted.**
`pack/placement.rs::select_many` (lines 75–154) builds `writes` from a single
`pending` slot:

```rust
let mut pending: Option<(i64, bool, Vec<PlacedGroup>)> = None;
for group in groups {
    let fits_open = match &self.open { Some(open) => append_fits(lane, open.assembled, open.groups.len(), &group)?, None => false };
    if !fits_open {
        if let Some((pack_id, created, placed)) = pending.take() {
            writes.push(self.assemble_write(lane, pack_id, created, placed, true)?);
        }
        ...                       // allocate a new pack, pending = Some((pack_id, true, vec![]))
    }
    if pending.is_none() { ... pending = Some((open.pack_id, false, Vec::new())); }
    ...
}
if let Some((pack_id, created, placed)) = pending {
    writes.push(self.assemble_write(lane, pack_id, created, placed, false)?);
}
```

With a **one-element** `groups`, `pending` can hold at most one pack, the mid-loop
`pending.take()` push can only fire when `pending` was already set (which needs a
second input group), and the loop-end push fires once. **One input group ⇒ exactly
one `SelectedWrite` ⇒ exactly one `write_pack` ⇒ exactly one SQL statement.**

**Both call sites do pass one group.**
* `cas/placement.rs:137`: `select_many(lane, vec![group], &mut self.next_pack_id)` —
  one group. Covers all **16,595** object groups.
* `cas/pool_lane.rs:294-295`: `select_many(lane, encoded, …)` where `encoded` is one
  entry per `fresh.chunks(VALUES_PER_GROUP)` (`cas/pool_lane.rs:270`) and
  `VALUES_PER_GROUP = 165` (`policy.rs:135`). The catalogue's largest row holds **97**
  values, so every such call carried **one** group. Covers all **207** pooled groups.
  Independent confirmation that no pooled group is missing: `SUM(count)` over
  `metadata_value_groups` = **10,163** = `store_policy.next_ordinal` − 1 = 10,164 − 1
  — every ordinal reserved landed in exactly one catalogue row.

**16,595 + 207 = 16,802.** Raw byte-sum cross-check: `pack_stats.json` reports
`writes == groups` for every one of the four lanes.

**Falsifiers, so the claim can be broken rather than believed.** If any pooled
`write_value_groups` call had carried two or more groups, the true write count would
be **lower** than 16,802 (one call writes each affected pack once). If any one-group
`select_many` emitted two writes, the true count would be **higher**. Both are
excluded by the reading above; neither can hide in the finished store, because the
store records groups, not writes.

### 12(b) The 1,250 pack rows — confirmed, four ways

1. `SELECT COUNT(*) FROM object_packs` on `sample.sqlite` = **1,250** (this is the
   source the coordinator read; `db_state.txt`).
2. `SELECT COUNT(*) FROM object_packs` on `base.sqlite` = **0** — so **all 1,250
   rows were created inside this run**: `packs_created = 1,250` for the row.
3. `store_policy.next_pack_id` = **1,251** — the allocator handed out ids 1…1250
   exactly once each (`pack/placement.rs:98-104`).
4. `saves.pack_ceiling` for save 2 = **1,250**.

**A discrepancy to fix before it propagates.** Squad A's synthesis carries
`object_packs_data_bytes: 304427008`. The measured value is
`SELECT SUM(length(data)) FROM object_packs` = **302,023,232**, which is *exactly*
the receipt's `resources.space.pack_bodies_bytes: 302023232`. The 0.79 % difference
does not change any conclusion (7.59× vs 7.60×) but the 302,023,232 figure is the one
the receipt itself publishes.

### 12(c) Does the receipt publish `packs_created` / `pack_appends`? — **No. Plainly not.**

* The pinned2 receipt's **entire** `counters` block is 15 keys:
  `pipeline.batches, pipeline.bindings, pipeline.chain_objects, pipeline.commits,
  pipeline.content_bytes, pipeline.content_objects, pipeline.declared_content_bytes,
  pipeline.declared_directories, pipeline.declared_files, pipeline.inserted,
  pipeline.largest_batch_bindings, pipeline.metadata_objects, pipeline.objects_emitted,
  pipeline.reused, timing_json_bytes`.
* `trace.jsonl` publishes the same 15 counters. A substring search over both files for
  `packs_created`, `pack_appends`, `profile_sql_ns`, `profile_commit_ns`,
  `full_records` and `presence_queries` returns **False for every one of them**.
* The row's identity is binary `cb21593db53ba10921cea555b1aa1db38bead5795115596c64a4c2d8ce86372c`,
  `source_commit 2a63aff0d0a24743deebb9129ab14e098d9e24ed`, started 2026-09-21T04:26:32Z.
* What *does* exist is an **uncommitted +55-line modification** to
  `core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs` in this worktree
  (mtime 12:47 local, i.e. after every receipt on disk) which adds
  `pipeline.packs_created`, `pipeline.pack_appends`, `pipeline.full_records`,
  `pipeline.presence_queries` and the `profile_*` group. **No receipt on disk was
  produced by that code**, and the modification's own comment says so:
  "this driver published none of them, which is why the pack-append call count had to
  be derived from the …".
* Neither does **Squad A's own diagnostic receipt**
  (`…squadA-profile-20260921T044041Z/diagnostic-receipt.json`, binary
  `71d7c40ce1d35cd1997e20a87f8298c98357cb4e5179acbb2c49e5a3457d552c`, started
  04:40:42Z) publish them: it carries the `profile_*` group but **no**
  `packs_created`/`pack_appends`.

**Consequence for the coordinator: yes — the amplification arithmetic rests on Squad
B's derived count, because no counter for it exists on this row. And the two numbers
Squad A's `pack-append-synthesis.json` lists under `"measured"` —
`sql_ns: 781237539`, `commit_ns: 1351360518` — come from Squad A's *diagnostic* run
on a *different* binary, not from the pinned2 row. They are not this row's numbers and
must not be quoted as this row's.**

### 12(d) What blob size did the UPDATE arm rewrite? The run's own distribution — and 243 KB is not its average

Scenario W (§6) replayed the prefix sizes of **one** pack (the largest Native pack,
id 365, final 262,112 B), so its blob cycled 38,081 → 262,112 B and its 27,208 ns
median is the median over that one pack's prefixes, **not** over the run.

`row2_defence.py` / `row2_defence.json` / `row2_defence.txt` replay **all 16,802
writes once each**, in pack order, at the size that pack actually had at that write,
read out of the pack directory. That is the run's own size distribution:

| write-size | bytes |
| --- | ---: |
| min | 191 |
| p10 | 38,407 |
| p25 | 75,639 |
| **median** | **137,544** |
| **mean** | **136,464** |
| p75 | 196,713 |
| p90 | 234,378 |
| max | 262,141 |

**The average pack rewrite is ≈134 KiB, not 243 KB.** 243 KB sits between p90 and the
maximum, near `PACK_LIMIT = 256 KiB` (`policy.rs:95`); the maximum any pack in this
store reached is 262,141 B. A 27,208 ns rewrite of a 243 KB body and of a 4 KB body do
mean very different things — and this statement's cost is close to linear in the blob:

| blob band | n | median ns | mean ns |
| --- | ---: | ---: | ---: |
| 0–32 KiB | 1,119 | 6,875 | 14,302 |
| 32–64 KiB | 2,350 | 11,041 | 28,821 |
| 64–96 KiB | 2,266 | 12,334 | 15,041 |
| 96–128 KiB | 2,278 | 19,209 | 25,297 |
| 128–160 KiB | 2,268 | 24,084 | 34,100 |
| 160–192 KiB | 2,309 | 31,708 | 54,817 |
| 192–224 KiB | 2,168 | 49,104 | 79,889 |
| 224–256 KiB | 2,044 | 63,855 | 83,704 |

Replayed at exactly the run's sizes, `UPDATE object_packs SET data = ?2 …` measures
**median 26,000 ns**, mean 43,069 ns, p10 9,917, p90 104,292 (n = 16,802, one
transaction). **Median × calls = 16,802 × 26,000 = 436.9 ms**, within 4.6 % of the
ranked table's 457.1 ms (which came from scenario W's 27,208 ns median, an independent
run). The mean-based figure would be 723 ms, but that mean is carried by a tail
(max 17.7 ms) that a single long transaction accumulates and the product does not,
because its UPDATEs commit at every step. **Row 2's total stands at 457.1 ms and is,
if anything, mildly conservative.**

### 12(e) Two corrections to Squad A §13.1 — and the append↔commit linkage

**(1) The intermediate pack lengths ARE readable; the amplification is measured, not
derived.** §13.1 states "the store keeps only final pack lengths, so the intermediate
lengths cannot be read back." They can. The pack directory stores `body_start`,
`encoded` and `decoded` per group for the ordinary lanes
(`pack/layout.rs:331-393`) and a 4-byte start offset per group for the compact
whole-file lane (`pack/layout.rs:397-454`). Reading all 1,250 directories reproduces
every intermediate assembled length exactly (`pack_parse.py` asserts
`Σ(entry + body) == length(data)` for every pack; 0 failures).

**Measured amplification: 2,292,865,337 bytes written / 302,023,232 bytes persisted =
7.5917×.** The `(k+1)/2` derivation gives 7.72×; the two agree to 1.7 % and the
measured one supersedes it. **Quote 2.29 GB and 7.59×, not "approximately 2.34 GB"
and 7.72×.**

**(2) `(k+1)/2` with a global mean k is the wrong shape.** 14.44 writes per pack is a
mean over a heavily skewed population:

| lane | packs | writes | writes/pack | bytes written | final bytes | amplification |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| **WholeFile** | 102 | 9,444 | **92.59** | 1,260,350,294 | 24,613,232 | **51.21×** |
| PooledMetadata | 3 | 207 | 69.00 | 25,584,656 | 726,096 | 35.24× |
| Native | 1,142 | 7,130 | 6.24 | 1,004,612,191 | 276,058,548 | 3.64× |
| Ordinary | 3 | 21 | 7.00 | 2,318,196 | 625,356 | 3.71× |
| **all** | **1,250** | **16,802** | 13.44 | **2,292,865,337** | **302,023,232** | **7.59×** |

The amplification is **not uniform**: 3.6× where a pack holds ~6 groups, **51×** where
it holds ~93. The WholeFile and PooledMetadata lanes together rewrite **56.1 %** of all
rewritten bytes to persist **8.4 %** of the final bytes. The WholeFile lane is the
extreme case because `cas/selection.rs:57-58,97-102` seals that lane after every single
member: 9,444 seals of one record each land in 102 packs, so each ~2.6 KB of new data
drags a ~241 KB pack body through SQLite again.

**(3) On "an append is issued once per open lane tail per transaction".** Appends are
**already** one per affected pack per transaction — no code path writes the same pack
twice inside one transaction (`cas/placement.rs:161` takes `writes.first()` and there
is exactly one write to take). What is observed is the opposite of tail-holding:
**1,142 Native packs received 7,130 appends across 7,130 separate transactions, and
102 WholeFile packs received 9,444.** So the linkage between appends and commits is
**seal-level, not lane-level**: appends = 16,595 seals + 207 pooled calls, and every
seal is its own transaction. The consequence is blunt: *"hold the tail and issue one
UPDATE per pack per transaction" is already the behaviour and cannot reduce anything.*
Only coalescing **seals** — i.e. widening the step the multi-writer contract forbids
widening (§0.4) — reduces appends and commits together.

**(4) The 3.4 % proximity is exactly accounted for; do not lean on it as a
coincidence.** `commits − appends = 17,378 − 16,802 = 576`, and 576 is
`207` (`reserve_ordinals`, one per pooled leaf) `+ 207` (`write_value_groups`, one per
pooled leaf) `+ 1` (acquisition) `+ 1` (publication) `+ 367` (`flush_candidates` that
found a write to flush). **The 367 is DERIVED BY SUBTRACTION** — the one term of the
commit budget that is in neither the receipt nor the database, and it must be quoted
with that label, exactly as row 6's and row 7's estimates are.

---

## 13. Closing: the table, the derivations, the NOT_EXAMINED list

### 13.1 The ranked table (top 8 by total cost)

| # | statement (file:line) | calls | call-count basis | ns/call | total ms |
| ---: | --- | ---: | --- | ---: | ---: |
| 1 | `COMMIT` — `sqlite/write.rs:52` | 17,378 | **EXACT** receipt `pipeline.commits` | 63,583 | **1,104.9** |
| 2 | `UPDATE object_packs SET data = ?2 …` `sqlite/write.rs:82` (+ `INSERT` `write.rs:70`) | 16,802 | **DERIVED-EXACT** — DB group count × `select_many` invariant (§12(a)); **no receipt counter exists** | 27,208 | **457.1** |
| 3 | `SELECT … WHERE o.object_id IN (?) AND o.pack_id <= ?2` `sqlite/lookup.rs:76-82` via `cas/collision.rs:22` | 25,245 | **EXACT** = `pipeline.inserted` | 7,083 | **178.8** |
| 4 | `BEGIN IMMEDIATE` — `sqlite/write.rs:45` | 17,378 | **DERIVED** 1 acquisition + 1 per commit | 5,375 | **93.4** |
| 5 | `INSERT INTO objects (…) VALUES …` — `sqlite/write.rs:173-189` | 16,595 | **EXACT** distinct `(pack_id,group_number)`; largest group 109 < chunk cap 128 | 4,250 | **70.5** |
| 6 | `INSERT OR REPLACE INTO content_signatures …` — `encoding/delta/candidates.rs:363` | 8,192–24,863 | **ESTIMATE**; lower bound EXACT (`base` 0 → `sample` 8,192) | 3,750 | 30.7–93.2 |
| 7 | `SELECT … IN (? × 43) …` `sqlite/lookup.rs:76-82` via `cas/save.rs:35` | ~575 | **ESTIMATE** ≥575 waves by the 512 KiB batch bound | 69,709 | ~40.1 |
| 8 | `SELECT next_pack_id FROM store_policy WHERE id = 1` — `sqlite/ownership.rs:161` | 17,377 | **DERIVED** one per `begin_write` | 1,500 | **26.1** |

Rows 1–5, 8 and the rest of §4's table sum to **1,956.7 ms of the 3,585.8 ms
`operation_ns` = 54.6 %** on exact call counts and measured ns/call alone.
**The single largest cost is row 1: the `COMMIT`, 1,104.9 ms = 30.8 % of the row on
its own**, and 43.6 % together with row 2.

### 13.2 The derivations, in one place

* **Receipt counter (EXACT): rows 1 and 3.** `pipeline.commits = 17,378`,
  `pipeline.inserted = 25,245`. Nothing else in this table is a receipt counter,
  because the receipt publishes 15 counters and none of the others is among them.
* **Database count (EXACT): rows 5, and the 1,250/207/8,192/16,802 structural numbers.**
  `SELECT COUNT(*)` on the two stores, raw in `db_state.txt`.
* **Database count × source invariant (DERIVED-EXACT): row 2, and the 16,802 itself.**
  §12(a).
* **Source control flow (DERIVED): rows 4, 8, 10–14, 16, 17, 19**, and the 367
  `flush_candidates` commits (§12(e)(4)). Each names its invariant in §4 and §5.
* **Bounded but not fixed (ESTIMATE): rows 6 and 7.** Both state their bounds.
* **Not derivable at all (NOT_MEASURED): row 18** (`pack_bytes` call count) and the
  wave presence-lookup identifier volume.
* **Zero, proven by the run: rows 20–22** (cleanup, dead code, create path).

### 13.3 NOT_EXAMINED

| not examined | why |
| --- | --- |
| `lookup::pack_bytes` (`sqlite/lookup.rs:161`) **call count** | No counter for acquired delta bases exists in the receipt or in `trace.jsonl`; the harness did not publish `SaveOutcome.chain` for this op. Per-call cost *is* measured (49,417 ns). |
| The presence lookup's identifier volume (`cas/dependencies.rs:66,91`) | No `presence_queries` counter on this row (§12(c)). The wave count is bounded; the reference count is not. |
| `full_records` / `prefix_records` | Not published; needed for row 6's exact count. The counter exists in the working tree (`ops/pipeline.rs` `pipeline.full_records`) but no receipt was produced by it. |
| Write amplification for a **reuse-heavy** save | This row had `pipeline.reused = 0`; every object was written. Nothing here generalises to reuse. |
| The read path (`cas/read.rs`, `cas/provider.rs`, `Store::read_batch`) | Out of the brief's scope (save path). Enumerated only where a function is shared. |
| `Store::create` (`S9`–`S11`) and `cleanup.rs` `K1`–`K5` | Plans recorded in `explain.txt`; **zero calls** in this run (opened from a copy, row PASSed). |
| `cas/membership.rs`, `cas/selection.rs`, `cas/owner.rs`, `cas/store.rs` SQL | They contain none; verified by grep, not assumed. |
| Compression, codec, chain and filesystem-build costs | Not SQL. The 1,956.7 ms accounted here leaves ≈1.6 s of `operation_ns` outside SQL plus the three unquantified rows. |
| Any product-side measurement | Forbidden by the brief. Every number is from the receipt, the two databases, or the Python diagnostic on a copy. |
| `ANALYZE`-informed plans | `sqlite_stat1` does not exist and was deliberately not created; §7 describes the planner **without** statistics, which is the state the row ran in. |
| Whether the +55-line harness modification changes the numbers | It was written after every run on disk; no receipt exercises it, and it was not run (the brief forbids running benchmarks). |

**Not a receipt. Not release admission. No budget class is claimed.**
