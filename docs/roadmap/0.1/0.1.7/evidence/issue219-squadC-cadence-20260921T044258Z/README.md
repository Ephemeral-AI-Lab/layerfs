# Squad C — what ends a transaction, and the cadence pre-registration (#219)

> **Status: analysis + pre-registration. No product source was changed, nothing was built,
> nothing was benchmarked, and neither registered treatment has been implemented or run.**
> Every claim resolves to a file:line in `source-citations.md`, to a receipt field, or to a
> SQL statement whose text and output are in this directory. Labels used throughout:
> **MEASURED**, **DERIVED** (arithmetic over measured inputs, stated), **NOT_MEASURED**.

Read for this note (read-only): `core/crates/layerfs-storage/src/{cas,sqlite,pack,encoding}`,
`core/crates/layerfs-storage/tests/`, the diagnostic receipt and its store, Squad A's and
Squad B's evidence directories, `docs/roadmap/0.1/0.1.7/issue-commit-time-rca-handoff.md`,
`concurrency-controls.md`, and `git show`/`git log -S` over the storage crate's history.

## 1. What a step is that triggers a commit

**One step is one uninterrupted hold of the per-Store arbitration lock, and it always ends in
`COMMIT` before that lock is released.** A step is opened by whichever of four sites takes the
lock and needs a transaction — `seal_group` frames and places **one** lane group
(`cas/placement.rs:132-200`), `write_value_groups` places a pooled leaf's value groups
(`cas/pool_lane.rs:288-355`), the ordinal reservation takes its own transaction
(`cas/pool_lane.rs:112-122`), and the preparation wave's candidate flush takes one per wave
(`cas/lifecycle.rs:180-202`) — and every one of them ends in `maybe_commit`
(`cas/lifecycle.rs:160-178`), whose own comment states the rule: *"A write transaction
therefore never outlives the step that opened it under the arbitration lock, so every step
commits before that lock is released. Batching stays inside a step; it cannot span steps"*
(`cas/lifecycle.rs:163-167`). The reason is not throughput: SQLite admits one writer per store
file and the second writer must not wait for the first one's whole upload
(`cas/lifecycle.rs:163-166`), so a transaction that outlived a step would starve it under the
declared `busy_timeout = 0` (`sqlite/connection.rs:46`). Batching is what can happen *inside*
a step — `insert_objects` issues one multi-row statement per 128-row chunk
(`sqlite/write.rs:101-164`) and `select_many` produces one write per pack touched in the call
(`pack/placement.rs:71-80`) — but the commit is per step, never per pack.

**Why 25,245 objects make 17,378 commits and not 1,250 (one per pack).** Because the commit
unit is the step, the pack is not: a pack stays open across many steps and is rewritten in
full by each of them (`sqlite/write.rs:80-89`). Measured from the store
(`store-geometry.txt`), 16,595 distinct `(pack_id, group_number)` pairs hold the 25,245
object rows — 16,595 seals over 1,250 packs, i.e. **13.3 steps per pack** (13.4 pack writes per pack
once the 207 value-group writes are counted) — and 10,081 of
those groups hold a single record while 5,662 hold two (`records_per_group_hist`). The shape
is set by the lanes: the whole-file lane seals **every record alone**
(`cas/selection.rs:57-58` `WholeFile => occupied`, `:97-102`, and the grammar refuses a
multi-record group outright: `pack/assemble.rs:117-120`), giving 9,444 objects → 9,444
groups → 9,444 commits; the native (chunk) lane seals when the next record would pass
`GROUP_TARGET` = 48 KiB (`policy.rs:93`, `cas/selection.rs:65-69`), giving 14,466 chunks →
7,130 groups → 7,130 commits (two ~19 KB chunks per group). The pooled metadata lane adds two
further transactions per inode leaf with fresh values — one to reserve ordinals and one to
write the value group (`cas/pool_lane.rs:122`, `:355`) — and every accept wave adds one for
the content-signature flush (`cas/lifecycle.rs:201`).

The exact decomposition (**DERIVED**, cross-checked three ways) is:

