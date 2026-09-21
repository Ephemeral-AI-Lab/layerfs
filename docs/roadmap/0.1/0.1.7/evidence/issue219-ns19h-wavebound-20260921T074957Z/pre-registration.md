# Pre-registration — #219 round 8: a wave is bounded by the transaction it runs in

Written **before** the first run of this arm and before any product edit.
Control: **D4b** (`benchmark-results/issue219/ns19-D4b-formula-20260921T071758Z`), the last row on
this product tree: `operation_work_ns` 1821.0 ms, `diag_commit_total_ns` 402.7 ms,
`diag_wave_ns` 112.7 ms, `diag_validate_ns` 52.1 ms, `diag_begin_ns` 11.5 ms, `pipeline.commits`
800, 13/13 gates, 14/14 pinned counters. Its product tree is identical to the one this round starts
from (`565f95366` and `789141c1e` are documentation); its *machine window* is ~50 minutes earlier,
which the drift bound below is about.

## The one difference

**A preparation wave is bounded by the transaction it runs in.** `PendingBatch`'s canonical byte
budget becomes the transaction's own declared capacity (`TRANSACTION_CANONICAL_BYTES_LIMIT`,
4 MiB − 1) instead of an independent 512 KiB figure; the 512 KiB figure keeps its other job — the
canonical byte bound of one *group* — as its own named constant. One bound moves, from 512 KiB to
4 MiB; the group bound, the object bound, the page size, the pragmas, the cadence and every work
counter stay where they are.

Today the two numbers are one constant used for two different jobs: `capacities.batch_bytes` bounds
the wave, and `cas::selection` also compares it against a *group's* canonical bytes
(`selection.rs:78`). The wave is therefore bounded 8x below the transaction it opens, and the row
pays a step for every 512 KiB: 302,406,480 canonical bytes come out as ~596 waves of ~507 KB each,
against the reference implementation's **73 transactions for the same 25,158 objects and
302,182,831 bytes** — the number section 3 of the handoff calls the most interesting one.

## The pin consequence, declared before the number exists

**`pipeline.commits` is pinned at 800 and this treatment moves it by design**, exactly as round 3's
wave treatment moved it from 17,378 (that round's pre-registration declared the same consequence
before its run, and its own `T3b`/`T3c` pair is the procedure this round follows). The first run of
this arm is therefore expected to fail `g1.o3-pinned-counters` on that one pin; `pipeline.commits`
is then **read from that receipt** — a count, not a duration — `tests/golden/expected.tsv` is
re-pinned once, the harness is rebuilt, and the covering run is taken once. Both receipts stay on
disk and both are reported, the red one as red.

**No other pin may move.** `pipeline.batches` 3, `bindings` 10100, `chain_objects` 382,
`content_bytes` 301171810, `content_objects` 24863, `declared_content_bytes` 300000000,
`declared_directories` 100, `declared_files` 10000, `inserted` 25245, `largest_batch_bindings` 4096,
`metadata_objects` 382, `objects_emitted` 67, `reused` 0 and `digest:filesystem_root`
`1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847` must all reproduce exactly: they
are what says the *work* did not change.

## Prediction, in the instruments' own units

| instrument | D4b | predicted | derivation |
| --- | ---: | ---: | --- |
| waves | ~596 | **~75–95** | 302,406,480 bytes / (4 MiB − 1) = 73 for the byte bound, plus the anchor's 256 KiB chunks and the 512-object bound on ~7,900 tiny objects |
| `diag_wave_ns` | 112.7 ms | **15–30 ms** | the wave's locator query and presence seed, ~189 us per wave, over 8x fewer waves |
| `diag_validate_ns` | 52.1 ms | **8–15 ms** | one collision check per wave, ~65 us per wave |
| `diag_commit_total_ns` | 402.7 ms | **320–375 ms** | only the fixed per-transaction part leaves: the probe's own two-point fit puts it at ~121 us of the ~503 us per commit, so ~725 fewer commits is ~88 ms; the per-page part stays |
| `diag_begin_ns` | 11.5 ms | ≤ 3 ms | one `BEGIN IMMEDIATE` and one watermark read per wave |
| `operation_work_ns` | 1821.0 ms | **1550–1700 ms** | the sum above |
| CPU user+system | 1835.1 ms | 1700–1830 ms | the same work in fewer steps |

The predicted row movement is larger than the ~250 ms this machine drifts in 13 minutes, so the row
is allowed to carry it; the count-driven instruments (`diag_wave_ns` over the wave count, the wave
count itself, `diag_commit_total_ns` over the commit count) carry it if the row does not. One sample
per arm after the re-pin; no confirmation run.

## What would refute it

1. The wave count does not fall below 150 (the byte bound is not what was bounding the wave).
2. `diag_wave_ns` >= 60 ms, or `diag_commit_total_ns` >= 400 ms, or `operation_work_ns` >= 1821.0 ms.
3. Any work counter or the root digest moves; the row is not PASS once `pipeline.commits` alone is
   re-pinned.
4. Any of the 33 test binaries fails - in particular `multi_writer` (the step is now ~8x wider, and
   the multi-writer rule is that the transaction never outlives the step), `memory_bounds` (the
   pending batch now holds 4 MiB of canonical objects), `cas_reuse`, `delta_payload`,
   `pack_watermark`, `visibility`, `persistence_failure`, `pack_locator`.

## What is not claimed

The reference's 73 transactions are a *hypothesis generator*, not evidence: different workspace,
different boundary, and its receipt carries no seal this side can be matched against. This round
does not compare itself to the reference; it removes a bound the reference's own step count suggests
is arbitrary, and it is measured against this lane's own control.
