# Report E — Reference C2 (root `crates/layerfs-layerstack-store`, v0.1.6) static complexity inventory

Static source reading only: no build, no tests, no timing, no new measurements, no
performance claims. Every complexity/bound statement below is derived from code, not
from a run. Companion: parallel agent inventories `core/crates/layerfs-storage`;
core statements here are **ORIENTATION ONLY** (greps/heads, not a full read).

## 0. Tree identity and frozen-reference check

- HEAD `5e45897dd9f56e7f563aada031a68070a438fb93`, branch `main`, `git status --porcelain`
  empty (clean). `git describe --tags` = `v0.1.6-120-g5e45897dd`.
- Although the repo is 120 commits past the `v0.1.6` tag,
  `git log v0.1.6..HEAD -- crates/layerfs-layerstack-store` is empty and
  `git diff --stat v0.1.6..HEAD -- crates/layerfs-layerstack-store` is empty: this crate is
  **byte-identical to the pinned v0.1.6 oracle**. Since the tag, all of `crates/` changed
  only by test/example files under `crates/layerfs-content` (`git diff --stat v0.1.6..HEAD
  -- crates/`: 5 files, +2141, all `examples/` or `tests/`). The frozen-reference claim in
  AGENTS.md holds for the module in my scope.

## Files actually read

Production code; inline `#[cfg(test)]`/`mod tests` spans skipped unless noted.

| File (under `crates/layerfs-layerstack-store/src/`) | Lines | Span read |
|---|---|---|
| `objects.rs` | 7099 | 1–4617 (prod; tests 4619–7099 skipped) |
| `objects/admission.rs` | 1940 | all |
| `objects/pack.rs` | 1953 | 1–1520 prod (1521+ tests) |
| `objects/read.rs` | 2078 | all (2078 = test mod decl) |
| `objects/delta.rs` | 116 | all |
| `objects/small_candidates.rs` | 125 | all |
| `objects/spill.rs` | 935 | all |
| `objects/whole.rs` | 424 | all |
| `objects/metadata.rs` | 548 | all |
| `objects/admission/metadata_values.rs` | 143 | all |
| `schema.rs` | 1021 | 1–638, 974–1021 (tests 640–969 skipped) |
| `workspace.rs` | 1206 | 255–484 (commit flow) only |
| `layerstack.rs` | 4284 | 3450–3609 only (cleanup-failure test region) |
| `statements.rs` | 384 | grepped for statement names only |

Not read (UNKNOWNs below): `query.rs`, `records.rs`, `telemetry.rs`, `staging.rs`,
`store.rs`, `branch.rs`, `ids.rs`, `error.rs`, `lib.rs`, `schema/compatibility.rs`,
test modules under `objects/`.

**Complexity variables**: `n` = objects in one batch/wave; `b` = canonical bytes of that
batch; `g` = groups per pack; `r` = records per group; `h` = predecessor hints per object
(≤4); `d` = dependency-chain depth; `V`,`E` = candidate-graph vertices/edges; `m` = bytes
of one match trial. **Bound classes**: ENFORCED = checked at runtime, rejects when
crossed; STRUCTURAL = inherent to dataflow, no independent guard; UNKNOWN = not
determinable from source.

## 1. Algorithm inventory (reference)

Rows marked **[CMP]** mirror a core mechanism (core was derived from this code) — direct
comparison points for the parallel agent.

