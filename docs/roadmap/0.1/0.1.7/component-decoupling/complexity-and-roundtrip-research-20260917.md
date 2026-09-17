# Core complexity and round-trip research: core/ vs the v0.1.6 reference (2026-09-17)

> **Status:** research record for
> [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176) (sub-issue of
> the closed Stage 5, #170), feeding Stage 6 (#171) decisions. Static source
> reading only: **no builds, no tests, no measurements, no product changes, and
> no performance claims.** Where a measured number appears, it cites an existing
> receipt. This study is **complementary** to
> [`parallelism-and-batching-study-20260918.md`](parallelism-and-batching-study-20260918.md):
> that study owns the SQLite-configuration and parallelism dimensions (its F1-F9,
> O1-O5); this one owns the algorithmic complexity inventory, the C1-side
> batching and trip analysis, and the cross-tree round-trip register.

| | |
| --- | --- |
| Reports | A (C1 file-content), B (C1 filesystem engine), C (attributes + ordering), D (C2 storage), E (reference C2), F (reference C1 + workspace), G (round trips) — all under [`../evidence/stage5-complexity-research-20260917T211600Z/`](../evidence/stage5-complexity-research-20260917T211600Z/) |
| Trees read | core at `5e45897dd` (product crates byte-identical through `2d93edf0e`, docs-only commits in between); reference `crates/layerfs-layerstack-store` byte-identical to the v0.1.6 tag; the wider reference tree read as found (frozen during migration) |
| Adjudication | every finding below was re-read at its `path:line` by the main agent before entering this document; the six highest-impact claims were independently confirmed in source |

## 1. Complexity comparison, core vs reference, per functional area

| Area | Replacement core | v0.1.6 reference | Verdict |
| --- | --- | --- | --- |
| CDC / chunking | per-byte scan + hash, per-chunk encode (`report-A`) | same shape (`report-F`) | shared, irreducible |
| Mapping construction | whole-file 2n encode peak (documented); chunked route one base read per edit (verified by the round-4 two-tree receipt) | encode→store→ID-rewrite→reread cycle for inode values — cut in the core (`report-F` verifies from the reference side) | **core better** |
| Whole-file edit | ~3n peak (out + value + canonical, derivable from `apply.rs:143-150` + `content.rs:98-102`) | — | reducible to ~n (T0-17 below) |
| Chunked edit | compare-then-construct double traversal; per-Retain-segment re-traversal; one discarded sibling load per edit (`report-A`, confirmed at `tree.rs:711`) | fallback re-decodes per inode, one tree apply per inode (`changes.rs:3050-3064`) | both have trips; core's are smaller and all removable (T0) |
| Sorted directory/inode engine | changed-path merge reads every child page per level (branch rows carry no summaries, `merge.rs:251-268`); waves capped at `BATCH_CHILDREN=32` (`page.rs:28`, confirmed) against a 4,096 demand ceiling (`objects.rs:20`) | reference pipeline walks the whole namespace per reconcile (`changes.rs:475-563`), re-decodes the namespace root per call (`resolve.rs:25-27` ×4 call sites) | **core structurally better**; core's remaining per-level reads are format-irreducible but wave-batched badly (T0-1) |
| Whole-tree validation | O(total bindings) per walk, 4,096-entry ceiling per walk (`limits.rs:59-78`, confirmed) | O(total namespace) with from-root resolve per entry | core bounded and better; O(log n) membership needs a format extension (T2-20) |
| Ordering (references) | tiered merge; **write term ≈ the Θ(r·log₂(r/P)) floor (×2.72/2.52/2.36 per doubling), read residual ≈ n^1.9 (×3.86/3.84/3.67)** decomposed from the existing grid receipt by `report-C` | frontier spill triple I/O (encode → in-place ID rewrite → sequential re-read), tiered merge cascade (`changes.rs:2961-3023`) | core better; its read residual is a removable defect, not the merge (T0-4) |
| Attributes | one extent-only root per value; patches rebuild the tree per patch (`patch.rs:83-126`) | — | patch route batchable |
| Admission / persistence | single save owner, 512/512 KiB batch flush trigger, 8,191-row/4 MiB−1 commit trigger (enforced, `report-D`) | 4-worker producer pool, same batch/transaction constants, bulk multi-row INSERTs, whole-BLOB pack UPDATE on append (`report-E`) | constants shared; core lacks the pool (owner's O3, gated by the single-worker rule) and multi-row INSERT (owner's O5); both share the whole-BLOB append (T2-19) |
| Read path | fresh connection + 5-pragma profile + 1 MiB workspace **per wave** (`store.rs:209`, confirmed), pack materialized whole per read, ordinary resolver decodes a group body once per record (`decode.rs:48-50`) | opens a read-only blob per record (worse); 32 MiB page cache (core leaves SQLite's 2 MiB default — the owner's F2, the strongest config gap) | mixed: core better on pack reuse (`F9`), worse on connection-per-wave (T0-1) and group-decode caching (T0-9) |
| Cleanup / connection | newest-first 128-row pages, one attempt, zero busy timeout | ascending 512-row pages from `Drop`, quarantine, 5 s busy timeout, EXCLUSIVE lock | core simpler and single-attempt by contract; `locking_mode`/`cache_size` are the owner's O1/O2 |

## 2. Optimization opportunity register

Tiered by risk. **Every entry is a hypothesis until Stage 6 measures it** — the
owner's study §5 states the precondition: no receipt yet compares the trees on a
matched workload. Parity safety below means "does not change emitted
roots/bytes"; the sealed-oracle parity set (`edit_reference`, `fixture_seal`,
`object_identity`, `filesystem_reference` and the sealed fixtures) is the guard.

