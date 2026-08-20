# CP-0004 — production edit/materialization/read/reopen baseline

Status: `BASELINE`
Date: 2026-08-20
Experiment mode: `baseline`
Total experiment wall: 102 seconds externally observed; 101 seconds runner wall
Retained artifact bytes: `425,742`
Transient databases and fixtures deleted: `yes`

## 1. Identity

| Field | Value |
|---|---|
| Parent checkpoint | `CP-0003 REVISE` |
| HEAD while built | `d781173a08ab4092eb539c3a0870056e6c6a77ff` |
| Compiled-source diff SHA-256 | `6380eefe31fd7c80ff279aa7371e567a60daa4552a21bedb77974f7556d34dc7` |
| Benchmark source SHA-256 | `159d473534af104a9228ca749ae046feb171a7a8d56cfd578acede65ad870376` |
| Release executable SHA-256 | `c2441d89a6d7b8c425f1e20a40373d79f154b88c297c1853d63d6079969966ec` |
| Runner SHA-256 | `8753d76e18b5c828b066ccd6bc1300374811ff12b20f5ce207a3eff0efc849be` |
| Raw JSONL SHA-256 | `8202fb6b65604a59cc879588eaef371d9f3b72599e87357b14961d69412454db` |

CP-0004 uses the exact CP-0003 executable. Only the prospective schedule
changed: invalid 1/10-MiB same-count labels were removed before this root
started. No result, threshold, engine algorithm, or fixture was changed.

## 2. Schedule

```text
1 MiB:  one smoke each for warm/fresh materialization, ranges, reopen
10 MiB: one smoke each for warm/fresh materialization, ranges, reopen
100 MiB:
  same-count edit: one warmup + three measured
  +1 edit: one structural guard
  warm materialization: three measured
  fresh-process materialization: three measured
  range suite: five measured
  reopen: five measured
```

The run returned 29/29 strict PASS rows. Every row used K64/F64, exact fixture
and prepared-state hashes, the expected root/transition/closure for its
operation, and terminal Q zero. Mutations used one transaction/COMMIT; read-only
operations used zero transactions/COMMITs.

## 3. 100-MiB user-workflow baseline

| Operation | Samples | Median | Min | Max | Spread | Interpretation |
|---|---:|---:|---:|---:|---:|---|
| Same-count middle edit | 3 | **8.137 ms** | 7.524 ms | 9.125 ms | 1.601 ms | durable edit acknowledgement |
| Warm materialization | 3 | **419.766 ms** | 419.365 ms | 420.448 ms | 1.084 ms | 238.228 MiB/s |
| Fresh-process materialization | 3 | **424.291 ms** | 421.648 ms | 425.336 ms | 3.687 ms | 235.687 MiB/s |
| Seven boundary range probes | 5 | **0.786 ms** | 0.724 ms | 0.793 ms | 0.069 ms | routing latency, not bulk throughput |
| Fresh-process reopen/head ready | 5 | **2.441 ms** | 2.208 ms | 2.679 ms | 0.471 ms | process startup excluded |

Each runner invocation is a new process. `materialize-fresh` times a fresh
SQLite connection/head read plus full reconstruction but excludes executable
process-launch wall. `materialize-warm` performs one untimed reconstruction on
the same connection and times the second. Its external CPU total therefore
includes both reconstructions; the reconstruction wall and phase counters
isolate the measured second pass.

Warm materialization is only 1.066% faster than fresh-process materialization
on this host. That is a baseline observation, not a cache-design claim.

## 4. Materialization components

| Component | Median | Min | Max |
|---|---:|---:|---:|
| Fresh connection/head portion | 2.636 ms | 2.490 ms | 2.864 ms |
| Fresh reconstruction portion | 421.630 ms | 419.134 ms | 422.445 ms |
| Fresh combined materialization | 424.291 ms | 421.648 ms | 425.336 ms |
| Warm reconstruction | 419.766 ms | 419.365 ms | 420.448 ms |

