# Issue 219 — Squad D: chunk-size distribution parity and scan/decode cost

Worktree: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`
Branch: `codex/219-ns10000` · HEAD: `9c46930b846600e5f3c6ca4a4c4cbcf44ecdc356`
UTC: `20260921T044229Z` · Read-only campaign: no build and no benchmark was run.
Every value below is either sourced to `file:line` / a receipt field, or marked
**NOT_MEASURED** with the reason. Nothing here is estimated from a run.

---

## Part 1 — Is the chunk-size distribution equal between the two arms?

### 1.1 What the v0.1.6 fixture actually declares

The generator is `benchmark/fs-bench-pro/workload/main.rs`.

| Fact | Value | Source |
|---|---|---|
| profile constant | `synthetic-small-heavy-v2` | `benchmark/fs-bench-pro/families/init_namespace/mod.rs:7`; asserted at `workload/main.rs:1862` |
| namespace-10000 plan oracle | `[100, 7_899, 1_500, 500, 1]`, `300_000_000` | `workload/main.rs:1819` |
| class order | empty, tiny, small, medium, anchor | `workload/main.rs:194-199`, `1828-1834` |
| class percentages | `[1, 79, 15, 5]` + 1 anchor | `workload/main.rs:200` |
| **band ranges** | Tiny `(1, 8)`; Small `(32, 256)`; Medium `(1_024, 8_192)` | `workload/main.rs:424-430` |

**Correction to the brief.** The ranges `1-8`, `32-256`, `1_024-8_192` are the
**relative weights** returned by `namespace_relative_weight`
(`workload/main.rs:424-449`), not byte sizes. The byte size is computed afterwards
by a largest-remainder (Hamilton) pass at `workload/main.rs:332-386`:

```
distributable = logical_bytes - anchor_bytes - positive
              = 300_000_000 - 100_000_000 - 9_899            = 199_990_101   (main.rs:340-344)
size(file)    = 1 + floor(distributable * weight / weight_sum)                (main.rs:363)
                then +1 for the `extra` largest remainders                   (main.rs:367-386)
```

So "7,899 tiny files of 1-8 bytes" is **not** what the v0.1.6 arm constructs. The
bands are weights; the plan is then scaled up to the declared 300 MB total. This is
stated independently in the v0.1.7 module doc at
`core/benchmark/fs-bench-pro-storage-content/src/ops/namespace_content.rs:25-28`.

*Documentation defect found:* that docstring cites a weight sum of `102,555,546`
(`namespace_content.rs:26`). The formula at `namespace_content.rs:113-129` actually
yields **2,555,546**. See `raw/03`. The v0.1.6 side is unaffected — its own
arithmetic does not use that number.

### 1.2 What the v0.1.7 arm actually produces

`core/benchmark/fs-bench-pro-storage-content/src/ops/namespace_content.rs`:

- declared band counts `TINY=(7_899,1,8)`, `SMALL=(1_500,32,256)`,
  `MEDIUM=(500,1_024,8_192)`, `EMPTY_FILES=100`, `ANCHOR_FILES=1`,
  `ANCHOR_BYTES=100_000_000` — lines 38-49.
- the **same** `relative_weight` formula — lines 113-129.
- the **same** Hamilton pass, `size = 1 + floor(distributable * weight / weight_sum)`
  then largest remainders — lines 205-258.
- `distributable = total_bytes - anchor_bytes - positive` — lines 222-225.
- permutation by `fixture::noise` instead of v0.1.6's SHA-256 sort key — lines 177-191.

`weight_sum` is a function of the **weight multiset only**
(`namespace_content.rs:208-213` vs `workload/main.rs:345-348`), and both arms build
the identical multiset because both call the identical weight formula with the
identical `(lower, upper, count)` triples. Empty and anchor files contribute weight
`0` on both sides (`workload/main.rs:426`; `namespace_content.rs:211`). Therefore:

> **`weight_sum = 2_555_546` and `distributable = 199_990_101` in both arms, and the
> resulting multiset of file sizes is identical.** Only the permutation of sizes onto
> paths differs (declared at `namespace_content.rs:16-23`).

Exact ladder (computed from the two source formulas, `raw/02` + `raw/03`):

| Band | Count | weight range | **byte-size range** | band byte total |
|---|---|---|---|---|
| Empty | 100 | 0 | 0 | 0 |
| Tiny | 7,899 | 1 … 8 | **79 … 627** | 2,789,651 |
| Small | 1,500 | 32 … 256 | **2,505 … 20,035** | 16,905,060 |
| Medium | 500 | 1,031 … 8,185 | **80,684 … 640,537** | 180,305,289 |
| Anchor | 1 | — | 100,000,000 | 100,000,000 |
| **Total** | **10,000** | | | **300,000,000** |

Largest-remainder top-ups: `extra = 4,214` files receive `+1`
(`workload/main.rs:367-386`; `namespace_content.rs:247-258`).

### 1.3 The two arms construct the SAME file-size population

**Yes.** Same counts, same weights, same `weight_sum`, same `distributable`, same
floor arithmetic, same `extra`. The size multiset is therefore identical; only the
path→size assignment differs, which is a declared difference
(`namespace_content.rs:16-23`). Both arms carry the identical
`100,000,000`-byte anchor and the identical 100 empty files.

### 1.4 Deriving 9,444 whole-file and 14,466 chunk objects

The cutoff is frozen at **131,072 bytes**, and it is an *exclusive* cutoff — files at
or above it are chunked:

- `core/crates/layerfs-content/src/policy.rs:13-14`
  `/// Default exclusive construction cutoff: files at or above this length are chunked.`
  `pub const DEFAULT_SMALL_FILE_THRESHOLD_BYTES: u64 = 131_072;`