### Tier 0 — zero canonical risk, pure I/O / batching / accounting fixes

| # | Cost today | Mechanism | Notes |
| --- | --- | --- | --- |
| 1 | Filesystem-tree navigation issues **one point provider call per page** (`directory/read.rs:208`, `inode/read.rs:51`, `patch.rs:199`, `validate.rs:82`), each a fresh SQLite connection + pragma profile + 1 MiB workspace under `StoreProvider` (`store.rs:209-216`) | batch page reads toward the 4,096-id demand ceiling (`BATCH_CHILDREN` 32 → wider, `page.rs:28` vs `objects.rs:20`) **and** pool the read connection/session per operation (keep the per-wave ceiling capture, `cas/read.rs:49-67`) | the single largest compounding trip (`report-G` RT-01 + RT-05, `report-B` finding 1). The round-2 review's "no per-node RPC left in C1" holds only for mapping navigation — the seam map is over-broad (§4) |
| 2 | `page_from_wire` point-reads one child per wave (`page.rs:470`); a neighbour merge can pay ~470 single-page waves | batch it like the edit path already is | `report-B` finding 2 |
| 3 | validate does one single-demand inode lookup per binding / listing entry (each a fresh root descent) and re-reads parent records 2-3× across phases; no page memo across the three walks (same directory pages up to 3×) | batch the lookups; memo decoded pages per operation | `report-B` finding 4 |
| 4 | **Ordering lookup residual ≈ n^1.9**: `spill()` resets **every** tier's scan (`runs.rs:231,299` → `scans.clear()`), though a spill into level k replaces only tiers ≤ k (`runs.rs:283-289`, confirmed); each passed cursor then restarts its tier from offset 0 | reset only `scans[0..=level]` | `report-C` findings 1-2; removes the dominant superlinear term; parity-safe (the spilled/unspilled root-identity test) |
| 5 | one discarded validation `load_node` per chunked edit (`tree.rs:711`, confirmed — result dropped, re-loaded at `:713`; mirror at `:764/765`) | delete it | `report-A` finding 1 |
| 6 | `compare_replacements` re-demands every page the split descent re-demands, plus one root-down traversal per 64 KiB window (`apply.rs:64-77`, `compare.rs:52-63`) | fuse comparison into the split descent (comparing-sink) | preserve the Equal verdict or `edit_noop` breaks |
| 7 | `assemble_final` traverses root-down once per Retain segment (`apply.rs:154-157`): R×h reads | one ordered cursor (the `PlanReader` shape, `apply.rs:377-465`; segments already in base order) | `report-A` finding 4 |
| 8 | `rightmost_payload` O(h) stored loads run even for pure deletions (before the `replacement_len == 0` check, `apply.rs:257-271`) | gate the walk | predecessor is advisory, outside hashed bytes |
| 9 | ordinary resolver decompresses the same zstd group body once per record (`decode.rs:48-50` via `delta/read.rs:223`); `PoolReader` already caches decoded groups (`pool/read.rs:131`) | port the group cache to the ordinary path | `report-D` finding 2 |
| 10 | per-object dependency-presence queries (`dependencies.rs:58`); a fresh `PoolReader` per pooled base trial re-materializing the same packs (`owner.rs:748`) | one wave-level batched query; one reader per save | `report-D` finding 4 |
| 11 | the requested object is hashed twice per wave (`delta/read.rs:176` + `cas/read.rs:91`); membership re-hashes what the resolver just authenticated (`membership.rs:20`) | hash once; for membership, note the cut trades the "membership never proves equality" invariant — **policy decision, see T2-21** | `report-G` RT-08/RT-04 |
| 12 | `consolidate()` copies the newest run via `copy_run` (`runs.rs:418-424`) though `merge_runs` never aliases inputs (`merge.rs:211-214`) | drop the copy **after** verifying the aliasing comment is not load-bearing (flagged UNKNOWN by `report-C`/`report-G`) | byte-identical output |
| 13 | `zero_count_serials` re-finds every serial `touched_serials` already visited (`update.rs:482`) | carry the state from the first pass | `report-C` |
| 14 | `SortedWork.pages_read` is documented "including batched ones" but batched decodes never increment it (`page.rs:40-41` vs `:185-186`) | fix the counter (a correctness fix to telemetry, not an optimization) | `report-B` finding 5 |
| 15 | `push()` recomputes the full page width sum per append (O(k²), k ≤ 740, `merge.rs:71-76`); `append_fits` re-sums all group bodies per placement (O(g²), g ≤ 256, `layout.rs:216-234`) | running totals | in-memory only, bounded — low priority |

