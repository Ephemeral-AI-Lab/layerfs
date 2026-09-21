# Pre-registration — Squad C (#219), 2026-09-21T04:42:58Z

> **Nothing here has been implemented or run.** Two registrations: **PR-C1**, the cadence
> treatment the task commissioned, and **PR-C2**, the separation the coordinator asked for
> against the residual. Both are written before any arm exists, both name what would refute
> them, and neither adds durability, changes the Store format, raises worker counts or
> changes the workload.

## PR-C1 — the step boundary moves from the group to the lane tail

### (a) The single difference

**Today:** the ordinary and native lanes end a step when the framed group they are building
would exceed `GROUP_TARGET` = 48 KiB (`cas/selection.rs:65-69`), and the whole-file, pooled
and singleton lanes end a step after every occupied record (`cas/selection.rs:57-58, 97-102`).
Each such seal is one transaction: lock → frame one group → place one group → write the whole
pack → `COMMIT` → release the lock (`cas/placement.rs:132-200`, `cas/lifecycle.rs:160-178`).

**The one change:** a lane's records are framed into groups as they arrive and the framed
groups are **held** (a bounded per-lane list, bounded by the existing wave bound) until the
step boundary — the end of the accept wave, or the point where the held work would touch a
declared bound — at which point the lane's whole tail is placed by **one**
`select_many(lane, groups, ...)` call and **one** pack write. Group composition, framing,
lane assignment, pack geometry, the transaction-per-step rule and the commit sites are
unchanged. The shape is not new: `cas/pool_lane.rs:294-296` already passes a vector of
groups to `select_many`, and `cas/lifecycle.rs:228-230` already seals all five lanes inside
one transaction at publication.

Nothing else moves: no new constant, no change to `GROUP_TARGET`, `GROUP_LIMIT`,
`PACK_LIMIT`, the pack directory, any frame version, any pragma, the writer budget, or the
workload. It must not hold a transaction across a step boundary: the lock is still taken and
released once per step (`cas/lifecycle.rs:163-167`).

### (b) The identity it is compared against

The diagnostic receipt this squad is explaining, unchanged and not to be re-measured:

`core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-squadA-profile-20260921T044041Z/pipeline-namespace-10000/receipt.json`

```
operation_ns                   3,538,935,458      pipeline.commits          17,378
pipeline.accept_span_ns        3,536,155,750      pipeline.inserted         25,245
pipeline.profile_sql_ns          781,237,539      pack writes (derived)     16,802
pipeline.profile_commit_ns     1,351,360,518      packs_created (derived)    1,250
pipeline.profile_total_ns      2,482,364,769      rewritten pack bytes  ~2.20-2.29 GB
CPU (user+system)              3,286,170,000      CPU/wall                    0.929
```

Same case (`pipeline-namespace-10000`), same prepared store, same single construction
worker, same harness identity; one sample per arm; the pinned row is **not** the comparison —
the diagnostic row is, because it is the only row that publishes `SaveProfile`.

### (c) Expected movement, in the instrument's own units

Mechanism behind each prediction: (i) fewer steps ⇒ fewer `COMMIT`s ⇒ less fixed
per-transaction cost; (ii) fewer steps that place a group into a given pack ⇒ the pack body is
bound fewer times (`sqlite/write.rs:80-89`) ⇒ less `sql_ns` in the statement and fewer dirty
pages for the following `COMMIT` ⇒ less `commit_ns`. The second is the term Squad A's §9
calibration could not see (it held bytes constant); the linkage is established in
`linkage-answer.md`.