- selected by `ConstructionPolicy::frozen_default()` — `policy.rs:51-57`.

**Whole-file arithmetic (exact):**

```
positive files                       = 10,000 - 100 empty            = 9,900
files with size >= 131,072 (non-anchor)                              =   455
  + the anchor (100,000,000 >= 131,072)                              =     1
chunked files                                                        =   456
whole-file objects = 9,900 - 456                                     = 9,444   <-- receipt: 9444
```

No file lands exactly on 131,072, so the exclusive/inclusive boundary is not
ambiguously exercised (`raw/03`).

**Chunk arithmetic (exact, two independent closures):**

```
chunked source bytes = 100,000,000 (anchor) + 175,564,964 (455 files) = 275,564,964
receipt chunk objects                                                =      14,466
implied mean chunk payload = 275,564,964 / 14,466                    =  19,049.15 B
```

That mean sits between `TARGET_CHUNK_BYTES = 16_384` and
`MAXIMUM_CHUNK_BYTES = 32_768` of the frozen FastCDC profile
(`core/crates/layerfs-content/src/file/cdc/gear.rs:13-17`) and above the target, which
is exactly the normalized-chunking signature. The frozen masks
(`gear.rs:23-24`) have popcounts 16 (small) and 12 (large), giving an analytic
expected chunk length of **19,441 B** for incompressible input — 2.0 % from the
implied 19,049 B. A per-file renewal model over the 456 chunked files predicts
14,418 chunks against the receipt's 14,466 (0.33 %). `raw/02`/`raw/04`.

**Byte-exact closure against the receipt.** `resources.space.canonical_bytes`:

```
whole-file:  24,435,036 raw + 9,444  x 23 B = 24,652,248   receipt: 24652248  MATCH
chunk:      275,564,964 raw + 14,466 x 21 B = 275,868,750  receipt: 275868750 MATCH
```