Full materialization authenticates and streams the complete raw 100-MiB file.
It is a production read path, not part of durable write acknowledgement.

## 5. Count-changing structural guard

The one 100-MiB middle `+1` row reports:

```text
durable edit wall:       454.291 ms
ratio to CP-0002 write:   78.883%
suffix references:         2,642
suffix bytes:         52,377,184
suffix objects:                86
mapping bytes rewritten:  185,915
exact Q high-water:        50,631 B
```

This is the expected fixed-ordinal suffix algorithm. It is retained as an
honest structural alarm and is not treated as a local-edit throughput metric.

## 6. Range routing detail

The range operation currently measures seven deterministic correctness probes:

| Probe | Returned bytes | Median wall | Objects authenticated | Canonical bytes authenticated |
|---|---:|---:|---:|---:|
| First byte | 1 | 0.126 ms | 4 | 22,286 |
| Cross chunk | 2 | 0.136 ms | 5 | 40,797 |
| Leaf boundary | 2 | 0.165 ms | 6 | 48,260 |
| Branch boundary | 2 | 0.163 ms | 7 | 45,639 |
| Last byte | 1 | 0.086 ms | 4 | 17,597 |
| Zero range | 0 | 0.014 ms | 1 | 129 |
| EOF range | 0 | 0.008 ms | 1 | 129 |

The combined `0.786-ms` metric is useful for authenticated routing regression.
It is not representative of a large sequential read. A later small checkpoint
should add one 1-MiB returned range before any read-throughput optimization.

## 7. Small-size smoke

| Operation | 1 MiB | 10 MiB |
|---|---:|---:|
| Warm materialization | 4.066 ms | 41.513 ms |
| Fresh materialization reconstruction | 4.389 ms | 41.928 ms |
| Range probe suite | 0.393 ms | 0.518 ms |
| Reopen/head ready | 2.642 ms | 2.215 ms |

These are single correctness/scaling smokes, not distribution claims.

## 8. Resources

| Operation | Max exact Q | Max RSS | Median user CPU | Median system CPU |
|---|---:|---:|---:|---:|
| Same-count edit | 2,222,803 B | 15,286,272 B | 0.65 s | 0.08 s |
| `+1` edit | 50,631 B | 12,681,216 B | 0.84 s | 0.09 s |
| Warm materialization | 34,243 B | 18,464,768 B | 1.21 s | 0.09 s |
| Fresh materialization | 34,243 B | 15,908,864 B | 0.82 s | 0.07 s |
| Range suite | 31,484 B | 7,913,472 B | 0.43 s | 0.04 s |
| Reopen | 17,127 B | 7,946,240 B | 0.43 s | 0.04 s |

Every row returns exact Q to zero. Same-count edit's roughly 2.12-MiB Q is the
bounded changed-region/oracle/expected-result working set, independent of the
100-MiB source size.

## 9. Decision

Decision: `BASELINE`

CP-0004 establishes separate production workflow baselines without repeated
full lifecycle work:

```text
durable write:               575.906 ms  (CP-0002)
durable same-count edit:       8.137 ms
warm full materialization:   419.766 ms
fresh full materialization:  424.291 ms
authenticated range suite:     0.786 ms
reopen/head ready:              2.441 ms
```

The next optimization remains 100-MiB durable full write because it has a
75.906-ms gap to 200 MiB/s. Edit, materialization, ranges, and reopen become
protected-operation baselines. Do not optimize all operations in one change.

## 10. Compact evidence

```text
raw rows: 29
raw bytes: 418,897
raw SHA-256: 8202fb6b65604a59cc879588eaef371d9f3b72599e87357b14961d69412454db
runner wall: 101 seconds
temporary residue: none
```

No database, fixture, authority, expectation, materialized output, or release
executable is retained in the checkpoint-report directory.