```
   1  slot acquisition            sqlite/ownership.rs:104,133   (cas/lifecycle.rs:95-99)
  16,595  object-group seals      cas/placement.rs:199          <- 16,595 distinct groups in the store
     207  pooled value-group writes  cas/pool_lane.rs:355        <- 207 metadata_value_groups rows
     207  ordinal reservations    cas/pool_lane.rs:122          <- the same 207 leaves
     367  candidate-signature flushes  cas/lifecycle.rs:201     <- by subtraction
       1  publication             cas/lifecycle.rs:256-258
  ------
  17,378  = pipeline.commits
```

Cross-checks: `16,595 + 207 = 16,802` is exactly the number of pack writes Squad B parsed out
of the pack bytes themselves (`pack_stats.json`); the same table's 17,377 `SELECT next_pack_id`
executions equal `16,802 + 207 + 367 + 1` — the pack-placing steps plus the ordinal reservations
plus the flush steps plus the publication step, every one of them a `begin_write`, whose only
reader of that row is `cas/lifecycle.rs:133`;
`207` is their call count for the ordinal reservation, the window read and the
`metadata_value_groups` insert; and `max(content_signatures.stamp) = 9,444` equals the
whole-file object count exactly, because the ring inserts only for that lane
(`encoding/delta/select.rs:297-303` and its three siblings). The full arithmetic, including the
one consistency condition a re-measurement should check (367 of ≥577 waves commit; ≥210 roll
back, ~191 of them explained by the single 100 MB anchor file), is in
`commit-decomposition.txt`.

## 2. What `commit_ns` actually charges

**It charges `write::commit` plus, in the step path only, the pack-watermark write that
precedes it — and nothing else.** There are exactly two charge sites in the product:

| site | charged region | source |
| --- | --- | --- |
| step path | `advance_pack_if_moved` **and** `write::commit` | `cas/lifecycle.rs:162` (`Instant::now`) → `:168` → `:169` → `:170` (`charge`) |
| publication | `write::commit` **only** | `cas/lifecycle.rs:254` (`advance_pack_if_moved`, uncharged) → `:255` (`Instant::now`) → `:256` → `:257` |

`BEGIN IMMEDIATE` is charged **nowhere**: `begin_write` (`cas/lifecycle.rs:129-139`) reads the
watermark, sets the state and increments the transaction counter, and contains no
`SaveProfile::charge` at all. `ROLLBACK` is charged nowhere either
(`cas/lifecycle.rs:197`, `:283`). Squad B's statement table prices the uncharged
`BEGIN IMMEDIATE` at 17,378 calls × 5,375 ns = **93.4 ms** that lives outside every bucket.

**The doc overstates, and should be corrected.** `cas/owner.rs:60-62` says:

> "Transaction cadence: `COMMIT`, `ROLLBACK`, and the `BEGIN IMMEDIATE` that restarts a
> bounded transaction."

Two of those three are not charged, and the field does charge something the doc does not name.
The accurate statement is:

> `commit_ns`: the engine's transaction-boundary work for this step — `write::commit`
> (`sqlite/write.rs:50-56`) and, on the step path only, the `UPDATE store_policy SET
> next_pack_id` that precedes it (`cas/lifecycle.rs:168`). It does not include
> `BEGIN IMMEDIATE` (`cas/lifecycle.rs:130`), `ROLLBACK` (`cas/lifecycle.rs:197`), or the
> statements whose pages the commit flushes — those are `sql_ns`.