The 23-byte and 21-byte per-object overheads are the product's own constants:
`WHOLE_FILE_CANONICAL_OVERHEAD = 23`
(`core/crates/layerfs-storage/src/policy.rs:127`, and the comment at 124-127 states the
chunk lane's equivalent is 21). The receipt independently confirms
`file-state = 456` — the chunked-file count derived twice above.

### 1.5 Implied chunk-size distribution

From the frozen profile and the derived partition:

- 456 files chunked; 9,444 stored whole.
- Chunk payload lengths are bounded to `[8_192, 32_768]`
  (`gear.rs:13,17`), content-defined by a two-byte rolling gear with
  `NORMALIZATION_SHIFT = 2` and seed `0` (`gear.rs:19-26`).
- Below `TARGET_CHUNK_BYTES` the 16-bit mask applies (mean run ≈ 65.5 KiB), at and
  above it the 12-bit mask applies (mean run ≈ 4.1 KiB); both bounded by min/max.
- Implied mean **19,049 B**; the anchor alone contributes 1,526 chunks at
  `ceil(100,000,000 / 65,536)`-scale bounded segmentation, i.e. the anchor dominates
  the chunk population.
- Per-object chunk canonical overhead is 21 B (`policy.rs:124-127`), confirmed to the
  byte by the receipt.

**UNDERIVED / NOT_MEASURED:** the exact per-chunk length histogram. The receipt
publishes chunk *count* and *canonical bytes* only; no per-chunk length is recorded
anywhere in `ns17-final-20260921T031259Z`, and this campaign ran no instrumentation.
Not obtainable without either a benchmark run or a product change — both out of scope.

### 1.6 VERDICT

> ## **Chunking is cleared as the differentiator.**

Evidence:

1. Both arms construct the **same** file-size population. The v0.1.6 bands are
   weights, not sizes; both arms run the identical weight formula
   (`workload/main.rs:424-449` ≡ `namespace_content.rs:113-129`) and the identical
   Hamilton pass (`workload/main.rs:332-386` ≡ `namespace_content.rs:205-258`).
   Identical `weight_sum = 2_555_546`, identical `distributable = 199_990_101`,
   identical multiset. Only the permutation differs, and it is declared
   (`namespace_content.rs:16-23`).
2. The v0.1.7 object population is fully explained by that shared ladder plus the
   frozen 131,072-byte cutoff: 9,444 whole-file and 456 chunked files, with
   `file-state = 456` as an independent cross-check.
3. The chunk *count* closes to 0.33 % against an independent renewal model built from
   the frozen masks, and both role canonical-byte totals close **to the byte**
   (23 B / 21 B per-object overheads).
4. Total canonical objects differ by only **87** between the arms:
   v0.1.6 `store_canonical_objects = 25158`
   (`benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl`,
   record 0) vs v0.1.7 `canonical_objects_total = 25245`
   (`core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-final-20260921T031259Z/pipeline-namespace-10000/receipt.json`).
   9,444 + 14,466 = 23,910 of the v0.1.7 total; the remainder is metadata roles. A
   chunking difference of the size needed to explain a v0.1.6→v0.1.7 performance gap
   would have to show up as a materially different object population. It does not.
   **0.35 % of the object count is not a differentiator.**

**Caveat that must travel with this verdict.** The verdict is about *workload shape*,
not about *timed work*. The two arms do not time the same thing (§2.1). The v0.1.6
timed phase pays for the scan, the construction and the save; the v0.1.7 timed phase
pays for the save only. That is a **timer-boundary** difference, not a chunking
difference, and it remains the live explanation.

---

## Part 2 — Scan and decode cost

### 2.1 What each timer includes

**v0.1.6 — `layerstack_init_ns`.** Bounded by `init_started = Instant::now()`
(`benchmark/fs-bench-pro/src/main.rs:2149`) and
`let layerstack_init_ns = elapsed_ns(init_started);` (`main.rs:2154`), wrapping exactly
one call, `client.initialize_layerstack(...)` (`main.rs:2150-2153`).

Inside that call, per file, the walker opens the source file and reads it to
completion through `CountedSourceReader` (`crates/layerfs-layerstack-store/src/layerstack.rs:2732-2744`),
then calls `filesystem::write_file` (line 2738) — which is where representation
selection (whole-file vs FastCDC chunking) and object construction happen — and adds
the read bytes to `scanned_bytes` (`layerstack.rs:2748-2750`). The accept path and
commit are also inside the same call.

**The v0.1.6 timer therefore INCLUDES:** the full 10,000-file / 300,000,000-byte source
scan, the source content read, the whole-file/chunked decision and CDC scan, object
construction, the accept path, and the commit.

The parity spec states the same boundary in the repo's own words:
`core/docs/benchmark/fs-bench-pro-storage-content/namespace-10000-parity-spec.md:41`
— *"timed phase | scan the fixture (10,000 files / 300,000,000 bytes), construct
content objects, admit them to the Store, commit"*.

**v0.1.7 — `measure("pipeline", ...)`** (`core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs:786-847`).
The C2 supplied-object rule is stated at
`core/benchmark/fs-bench-pro-storage-content/src/ops/c2.rs:13-16`:

> `//! C2 never runs C1 file construction inside a measured phase. Every canonical`
> `//! object a C2 row saves is supplied by the harness: built before the timer, and`
> `//! offered to the Store as the finalized objects they are, so no envelope is`
> `//! re-guessed on the way in.`

The timer **EXCLUDES**, all constructed before it:

| Excluded work | Source |
|---|---|
| tree recipe `Recipe::prepare()` | `pipeline.rs:598-604` |
| the byte plan `namespace_content::plan(...)` | `pipeline.rs:611-619` |
| **all 300 MB of content construction** — `construct_bytes` (chunking decision, FastCDC scan, envelope encode, object identity) wrapped in `Timing::disabled("setup.construct", ...)` | `pipeline.rs:621-664` |
| base store creation `c2::create_and_save_untimed` and sample prep/de-warm `c2::prepare_sample` | `pipeline.rs:678-679` |
| batch split `prepared.batches(...)` | `pipeline.rs:691-694` |
| `fs::batch_backings` ×3 | `pipeline.rs:707-718` |
| the reader chain / prefix snapshots | `pipeline.rs:719-767` |
| the oracle replay | `pipeline.rs:860-876` |

The timer **INCLUDES:** `Store::open`, `begin_save`, `build_filesystem`/`update_filesystem`
per batch, re-offering the already-finalized objects with `operation.accept(object)`
(`pipeline.rs:834-843`), and `operation.finish` (`pipeline.rs:844`). The row's own
`notes` field states it: `"measured_region: Store::open + build_filesystem + content
accept + save + acknowledgement"` and `"content: constructed before the timer, accepted
inside it (C2's supplied-object rule)"`.

**Net:** the v0.1.6 timer pays for scan + decode + construct + save; the v0.1.7 timer
pays for envelope re-offer + save only. The FastCDC scan of all 300 MB is inside the
v0.1.6 timer and outside the v0.1.7 timer.

### 2.2 `scanned_bytes: 300000000` vs `initialization_disk_read_bytes: 729088`

```
scanned_bytes                  = 300000000   (= 300.0 MB, the full logical fixture)
initialization_disk_read_bytes =    729088   (= 0.73 MB)
ratio                          = 0.243 %
```

`scanned_bytes` counts bytes genuinely read through the file reader
(`layerstack.rs:2748-2750`, fed by `CountedSourceReader`, `layerstack.rs:2732-2735`).
It is 411× larger than the measured disk reads. **The v0.1.6 scan did not pay a
storage read for any meaningful part of its 300 MB.** The pages were already resident,
so the scan was served from page cache inside the timed phase.

The cache-declaration fields, verbatim:

| Field | Verbatim value | Source |
|---|---|---|
| `record[0].fixture_cache_profile` | `"reused-first-sample-uncontrolled"` | `perf.jsonl` line 2, `records[0]` |
| `header.cache_contract` | `null` | `perf.jsonl` line 1 |
| `sample.sampled_paths_or_ranges` | `[]` | `perf.jsonl` line 2 |
| `sample.setup.fixture_reuse_method` | `"host-prepared-source"` | `perf.jsonl` line 2 |

`cache_contract` is `null` because the harness's cold contract does not cover this
case — `benchmark/fs-bench-pro/shared/runner.py:1337` writes
`"cache_contract": cold.CONTRACT if cold.applies(selection) else None`, and
`benchmark/fs-bench-pro/shared/cold.py:22-25` restricts `applies()` to
`case == "namespace-100000"`. `CONTRACT = "namespace-100000-cold-v2"`
(`cold.py:13`). The parity spec records the same fact:
`namespace-10000-parity-spec.md:44` — *"`cache_contract: null` (the harness's cold
contract covers only `namespace-100000`)"*.

