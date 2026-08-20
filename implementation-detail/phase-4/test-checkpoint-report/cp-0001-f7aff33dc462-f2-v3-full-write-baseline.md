# CP-0001 — accepted F2-v3 100-MiB durable full-write baseline

Status: `BASELINE`
Date: 2026-08-20
Experiment mode: `baseline`
Primary operation: `durable-full-write`
Total experiment wall: 23 seconds externally observed; 21 seconds runner wall
Retained artifact bytes: `166,885`
Transient databases deleted: `yes`

## 1. Checkpoint identity

| Field | Value |
|---|---|
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| Parent checkpoint | `None: first fast-lane checkpoint` |
| Tested source commit | `f7aff33dc46237ed06a94858c9a3b71bc02e82c8` |
| Tested source tree | `d54de4c2aeb87969cd9c9e2863e75b476a8c6886` |
| Benchmark source SHA-256 | `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158` |
| Release executable SHA-256 | `68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0` |
| Fixture SHA-256 | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| Fixture manifest SHA-256 | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| Runner SHA-256 | `26cc8311b50484f8a45e83afab95e8e7f6100bd7a1aab3d41847f3ee16b1aec0` |
| Raw JSONL SHA-256 | `467e6fa80487e1d9769ee55a828db057d29e210e7aa1eeda49bf20315b2482af` |
| Rust / Cargo | `1.96.0 / 1.96.0` |
| SQLite | `3.51.0` |
| Host | `Apple M3 Max, 38,654,705,664 bytes RAM` |
| OS | `macOS 26.4.1 (25E253)` |

The tested executable is the already accepted and audited F2-v3 release
binary. Commit `f7aff33dc462…` is the first clean commit containing its exact
benchmark source bytes. The current reporting worktree is dirty at later HEAD
`d781173a08ab…`; none of those dirty WP4-M profile-campaign bytes were compiled
or timed in this checkpoint.

## 2. Changed variable and purpose

There is no candidate change. CP-0001 establishes the live fast-lane control
on the current host using the accepted F2-v3 binary and retained fixture.

Explicitly unchanged:

```text
FastCDC 8/16/32 KiB and ordered sequence
raw ChunkId and canonical ObjectId
K64/F64 mapping profile
canonical CAS and mapping bytes
workspace root, transition, delta, and closure
SQLite FULL + DELETE
one transaction and one COMMIT
```

The historical accepted F2-v3 durable median was `659.593 ms`. The new live
median is lower on the same binary and fixture. That difference is host/state
drift, not a code improvement; future candidates compare against CP-0001 in
adjacent balanced runs.

## 3. Test contract

This first baseline deliberately runs only the controlling 100-MiB full-write
workflow. The 1/10-MiB cases and separate edit/warm/fresh/read/reopen operations
are not silently inferred from it.

```text
fixture: S1-100, 104,857,600 bytes
schedule: one warmup followed by five measured rows
profile: K64/F64
operation: full
qualification: accepted F2 construction proof
row databases: isolated temporary state, freshly checked, then deleted
```

Timer equations, checked by every row:

```text
durable full write
  = canonical CAS + mapping construction
  + pre-COMMIT proof consumption
  + SQLite publication and COMMIT

complete lifecycle
  = durable full write
  + fresh-connection head validation
  + fresh full scrub
  + reconstruction
  + range verification
```

The nested reopen is a fresh SQLite connection in the same process. It is not
reported as fresh-process or cold-disk materialization.

## 4. Correctness and authority result

| Check | Result |
|---|---|
| Rows / warmups / measured | `6 / 1 / 5` |
| Row statuses | `PASS` in 6/6 |
| Source fingerprint | `bb883eec…bab7`, equal in 6/6 |
| Ordered CDC references | `5,284`, equal in 6/6 |
| File root | `2d41c27f…25d0`, equal in 6/6 |
| Transition | `ba15fd20…6412`, equal in 6/6 |
| Ordered closure | `d6aac6e4…d54a`, equal in 6/6 |
| Publication | `Committed` in 6/6 |
| Transactions / COMMITs | `1 / 1` in 6/6 |
| COMMIT dispatch/return/success | `1 / 1 / 1` in 6/6 |
| Durable and lifecycle timer equations | `PASS` in 6/6 |
| Terminal exact Q | `0` in 6/6 |
| Fresh scrub/reconstruction/ranges | `PASS` in 6/6 |
| Malformed/tamper/provenance regression | `NotRerun: inherited from accepted sealed F2-v3 checkpoint` |

CP-0001 introduces no engine change, so the already sealed F2-v3 focused/full
correctness evidence is reused rather than rerun.

## 5. Primary performance baseline

| Metric | Median | Min | Max | Spread |
|---|---:|---:|---:|---:|
| Durable 100-MiB full write | **578.367 ms** | 567.372 ms | 692.541 ms | 125.169 ms |
| Durable throughput | **172.901 MiB/s** | derived from median | derived | diagnostic |
| Mapping/construction | 489.540 ms | 478.803 ms | 587.896 ms | 109.092 ms |
| Pre-COMMIT proof | 0.055 ms | 0.043 ms | 0.068 ms | 0.025 ms |
| SQLite publication + COMMIT | 90.021 ms | 77.789 ms | 104.577 ms | 26.788 ms |
| Complete lifecycle | **1,288.611 ms** | 1,276.687 ms | 1,396.256 ms | 119.570 ms |
| Lifecycle throughput | **77.603 MiB/s** | derived from median | derived | diagnostic |