| # | Algorithm | path:line | Time (n defined) | Peak owned memory | I/O trips | Bound |
|---|---|---|---|---|---|---|
| 1 | Locator/membership probe `object_locations` | objects/read.rs:422-481 | O(n log n) BTreeMap insert over n ids; SQL ceil(n/128) | n·(32B id + ~40B loc) | 1-id: 1 prepared stmt (read.rs:433); n ids: 1 dynamically formatted `IN` stmt per ≤128-id chunk (read.rs:448-460) | ENFORCED page=128 (objects.rs:33) |
| 2 | Presence-only probe `objects_exist` | objects/read.rs:393-418 | O(n) | O(n) ids | 1 `count(*) IN` stmt per ≤128 chunk | ENFORCED page=128 |
| 3 | Exact-reuse/CAS compare `compare` | objects/admission.rs:1871-1933 | O(n log n) sort + O(b) byte compare per known id | bounded by VALIDATION_RESERVE 1 MiB (read.rs:11,1915-1917) | locator query + full group reads via `visit_locations_with_reserve` per known id | ENFORCED (batch ≤512, admission.rs:1879) |
| 4 | Fresh-store Bloom-ish filter | objects.rs:2444-2482 | O(1)/id (4 bit tests) | 4 MiB bitmap (objects.rs:51) | none (skips SQL) | ENFORCED (only when store initially empty, objects.rs:2276-2287) |
| 5 | Session duplicate guards | objects.rs:4234-4251 | O(1) expected/id (HashMap) | pending map ≤512 entries (cleared per flush, objects.rs:4388) | none | ENFORCED batch ≤512 (objects.rs:4301) |
| 6 | Comparison-reuse cache | objects.rs:4194-4219 | O(log n) per lookup | ≤2 MiB, clear-on-saturation (objects.rs:52,4212-4216) | avoids re-read when location unchanged | ENFORCED 2 MiB |
| 7 | Producer pipeline `run_finalized_output` **[CMP]** | objects.rs:372-517 | O(tasks) work-steal over mutex-held iterator (objects.rs:426-436) | workers·(slab 256 KiB) + 4 queued slabs (1 MiB) (objects.rs:41-43,44-48) | 0 SQL; slabs hand off via sync_channel(4) (objects.rs:397) | ENFORCED workers ≤4 (objects.rs:48), slab 512 obj/256 KiB (objects.rs:821-829) |
| 8 | Incoming-wave buffering | objects.rs:4252-4270 | O(1)/object | ≤512 obj/256 KiB + index | triggers row 9/10 per wave | ENFORCED 512/256 KiB (objects.rs:4253-4256); >256 KiB object isolated probe+flush (objects.rs:4259-4270) |
| 9 | Pending-batch flush | objects.rs:4292-4363,4365-4391 | O(n) per batch | ≤512 obj/512 KiB (2·256 KiB, objects.rs:4300-4304) | 1 `consume_checked_owned_page` per batch → row 11 | ENFORCED |
| 10 | SQL cohort (open transaction) | objects.rs:2197-2341 | O(1) amortized per batch | 0 extra | `BEGIN IMMEDIATE` once; `COMMIT` at 8191 rows/4 MiB-1 (objects.rs:2316-2340) | ENFORCED 8191 obj / 4 MiB-1 (objects.rs:35-36) |
| 11 | Prepared admission `prepare_missing`/`prepare_full` **[CMP]** | objects/admission.rs:178-333 | partition O(n); lane packing O(n) with per-object charge | ≤6 MiB data + 2 MiB physical scratch (admission.rs:215,345-381) | 0 SQL (pure prep) | ENFORCED 512 obj / 4 MiB-1 / 512 KiB multi (admission.rs:195-199) |
| 12 | Small lane `prepare_small` | objects/admission.rs:383-572 | O(n) objects; per object 1 FULL + ≤1 delta compress (admission.rs:491-514) | static 2 MiB codec workspace (pack.rs:1226-1237) | predecessor locator+group read per anchored object (admission.rs:421,471-476) | ENFORCED pack 256 KiB/256 groups (admission.rs:518-519) |
| 13 | Small-candidate hint cache | objects/small_candidates.rs:68-124 | O(1) insert, O(8·refs) find | 128 KiB fixed (INDEX_BYTES, small_candidates.rs:4,23-31) | none; converts a would-be read into reuse | ENFORCED 1024 slots/8192 refs |
| 14 | Rolling-hash signature | objects/small_candidates.rs:42-66 | O(raw) per value | 8·u64 | none | STRUCTURAL |
| 15 | Native lane `prepare_native` | objects/admission.rs:574-827 | per object: 1 FULL encode + ≤1 prefix trial (admission.rs:615-626,735-743) | 1 MiB static workspace + frames (pack.rs:233-235) | `read_native_prior` locator+chain read per hinted object (admission.rs:676) | ENFORCED trials ≤512/batch, depth ≤4, closure ≤1 MiB (admission.rs:699-703,1419-1421) |
| 16 | Ordinary lane `prepare_ordinary` | objects/admission.rs:919-1195 | per object: hint search (row 17) + group encode (row 18) | group buffers ≤2 MiB scratch (admission.rs:995,1035) | hint reads per target | ENFORCED group targets 32 KiB content / 16 KiB metadata (admission.rs:952), pack 256 KiB (128 KiB metadata lane, admission.rs:290-294) |
| 17 | Delta-candidate search `DeltaSearch::candidate` **[CMP]** | objects/admission.rs:1696-1828 | O(h·m) per object, h ≤4 hints | match budgets: 16 MiB/batch, 512 KiB/target, 8 fetches/target (read.rs:59-74,1239) | locator + group read per hint (admission.rs:1742-1759) | ENFORCED (all three budgets) |
| 18 | Delta match engine `delta_record` | objects/pack.rs:808-954 | seed pass O(base/16) + match pass O(target·4·matchlen), byte-charged | 4096×4×u32 table = 64 KiB (pack.rs:860) | none | ENFORCED by shared 16 MiB `remaining` (admission.rs:1689); instructions ≤8191 (pack.rs:9) |
| 19 | Group encoding full-vs-mixed `encode_group` | objects/pack.rs:958-1048 | O(group bytes) ×2 alternatives | group bytes ×2 transiently | none | ENFORCED group ≤64 KiB, records ≤8191 (pack.rs:6,9); mixed needs ≥max(64,len/8) saving (pack.rs:968-969) |
| 20 | Oversized singleton spool | objects/admission.rs:1197-1246 | O(object bytes) | object bytes once (spooled via temp file, re-read) | 1 temp-file write + 1 read + 1 pack INSERT | ENFORCED canonical ≤16 MiB (pack.rs:10), >64 KiB → singleton (admission.rs:299-306) |
| 21 | Pooled-value prep `prepare_values` | objects/admission/metadata_values.rs:31-143 | O(rows) dedup + O(new) encode | ≤128 KiB input (metadata_values.rs:38-44) | 1 `ValueIndex` sync + 1 batched fingerprint lookup per wave (metadata_values.rs:51,86) | ENFORCED 165 values/group (metadata.rs:9), index ≤131072 entries (metadata.rs:194) |
| 22 | Pooled-value fingerprint index `ValueIndex` | objects/metadata.rs:209-419 | find O(candidates); first sync O(groups) header audit (metadata.rs:252-281) | 32 MiB scratch file cap, 4 MiB cache (metadata.rs:221) | 1 scratch SQLite conn (opened on demand); chunked `IN` selects ≤512 params (metadata.rs:361-371); group reads per candidate | ENFORCED 131072 retained (whole-index DELETE on overflow, metadata.rs:305-307) |
| 23 | Publication `insert` **[CMP]** | objects/admission.rs:1439-1626 | O(n log n) locator sort (admission.rs:1598) + O(packs) | assembled packs ≤256 KiB each | per new pack 1 INSERT row; bulk multi-row pack INSERT ≤1 MiB BLOBs/statement (admission.rs:1525-1563); locator bulk INSERT ≤128 rows/5 params each (admission.rs:1600-1624); pool catalogue INSERT per group (admission.rs:1581-1591); 1 `SELECT MAX(pack_id)` (admission.rs:1447-1449) | ENFORCED (SQLite limits via sql_rows, admission.rs:1831-1846) |
| 24 | Open-pack append **[CMP]** | objects.rs:2361-2413; admission.rs:1474-1505 | O(groups² ) re-assemble per append (clone+extend, objects.rs:2390-2392) | candidate assembly ≤256 KiB | 1 `UPDATE object_packs SET data=?2` per append (objects.rs:2402-2408) | ENFORCED 256 KiB/256 groups/8191 records (objects.rs:2384) |
| 25 | Rollback/cleanup `AdmissionSession::rollback` **[CMP]** | objects.rs:2508-2568 | O(owned objects) in 512-row pages, sequential id order | ≤512 rows/page | per page: 1 SELECT + 1 txn of 512 single-row DELETEs + COMMIT (objects.rs:2530-2552); packs in 512-row DELETE pages (objects.rs:2553-2564); `clear_metadata_index` | ENFORCED 512/page; runs in Drop too (objects.rs:2579-2598) |
| 26 | Batched read `read_object_rows` | objects.rs:3865-3910 | O(n log n) map + duplicate cloning | output n·bytes | 1 locator query + wave reads (row 27); duplicates clone bytes (objects.rs:3896-3906) | ENFORCED page 128 (objects.rs:3872) |
| 27 | Grouped read wave `visit_locations_with_reserve` | objects/read.rs:1698-1732 | O(n log n) sort by (pack,group,record) then O(wave bytes) | reserve ≤1 MiB (VALIDATION_RESERVE, read.rs:11; per-object charge 512+41+9·len, read.rs:129-138) | wave ≤128 objs: per group blob_open + 16 B header + 16 B directory + payload range (read.rs:895-967) | ENFORCED 128/wave + 1 MiB reserve |
| 28 | Native group extraction `extract_record_group` | objects/read.rs:883-1066 | O(record bytes) | 1 group + 1 record | 2 blob ranges base + per-native: count(4 B)+ends(4·count)+record (read.rs:1032-1057) | ENFORCED budget charges per read (read.rs:892,929,957,993,1041,1053) |
| 29 | Two-pass delta wave `visit_wave` | objects/read.rs:1734-1946 | pass 1 groups, pass 2 bases: O(n) + O(pending) | wave + pending instructions | pass-2 base locator query + group reads (read.rs:1883-1941) | STRUCTURAL (bounded by wave reserve) |
| 30 | Hint reader `read_hint` | objects/read.rs:1233-1309 | O(group) per fetch | ≤512 KiB/target (read.rs:63-64) | ≤8 fetches/target (read.rs:1239) | ENFORCED |
| 31 | Small-chain reconstruction `small_chain` | objects/read.rs:680-803 | O(d) fetch + O(d·frame) decode | ≤16 KiB nodes + frames ≤256 KiB (read.rs:720-730) | locator+group per edge (read.rs:742-767) | ENFORCED CHAIN_EDGES=8, 512 KiB canonical, 256 KiB encoded (objects/delta.rs:6-9) |
| 32 | Metadata chain `metadata_chain` | objects/read.rs:250-389 | O(d) fetch + O(d·program) apply | nodes ≤16 edges | locator+group per edge (read.rs:301-333) | ENFORCED METADATA_EDGES=16, closure 128 KiB (read.rs:148-149) |
| 33 | Pooled reader `PoolRead` | objects/metadata.rs:465-547 | per row O(1) cache probe else 1 group fetch | ≤512 KiB/128 groups (metadata.rs:535) | 16 KiB work unit per miss; ≤32 MiB/chain (metadata.rs:522-524) | ENFORCED (all three) |
| 34 | Whole-file chain `whole_chain` | objects/whole.rs:338-407 | O(d) fetch, replay re-fetches each frame (whole.rs:390-391) | 51 ids/locs + 4 MiB owner cache (whole.rs:228-243) | 2× encoded bytes per chain (fetch + replay) | ENFORCED EDGES=50, CLOSURE 64 MiB (whole.rs:19-20) |
| 35 | Oversized read `singleton` | objects/read.rs:1603-1625 | O(bytes) | full object | 1 blob_open + 1 read per 64 KiB chunk (read.rs:1612-1621) | ENFORCED chunk 64 KiB |
| 36 | Seen-set `SpillableObjectSet` | objects/spill.rs:149-325 | O(log n) mem; spill: paged INSERT OR IGNORE … RETURNING | mem BTreeSet (48 B/id est.) else scratch SQLite conn | 1 scratch conn per spill; insert page per ≤128 ids (spill.rs:186-210) | ENFORCED 64 MiB index budget (objects.rs:50) |
| 37 | Candidate spool `SpillObjects` | objects/spill.rs:327-685 | put O(log n); `visit_ordered` O(V) seeks | 1 MiB buffer (objects.rs:58) + BTreeMap→disk index | seeks+reads per object in graph order (spill.rs:548-616) | ENFORCED index 64 MiB → SQLite disk index (spill.rs:336-396) |
| 38 | Reachability `reachable_from` | objects.rs:3013-3079 | O(V+E) DFS + O(children log children) sort per node | seen set + stack + order | no SQL (local store) | ENFORCED index_limit 64 MiB |
| 39 | Store open/one-connection | objects/schema.rs (schema.rs:292-360) | O(1) + schema verify | 32 MiB page cache (schema.rs:15) | 1 Connection per Store behind Mutex (schema.rs:74,447-457); TicketGate serializes ops (schema.rs:133-151) | ENFORCED (EXCLUSIVE lock, MEMORY journal, sync OFF, schema.rs:510-533) |