### Tier 1 — parity-safe structure/memory tradeoffs

| # | Cost today | Mechanism | Tradeoff |
| --- | --- | --- | --- |
| 16 | ordering merge cascade: each spilled batch participates in up to log₂(batches) merges (`merge.rs:6-9`) | size-tiered / fanout-4 merge policy (log₂ → log₄ participations) | more live tiers (≤ 32 cap holds); parity-safe |
| 17 | whole-file edit peaks at ~3n (`apply.rs:143-150` + `content.rs:98-102`) | assemble directly into one pre-sized canonical buffer (~n) | none — bytes identical; widths are known constants |
| 18 | ordering lookups that restart pay O(position) per tier | hybrid: keep the sequential resume for ascending sweeps, binary-search on restart (rows sorted, fixed 96 B, `read_at` at any offset) | random vs sequential reads; **pure** binary search would worsen the pinned one-pass ascending sweep |
| 19 | spills begin at `maximum_pending_records` = 4,096 (`reduce.rs:25`); the 64 MiB ordering ceiling could own ~349k pending rows under the ×2 charge (`runs.rs:136,39`) | raise the ceiling per workload (already configurable in `FilesystemResources`) | memory vs merge work: ≤ ~349k touched serials could run spill-free O(r); the round-4 grid **forced** 64, so its shape would not spill at defaults |

### Tier 2 — format / policy changes (owner decision required)