Component medians can come from different rows and are not added as an exact
same-row equation. Each individual row carries and passes the exact timer
equations.

Planning gaps from the live median:

```text
to 500.000 ms / 200 MiB/s: 78.367 ms, requiring 13.550% reduction
to 333.333 ms / 300 MiB/s: 245.034 ms, requiring 42.366% reduction
```

The live median is 12.315% lower than the historical accepted `659.593-ms`
measurement on the same executable/fixture identity. It does not retroactively
relabel that historical evidence.

## 6. Nested protected observations

| Operation | Median | Min | Max | Classification |
|---|---:|---:|---:|---|
| Fresh-connection reopen/head validation | 0.928 ms | 0.784 ms | 1.065 ms | observed; same process |
| Fresh complete scrub | 270.740 ms | 268.341 ms | 287.124 ms | observed |
| Reconstruction | 431.837 ms | 428.165 ms | 443.625 ms | observed after scrub |
| Range verification | 0.756 ms | 0.680 ms | 0.793 ms | observed |
| Warm materialization | `NotRun` | — | — | requires separate operation |
| Fresh-process materialization | `NotRun` | — | — | requires separate child process |
| Same-count edit | `NotRun` | — | — | requires separate operation |
| Count-changing edit | `NotRun` | — | — | structural guard only |

## 7. Direct counters

All measured rows agree exactly:

| Counter | Value |
|---|---:|
| Source bytes | 104,857,600 |
| Chunks / references | 5,284 / 5,284 |
| File leaves / branches | 83 / 2 |
| Objects created | 5,372 |
| Canonical bytes written | 105,291,554 |
| Mapping bytes rewritten | 365,262 |
| SQL query / execute / total calls | 5,581 / 5,379 / 10,960 |
| Row BLOB writes | 10,748 |
| Transactions / COMMITs | 1 / 1 |

The reused F2-v3 binary predates the final W/D implementation and emits
`U_WD`; W/D are therefore `Unavailable` in CP-0001 rather than reconstructed
from unrelated counters.

## 8. Resource and storage baseline

| Metric | Median | Min | Max |
|---|---:|---:|---:|
| User CPU | 1.12 s | 1.11 s | 1.12 s |
| System CPU | 0.21 s | 0.20 s | 0.21 s |
| Exact Q high-water | 55,325 B | 55,325 B | 55,325 B |
| Terminal exact Q | 0 B | 0 B | 0 B |
| Maximum RSS | 93,421,568 B | 93,306,880 B | 93,536,256 B |
| Peak footprint | 92,225,968 B | 92,111,280 B | 92,324,248 B |
| Post logical/apparent store | 109,269,024 B | equal | equal |
| Post allocated store | 117,510,144 B | equal | equal |
| Allocated-store delta | 117,485,568 B | equal | equal |

Physical-media I/O, VFS byte totals, xSync wall, true journal peak, and true
temporary-file peak remain `Unavailable` in this reused executable.

## 9. Decision

Decision: `BASELINE`

CP-0001 is the live control for the fast 100-MiB durable-full-write loop. It is
not an optimization claim and it does not select or promote a mapping profile.
All six rows pass identity, publication, lifecycle, and terminal-Q checks; the
entire run completed in 23 seconds and retained only compact JSON/report data.

Next action:

```text
Add 1/10-MiB fixture support and separate edit/materialization/read/reopen
operations without changing the engine algorithm, then create the remaining
baseline checkpoints. Keep durable full write as the first optimization target.
```

## 10. Reproduction and compact evidence

Runner:

```text
implementation-detail/phase-4/test/run-phase4-fast.sh
SHA-256: 26cc8311b50484f8a45e83afab95e8e7f6100bd7a1aab3d41847f3ee16b1aec0
```

Command:

```bash
implementation-detail/phase-4/test/run-phase4-fast.sh \
  CP-0001 \
  target/wp4m-f2-construction-proof-k64-20260819-v3/binaries/phase4_create_edit_benchmark-f2-v3-candidate \
  target/wp4m-f2-construction-proof-k64-20260819-v3/S1-100.source \
  target/wp4m-f2-construction-proof-k64-20260819-v3/wp4m-retained-fixture-manifest.json \
  implementation-detail/phase-4/test-checkpoint-report/cp-0001-f7aff33dc462-f2-v3-full-write-baseline.jsonl \
  5
```

Raw evidence:

```text
file: cp-0001-f7aff33dc462-f2-v3-full-write-baseline.jsonl
SHA-256: 467e6fa80487e1d9769ee55a828db057d29e210e7aa1eeda49bf20315b2482af
rows: 6
bytes: 157,702
```

No database, generated fixture, copied authority, copied expectation, output
file, or release executable is retained in this checkpoint-report directory.