## 2. Comparison notes (per functional area)

Core statements are **ORIENTATION ONLY** — definitive inventory belongs to the parallel agent.

**Exact-reuse/membership.** Reference: membership is one locator query per ≤128-id page
(read.rs:448-460); exact reuse re-reads stored canonical bytes and compares them byte-for-byte
(admission.rs:1918-1923), amortized by a 2 MiB location-keyed compared-cache
(objects.rs:4194-4219) and a 4 MiB fresh-store bitmap filter (objects.rs:2444-2482).
Core orientation: `cas/membership.rs` exposes `reuse_or_collide` (grep hit line 30);
`LOOKUP_PAGE_IDS=128` matches the reference page (core policy.rs:63) but `READ_OBJECT_LIMIT=4096`
(core policy.rs:80) declares a much larger wave ceiling than the reference's 128 — a
deliberate divergence the core's own comment explains ("the reference's lookup page (128) is
the wrong figure to copy").

**Admission and batch cadence.** Reference is two-tier: an in-session wave of 512 objects /
256 KiB (objects.rs:4253-4256) that probes membership, then a pending batch of 512 objects /
512 KiB (objects.rs:4300-4304) published inside a cohort transaction that commits at 8191
rows / 4 MiB-1 (objects.rs:2316-2340). Workspace commits reuse the same accumulator and end
in `admit_remaining` → `flush` → `commit_pending` → `finish` (objects.rs:4547-4574); the
Workspace token is the admission owner (objects.rs:4452-4469), i.e. admission is
workspace-coupled at the API level. Core orientation: `policy.rs` declares
`BATCH_OBJECT_LIMIT=512`, `BATCH_CANONICAL_BYTES_LIMIT=512 KiB`, `TRANSACTION_ROW_LIMIT=8191`,
`TRANSACTION_CANONICAL_BYTES_LIMIT=4 MiB-1` (core policy.rs:65-71) — same numbers, but the
core has **no producer pipeline**: no worker pool, slab, or queue equivalent to rows 7/8
appears in the core module list; `cas/owner.rs` (962 lines) is a single save owner.

