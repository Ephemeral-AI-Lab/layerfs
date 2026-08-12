# M2 mini benchmark

Engine-level storage benchmark for the accepted Milestone 2 SQLite foundation. It
complements the [`release-benchmarks.md`](./release-benchmarks.md) B01-B09 plan: the
mini benchmark measures the storage engine directly (no FUSE, no page cache, no base
filesystem), runs in under two minutes, and provides the cold/warm baseline that the
M3/B01-B05 gates and the M9 DOFS comparisons build on.

## Harness

- Script: `tests/performance/mini-bench.mjs`
- Output: one machine-readable JSON artifact per cell under
  `tests/performance/artifacts/` using the `efs-benchmark-result-v1` result schema (see
  [`release-benchmarks.md`](./release-benchmarks.md) section 16); raw trial data is
  retained alongside the summary
- Runtime budget: under 120 seconds per full matrix run (the harness enforces an overall
  115 s wall budget and a bounded A6 edit budget)
- Driver profile: file-backed Node SQLite, WAL, `synchronous=FULL`, default 16 MiB
  cache, zero mmap; fixtures are deterministic with recorded seeds (seed `0x5eed5eed`);
  the A group uses one 100 MiB file, B uses 100 x 1 MiB files, C uses a 4 MiB mixed
  workspace

Every cell records the same metric set:

| Metric                        | Source                     |
| ----------------------------- | -------------------------- |
| wall ms / MiB per second      | harness timing             |
| database + WAL growth (bytes) | `driver.physicalStorage()` |
| managed-resident peak (bytes) | admission controller peak  |
| statement count               | transaction counters       |
| storage overhead percent      | `(db - payload) / payload` |

Each cell runs in two phases:

- **cold**: fresh database, empty caches, first access
- **warm**: same database, caches populated, repeated access (dedup hits apply)

## A. Big file - 1 x 100 MiB

| ID  | Workload                                    | Plan benchmark | Key metrics                  |
| --- | ------------------------------------------- | -------------- | ---------------------------- |
| A1  | cold first write                            | B04            | MiB/s, db growth, overhead % |
| A2  | rewrite identical content                   | B04            | MiB/s, storage delta (dedup) |
| A3  | cold sequential read                        | B03            | MiB/s (verification-bound)   |
| A4  | warm sequential read                        | B03            | MiB/s (cache-trust path)     |
| A5  | one-byte edit at start / middle / end       | B01            | ms per edit, storage delta   |
| A6  | 500 scattered one-byte edits                | B01/B04        | total ms, storage growth     |
| A7  | cold materialization (reopen then read all) | B05            | ms, managed peak             |

## B. Small files - 100 x 1 MiB

| ID  | Workload                              | Plan benchmark | Key metrics       |
| --- | ------------------------------------- | -------------- | ----------------- |
| B1  | cold write all files                  | B04            | total ms, MiB/s   |
| B2  | cold read all files                   | B03            | total ms          |
| B3  | warm re-read all files                | B03            | total ms          |
| B4  | one-byte edit per file (100 edits)    | B01            | total ms, db size |
| B5  | reopen and read all (materialization) | B05            | ms                |

B4 is the direct head-to-head against the Aug 2026 agentfs prototype session, which
measured 25.0 ms and 6.3 MiB for this workload with 64 KiB CAS chunks.

## C. Messy workspace - mixed script

| ID  | Workload                                                                                    | Plan benchmark | Key metrics                               |
| --- | ------------------------------------------------------------------------------------------- | -------------- | ----------------------------------------- |
| C1  | mixed script: 1-byte, 4-KiB, 512-KiB edits; new-file writes; range and full reads; rewrites | B01-B05        | total wall ms, peak memory, final db size |
| C2  | same script on the warm database                                                            | B05            | ms, storage delta vs C1                   |
| C3  | storage evolution per phase vs native copy baseline                                         | -              | db size per phase, % of native            |

## Current reference anchors (measured 2026-08-11, Windows x64, Node 24.11.1)

Baseline before the M2 improvements (M2 candidate `2e06a44`, pure-JS SHA-256), then the
measured outcome after R3 (host-injected native hashing) + R5 (statement batching) + the
FastCDC copy reduction. All values below are transcribed directly from the checked-in
artifacts (`tests/performance/artifacts-baseline/` and `tests/performance/artifacts/`);
single-trial runs carry 10-20% run-to-run variance, so anchors are only meaningful to ~2
significant figures.

