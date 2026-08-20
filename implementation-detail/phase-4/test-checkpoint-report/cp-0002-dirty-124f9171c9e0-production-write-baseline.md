# CP-0002 — production-shaped 1/10/100-MiB durable-write baseline

Status: `BASELINE`
Date: 2026-08-20
Experiment mode: `baseline`
Primary operation: `durable-full-write`
Total experiment wall: 30 seconds externally observed; 28 seconds runner wall
Retained artifact bytes: `225,616`
Transient databases and fixtures deleted: `yes`

## 1. Checkpoint identity

| Field | Value |
|---|---|
| Repository / branch | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` / `codex/empty-worktree` |
| Parent checkpoint | `CP-0001` |
| HEAD while built | `d781173a08ab4092eb539c3a0870056e6c6a77ff` |
| Compiled-source diff SHA-256 | `124f9171c9e04842c0570666e46918dc2483499060e42f7f07ac1a78f4bd8687` |
| Benchmark source SHA-256 | `be7fdf56922c68776946f8d2afa45fe149c6514c477788e8a61f7db44678f6fc` |
| Release executable SHA-256 | `3d3181ed7135b8441cb2fa70598f9f0b16b3b1749f8a9395050ba1de276e374b` |
| Runner SHA-256 | `443bbf4b6db72238d3b9b376c8e2abdbed246c0a24e543cd41bd4404196bdd39` |
| Raw JSONL SHA-256 | `c0f4168bbf3121d1c73add549147893f3a8645b610889a84caf3e70ff39478d9` |
| Rust / Cargo / SQLite | `1.96.0 / 1.96.0 / 3.51.0` |
| Host / OS | `Apple M3 Max, 38,654,705,664 bytes RAM / macOS 26.4.1 (25E253)` |

The worktree contains unrelated user-owned documentation and research changes.
The diff hash above is the complete binary diff of the compiled Phase-4 core
and benchmark source paths only. The executable hash is the final authority for
the measured program. No unrelated path is attributed to the measurement.

## 2. One harness change

CP-0002 adds a private K64/F64 fast lane with two explicit validation scopes:

```text
capture-only:
  source -> CDC/CAS/mapping -> construction proof -> durable COMMIT -> report

complete-roundtrip:
  capture-only -> fresh connection -> scrub -> reconstruction -> ranges
