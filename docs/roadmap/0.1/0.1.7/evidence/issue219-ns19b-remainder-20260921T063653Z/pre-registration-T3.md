# Pre-registration — T3: the step is the wave (written before the run)

Control: **D2b** (`ns19-D2b-instrument-20260921T064257Z`), which is the committed product code
(`a5c54df16`) with this round's instrument and no treatment: `operation_ns` 2714.3 ms,
`accept_span_ns` 2710.3 ms, `diag_begin_ns` 257.5 ms, `commit_ns` 742.4 ms, `span_finish_ns`
39.7 ms, 13/13 gates, 14/14 pinned counters. It is the control because it carries the same
instrument; T1c does not.

## The one difference

> The save's write transaction spans **one preparation wave** instead of one seal. A wave holds the
> Store's arbitration for its whole duration, every seal inside it joins the transaction the wave
> opened, and the transaction is acknowledged once at the wave's end - before the lock is released.
> Every nested acquisition of the arbitration inside a wave is a no-op; the step, which the
> multi-writer rule already defines as the unit that owns a transaction, becomes the wave.

Implementation: `MutationOwner::with_wave`, `ownership::lock_unless_held`, `maybe_commit` refuses to
close a wave's transaction, and the lock sites reachable inside a wave (`save`, `selection`,
`placement`, `lifecycle`, `pool_lane`, `delta::select`, `owner`) consult the owner's wave flag.

## The two consequences, declared before the run

1. **`pipeline.commits` is a pinned counter and this treatment moves it by design.** The wave count
   is ~590 and the ordinal reservations commit as their own step (382 of them), so `commits` falls
   from **17,378** to roughly **1,000**. `tests/golden/expected.tsv` pins `pipeline.commits 17378`
   for this row, so **the row will FAIL `g1.o3-pinned-counters` unless that one pin is re-pinned**.
   The re-pin is declared here, before the number exists, and not after: it is part of the treatment
   and not a rescue of it. **No other pin may move.** The work counters - `inserted` 25245,
   `statements` 16595, `packs_created`, `pack_appends`, `presence_queries` 398, `full_records`
   25241, `prefix_records` 4, `content_bytes`, `reused`, `batches`, `bindings`, `metadata_objects`,
   `objects_emitted` - and `digest:filesystem_root` are the ones that say the *work* did not change,
   and all of them must reproduce exactly.
2. **One semantic difference was found by the suite and is preserved, not waived.**
   `metadata_pool_index::a_failed_save_cannot_change_the_published_candidate_set` pins that an
   aborted save's ordinal reservations are **never reused**: under per-seal commits the reservation
   was durable on its own. Under a wave transaction it would have rolled back with the wave, so the
   reservation is committed as **its own step** (`commit_reservation`), which closes the wave's
   transaction and lets the next write reopen it - exactly the boundary every seal used to draw.
   This is the only behaviour difference the 33 test binaries found.

## Expected movement, in the instrument's own units

| instrument | D2b | expected after T3 |
| --- | ---: | ---: |
| `diag_begin_ns` | 257.5 ms | <= 40 ms (17,378 `BEGIN IMMEDIATE` -> ~982: ~600 waves + 382 reservations) |
| `commit_ns` | 742.4 ms | 550-650 ms (the fixed per-transaction cost of ~16,400 eliminated commits at the campaign's measured 6-10 us bound, plus one deduped control page per append) |
| `operation_ns` | 2714.3 ms | 2400-2600 ms |
| CPU | 2533.8 ms | a comparable fall |

**What is deliberately NOT predicted to fall:** the *page work*. The registration withdraws round
2's 400-600 ms prediction and its reasoning: an append writes bodies at offsets the previous append
never touched, so batching transactions dedupes the control page and the fixed cost, and almost
nothing else. If `commit_ns` falls by more than ~200 ms, the mechanism is something other than the
one written here and the row must say so.

## Refuted if

1. any counter other than `pipeline.commits` moves, or `digest:filesystem_root` moves;
2. `diag_begin_ns` does not fall below 100 ms (then the BEGIN cost is not what this removes);
3. any of the 33 test binaries fails, or the two-thread `multi_writer` case deadlocks or starves a
   writer;
4. the row is not PASS once `pipeline.commits` alone is re-pinned.

## Method

One sample per case per arm, fresh `--out`, single thread, no re-run. The control is D2b, measured
on the same product code with the same instrument, 20 minutes before this run.