| Cell                  | Baseline (pre-change)                                   | After R3+R5+copy reduction                 |
| --------------------- | ------------------------------------------------------- | ------------------------------------------ |
| A1/A2 write           | 17.9 MiB/s cold; 13.4 MiB/s rewrite                     | 44.2 MiB/s cold (2.5x); 36.3 MiB/s         |
| A3/A4 read            | 43.8 MiB/s cold; 44.4 MiB/s warm                        | 118.1 MiB/s cold (2.7x); 118.6 MiB/s       |
| A5 one-byte edit      | 18.6 s for 3 (fallback ~6.2 s/edit)                     | 9.4 s for 3; per-edit [4.66, 4.53, 0.23] s |
| Small random read     | 2.85 ms/op (4 KiB, cold)                                | 1.17 ms/op (2.4x)                          |
| A7/B5 materialization | 33.8 / 33.7 MiB/s                                       | 67.9 / 64.6 MiB/s                          |
| B1/B2/B3 (1 MiB)      | 13.8 / 44.2 / 44.8 MiB/s                                | 21.5 / 129.3 / 130.3 MiB/s                 |
| B4 (100 edits)        | 8.95 s (~89 ms/edit)                                    | 4.44 s (~44 ms/edit)                       |
| C1/C2 mixed           | 5.99 / 5.65 s                                           | 2.28 / 2.33 s                              |
| Storage overhead      | 7.19% fresh data (A1); +4.47 MiB rewrite delta; B1 ~52% | unchanged (dedup and quotas preserved)     |

Statement counts on the write path dropped ~4.3x (A1: 12,472 -> 2,880; A1 is the first
cell so its counts are clean per-cell numbers); the 100,001-entry closure test reports
4,655 reconciliation statements (0.0465 per manifest entry).

Deviations recorded by the harness:

- A6's 1,000 scattered one-byte edits land on leaves larger than the bounded path-copy
  window (default 100 MiB file, leaves ~16 MiB vs the 14.2 MiB window), so the engine
  uses the streamed fallback (O(file)). The harness caps the A6 edit loop at 8 s and
  records `completedEdits` / `scaledEdits` (2 of 1,000 completed in the final run); the
  cell marks `pass: false` when the full count cannot finish. R1 (M3) targets this with
  bounded local reconnection.
- A5's per-edit split shows both paths: the first two edits fall back (~4.6 s each), the
  final-byte edit lands in the small last leaf and takes the path-copy path (230 ms).
  The table above quotes the total, not a single-path mean.
- Read cells write a small amount: streamed reads (A3/A4/A7) grow the database by +53.6
  KiB each from read-lease bookkeeping (acquire/release of the pinned read lease),
  independent of the engine state.
- `peakManagedResidentBytes` is reliable for write cells but stale for stream-read
  cells: the filesystem observer event fires at `readStream()` construction, not at
  stream consumption, so A3/A7 record construction-time snapshots and A4/B2/B3 inherit
  the previous cell's cumulative peak. Treat the peak column for read cells as a ceiling
  proxy; fixing this (per-cell admission peak emitted at stream close) is M3
  housekeeping before the R7 gate.
- The original anchors (26 MiB/s write, 61 MiB/s read) were measured on a 64 MiB
  buffered materialized profile; the matrix cells use the 100 MiB streamed profile, so
  the refreshed anchors above are the measured matrix numbers. OS and SQLite page caches
  are warm after the untimed setup writes that precede A3/A6, so the "cold" label means
  cold engine cache, not cold OS cache (the reopen cells A7/B5 measure the truly cold
  path at ~66 MiB/s).

These anchors refresh with each change; the artifacts under `tests/performance/` keep
the history (`artifacts-baseline/` pre-change, `artifacts-r3/` after R3, and
`artifacts/` after the full change set). The M3 gates defined in
[`m3-improvements.md`](./m3-improvements.md) re-base these anchors.

## Historical M3 tuning run (2026-08-13, Windows x64, Node 24.11.1)

M3.1 (R7 read-path batching), M3.2 (R1 bounded local reconnection), and M3.3 (async
WebCrypto write hashing) land on the M2 anchors. The A-group values below are the
rounded medians from an intermediate five-trial fresh-database tuning run. They are kept
as history and are not the accepted result; the exact current samples are retained under
`docs/evidence/m3/benchmarks/`.