| instrument | measured | predicted under PR-C1 | why |
| --- | ---: | ---: | --- |
| `pipeline.commits` | 17,378 | **1,600 – 2,800** (−84 % … −91 %) | ≤ 5 lane-tail steps per wave (≈577 waves) + 207 ordinal + 207 value-group + 367 flush + 2 fixed |
| `pipeline.pack_appends` (measured by the product, not published by this driver: `OutcomeCounters.pack_appends`, `cas/owner.rs:229-230`) | 15,552 | **1,050 – 2,400** | one write per pack touched per step; the first write of each pack is `insert_pack` |
| `pipeline.pack_writes` (16,802 derived) | 16,802 | **1,300 – 2,600** (−85 % … −92 %) | same, including the 1,250 creations |
| `pipeline.packs_created` | 1,250 | **1,250 ± 1** | group composition and `append_fits` are untouched |
| rewritten pack bytes (new counter) | ~2.20–2.29 GB | **0.30 – 0.50 GB** (−78 % … −87 %) | `L*(k+1)/2` with k falling from 13.4 to ≈1–2 writes per pack |
| `pipeline.profile_sql_ns` | 781,237,539 | **330 – 470 ms** (−40 % … −58 %) | Squad B prices the append UPDATE at 457.1 ms binding ~2.29 GB; the bound volume falls ~5×, the statement count ~11× |
| `pipeline.profile_commit_ns` | 1,351,360,518 | **250 – 560 ms** (−59 % … −81 %) | fixed 138–227 ms → 16–26 ms; pack overflow pages 886–963 ms → 121–210 ms; residual 161–327 ms partly cadence-driven and left unpredicted |
| `operation_ns` | 3,538,935,458 | **2.0 – 2.5 s** (−29 % … −43 %) | the two buckets above, plus the uncharged per-step work (`BEGIN IMMEDIATE` 93.4 ms, the per-step SELECTs 47.5 ms) |
| CPU (user+system) | 3,286,170,000 | **2.4 – 2.9 s** (−12 % … −27 %) | ~1.8 GB fewer page writes (memcpy + syscalls) and ~15,000 fewer statement executions |
| `pipeline.profile_commit_ns` share of operation | 38.2 % | **~10–15 %** | derived from the two rows above |

Expected artifacts: identical `filesystem_root` (1d6fba29…), identical 14 pinned counters,
identical logical content, identical pack bodies (the same groups land in the same packs in
the same order, because `append_fits` sees the same sequence); the **file hash is not
predicted to be identical** (different transaction boundaries can change page and freelist
layout) and must be reported either way, never required.

### (d) What would refute it

1. `pipeline.commits` not below **6,000** — the step boundary did not move as modelled.
2. `pipeline.pack_appends`/`pack_writes` not below **6,000** — the append is **not** bound to
   the transaction boundary, i.e. the linkage is refuted and the coordinator's synthesis
   (and §3 of `linkage-answer.md`) is wrong.
3. Rewritten bytes not below **1.2 GB** — the amplification did not follow the append count.
4. `profile_sql_ns + profile_commit_ns` falling by less than **300 ms** (< 15 % of the
   measured 2,132,598,057) while the counters fall as predicted — the page-flush model is
   refuted for this row: the 38.2 % is then neither cadence nor bound-byte volume, and the
   residual's four candidates (`pre-registration` PR-C2) become the only live explanation.
5. `operation_ns` falling by less than **15 %** (less than 530 ms) — the treatment fails its
   purpose whatever the counters say.
6. The second writer is refused, waits or loses a save (`multi_writer.rs` red, or the
   second-writer probe degraded) — failed treatment, no timing result counts
   (`issue-commit-time-rca-handoff.md:167-170`).
7. Any pinned identity moves, or `pack_watermark.rs` / `visibility.rs` / `cas_reuse.rs` /
   `persistence_failure.rs` go red — invalid regardless of the numbers.
8. The complete command leaves the 15 s perceptual budget (the diagnostic row is 4.79 s).

## PR-C2 — separating the residual (the coordinator's update 1, question b)

### The premise to correct first

