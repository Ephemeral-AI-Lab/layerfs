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
| A6  | 1,000 scattered one-byte edits              | B01/B04        | total ms, storage growth     |
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
FastCDC copy reduction.

| Cell                  | Baseline (pre-change)                                  | After R3+R5+copy reduction                 |
| --------------------- | ------------------------------------------------------ | ------------------------------------------ |
| A1/A2 write           | 17.9 MiB/s cold; 13.4 MiB/s rewrite                    | 46.3 MiB/s cold (2.6x); 37.2 MiB/s rewrite |
| A3/A4 read            | 43.8 MiB/s cold; 44.4 MiB/s warm                       | 121.2 MiB/s cold (2.8x); 121.3 MiB/s warm  |
| A5/A6 small edit      | fallback O(file): ~6.2 s/edit                          | fallback O(file): ~3.0 s/edit              |
| Small random read     | 2.85 ms/op (4 KiB, cold)                               | 1.19 ms/op (2.4x)                          |
| A7/B5 materialization | 33.8 / 33.7 MiB/s                                      | 66.0 / 64.7 MiB/s (~2x)                    |
| B1/B2/B3 (1 MiB)      | 13.8 / 44.2 / 44.8 MiB/s                               | 24.3 / 133.3 / 133.8 MiB/s                 |
| B4 (100 edits)        | 8.95 s (~89 ms/edit)                                   | 4.55 s (~46 ms/edit)                       |
| C1/C2 mixed           | 5.99 / 5.65 s                                          | 2.55 / 2.29 s                              |
| Storage overhead      | 7.19% fresh data (A1); +4.7 MiB rewrite delta; B1 ~52% | unchanged (dedup and quotas preserved)     |

Statement counts on the write path dropped ~4.3x (A1: 12,472 -> 2,880); the
100,001-entry closure test reports 4,655 reconciliation statements (0.0465 per manifest
entry).

Deviations recorded by the harness:

- A6's 1,000 scattered one-byte edits land on leaves larger than the bounded path-copy
  window (default 100 MiB file), so the engine uses the streamed fallback (~3-9 s per
  edit, O(file)). The harness caps the A6 edit loop at 8 s and records `completedEdits`
  / `scaledEdits`; the cell marks `pass: false` when the full count cannot finish. R1
  (M3 scope) targets sub-10 ms path-copy edits.
- The original anchors (26 MiB/s write, 61 MiB/s read) were measured on a 64 MiB
  buffered materialized profile; the matrix cells use the 100 MiB streamed profile, so
  the refreshed anchors above are the measured matrix numbers.

These anchors refresh with each change; the artifacts under `tests/performance/` keep
the history (`artifacts-baseline/` pre-change, `artifacts-r3/` after R3, and
`artifacts/` after the full change set).