| Cell                    | M2 anchor           | M3 measured              | M3 gate       |
| ----------------------- | ------------------- | ------------------------ | ------------- |
| A3 cold read            | 118.1 MiB/s         | 275.7 MiB/s              | >=250 MiB/s   |
| A4 warm read            | 118.6 MiB/s         | 833.5 MiB/s              | >=250 MiB/s   |
| A4/A3 warm ratio        | 1.00x               | 3.02x                    | >=1.2x        |
| Read txs / 100 MiB      | 402                 | 55                       | <=55          |
| Read stmts / 100 MiB    | 1,787               | 84                       | <=250         |
| A6 small reads          | 1.17 ms/op          | 0.86 ms/op               | <=1.0 ms/op   |
| A5 canonical 3 of 100   | 9.4 s               | 0.051 s for canonical 3  | <1 s for 3    |
| A6 scattered edits      | 2 in 8 s            | 500 in 9.83 s, pass=true | 500 in <=20 s |
| A1 write                | 44.2 MiB/s          | 73.7 MiB/s               | -             |
| A7/B5 materialization   | 67.9 / 64.6 MiB/s   | 114.9 / 98.1 MiB/s       | -             |
| B4 (100 one-byte edits) | 4.44 s              | 2.13 s (~21 ms/edit)     | -             |
| B2/B3 (1 MiB reads)     | 129.3 / 130.3 MiB/s | 168.0 / 528.2 MiB/s      | -             |
| C1/C2 mixed             | 2.28 / 2.33 s       | 2.29 / 2.30 s            | -             |

M3.3 workerd write-path hashing (workerd parity check `write-path-hashing`): 399 MiB/s
async WebCrypto (16-way batch concurrency) vs 72.5 MiB/s pure-JS baseline, a 5.51x
result meeting the >=300 MiB/s and >=1.5x write-path gates.

### Harness profile changes (M3)

- The driver profile raises the SQLite page cache from 16 MiB to a finite 128 MiB: the 2
  MiB read windows span ~520 4 KiB pages per pull, and the M2-era 16 MiB cache left
  every pull page-cache cold. The former 64 MiB profile measured 245-248 MiB/s, while a
  96 MiB profile admitted an isolated 169.8 MiB/s eviction outlier; neither retained a
  stable per-trial margin above the unchanged 250 MiB/s gate.
- The filesystem runtime profile sets `maxCacheBytes: 128 MiB` and
  `maxManagedResidentBytes: 192 MiB` so the warm-read gate can hold the whole 100 MiB
  fixture in the engine content cache. The Node benchmark profile uses a finite 128 MiB
  SQLite page cache, enough for the approximately 115 MiB database page set. Exact
  accepted cold and warm values remain in the retained evidence artifacts; the 250 MiB/s
  gate is unchanged.
- Housekeeping: `--trials=N` repeats the complete A group in a new isolated database for
  every trial and retains p50/p95/p99/min/max/mean plus every raw sample. Cold cells
  recreate the engine content cache after identical untimed setup; no SQLite, filesystem
  cache, or mutable fixture state crosses a trial boundary. Every trial verifies its
  final digest. Artifacts also retain the commit, clean-worktree marker, fixture digest,
  per-cell admission peak, and exact database-isolation profile.

### M3 final status

- The A6 acceptance gate is 500 scattered one-byte edits in <=20 s. Fresh-database
  five-trial validation completes all 500 in about 9.8 s at the median and records
  `pass: true`; every trial performs real edits rather than replaying already-set bytes.
- A5 retains the three representative offsets as its <1 s compatibility gate, while
  every measured trial executes 100 guaranteed-different one-byte edits and reports the
  per-operation distribution. After the canonical start/middle/end edits, the remaining
  calls toggle the same middle byte, matching the normative repeated-overwrite profile.
  Each trial starts from identical bytes and verifies its separately computed post-edit
  digest. Exact current timings live in accepted evidence.
- The A6 small-reads gate (<=1.0 ms/op) measures about 0.86 ms/op at the fresh-database
  five-trial median. Every 4 KiB result is compared byte-for-byte with the deterministic
  fixture and followed by a complete digest verification.
- M3.3 grants the streaming write pipeline's trusted-digest put (the pipeline computes
  digests from its own detached chunk copies; read paths still authenticate every
  object). The in-transaction re-verification stays for every other put path.