| # | Cost today | Barrier | Variant notes |
| --- | --- | --- | --- |
| 20 | whole-tree validation is O(total bindings) per walk; no ancestor summary exists in the page format | per-child summaries in branch rows **change canonical partitions** — parity break | the only route to O(log n) cycle/reachability checks; needs an owner format decision and a new oracle seal |
| 21 | pack append rewrites the whole ≤256 KiB BLOB per sealed group (`write.rs:88-97`, confirmed; O(k²·B) bytes for k appends) | chunked BLOB rows alter the `sqlite_master` text pinned by `schema.rs:22-58` | SQLite incremental blob I/O with `zeroblob` pre-sizing keeps the schema text identical but needs the pack's final size at insert — write-path redesign; also blocked by `seal_pending` needing rows+bytes at seal time (`owner.rs:278-310`) |
| 22 | membership re-hash per reuse offer | cutting it trades the documented "membership never proves equality" invariant (`membership.rs:1-6`) | policy, not refactor |
| 23 | cold-start pool sync streams the entire value-group catalogue (`index.rs:224-248`) | persisting the cursor is a schema change | reference used a 32 MiB scratch DB instead (`metadata.rs:209-419`) |
| 24 | reference-only, for the record: the producer pool (owner's O3, gated by the repository's single-construction-worker rule), multi-row INSERT (owner's O5), `cache_size`/`cache_spill` (owner's O1), `locking_mode` (owner's O2) | owned by the parallelism study | do not re-open here |

## 3. Round-trip register

`report-G` carries the full 16-row register (RT-01..RT-12 core, RT-R1..RT-R4
reference), each with both sides of the trip, its cost class and an elimination
proposal. The top five by structural impact, all adjudicated:

1. **RT-01** per-page point reads across the C1←provider seam, compounded by
   **RT-05**'s connection-per-wave (entry 1 above).
2. **RT-02** the chunked edit's double `ExtentLeaf` demand (entries 5-6).
3. **RT-03** the whole-BLOB pack append (entry 21).
4. **RT-04/RT-08** double hashing on reuse and per-wave (entry 11).
5. **RT-09** `consolidate`'s avoidable copy (entry 12).

Reference-side trips the core already avoided or reduced (verified from the
reference by `report-F`/`report-G`): the inode encode→store→ID-rewrite→reread
cycle; the frontier spill triple I/O; uncached namespace-root decodes at four
call sites; the O(total-namespace) reconcile walk; whole-file digests on both
sides for equality checks; spool write+read-back per file and the twice-visited
checkpoint journal. One suspected reference trip was **not confirmed** and is
recorded as UNKNOWN: "per-file flushes" (construction flushes per slab; the FUSE
flush is a no-op gate).

## 4. Corrections this research makes to closed records

Dated pointers, not rewrites:

1. **`stage-5-report.md` §15 attributes the ordering superlinearity to "the
   tiered merge itself".** The decomposition of the same receipt shows the write
   term indeed tracks the merge floor (×2.72/2.52/2.36 per doubling ≈
   Θ(r·log₂(r/P))), but the **larger read residual (×3.86/3.84/3.67 ≈ n^1.9) is
   the lookup path** — scan resets on spill — and is removable (entry 4). The
   receipt's numbers are unchanged; the attribution was incomplete. A dated
   note is appended to §15.
2. **The round-2 review's seam map** ("no per-node RPC left in C1", §8.4) is
   over-broad: it holds for file-content mapping navigation only. Filesystem-tree
   navigation (directories, inodes, symlink resolution, attribute patches,
   validation) still issues one provider call per page. The review is the
   reviewer's dated record and stays as written; this document is the correction
   of record.
3. **`SortedWork.pages_read` undercounts** batched merges (entry 14) — a counter
   bug to fix whenever the counters are next touched.
4. **A prior audit's "≤1 MiB pack INSERT" wording** does not match the code's
   ~16 MiB singleton-pack binds (`report-D`, citing a round-2 delegated-audit
   receipt) — recorded for the next documentation pass.

## 5. What must not change

The sealed-oracle parity set (canonical roots, page partitions, profile
identity); the `sqlite_master` text pin; one-attempt semantics and zero busy
timeout; the single-construction-worker rule (AGENTS.md §8 — parallelism is the
owner's study's domain, gated); declared memory ceilings (the pending/run/scan
ownership accounting); append-only evidence. Every Tier 0/1 item above is
implementable without touching any of these; every Tier 2 item requires an owner
decision and a Stage 6 measured contract **before** implementation, per the
parallelism study's §5 precondition.

## 6. Method, provenance and limitations

Seven read-only agents, one report each (A-G), every claim `path:line`-cited;
the main agent re-read the six highest-impact findings in source before entering
them here. No builds, tests or measurements were run by this research; the only
measured figures are the existing ordering-grid receipts, re-decomposed
arithmetically. Limitations inherited from the reports: SQLite query plans
unknown (no EXPLAIN); rusqlite/zstd runtime behaviors (statement caching,
incremental blob I/O availability) unverified; the reference's
`layerfs-layerstack-store` is byte-identical to the v0.1.6 tag, but the wider
reference tree was read as found; several reference modules were grep-surveyed
only (per-report honesty notes); the restart-vs-dedup split of the ordering read
residual is bounded, not separated (no counter distinguishes them today —
adding one is itself a Tier 0 candidate).
