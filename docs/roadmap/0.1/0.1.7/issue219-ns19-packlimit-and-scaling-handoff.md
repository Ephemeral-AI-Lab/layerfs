# Handoff: bank the pack capacity, then answer the scaling question

> **Status:** handoff and commission. Filed from [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
> after rounds 17–19 (ledger L76–L78). **It commissions a three-step programme and makes no
> performance claim of its own.** Every number is sourced to a receipt, a counter or a `file:line`,
> and the ones that are inferences are labelled.

**What this document is for.** Rounds 17–19 turned the v0.1.6 question into a measured answer and
then found a lever nobody had priced. This handoff gives the next agent (a) the state of the row,
(b) the three findings that changed the plan, (c) the three steps in order with the price of each,
and (d) four corrections that are now on file and must not be re-derived. It does **not** pre-authorise
a case change or a new fixture.

---

## 1. Where the row is, all of it measured

Control and current row: `benchmark-results/issue219/ns19-S1-indexdrop-20260921T103500Z`, **PASS,
13/13 gates, 14/14 pinned counters**, one sample, `--verify full`, sealed clean tree at `e3a46d74b`.

| | ms | source |
| --- | ---: | --- |
| **`pipeline.operation_work_ns`** | **1126.73** | `counters` |
| target | 1000.00 | the owner's bar |
| **gap** | **−126.73** | |
| serial floor (L74's six terms, recomputed) | **764.82** | commit 431.24 + inserts 99.62 + pack writes 121.48 + wave 62.29 + collision 48.55 + begin 1.64 |
| headroom above the floor | 235.18 | |
| `phases.cpu_user_ns + cpu_system_ns` | 1132.99 | `phases` |
| `phases.operation_ns` (wall) | 1210.56 | `phases` |

**Boundary-matched** (round 17's boundary: `operation_work_ns` + `construct_ns` + `construct_noise_ns`)
the row is **1680.22 ms of work / 1686.48 ms of CPU** against the reference's fastest same-shape row
at **1789.51 ms** — **−6.11 % work, −5.76 % CPU.** *The v0.1.6 question is answered: this row is
ahead of the fastest reference row it can be compared with, on the only boundary the two can share.*

**The remaining blocks**, largest first: commit 431.24 (38.3 %), pack writes 121.48, row inserts
99.62, C1 build span 89.65 (**~70–100 ms of it uncharted**), profile full 70.12, wave 62.29, validate
50.07, collision 48.55, begin 1.64. **These are not a partition** — `diag_write_pack_total_ns`
contains the codec buckets and the `profile_*` figures are sub-buckets — so they cannot be summed to
the row and no residual may be computed from them.

---

## 2. The three findings that changed the plan

### 2a. The commit term is a **page-write** term, and its closure was wrong (L78)

Round 18 removed 1,101,824 B of index and `diag_commit_total_ns` fell **27,022,997 ns** — 24.5 ns per
byte — while the same term prices 301,865,004 B of pack payload at 1.43 ns per byte. One term, two
prices, 17× apart. Round 19 read the engine's own counters
(`sqlite3_db_status(CACHE_WRITE / CACHE_SPILL / CACHE_USED)`, `sqlite3_status(PAGECACHE_*)`) and
settled it:

- `cache_write` **is** the flush and tracks `page_count` growth to within three pages on every arm;
- the index adds **265 page-writes** and **+2,478,834 ns** of commit = **9,354 ns per index
  page-write**, so round 18's 27.02 ms is **2,889 page-writes for 265 pages — each index page written
  about 10.9× across the row's 95 transactions.** The index was never expensive per byte; a B-tree
  page is rewritten many times where an append-only pack page is written once;
- the row's commit term is **81,280 pack pages at 5,306 ns each** — and 431,236,291 ÷ 5,306 = 81,280
  exactly;
- **`cache_spill` is large** (61,440 pages on the pure-pack arms), so most of the flush happens
  *during* the transaction and **`commit_ns` alone undercounts it.** Any future page-price work must
  use the whole transaction over `cache_write`, not `commit_ns`.

### 2b. The declared pack capacity **is** flushed, and it is 40.2–45.6 ms (L78)

Round 17 measured that `object_packs.data` sums to **332,922,880 B = 1,270 × 262,144 = `packs_created`
× `PACK_LIMIT`**, because `sqlite/write.rs:88-95` creates every pack zero-filled at capacity with
`zeroblob(?2)`, while only **301,865,004 B** of content is written into them. Round 17 called the
difference a space note. Round 19 refuted that: **`packs-zeroblob` writes all 81,441 pages for a
payload of zero bytes**, because **a `zeroblob`'s overflow chain still requires every page's
next-page pointer to be written.** The confirmation is the pair: `packs-zeroblob` and `packs-full`
write the same 81,441 pages and cost 490.07 and 493.62 ms of transaction time — **0.7 % apart for
31 MB more payload.**

**7,582 of the row's 81,280 pack pages (9.33 %) hold no content: 40.2 ms at the row's own price,
45.6 ms at the pack arms'.** That is 32–36 % of the remaining gap.

### 2c. The v0.1.6 premise, and four corrections now on file

**The premise is refuted on a matched boundary** (§1). Four things found while checking it must not
be re-derived:

1. **Two denominators are in circulation and neither is labelled.** The harness publishes
   `init_bytes_per_second = rate(fixture_manifest.logical_bytes, layerstack_init_ns)` — **300 MB**
   (`benchmark/fs-bench-pro/src/main.rs:2710`, `:3016`). L69 and
   `issue219-v016-gap-rca-handoff.md` derived rates on **400 MB** (300 MB logical + the 100 MB
   anchor). The same reference row is **323.3 MB/s** or **431.0 MB/s** depending on which. Quote the
   denominator or do not quote a rate.
2. **"v0.1.6 did 700 MB/s on `init_namespace`" is not supported.** 691.8 MB/s is the **approved
   bar** (`issue219-namespace-10000-plan.md:22`), and that document says of the row carrying it:
   *"`cache_contract: null` and `verification_status: NOT_RUN`, so this step buys reproducibility and
   verification, **not speed**."* The same document records **`baseline rows: none (8 candidate /
   0 baseline)`** — **there is no v0.1.6 baseline row for this case at all.** The three fast rows
   (744.9 / 736.0 / 518.8 MB/s published) are **v0.1.3-era**: `docs/roadmap/0.1/0.1.3/` names issue
   #38 (5 files) and #49 (8 files), and their campaigns are `issue38-main-refresh` and
   `nine-family-fast-baseline`. The only row tied to v0.1.6 is issue #152 → **272.6 MB/s**. The six
   rows carry **six different `product_identity` and six different `image` values.** **v0.1.6's
   `init_namespace` throughput for this case is `NOT_MEASURED`.**
3. **The fixture was identical across all six rows** — same profile `synthetic-small-heavy-v2`, same
   digest `5a464369ea…`, same 7,899/1,500/500/100 file mix, same 300,000,000 logical bytes. What
   differs is the **object decomposition**: 54,46x objects against 25,158, and 305.0 MB of canonical
   bytes against 302.2 MB. So the fast rows are an *older build's decomposition of the same input*,
   not a different workload — and they used **2.92–4.43 cores** against 1.93–2.07.
4. **The bar's case and this campaign's row are not the same case, and the mapping has never been
   written down.**

   | | approved bar (`issue219-namespace-10000-plan.md:14-16`) | this campaign |
   | --- | --- | --- |
   | case | `namespace-10000` | `pipeline-namespace-10000` |
   | route / timer | `namespace` / `layerstack_init_ns` | `pipeline.*` / `operation_work_ns` |
   | setup / cache | `fresh-output` | `prepared-dewarmed` |
   | harness | `benchmark/fs-bench-pro/` (`families/init_namespace/`) | `core/benchmark/fs-bench-pro-storage-content/` |

   A **third** thing shares the name: the core harness's `namespace-10000` is in family
   `c1.fs.build-scale` (C1 build only, `warm-in-process-fixture`). The plan's **S0 was *"a written
   decision on pairing feasibility"*** and S1–S3 were never executed; the campaign went straight to
   optimising. **Writing that one page is cheap and should happen before anything scales.**

---

## 3. Step 1 — bank the measured lever: `PACK_LIMIT` (§2b)

**The change.** `core/crates/layerfs-storage/src/policy.rs:110`, `PACK_LIMIT` 256 KiB → 1 MiB, with
`GROUP_LIMIT`, `GROUP_TARGET`, the framing and every stored byte unchanged.

**Why it is worth one round.** The tail is `PACK_LIMIT mod group size` and round 17 measured it at
**24,455 B per pack** — about half a ~50 KiB group — so **the waste is proportional to the pack
count**, and there are 1,270 packs because `PACK_LIMIT` is 256 KiB.

| | packs | reserved | content | tail |
| --- | ---: | ---: | ---: | ---: |
| today, `PACK_LIMIT` 256 KiB | 1,270 | 332.9 MB | 301.9 MB | **31.1 MB** |
| `PACK_LIMIT` 1 MiB, same `GROUP_LIMIT` | ~295 | 309.3 MB | 301.9 MB | **7.4 MB** |

**Registered prediction: ~23.7 MB = 5,781 pages ≈ 30.7 ms off `diag_commit_total_ns`**, with
`packs_created` falling 1,270 → ~295 and `pack_bodies_bytes` 332,922,880 → ~309.3 MB. Refuted if
`packs_created` stays above ~400 or `pack_bodies_bytes` does not fall by ~23 MB.

**The price, verified by reading — this is not a constant flip:**

| surface | `file:line` | consequence |
| --- | --- | --- |
| lane capacity | `pack/layout.rs:173-174` | ordinary/native/whole-file/pooled packs reserve 1 MiB |
| derived body bound | `pack/assemble.rs:85` (`PACK_LIMIT - HEADER_LEN - 4 * GROUP_COUNT_LIMIT`) | grows with it |
| **reader refuses a longer pack** | `pack/layout.rs:412`, `:525` | a 1 MiB pack is **unreadable by a 256 KiB build**, so this needs a **`SCHEMA_VERSION` bump** — the version is what catches it, exactly as round 18 used it |
| **ordinary-vs-singleton routing** | `encoding/full.rs:274`, `:297` | more objects take the ordinary lane, so **`packs_created` and `pack_appends` pins will move** — expect a **re-pin**, the established pattern of L73/L74 |

`PACK_LIMIT` is **not persisted**: `store_policy` carries `format_profile`,
`small_file_threshold_bytes`, the three `*_delta_max_depth` columns, `max_concurrent_writes` and
`publication_sequence` — so there is no policy migration.

**Do not move `GROUP_LIMIT` in the same round.** Round 19 showed that raising `GROUP_LIMIT` *alone*
makes the tail **worse** (half a bigger group per pack), so commission direction #3 cannot move the
two together one-for-one. One difference per arm.

---

## 4. Step 2 — answer the transfer question on the C1 ladder (100 → 100k)

**Every round so far has optimised a single point.** Whether the 10k gains transfer is unknown, and
the cheapest way to find out is a ladder that already exists.

| case | family | entries | cache | pins |
| --- | --- | ---: | --- | ---: |
| `namespace-100-compact-v3` | `c1.fs.build-scale` | 100 | `warm-in-process-fixture` | — |
| `namespace-1000-compact-v3` | `c1.fs.build-scale` | 1,000 | `warm-in-process-fixture` | — |
| `namespace-10000` | `c1.fs.build-scale` | 10,000 | `warm-in-process-fixture` | 13 |
| **`namespace-100000`** | `c1.fs.build-scale` | **100,000** | `warm-in-process-fixture` | **13** |

**It is registered, pinned and cheap — verified, not assumed:**

| receipt | status | complete command | limit |
| --- | --- | ---: | ---: |
| `ns17-fsbuild-10000-300mb-20260921T031259Z` | PASS | **0.36 s** | 15 s |
| `ns17-namespace-10000-full-20260921T031259Z` | PASS | **0.35 s** | 15 s |

Operation 313–321 ms, preparation 19–21 ms. Ten times the entries (and 500 MB against 300 MB) lands
near **1–4 s**, and `namespace-100000` is **not** in `runner.py`'s `DECLARED_EXCEPTIONS`, so it must
fit 15 s anyway. **There is no `namespace-100000` receipt in this worktree** — it is a first
measurement at that rung, not a re-run.

**Why this ladder and not the pipeline row.** The C1 build span is 89.65 ms of the row with **~70–100
ms uncharted** — the largest unattributed thing in it — and this ladder measures exactly that half,
in isolation, with no C2 save in the way. **Do not compare against the reference harness's 47
`namespace-100000` rows**: they span **279 ms to 105.9 s (~380×)**, carry `TARGET_MISS` in both arms,
`cache_contract: null` and `verification_status: NOT_RUN` throughout, and come from a different
harness and different builds. That is correction 2 at ten times the size.

**Register the prediction before the first run.** The honest one is a *ratio*: if the C1 build's cost
per entry is flat from 10k to 100k, the uncharted block is not a scaling problem and the 10k work
transfers; if it grows superlinearly, say by how much and at which rung it bends.

---

## 5. Step 3 — then decide whether `pipeline-namespace-100000` is worth building

**There is no `pipeline-namespace-100000`.** The pipeline family has five cases and exactly one is
namespace-scale (`pipeline-namespace-10000`, 10,000 entries). Adding 100k is a **harness round, not a
measurement round**: a new case declaration, a 600 MB fixture (500 MB + the 100 MB anchor), a new
prepared artifact and new pins. Decide it **after** step 2 says whether the scaling is benign — if
the C1 ladder is flat, a pipeline rung buys less.

---

## 6. What must not be redone

| closed | why |
| --- | --- |
| the v0.1.6 premise | §1: boundary-matched, the row is **ahead** of the fastest comparable reference row |
| the "does the reference spill its payload" fork | L76: it does not; `admission.rs:1547` binds the assembled pack into `object_packs.data`, and its file is 1.0081–1.0092 canonical bytes per canonical byte |
| **the commit term as "byte-bound, closed as a lever"** | **L78: it is a page-write term, and 40.2–45.6 ms of it is capacity holding nothing** |
| the locator's second B-tree | L77: removed, −92.15 ms, every pin unchanged |
| codec level, stored frames, page size, cache-in-pages, journal modes | refuted in L67/L72; **the page size is owner-ruled at 4 KiB and is not a lever** |
| transaction cadence | L74: a COMMIT costs 0.21 ms; L78 adds that most of the flush happens before it |
| "more workers" | `AGENTS.md` §3.8; the ceiling is the floor, now **764.82 ms** |
| whole-file lane granularity, ordinal reservations | landed in L73/L74 |
| the 700 MB/s figure | §2c: it is the approved bar, carried by an unverified row, with **zero recorded baseline rows** |

## 7. Housekeeping

- Branch `codex/219-ns10000`. Six commits from rounds 17–19 are on it; **push before filing anything
  that cites a hash.**
- The measurement lock is **per worktree** (`core/benchmark/fs-bench-pro-storage-content/.measurement.lock`).
  Never interrupt another owner's run.
- Harness receipts are **not** tracked by git (`benchmark-results` is in `.git/info/exclude`), so any
  receipt a report depends on must be copied into the evidence directory or it will not survive.
- `AGENTS.md` §2: `--setup clone` for every post-initialization case, fresh `--out` per run, one
  sample per case per arm, never retune a receipt.