That distinction is load-bearing for this campaign: because the profile is `journal_mode =
MEMORY` with `synchronous = OFF` (`sqlite/connection.rs:32-48`), the step's `UPDATE
object_packs SET data = ?2` does not pay for its pages — it only dirties them — and the
`COMMIT` that follows pays for them. `commit_ns` is therefore a **page-flush** region, which
is exactly the mechanism #209 recorded on its own row
(`issue-commit-time-rca-handoff.md:51-58`).

## 3. The 1,351,360,518 ns, split into a fixed and a per-byte part

`77,762.7 ns` per commit is the **measured** average — `profile_commit_ns / commits`, two
receipt fields divided. Everything below is **DERIVED**, and it is labelled because two of its
three inputs come from outside this row. Full arithmetic in `commit-decomposition.txt`.

| component | ns | share of `commit_ns` | source of the input |
| --- | ---: | ---: | --- |
| fixed per-transaction | 138,311,502 – 227,373,752 | 10.2 % – 16.8 % | 17,378 × 7,959 … 13,084 ns, Squad A's synthetic calibration at the product profile (`cadence-calibration.json`) |
| pack overflow pages | 886,153,042 – 962,824,311 | 65.6 % – 71.2 % | 2.20–2.29 GB rewritten ÷ 4096 × 1.65–1.72 µs (this evidence + Squad B's parse; page price from #209, a different row) |
| **NOT_MEASURED residual** | **161,162,455 – 326,895,974** | **11.9 % – 24.2 %** | object b-tree pages, `objects_save`/`signatures_save`/`packs_save`, freelist and pointer pages — no counter in this row separates them |

In the instrument's own units per commit: **77.76 µs measured** = **8.0–13.1 µs fixed** +
**51.0–55.4 µs of rewritten pack pages** + **9.3–18.8 µs not measured**. Per byte of final pack
body the row pays 4.474 ns; per byte the pager is handed (DERIVED, 7.28×) it pays 0.589 ns.

**What is measured:** `commits`, `profile_commit_ns`, `profile_sql_ns`, the object/pack/group
counts, the pack bytes, the rewritten-byte volume (two independent derivations 4.1 % apart) and
the 16,802 pack-write count. **What is derived:** the fixed/byte split, everything that uses
the cross-row pwrite price, and the 367 flush commits. **What is NOT_MEASURED:** the residual
above; the volume the engine actually pays (`CACHE_WRITE` was never read on the product's own
connection); and this row's `cache_size`/`cache_spill`/`mmap_size`, which
`cas/store.rs:518-528` reads and the receipt does not publish.

## 4. What a cadence change is not allowed to break, and the #216 decision

**Tests that constrain the cadence** (full quotes in `source-citations.md` §"What a cadence
change may not break"):

1. `tests/pack_watermark.rs:42-72` — at **every** step boundary, read through an independent
   connection, `next_pack_id` must be ahead of every committed pack id; `:74-116` repeats it
   with two interleaved writers sharing no pack identifier. `cas/lifecycle.rs:141-150` states
   that deferring the watermark to publication is the change those cases fail on.
2. `tests/visibility.rs:109-160` — early bounded commits must not publish anything to an
   unrelated reader; `:342-394` — a pooled value group committed above the publication
   ceiling is refused with `VisibilityCeiling`.
3. `tests/cas_reuse.rs:51-54` and `tests/persistence_failure.rs:260-263` —
   `outcome.commits == 2` for a save that writes nothing: "slot acquisition and publication".
4. `tests/multi_writer.rs:6-51` — two private saves over one Store overlap and publish in
   either order; second-writer latency is part of the acceptance, not a footnote
   (`issue-commit-time-rca-handoff.md:167-170`).
5. Declared bounds that must keep failing closed rather than growing:
   `transaction_rows` (`cas/placement.rs:154-160` — the only transaction bound the code ever
   reads; `capacities.transaction_bytes` is declared at `policy.rs:352` and never used),
   the wave bounds 512 objects / 512 KiB (`policy.rs:105,107`; `cas/batch.rs:48-69`),
   `append_fits` (`pack/layout.rs:204-219`), and the group body ceiling
   `GROUP_LIMIT` = 65,536 (`pack/assemble.rs:21-22`).
6. `cas/placement.rs:203-217` and `cas/owner.rs:360-370` — every pack write invalidates the
   caches that hold pack bodies; a stale entry is refused as `Integrity("group ordinal")`.
   Any batching of placements must keep exactly one invalidation per pack write.

**The #216 writer-budget decision.** `grep -rn max_concurrent_writes` returns the schema
column, the allocator's read inside its own `BEGIN IMMEDIATE`
(`core/crates/layerfs-storage/src/sqlite/ownership.rs:105`), the service/transport capacities,
and the tests (`tests/write_admission.rs`, `tests/admission.rs`). The decision itself:
`store_policy.max_concurrent_writes` 1..=64, **default 2, unchanged from v0.1.6**
(`docs/roadmap/0.1/0.1.7/concurrency-controls.md:30-34`); a writer is a logical mutation, "not
one SQLite transaction and not one local FUSE `write()` callback; short database transactions
still serialize inside the Store" (`:36-40`); and raising it **buys no throughput** — 125.29 /
142.68 / 140.99 / 132.76 MiB/s at budgets 1/2/4/8
(`evidence/issue216-writer-budget-.../README.md:96-100`, and
`issue219-v016-gap-rca-handoff.md:138`) — so it is an isolation control, never a cadence
lever.

**Provenance correction (the cadence is not an #216 artifact).**
`docs/roadmap/0.1/0.1.7/issue219-namespace-3x8-gap-rca-handoff.md:86-89` attributes "every
step commits before releasing the arbitration lock" to #216's commit `7075f338`. It is not:
`git show 7075f338` touches `sql/schema.sql`, `cas/store.rs`, `policy.rs`,
`sqlite/{lookup,ownership,schema}.rs` and two tests — not `cas/lifecycle.rs`, not
`cas/placement.rs`, not `cas/pool_lane.rs` — and
`git show 7075f338^:core/crates/layerfs-storage/src/cas/lifecycle.rs` already contains
`advance_pack_if_moved` (line 151), `maybe_commit` (160) and the "never outlives the step"
comment (165). `git log -S "never outlives the step"` attributes the sentence to
`eb319aaa9` ("land the multi-writer storage model", 2026-09-21 02:56:40), six hours before
#216. #216 gives the Store a *configured budget*; the *step-scoped transaction* came from the
multi-writer model, and it is a deliberate design property, not an accident of #216.

## 5. The pre-registration

Registered in full, with the single difference, the identity, the expected movement in the
instrument's own units and the refutations, in **[pre-registration.md](pre-registration.md)**.
In one line each — and both read two counters the product already keeps while this driver
never publishes them, `OutcomeCounters.packs_created` and `pack_appends` (`cas/owner.rs:226-230`),
the same class of omission as `SaveProfile` itself before Squad A's diagnostic change:

- **PR-C1 — the step boundary moves from the group to the lane tail.** A lane frames its
  groups as records arrive and holds them until the step boundary (the accept wave, or a
  declared bound), then places the whole tail in one `select_many` and one pack write. No new
  constant; the same call shape `cas/pool_lane.rs:294-296` and `cas/lifecycle.rs:228-230`
  already use. Predicted: `commits` 17,378 → 1,600–2,800; pack writes 16,802 → 1,300–2,600;
  rewritten bytes 2.20–2.29 GB → 0.30–0.50 GB; `sql_ns` 781 ms → 330–470 ms; `commit_ns`
  1,351 ms → 250–560 ms; `operation_ns` 3.539 s → **2.0–2.5 s**. Refuted if `commits` stays
  above 6,000, or if the two database buckets fall by less than 300 ms while the counters
  fall.
- **PR-C2 — separating the residual.** Arms A/B/C (foreign keys off, journal off, indexes
  dropped) plus the instrument the row lacks (`CACHE_WRITE`/`PAGECACHE_*` and the row's own
  `cache_size`/`cache_spill`/`mmap_size` on the product's connection), with a pre-declared
  decision rule. It also corrects the "~142 MB/s" premise: 302 MB is what the Store *keeps*;
  what the pager is *handed* is the 2.20–2.29 GB of rewritten pack bodies.

### Is PR-C1's movement consistent with the 3.9–6.4 % bound? No — and here is the mechanism

`cadence-calibration.json` bounds **the fixed per-transaction price at constant bytes**: its
replica wrote every blob exactly once, so it never issued an `append_pack`, and the 3.9–6.4 %
is the correct price *of the COMMIT boundary*. It does not bound the lever that this row's
cadence creates: because a pack stays open across steps and every placing step re-binds the
whole pack body (`sqlite/write.rs:80-89`), **the bytes the pager is handed scale with the
number of steps, not with the bytes the Store keeps** — 7.28× measured here
(`pack-rewrite-estimate-2.txt`). The mechanism that makes PR-C1's movement larger than
3.9–6.4 % is therefore *the append-time whole-row rewrite*, and the linkage that carries it is
established and quoted in `linkage-answer.md`: 16,802 appends = 16,595 seals + 207
value-group writes, every one of them a committed step, and 13.4 steps per pack.

**What would refute the larger claim** (also in PR-C1(d)): the counters falling by
≥ 80 % while `sql_ns + commit_ns` fall by less than 300 ms (< 15 % of 2,132,598,057). If that
happens, the page-flush model is wrong for this row, `commit_ns` is not byte-compositional,
and the 3.9–6.4 % fixed-cost bound is the whole lever after all.

**What caps it, and why the coordinator should not expect more than PR-C1 predicts:** the
whole-file lane's group grammar is one unframed record (`pack/assemble.rs:117-120`,
`pack/layout.rs:114-117`), so its 9,444 appends and ~1.17–1.26 GB of rewrite cannot be
coarsened at all without a Store-format change; and the ordinary/native lanes cannot pass a
64 KiB group body (`pack/assemble.rs:21-22`) against a 48 KiB target. The residual after
PR-C1 is therefore dominated by (a) the pack geometry itself — an owner ruling — and (b) the
candidates PR-C2 separates.

## 6. NOT_MEASURED, and other corrections

**NOT_MEASURED**
- the residual inside `commit_ns` (11.9–24.2 % of it);
- the pager's true byte volume (`SQLITE_DBSTATUS_CACHE_WRITE` on the product's own connection
  was never read);
- this row's `cache_size`, `cache_spill`, `mmap_size`;
- which of the ≥577 waves roll back (the arithmetic forces ≥210; ~191 are explained by the
  100 MB anchor file, the rest is not);
- the second writer's latency and throughput on this row (the row is single-threaded);
- any spread: every number here comes from **one** diagnostic sample, so every prediction in
  PR-C1/PR-C2 must be read against an in-window control, never across windows
  (`issue-commit-time-rca-handoff.md:174-177`).

**Corrections to inputs this squad was given**
1. Squad B's ranked table lists `UPDATE object_packs SET data = ?2` at **16,802 calls**. That
   is the pack-write count (INSERT + UPDATE): each pack's **first** write is `insert_pack`
   (`cas/placement.rs:219-220`, `sqlite/write.rs:68-77`), because `created` is true exactly
   when the placed group is `group_number == 0` (`pack/placement.rs:183`). With 1,250 pack
   rows (`store-geometry.txt`), the UPDATE runs **15,552** times; if the whole 457.1 ms
   belongs to it, that is **29.4 µs** per call, not 27.2 µs. The aggregate and the
   amplification arithmetic are unaffected.
2. The handoff's attribution of the step-commit to #216 (correction §4 above).
3. `cadence-calibration.json` prices only the fixed per-transaction cost; quoting its
   3.9–6.4 % as "the cadence lever" understates what a cadence change can reach (see §5).
4. `pipeline.profile_commit_ns` (1,351.4 ms) exceeds Squad B's `COMMIT` statement total
   (1,104.9 ms) by 246.5 ms. The only other work inside that charged region is
   `advance_pack_if_moved`, whose statement count is 1,250 and whose price they put near
   1 µs; the difference is **NOT_MEASURED** here and is flagged rather than explained.

**Raw files in this directory**

| file | what it is |
| --- | --- |
| `sqlite-queries.sql` | first query set; one alias (`values`) collided with a SQLite keyword and the statement failed — kept as run |
| `sqlite-queries-2.sql` + `store-geometry.txt` | the corrected set and its verbatim output (geometry, group uniqueness, ring stamps, per-role lanes) |
| `pack-rewrite-estimate.sql` + `pack-rewrite-estimate.txt` | first attempt; the `WITH` clause was in its own statement, so the last two SELECTs failed — kept as run |
| `pack-rewrite-estimate-2.sql` + `pack-rewrite-estimate-2.txt` | the corrected per-lane amplification estimate (the one §1/§5 cite) |
| `commit-decomposition.txt` | the commit decomposition, per-commit arithmetic and the residual, with every input listed |
| `linkage-answer.md` | the append↔transaction linkage and what a cadence treatment may not break |
| `source-citations.md` | every file:line behind every claim |
| `pre-registration.md` | PR-C1 and PR-C2, with expected movement and refutations |

**Not this squad's to do:** the append-only ledger entry (next free **L61**), the #209/#219
updates, and any implementation of PR-C1/PR-C2 — this squad's remit was one evidence directory
and no build, no benchmark, no Rust edit. The stores were read through byte copies at
`/tmp/layerfs-squadC-cadence-20260921T044258Z`; the originals were never opened for write.