- Staging membership writes (`appendBatch`) are batched per binding budget: generic and
  streamed paths retain one existing-member lookup plus one multi-row insert per chunk,
  while a fresh local-rebuild batch skips only its redundant lease-local membership
  probe. The local insert still requires `changes === expected`, and immutable
  object/node backing validation and chain accounting remain enabled. The local path
  also accounts source-authenticated/count-only and freshly CAS-validated objects
  directly during reconciliation, avoiding transient object queue rows. The latest A6
  artifact records 24,467 statements (about 49/edit); the local allocation-range
  reservation removed two efs_meta updates per edit without changing the two-transaction
  durable shape. Wall time remains disk/transaction floor bound and varies materially by
  single trial.
- Durable persistence is single-transaction when the projected row, binding-byte, and
  reconciliation-unit budgets fit (`runPersistenceSteps`): for small edits the whole
  staging persistence (begin, puts, membership, root, reconciliation, seal) commits in
  one write transaction instead of ~11, collapsing the WAL/fsync floor to one commit.
  Large closures and tight-budget profiles keep the established per-step transactions;
  the reconciliation loop still commits per bounded call in the fallback shape. B4's 100
  one-byte edits dropped from ~4.0 s to ~2.6 s (~26 ms/edit) and A5's three edits from
  ~0.65 s to ~0.58 s; the 65,537-entry closure and the `maxFinalTransactionRows: 64`
  profiles behave exactly as before (covered by the storage suite).
- The multi-row `putEntriesBatch`/`putLevelRecordsBatch` binding envelopes are bounded
  by `maxQueryBatchSize` rows, which on workerd (`maxBindings` 100) exceeds the
  per-statement binding budget for 4-bindings-per-row inserts; the reconciliation
  leaf-edge batching was made explicitly binding-bounded (R5b) in M3, the remaining
  write-side sites are a documented follow-up.

## D. Concurrency sweep - 100 x 1 MiB files at 1/5/10/20 concurrent operations

The D group (`--cell D1`) writes, reads, and one-byte-edits the 100 small files in
batches of 1, 5, 10, or 20 concurrent operations (`D1-write-cN`, `D2-read-cN`,
`D3-edit-cN`); each D1 cell writes fresh content so the writes stay cold, and the sweep
runs on its own 1 GiB managed envelope (20 concurrent write pipelines each reserve ~18
MiB). Measured 2026-08-12, Windows x64, Node 24.11.1:

| Cell                | c1 (serial)            | c5              | c10             | c20             |
| ------------------- | ---------------------- | --------------- | --------------- | --------------- |
| D1 write (100 MiB)  | 4.685 s (21.3 MiB/s)   | 9.806 s (10.2)  | 12.022 s (8.3)  | 12.221 s (8.2)  |
| D2 read (100 MiB)   | 0.802 s (124.7 MiB/s)  | 1.246 s (80.3)  | 1.250 s (80.0)  | 1.224 s (81.7)  |
| D3 edit (100 edits) | 0.916 s (~9.2 ms/edit) | 1.634 s (~16.3) | 2.383 s (~23.8) | 1.748 s (~17.5) |

Finding: **concurrency does not help on this engine** - every level is 1.5-2.6x slower
than the serial baseline. The SQLite driver is a single connection whose synchronous
transactions serialize (writes under BEGIN IMMEDIATE, reads under BEGIN DEFERRED), so
concurrent operations only overlap the async gaps (hashing, admission, scheduling) while
adding contention: WAL/fsync and statement-cache pressure, content-cache thrash, and 20
x ~18 MiB write-pipeline managed memory. The serial cells (B1/B2/B3/B4) remain the best
single-stream anchors; the D group records the engine's concurrent behavior for the
DOFS/M9 comparisons. A focused `D3-edit-c100` run launches all 100 edits in one
`Promise.all` against the same database (one batch, no coalescing): 2.096 s, 200
transactions, and 6,300 statements. Its artifact is
[`D3-edit-c100.json`](../../tests/performance/artifacts/D3-edit-c100.json).

### Edit-path improvements (post-investigation)

The 2026-08-11 subagent investigation found the durable-path-copy re-chunked the whole
authenticated leaf for equal-length edits, while the bounded local rebuild re-converges
within one FastCDC chunk (the gear stream reconverges ~32 bytes past the edit). The
routing now attempts the bounded local rebuild first (with the host hasher threaded
through the local rebuild instead of the pure-JS fallback) and keeps the path-copy as
its fallback. B4 dropped from ~2.6 s to ~2.1 s and A5 from ~0.58 s to ~0.52 s; the
65,537-entry closure and tight-budget profiles behave exactly as before (covered by the
storage suite).