**Delta selection and reconstruction.** Reference: hint-driven trials — ≤4 prior ids per
object (objects.rs:167), ≤512 trials per prepared batch, 16 MiB shared match budget, 8 fetches
per target (admission.rs:1736, read.rs:1239); the match engine is a 4096-bucket/4-slot seed
table with byte-charged extension (pack.rs:860-911). Mixed full/delta groups are chosen only
when the mixed alternative saves ≥ max(64, len/8) (pack.rs:968-969). Reconstruction walks
chains with role-specific depth/closure bounds (rows 31-34). Core orientation:
`encoding/delta/select.rs` (390 lines) + `candidates.rs` (166) split selection from matching;
`METADATA_MATCH_BUDGET_BYTES=128 KiB` per pooled trial (core policy.rs grep line 13) replaces
the reference's shared 16 MiB batch pool — the budget is per-trial, not per-batch.

**Pack placement/assembly and BLOB writes.** Reference: lanes (small v3/v4, native v2,
ordinary v1, pooled-metadata v6) each keep at most one open pack per session; a lane's final
pack retains its group vector and is re-assembled exactly once on publish or appended into
the open row (admission.rs:57-149, objects.rs:2355-2442). Pack rows are written as bulk
multi-row `INSERT` statements built with `format!` per chunk, capped at 1 MiB of BLOBs per
statement (admission.rs:1546-1563); appends rewrite the whole pack BLOB via `UPDATE`
(objects.rs:2402-2408). Core orientation: `pack/placement.rs` documents "exact
append-or-new placement and the retained open-pack tail" (core placement.rs:1-8) — a direct
port of the open-pack mechanism; `sqlite/write.rs` has single-row `insert_pack` /
`insert_object` / `append_pack` (core write.rs:76-106) instead of bulk formatted SQL.
Core `GROUP_TARGET=48 KiB` unified (core policy.rs:53) vs the reference's per-role 32/16 KiB
targets (admission.rs:952).