```

It also adds deterministic 1- and 10-MiB fixtures. The engine algorithm,
canonical representation, schema, profile, transaction, and durability mode do
not change. Core codec/error fixes discovered by WP4-M remain; the fast CLI
cannot select K59/F101, K256/F256, or a directory candidate.

## 3. Schedule and fixture custody

```text
1 MiB:   one capture-only smoke row
10 MiB:  one capture-only warmup + three measured capture-only rows
100 MiB: one capture-only warmup + five measured capture-only rows
100 MiB: one complete-roundtrip checkpoint row
```

| Fixture | Bytes | File SHA-256 | Raw fingerprint | CDC refs | CDC sequence |
|---|---:|---|---|---:|---|
| S1-1 | 1,048,576 | `4a3acf60…a2a` | `f79de600…9cf8` | 53 | `6a1d02f7…f1c1` |
| S1-10 | 10,485,760 | `0c7a6693…430e` | `e40db05d…2449` | 531 | `982e9922…f3ed` |
| S1-100 | 104,857,600 | `63b3695b…eff4` | `bb883eec…bab7` | 5,284 | `5bb376c3…f994` |

Fixtures, preparation, source SHA-256, and independent expectations are outside
the measured interval. Each row uses an isolated prepared database whose
database, authority, and expectations hashes are checked before execution.

## 4. Correctness and scope result

| Check | Result |
|---|---|
| Planned / returned rows | `12 / 12` |
| Row statuses | `PASS` in 12/12 |
| Purposes / operations | `performance_baseline / write` only |
| Capture-only post-COMMIT work | exactly zero in all 11 capture-only rows |
| Complete round trips | exactly one, at 100 MiB |
| Per-size root/transition/closure | one exact identity per size |
| 100-MiB retained F2-v3 identity | exact match |
| Transactions / COMMITs | `1 / 1` in every row |
| COMMIT dispatch/return/success | `1 / 1 / 1` in every row |
| Durable/lifecycle timer equations | `PASS` in every row |
| Terminal exact Q | `0` in every row |
| Temporary residue | none |

The 100-MiB round trip independently reopens, scrubs, reconstructs, and checks
ranges. Capture-only rows do none of those operations after COMMIT.

## 5. Durable-write baseline

| Size | Measured samples | Median | Min | Max | Spread | Median throughput |
|---|---:|---:|---:|---:|---:|---:|
| 1 MiB | 1 smoke | 7.877 ms | 7.877 ms | 7.877 ms | 0 | 126.954 MiB/s |
| 10 MiB | 3 | 63.483 ms | 63.327 ms | 65.236 ms | 1.909 ms | 157.523 MiB/s |
| 100 MiB | 5 | **575.906 ms** | 574.064 ms | 584.949 ms | 10.885 ms | **173.639 MiB/s** |

The 100-MiB result is stable enough to serve as the production-write baseline:
the five measured rows span only 10.885 ms. Compared with CP-0001's same-host
`578.367-ms` median, the difference is `-0.425%` and is not a performance claim.

Planning gap:

```text
200 MiB/s target: 500.000 ms
current median:    575.906 ms
remaining gap:      75.906 ms
required reduction: 13.180%
```

## 6. 100-MiB phase and resource baseline

| Metric | Median | Min | Max |
|---|---:|---:|---:|
| Mapping/construction | 487.605 ms | 481.021 ms | 500.818 ms |
| Pre-COMMIT proof | 0.050 ms | 0.046 ms | 0.056 ms |
| SQLite publication + COMMIT | 89.294 ms | 84.079 ms | 94.835 ms |
| User CPU | 0.54 s | 0.54 s | 0.54 s |
| System CPU | 0.13 s | 0.13 s | 0.13 s |
| Maximum RSS | 93,421,568 B | 93,339,648 B | 93,503,488 B |
| Exact Q high-water / terminal | 88,093 B / 0 B | equal | equal |
| W / D | 210,493,394 B / 0 B | equal | equal |

The Q increase from historical CP-0001 reflects the newer exact-Q/report/W/D
ownership accounting retained from WP4-M, not source-sized resident state. It
remains below 0.1 MiB and returns to zero on every exit.

## 7. Exact work by size

| Size | Objects | Canonical bytes | Mapping bytes | References |
|---|---:|---:|---:|---:|
| 1 MiB | 57 | 1,053,105 | 3,840 | 53 |
| 10 MiB | 543 | 10,529,551 | 36,888 | 531 |
| 100 MiB | 5,372 | 105,291,554 | 365,262 | 5,284 |

The objects, canonical bytes, mapping bytes, roots, transitions, closure, and
CDC sequence are deterministic across every row of the same size.

## 8. One complete round trip

The single 100-MiB checkpoint row reports:

```text
durable write:       578.522 ms
fresh connection:      1.034 ms
complete scrub:       270.631 ms
reconstruction:       428.084 ms
range verification:     0.685 ms
complete lifecycle: 1,278.956 ms
terminal Q:                  0
```

This row proves the capture-only samples did not trade away reopen,
authentication, reconstruction, or range semantics.

## 9. Decision

Decision: `BASELINE`

CP-0002 becomes the controlling production-shaped durable-write baseline:

```text
100 MiB: 575.906 ms / 173.639 MiB/s
```

It is not an optimization claim. It validates the fast separation and freezes
1/10/100-MiB identities while retaining one full round trip. The next engine
candidate must compare adjacent control/candidate capture-only rows against
this executable and predict at least part of the 75.906-ms target gap.

## 10. Reproduction and retention

```bash
implementation-detail/phase-4/test/run-phase4-fast-v2.sh \
  CP-0002 \
  target/release/phase4_create_edit_benchmark \
  implementation-detail/phase-4/test-checkpoint-report/cp-0002-dirty-124f9171c9e0-production-write-baseline.jsonl
```

```text
raw rows: 12
raw bytes: 218,428
raw SHA-256: c0f4168bbf3121d1c73add549147893f3a8645b610889a84caf3e70ff39478d9
```

No SQLite database, fixture, authority, expectation, release executable, or
materialized output is retained in this checkpoint directory.