**Implication.** This row has **no cold claim and no cache contract**. Its cache state
is declared *uncontrolled*. The cold contract's own corroboration test
(`cold.py:232-236`: `reads < allocated` ⇒ *"operation read volume does not corroborate
cold acquisition"* ⇒ INELIGIBLE) would have rejected this row outright had it applied:
0.73 MB of reads cannot corroborate a 300 MB cold scan.

Under `AGENTS.md` §1 ("A warm cache must never credit a measured phase"), a phase whose
cache state is undeclared or unknown is **INCOMPLETE or INELIGIBLE — never PASS**. The
v0.1.6 row's `layerstack_init_ns = 944880958` (0.945 s) is therefore **not admissible
as a cold scan measurement**, and the 300 MB it reports as scanned was not paid for
from storage inside the timer. `header.verification_status = "NOT_RUN"` and
`record[0].measurement_mode = "init-only-diagnostic"` are consistent with that reading.

### 2.3 `SaveProfile` — bucket DEFINITIONS only

Source: `core/crates/layerfs-storage/src/cas/owner.rs:30-175`.
The struct is documented at `owner.rs:30-42` as *"Seven disjoint buckets, each charged
at the call site that does that kind of work… an **aggregate over the whole
operation**"*. `ResolveProfile` is the five-part split of bucket 1
(`owner.rs:79-103`); `SaveProfile::total_ns` sums the seven (`owner.rs:147-163`).

**Every measured value below is NOT_MEASURED.** The v0.1.7 SaveProfile is not published:
the receipt `ns17-final-20260921T031259Z/pipeline-namespace-10000/receipt.json` contains
no key matching `save`, `accept`, `remainder` or (other than the empty top-level
`profile: ""`) `profile`. Its `phases` publish `operation_ns`, `handoff_ns`,
`preparation_ns` and the wall, but not the seven buckets. No bucket has been measured
in this campaign, and no benchmark or build was run.

| # | Bucket | Definition (verbatim scope) | Source | Value |
|---|---|---|---|---|
| 1 | `resolve` | The sum of the five disjoint parts of resolution (`resolve_ns()`, `owner.rs:130-133`). | `owner.rs:45-46`, `79-103` | **NOT_MEASURED** |
| 1a | `resolve.eligible_ns` | "Candidate eligibility: the `depth_of` edge walk, which reads each edge through `ChainBases` and is charged to no other counter in the product." | `owner.rs:90-92` | **NOT_MEASURED** |
| 1b | `resolve.acquire_ns` | "Base acquisition: the chain rebuild that produces the offered bytes a prefix frame is taken against." | `owner.rs:93-95` | **NOT_MEASURED** |
| 1c | `resolve.cost_ns` | "The post-trial cost walk that records the admitted object's own depth." | `owner.rs:96-97` | **NOT_MEASURED** |
| 1d | `resolve.reuse_ns` | "Exact-reuse verification: stored-object reconstruction, the identity re-hash that authenticates it, and the byte comparison." | `owner.rs:98-100` | **NOT_MEASURED** |
| 1e | `resolve.pooled_ns` | "The pooled lane's value lookup, base acquisition and index synchronization." | `owner.rs:101-102` | **NOT_MEASURED** |
| 2 | `full_ns` | "FULL representation encode: the ordinary lane's tree-role and payload frames, and the pooled lane's leaf body." | `owner.rs:47-49` | **NOT_MEASURED** |
| 3 | `delta_ns` | "Delta representation encode: ordinary prefix frames and pooled COPY/INSERT program construction." | `owner.rs:50-52` | **NOT_MEASURED** |
| 4 | `group_ns` | "Group codec: ordinary group framing and pooled value-group compression." | `owner.rs:53-54` | **NOT_MEASURED** |
| 5 | `place_ns` | "Pack placement: lane selection and the write it produces." | `owner.rs:55-56` | **NOT_MEASURED** |
| 6 | `sql_ns` | "SQL: object rows, pack bodies, value-group rows, the content-signature flush and the publication watermark." | `owner.rs:57-59` | **NOT_MEASURED** |
| 7 | `commit_ns` | "Transaction cadence: `COMMIT`, `ROLLBACK`, and the `BEGIN IMMEDIATE` that restarts a bounded transaction." | `owner.rs:60-62` | **NOT_MEASURED** |

**Deliberately OUTSIDE all seven** — reported as the *remainder*
(`owner.rs:40-42`): *"The instrument measures the accept path, so the caller's own
per-object work (presence validation, group assembly outside the codec,
`raw_payload`) is deliberately outside all seven and is reported as the remainder."*
The remainder is defined operationally as the gap between `SaveProfile::total_ns()`
and the accept span (`owner.rs:147-150`), i.e. `storage.accept_loop` minus the seven.
Value: **NOT_MEASURED**. The accept span exists as `accept_span_ns`
(`pipeline.rs:784-789, 845`) but is not published in the receipt either.

**Not a bucket, explicitly.** `reuse_repeat` (`owner.rs:63-76`) is "A **count**, not a
duration, and deliberately not an eighth time bucket: `total_ns` still sums the seven
durations". Value: **NOT_MEASURED**.

**Why no bucket value exists.** `SaveProfile` is an aggregate over the accept path and
is published "once per state" (`owner.rs:34-36`). The v0.1.7 receipt is
`status: "INCOMPLETE"`; its `resources.space.incomplete` carries
`"no reading was taken before the chain"`. A `grep -c "profile_"` over that receipt
returns **0**, independently confirming the run predates any publication. Recovering
the seven bucket values requires a benchmark run, which this campaign was explicitly
forbidden to perform.

**Pending, not measured.** An **uncommitted** change in the working tree
(`pipeline.rs`, +55 lines, not in HEAD `9c46930b8`) adds `accept_span_ns` and a loop
publishing `pipeline.profile_resolve_ns`, `…_eligible_ns`, `…_acquire_ns`,
`…_cost_ns`, `…_reuse_ns`, `…_pooled_ns`, `pipeline.profile_full_ns`, `…_delta_ns`,
`…_group_ns`, `…_place_ns`, `…_sql_ns`, `…_commit_ns`, `pipeline.profile_total_ns` and
`pipeline.accept_span_ns`. That is the correct shape — every bucket beside its
denominator. It must **not** be cited as evidence until a receipt publishes it. See
`raw/10`.

---

## Directory contents

| File | Contents |
|---|---|
| `README.md` | this report |
| `raw/01-v016-fixture-source-citations.md` | v0.1.6 generator citations, band table, Hamilton arithmetic |
| `raw/02-v017-ladder.py` | the derivation script (pure arithmetic; reproduces every number here) |
| `raw/03-v017-ladder-output.txt` | its output, including the docstring weight-sum defect |
| `raw/04-canonical-closure.txt` | byte-exact closure of the object roles against the receipt |
| `raw/05-v016-receipt-fields.json` | v0.1.6 record 0 extract + verbatim cache-declaration fields |
| `raw/06-v017-receipt-fields.json` | v0.1.7 `resources.space`, `counters`, `notes`, `status` |
| `raw/07-saveprofile-buckets.md` | seven bucket definitions + the excluded remainder, all NOT_MEASURED |
| `raw/08-verdict.txt` | the verdict, standalone |
| `raw/09-timer-boundaries.md` | included/excluded work for both timers, with citations |
| `raw/10-pending-worktree-change.md` | the uncommitted `pipeline.rs` change and why it matters |

**No build was run. No benchmark was run. No pre-existing file was modified.**

## Working-tree state (read this before quoting any `pipeline.rs` line)

The tree is **not clean**:
`core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs` carries **+55
uncommitted lines** that are not in HEAD `9c46930b8`. Squad D did not write them and
did not touch that file.

The pending hunks add `accept_span_ns` and a `pipeline.profile_*` publication loop
carrying the seven `SaveProfile` buckets — i.e. exactly the publication §2.3 says a
future measurement must record. It is **not** evidence yet: the existing v0.1.7 receipt
contains **zero** occurrences of `profile_`, which independently confirms that the run
being analysed predates the change. The `NOT_MEASURED` markings below stand.

Every `pipeline.rs` citation at or before line 767 is identical in HEAD and in the
working tree. The timed-region citations shift by +11: `measure("pipeline", …)` is
**775** in HEAD and **786** in the working tree. See `raw/10`.