**Pooled-value window/index.** Reference: 73-byte inode values pooled 165/group
(metadata.rs:9); dedup at admission via a publication-local pending map plus a **SQLite
scratch-database fingerprint index** (`ValueIndex`, metadata.rs:209-419, 32 MiB file cap,
131072-entry window with whole-index eviction). Reads go through a wave-scoped decoded-value
cache (≤512 KiB/128 groups) with per-chain work allowances (metadata.rs:465-547).
Core orientation: `encoding/pool/index.rs` is an **in-memory** `PoolIndex` with
`live_bytes` reporting (core policy.rs grep lines 16-22); `METADATA_INDEX_VALUES=131072`
matches the window size but the storage is process memory, not a scratch DB; depths are
persisted policy (`metadata_delta_max_depth`, default 8, core policy.rs grep line 5) vs the
reference's compile-time `METADATA_EDGES=16` (read.rs:148).

**Read paths.** Reference: every read is locator SQL → blob_open → 2-3 range reads per group
(rows 26-29), waves sorted by physical location so a group is decompressed once
(read.rs:1711-1712), delta targets deferred to a second base pass (read.rs:1883-1941).
Core orientation: `sqlite/lookup.rs` (172 lines) + `encoding/decode.rs`; the core groups
one wave into "one grouped SQL lookup plus one shared decode workspace" (core policy.rs:75-79),
i.e. same wave architecture with a larger declared ceiling (4096).