`sql_ns + commit_ns = 2,132,598,057 ns` against a 302 MB store is quoted as "~142 MB/s".
That is the rate over the **final** bytes. The pager is handed the **rewritten** bytes —
2.20 GB by this evidence's equal-increment estimate, 2.29 GB by Squad B's parse of the same
packs (`pack-rewrite-estimate-2.txt`, `pack_stats.json`) — so by the same arithmetic the
pager's rate is **~1.03–1.07 GB/s**, and Squad A's own synthesis already notes the implied
1385 MB/s exceeds its 677.5 MB/s synthetic floor for this geometry. Which of the two volumes
the engine actually pays is **NOT_MEASURED**: nothing in this row reads a page-write counter
on the product's own connection.

### Zero-cost prerequisite (no product change)

Publish the row's own connection facts: `cache_size`, `cache_spill`, `mmap_size`
(`cas/store.rs:518-528` reads them through `connection_profile()`; the receipt publishes
none of them). Until that exists, "the page cache" cannot be either blamed or cleared, and
the campaign is quoting a profile it has not read.

### The separating arms — one difference each, all measurement-only, none shippable

| arm | the single difference | expected movement (if the candidate is material) | refuted if |
| --- | --- | --- | --- |
| **A. foreign keys** | `PRAGMA foreign_keys = OFF` on the save connection for this arm only | ≥ 80 ms of `sql_ns + commit_ns`: 25,245 `objects` rows × 2 parent lookups (`saves`, `object_packs`) | < 40 ms (< 2 % of operation) |
| **B. MEMORY journal** | `PRAGMA journal_mode = OFF` for this arm only (contract-breaking diagnostic; #209 ran the same class) | ≥ 100 ms: the in-memory rollback journal touches every original page once more | < 50 ms |
| **C. index maintenance** | drop `objects_save`, `signatures_save`, `packs_save` on a **copy** for this arm only | ≥ 60 ms of page-write work | < 30 ms |
| **D. the instrument** (not an arm) | read `sqlite3_db_status(CACHE_WRITE / CACHE_SPILL / CACHE_USED)`, `sqlite3_status(PAGECACHE_*)` and `page_count`/`freelist_count` on the product's own connection, per save | decides the volume question above | — |

Arms A–C change the declared profile or the schema, so each is **diagnostic evidence only**:
`sqlite/connection.rs:57-72` refuses a connection whose profile differs, and a shipped change
in any of them is a durability, profile or format decision that needs an owner. Arm B in
particular must never ship (it removes rollback atomicity).

### Pre-declared decision rule

1. If arm D shows `CACHE_WRITE` pages ≥ **3×** the final pack pages (73,736 × 3 = 221,208),
   the volume hypothesis holds and the next lever is the pack-write geometry (a
   fixed-size pack row or incremental write) — **an owner ruling, not an agent's**, per
   `issue-commit-time-rca-handoff.md:159-163`.
2. If arm D shows `CACHE_WRITE` ≈ the final pack pages, the residual is per-page cost and
   arms A–C decide which of the four candidates owns it.
3. If arms A–C each move < 2 % of the operation, the residual is attributed to the pwrite
   path itself at this page width, and the campaign stops looking for a clause-level cause.
4. **PR-C2 runs after PR-C1** where possible: on a tree whose rewritten volume is ~5×
   smaller, the separating arms have far more signal per page.

### What would refute PR-C2 as a whole

If arm D reports `CACHE_WRITE` pages within 20 % of the final pack pages **and** arm B moves
nothing, then the append-time rewrite is not being paid twice, this squad's amplification
model is wrong about where the bytes go, and the 38.2 % must be explained without it.

## Explicitly out of scope for both registrations

- No durability (nothing here calls `fsync`; `journal_mode = MEMORY` + `synchronous = OFF`
  stay, `sqlite/connection.rs:32-48`).
- No Store-format change, in particular none that stops the pack directory from growing —
  the coordinator's own boundary.
- No worker-count change: `max_concurrent_writes` stays 2, one construction worker stays.
- No workload change: 10,000 files / 100 directories / 300,000,000 declared bytes.
- No ledger edit from this squad: the evidence directory is this squad's whole remit. The
  append-only ledger entry (next free **L61**) and the #209/#219 updates are the parent's.