**Transactions.** Reference: one shared connection; cohort transactions opened with
`BEGIN IMMEDIATE`, committed at row/byte bounds or kept open across batches when coalescing
(objects.rs:2309-2341, 2247); publication (commit-record + branch advance) runs inside the
final admission transaction via a `publish` callback (workspace.rs:326-396); failure paths
call `resolve` → `rollback` (objects.rs:2493-2506). Core orientation: `sqlite/write.rs`
attempts `BEGIN IMMEDIATE` exactly once — "a lost write lock is an immediate failure, never a
wait" (core write.rs:4-5) — vs the reference's 5 s busy timeout (schema.rs:531).

**Cleanup.** Reference: rollback deletes owned objects above `baseline_pack` in 512-row
pages in **ascending id order**, then pack rows in 512-row pages, then clears the metadata
index (objects.rs:2529-2565); it also runs from `Drop` with write quarantine on failure
(objects.rs:2579-2598). Core orientation: `sqlite/cleanup.rs` walks locators **newest first**
("every direct dependent is gone before its base", core cleanup.rs:5-7) in 128-row pages
(`CLEANUP_PAGE_ROWS=128`, core policy.rs:73), one attempt, no destructor fallback
(core cleanup.rs:8-10).

**Connection handling.** Reference: exactly one rusqlite Connection per Store behind a
Mutex, `reader()`/`writer()` are the same lock (schema.rs:447-457); EXCLUSIVE locking mode,
MEMORY journal, synchronous OFF, 32 MiB cache, 5 s busy timeout (schema.rs:510-531);
operations serialized by a TicketGate (schema.rs:133-151); private scratch SQLite DBs are
opened for seen-sets, spill indexes and the value index (spill.rs:687-717). Core orientation:
`sqlite/connection.rs` configures MEMORY journal / synchronous OFF / foreign_keys ON and
**verifies** rather than rewrites pragmas on reopen (core connection.rs:51-53); busy timeout
is 0 (core connection.rs:65). Whether the core shares one connection per store: UNKNOWN from
my skim.

## 3. Opportunity register (reference-side costs)

**IRREDUCIBLE** (inherent to content-addressed integrity):

- O(b) canonical byte comparison per known id in `compare` (admission.rs:1918-1923): an id
  match alone never authorizes reuse; bytes must be re-read and compared.
- O(V+E) reachability with per-node child sort O(c log c) (objects.rs:3044-3053): the
  dependency graph must be walked; sorting makes dedup/determinism cheap.
- Two encode alternatives per group (full + mixed) in `encode_group` (pack.rs:963-967):
  choosing membership requires measuring both.
- Two-pass delta wave (read.rs:1734-1946): delta targets cannot emit before their bases, so
  the base pass is structurally second.

**REDUNDANT** (bounded work the design accepts; candidates the core inherited or avoided):

- Preexisting-object re-reads across batches: when the 2 MiB compared-cache clears on
  saturation (objects.rs:4212-4216), every known preexisting id is re-read from storage and
  re-compared in the next wave; the code itself marks this "ponytail: clear on saturation;
  use incremental eviction only if measured thrashing warrants it" (objects.rs:4213).
- Open-pack append rebuilds the full candidate pack (clone + extend + re-assemble, ≤256 KiB)
  per append (objects.rs:2390-2398) — O(existing groups) work per appended group; amortized
  only by pack size.
- `singleton` re-opens the blob and re-acquires the connection lock per 64 KiB chunk
  (read.rs:1612-1621) — O(size/64 KiB) open/lock round trips for one object.
- `prepare_singleton` writes the object to a temp file and reads it back (admission.rs:1212-1224)
  purely to avoid a second resident multi-MiB copy — deliberate memory/IO trade.
- `whole_chain` replay re-reads every frame a second time (fetch pass whole.rs:374-378,
  replay pass whole.rs:390-391) — 2× encoded bytes per chain to keep only ids resident.
- Dynamic `IN`-list SQL strings are rebuilt per chunk in `object_locations` (read.rs:455-459)
  and in pack/locator bulk INSERTs (admission.rs:1546-1549,1602-1604) even though a fixed
  128-parameter `GET_MANY_128` statement exists and is prepared at open
  (statements.rs:20,83; schema.rs:571-590) but has no call site in this crate's src (grep).
- `ValueIndex` first sync replays the whole-group eviction recurrence O(groups) before
  reading any payload (metadata.rs:252-281, flagged "ponytail" at metadata.rs:248-249).
- Duplicate ids in `read_object_rows` clone the same bytes per duplicate occurrence
  (objects.rs:3896-3906; instrumented by `note_read_batch_clone`, objects.rs:97-106).

**O(n^k) shapes**: the delta match engine is worst-case O(target_len · 4 · match_len) per
trial (pack.rs:884-911) — quadratic-ish in object size, ENFORCED-bounded by the 16 MiB batch
budget (admission.rs:1689) rather than by an asymptotic guard; class UNKNOWN-as-experienced,
ENFORCED-as-budget.

## 4. Round trips (repeated reads/decodes/SQL trips per operation, reference)

- **Admit one 512-object wave** (worst case all known preexisting): 1 locator query (4
  chunks of 128, read.rs:448-460) → compare re-reads every known object's group: per group
  blob_open + 16 B header + 16 B directory + payload (read.rs:895-967) → byte compare per id
  (admission.rs:1918-1923) → on flush: `SELECT MAX(pack_id)` (admission.rs:1447-1449), pack
  INSERT(s) ≤1 MiB BLOBs each (admission.rs:1539-1543), locator bulk INSERTs (admission.rs:1601),
  BEGIN/COMMIT (objects.rs:2334,2208). Known ids re-read again next wave unless the
  compared-cache retains them (objects.rs:4130-4148).
- **Dependency closure per batch**: `close_dependencies` decodes `referenced_objects` for
  every batched object then probes existence per 128-id page (objects.rs:4071-4087).
- **Read one object**: 1 locator SQL + blob_open + 2-3 blob range reads per group; native
  record adds count+ends+record reads (read.rs:1032-1057); a DELTA adds the base's locator +
  group read (read.rs:1888-1900); a pooled leaf adds per-row group fetches on cache miss
  (metadata.rs:517-532); a whole-file slice adds owner chain fetch + replay re-fetch.
- **Spilled candidate**: every object is written to the spool once (spill.rs:450-502) and
  read back in graph order with a seek per object (spill.rs:564-615), then possibly
  re-authenticated on delivery (objects.rs:2932-2941).
- **Connection opens per operation**: 0 for ordinary reads/writes (shared Mutex connection,
  schema.rs:447-457); +1 scratch SQLite connection per spill event (seen-set spill.rs:265-290,
  disk index spill.rs:719-738, value index metadata.rs:219-228); temp files via
  `temporary_file` per spool/singleton (spill.rs:812-841).
- **Per statement, always**: `fail_transaction_statement` counter check
  (admission.rs:1554,1616; debug-only effect, schema.rs:55-65).

## 5. Honesty notes

- **Working tree vs pinned oracle**: the module in scope is byte-identical to v0.1.6 (§0);
  however the repo HEAD is 120 commits past the tag, so cross-crate behavior (e.g.
  `layerfs-content` APIs called at objects.rs:19-22) could differ from the v0.1.6 oracle even
  though this crate's bytes do not. I did not diff `layerfs-content` against the tag.
- **Not read** (claims about them are UNKNOWN): `layerstack.rs` beyond the cited span
  (transactions/cleanup outside the test region at layerstack.rs:3550 are cited from tests,
  not from production code — the production cleanup is `AdmissionSession::rollback`,
  objects.rs:2508-2568, which I did read); `query.rs`, `records.rs`, `telemetry.rs`,
  `staging.rs`, `store.rs`, `lib.rs`, and all test modules. SQL text under
  `crates/layerfs-layerstack-store/sql/` was only grepped, not read.
- **Code-vs-doc tension**: `GET_MANY_128` is prepared at open (schema.rs:571-590 via
  statements.rs:19-20) but no production call site exists in this crate (grep over src/);
  `object_locations` instead formats its own per-chunk SQL (read.rs:455-459).
- **Bound-class caveats**: "ENFORCED" means a runtime check exists; I did not verify that
  every path reaches the check (e.g. whether `visit_locations_with_reserve`'s reserve can be
  bypassed by a single >1 MiB object — the code returns an error in that case at
  read.rs:1725-1727, so it fails closed, but I did not enumerate callers).
- **No measurements**: all counts above (chunks, budgets, sizes) are source constants or
  loop-shape arithmetic, not observations. No performance claim is made or implied.
- **Core statements**: every core claim above comes from partial reads/greps of
  core/crates/layerfs-storage and is marked ORIENTATION ONLY; the parallel agent's inventory
  is authoritative for that side.
