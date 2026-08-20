# WP4-M F4-A — accepted F2-v3 residual attribution

## Prospective diagnostic preregistration

- Date: 2026-08-20.
- Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` only, branch
  `codex/empty-worktree`.
- Documentation checkpoint / tree:
  `83d085bd80e82ae22b4a9766f2fc8aed03501fb8` /
  `6bbf29f0a6d51a0571e63728c4182057a8b49c30`.
- Starting status at checkpoint: clean.
- Authority: the user's prospective amendment permits retained F2-v3 to enter
  this diagnostic-only residual attribution after terminal F3 `NO-GO`. It does
  not amend any historical F3 result or authorize F3-v4.
- Terminal F4-A result: exactly `GO`, `NO-GO`, or `REVISE`.

F4-A does not implement an optimization. It does not start F5/F6, select or
change a profile, change schema or durability, resurrect insertion grouping,
build a carrier, integrate production, or commit.

### Entry custody

| Item | Frozen value |
|---|---|
| accepted F2-v3 live source SHA-256 | `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158` |
| accepted F2-v3 executable SHA-256 | `68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0` |
| retained source bytes / SHA-256 | `104,857,600` / `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained CDC references | `5,284` |
| runtime engine | system SQLite `3.51.0`, `FULL`, `DELETE`, `temp_store=FILE`, `mmap_size=0` |
| terminal F3 D1-v2 manifest | `f70dd3c87fcecab22fa2af8e5d6bc48cad06bf478581733ca25cfe9c66a9b905` |
| terminal F3 D1-v2 attestation | `84dc10435fdeefcc6ec4823c86f6d604a412e7c5db3036d364bcd36298fb3a61` |
| F4-A artifact root | `target/wp4m-f4a-residual-attribution-k64-20260820-v1` |

The diagnostic source must be accepted F2-v3 plus observations only. The
retained implementation, SQL, schema, profile, transaction, one COMMIT,
durability, identities, canonical bytes, and publication semantics remain
unchanged. The live benchmark source is restored byte-for-byte before the
terminal disposition.

## Question and threshold

F4-A asks whether the accepted 100-MiB full-create mapping or standalone
COMMIT contains one isolated mechanism with at least `33 ms` of directly
removable durable-capture budget. `60–80 ms` is the preferred strategic range.

For this diagnostic, directly removable budget means:

```text
median directly timed mechanism wall
- mandatory replacement work
- measured observer cost attributable to that mechanism
```

The mechanism must be semantically removable from the retained path, not just
expensive. Required source read, CDC, canonical construction, identity work,
B-tree mutation, pager writes, and durability are not removable merely because
their gross wall is large. A nested or inseparable composite is not isolated.

`GO` requires all of:

1. valid evidence and exact phase equations in all rows;
2. one named, non-composite mechanism whose median directly removable budget
   is at least `33 ms`;
3. at least four of five measured rows independently have at least `33 ms` for
   that same budget; and
4. a separately preregisterable one-variable removal that preserves every
   protected semantic and durability gate.

A `33–<60 ms` result is a low-margin `GO`; `60–80 ms` or more is preferred.
If evidence is valid but no mechanism passes, the result is `NO-GO`. A timer,
custody, semantic, equation, or observer defect is `REVISE`, never `GO`.

## Non-overlapping attribution contract

The required user-facing families are `source`, `CDC`, `hash`, `encode`,
`bind`, `copy`, `VDBE`, `pager`, and `VFS`. Parent timers are never added to
children. Observer and unattributed residual are reported only to close the
equation and are never GO mechanisms.

### Mapping

- `source`: wall inside a timed `Read::read` wrapper.
- `CDC`: `FastCdc::scan` wall minus source-read wall and chunk-callback wall;
  it includes the existing gear scan and scanner-owned chunk-buffer copy.
- `hash`: disjoint raw `ChunkId`, construction source/sequence, and canonical
  `ObjectId` hash intervals.
- `encode`: disjoint raw canonical framing plus mapping leaf, branch, file
  root, workspace root, and transition encoding intervals.
- `bind`: statement acquisition plus non-canonical-BLOB parameter binding.
- `copy`: canonical-BLOB transient-bind calls and explicit row-materialization
  copies, reported separately. A bind-call interval is an upper bound on the
  copy inside that call and cannot by itself establish removable copy wall.
- `VFS`: direct named proxy-VFS callback wall, split by file kind and callback.
- `VDBE` / `pager`: public system SQLite does not expose separate wall clocks,
  and Apple ships its private frames stripped. Record VM steps and pager
  hits/misses/writes/spills separately, but publish their non-overlapping wall
  as `VDBE+pager = step/reset wall - nested VFS wall`. Individual VDBE and
  pager wall remain explicitly `Unavailable`; the composite cannot issue GO.
- `observer` and `residual`: direct timer/status overhead and checked remainder.

The mapping equation is:

```text
mapping parent
= source + CDC + hash + encode + bind + copy
 + VDBE+pager + VFS + observer + residual
```

Every subtraction is checked and residual must be nonnegative. The report
also preserves raw subcomponents and counts; rolled-up families are calculated
once, not added to their children.

### Standalone COMMIT

The direct boundary is the existing `execute_batch("COMMIT")` dispatch to
return. Publication preparation before dispatch remains a separately reported
parent and is not charged to standalone COMMIT.

`source`, `CDC`, `hash`, `encode`, `bind`, and `copy` must be zero inside the
standalone boundary. Direct VFS callback wall is subtracted once. The remaining
wall is the honest `VDBE+pager` composite, with VM/pager counters attached and
individual VDBE/pager wall `Unavailable` for the reason above:

```text
standalone COMMIT
= VDBE+pager + VFS + observer + residual
```

VFS logical calls/bytes and callback wall are not physical-media bytes. xSync
wall is durable work, not removable budget merely because it dominates.

## Diagnostic implementation and checks

Reuse the sealed D1 proxy-VFS and raw-binding observation patterns after
removing all F3/grouping branches. Add only bounded scalar counters/timers and
fixed callback aggregates. No event log, source-sized state, dependency,
sidecar, table, worker, async path, feature flag in retained production, or
private SQLite replacement is allowed.

Before release rows:

1. prove the diagnostic source differs from accepted F2-v3 only by observation;
2. run the smallest direct timer-equation self-check;
3. run the affected F2 construction, commit-observation, transaction/error,
   and release self-tests;
4. run the full workspace tests and Clippy with warnings denied;
5. build one release diagnostic executable with `debug_assertions=false`;
6. freeze source, executable, dependency/runtime, fixture, base, and command
   hashes; and
7. prepare every empty database/authority/expectations triple outside timers.

Any observer that changes identity, work counters, pager/storage equations,
transaction/COMMIT count, schema, endpoint bytes, or residue invalidates the
root. Timer calls and wrapper wall are measured; no correction is applied
post hoc unless it was defined before rows.

### Pre-row implementation correction

Frozen after focused/full/static/release-self-test and one non-admissible smoke
row, before any scheduled row:

- the reused proxy VFS adds main-database and main-journal buckets; its forwarding
  behavior, default profile, and SQLite runtime remain unchanged;
- `VDBE+pager` remains the only honest wall composite and remains ineligible for
  `GO`;
- canonical transient-bind wall is only a bind-call upper bound, not an isolated
  memcpy lower bound, so it is ineligible for `GO` without a separate direct
  copy observation;
- the only directly timed, semantically removable candidate in this diagnostic
  is explicit row-materialization copy; and
- five optimized 200,000-interval `Instant::now`/`elapsed` probes define a
  conservative timer-observer ceiling. The maximum complete probe wall is
  `11,244,750 ns`; raw component values are not corrected, and this ceiling is
  subtracted before any removable-budget claim.

This correction changes no decision threshold, schedule, implementation,
SQLite call, schema/profile/durability value, or accepted F2-v3 semantic path.

## Frozen schedule and acceptance

Run exactly one uncounted warmup and five measured full-create rows, each in a
fresh process against a separately prepared physical byte copy of the accepted
empty base. Source/fixture preflight occurs outside the measured child. Label
source cache `warm_or_unknown_after_manifest_preflight` and store state
`fresh_logical_store_cache_unknown`.

Every row must preserve the accepted root, transition, ordered closure, 5,284
references, 5,372 objects, 105,291,554 canonical bytes, 365,262 mapping bytes,
FULL/DELETE, one transaction/COMMIT/publication, exact Q terminal zero, pager
and storage equations, fresh scrub, reconstruction, ranges, and no final
journal/WAL/SHM. Preserve raw JSONL, commands, environment, `/usr/bin/time -l`,
source/binary custody, tests, analysis, and a complete manifest.

Report all five values plus median/min/max/spread. F4-A is not an A/B,
throughput acceptance, profile campaign, or F5 decision campaign. A `GO`
authorizes only a separately requested and preregistered one-mechanism F4-B;
it does not authorize implementing that mechanism in this task.

## Terminal result — VALID / NO-GO

The frozen one-warmup plus five-measured schedule completed 6/6 rows with zero
child, JSON, semantic, timer-equation, or VFS failures. Raw JSONL SHA-256 is
`5241b106a9d1d841e124d73ff247f2abadb2bf27759ef54d62a3ab3af3eb212f`;
analysis SHA-256 is
`ee30693a372e0a3bca6a9831055683e2be80e24191012b20eea7d3615ad5a3b2`;
storage-audit SHA-256 is
`ce406ac832c85c49f707726d3f071fd4ff9c4e7d1667115bb3ee876ac2c6f48b`.

### Overall 100-MiB phase statistics

These are component-wise statistics from the five measured rows. A component
median can come from a different row than the durable or lifecycle median, so
the median cells in this table are descriptive and are not added together.

| Overall phase | Median ms | Min ms | Max ms | Spread ms |
|---|---:|---:|---:|---:|
| Mapping / full-create construction | **524.111750** | 510.077916 | 529.233000 | 19.155084 |
| Pre-COMMIT proof consumption | **0.063334** | 0.057583 | 0.073916 | 0.016333 |
| SQLite publication + COMMIT | **112.324834** | 100.076125 | 118.225791 | 18.149666 |
| Standalone COMMIT dispatch-to-return | **112.144334** | 99.942750 | 118.072916 | 18.130166 |
| **Durable 100-MiB create** | **636.836792** | 620.337125 | 639.101584 | 18.764459 |
| Fresh reopen / head validation | **1.098750** | 0.992667 | 1.622625 | 0.629958 |
| Fresh complete scrub | **280.250583** | 276.154459 | 282.183500 | 6.029041 |
| Reconstruction | **438.069792** | 437.153333 | 453.276541 | 16.123208 |
| Range verification | **0.725958** | 0.706125 | 0.995417 | 0.289292 |
| **Complete lifecycle** | **1,357.130500** | 1,340.645584 | 1,374.244833 | 33.599249 |

The row that defines the median durable result reconciles exactly:

```text
mapping / construction                 524.438042 ms
pre-COMMIT proof                         0.073916 ms
SQLite publication + COMMIT            112.324834 ms
                                         -------------
durable 100-MiB create                  636.836792 ms
```

That row reports `157.026103 MiB/s`. The controlling diagnostic summary is:

```text
100-MiB durable create                  636.836792 ms
diagnostic durable throughput           157.026 MiB/s
primary target                          500.000000 ms
remaining diagnostic-to-target gap      136.836792 ms
required reduction from diagnostic      21.49%
```

The durable median is composed approximately as:

| Durable phase | Same-row wall ms | Share of durable wall |
|---|---:|---:|
| Mapping / construction | 524.438042 | 82.35% |
| Pre-COMMIT proof | 0.073916 | 0.01% |
| Publication + COMMIT | 112.324834 | 17.64% |
| **Durable total** | **636.836792** | **100.00%** |

The independently selected post-COMMIT component medians are:

```text
fresh reopen / head validation            1.098750 ms
fresh complete scrub                     280.250583 ms
reconstruction                           438.069792 ms
range verification                         0.725958 ms
                                            -------------
component-median post-COMMIT sum          720.145083 ms
```

The complete-lifecycle median is `1,357.130500 ms`, equivalent to approximately
`73.685 MiB/s`. The independently selected post-COMMIT sum is not subtracted
from or added to another median as an exact same-row equation.

### Mapping / construction breakdown

The following component-wise medians partition the mapping parent without
adding a parent to its children. The hash subtotal is shown for interpretation
and is not added again to the three hash rows.

| Mapping component | Median ms | Min ms | Max ms | Approx. mapping share | Interpretation |
|---|---:|---:|---:|---:|---|
| Source reads | 16.468330 | 13.698211 | 20.227606 | 3.14% | required input read |
| CDC-exclusive scan | 128.723024 | 127.892139 | 130.443292 | 24.56% | required scan; copy not isolated |
| Raw `ChunkId` hash | 95.185147 | 94.345443 | 98.851331 | 18.16% | required raw identity |
| Construction source/sequence hash | 89.067215 | 88.664111 | 94.402197 | 16.99% | required fixture/CDC qualification |
| Canonical `ObjectId` hash | 96.068155 | 94.792248 | 96.558815 | 18.33% | required canonical identity |
| **All disjoint hash intervals** | **280.146626** | 277.801802 | 289.812343 | **53.45%** | subtotal; three distinct outputs |
| Canonical + mapping encoding | 3.161540 | 3.087421 | 8.325569 | 0.60% | required bytes |
| Prepare + non-copy bind | 1.385969 | 1.278903 | 1.432414 | 0.26% | below gate |
| Transient canonical bind-call upper bound | 2.745299 | 2.637017 | 3.039594 | 0.52% | not isolated memcpy |
| Explicit row-materialization copy | 0.000000 | 0.000000 | 0.000000 | 0.00% | removable lane absent |
| Mapping VDBE+pager composite | 48.853618 | 44.464730 | 54.997544 | 9.32% | inseparable and ineligible |
| Mapping direct VFS | 24.281657 | 21.272064 | 25.482168 | 4.63% | required database/journal work |
| Proof/bookkeeping | 1.306847 | 1.191882 | 1.419357 | 0.25% | required bookkeeping |
| Diagnostic observer | 6.908268 | 6.716400 | 7.273044 | 1.32% | instrumentation, not product work |
| Unattributed residual | 4.543490 | 4.464644 | 4.666210 | 0.87% | nonnegative, not isolated |

The row that defines the median mapping result reconciles exactly:

```text
source read                              17.355134 ms
CDC exclusive                           129.590189 ms
hash total                              281.154969 ms
  raw ChunkId                            95.356407 ms
  construction                           89.379403 ms
  canonical ObjectId                     96.419159 ms
encode                                    3.223793 ms
bind                                      1.432414 ms
transient-copy upper bound                3.039594 ms
explicit materialization copy             0.000000 ms
VDBE+pager                               50.737500 ms
direct VFS                               24.281657 ms
proof/bookkeeping                         1.414377 ms
observer                                  7.273044 ms
residual                                  4.609079 ms
                                           -------------
mapping parent                           524.111750 ms
```

### Standalone COMMIT breakdown

| COMMIT component | Median ms | Min ms | Max ms | Interpretation |
|---|---:|---:|---:|---|
| Standalone dispatch-to-return | **112.144334** | 99.942750 | 118.072916 | parent |
| VDBE+pager composite | 18.199272 | 12.062865 | 19.113344 | inseparable; below gate |
| Direct VFS callbacks | 93.030990 | 87.879885 | 102.238877 | required write/sync work |
| Main-database writes | 48.194103 | 46.334094 | 50.105503 | required final pager writes |
| Main-database FULL sync | 42.817791 | 41.367958 | 51.944583 | required durability fence |
| Main-journal sync | 0.133209 | 0.092958 | 0.157874 | below gate |
| Main-journal write | 0.020334 | 0.003209 | 0.025417 | negligible |
| Main-journal close | 0.041041 | 0.021500 | 0.145375 | negligible |

The row that defines the median standalone COMMIT result reconciles exactly:

```text
VDBE+pager                               19.113344 ms
direct VFS                               93.030990 ms
  main-database writes                   49.929115 ms
  main-database FULL sync                42.817791 ms
  journal read/write/sync/close           0.284084 ms
                                           -------------
standalone COMMIT                        112.144334 ms
```

### Accepted-checkpoint comparison

| Metric | Accepted F2-v3 | F4-A diagnostic |
|---|---:|---:|
| Mapping | approximately 492.777 ms | 524.111750 ms |
| Standalone COMMIT | approximately 168.426 ms | 112.144334 ms |
| Durable 100-MiB create | **659.593 ms** | **636.836792 ms** |
| Durable throughput | **151.609 MiB/s** | **157.026 MiB/s** |
| Complete lifecycle | **1,353.841 ms** | **1,357.130500 ms** |
| Complete-lifecycle throughput | **73.864 MiB/s** | approximately **73.685 MiB/s** |

F4-A is an observer-heavy single-arm attribution diagnostic, not a balanced F5
acceptance A/B. Its phase timings shifted substantially and current APFS
allocation state differs from the older checkpoint. Therefore the diagnostic
durable result is not a retained improvement and does not replace or relabel
the accepted F2-v3 checkpoint.

The gross CDC and three hash lanes are distinct required outputs, not removable
passes. The large COMMIT VFS wall is exact required pager-write and FULL-sync
work under the frozen schema/profile/durability. The VDBE+pager values are
inseparable composites and ineligible. The transient-copy observation is only
an API-call upper bound and is below the threshold. The only eligible directly
timed removal, explicit materialization copy, is zero in all five rows.

No isolated mechanism therefore supplies 33 ms directly removable budget in
four of five rows, before or after the conservative 11.244750-ms timer ceiling.
The terminal result is `NO-GO`; F4-B, F5, and F6 remain ineligible.

All six rows preserve the accepted root/transition/closure, 5,284 references,
5,372 objects, 105,291,554 canonical bytes, 365,262 mapping bytes, 26,676 dirty
writes, 6,675 spills, FULL/DELETE, one transaction/COMMIT/publication, complete
post-COMMIT verification, and terminal Q zero. Six databases pass integrity
with one schema hash and no journal/WAL/SHM. A fresh immutable accepted-binary
parity row matches the current APFS allocated-byte observation, proving host
allocation drift rather than observer-induced storage change.

The private tests pass 56/56, the full workspace and Clippy `-D warnings` pass,
and the release self-test passes. The live benchmark source is restored
byte-for-byte to accepted F2-v3 SHA-256
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
No optimization, F5/F6 work, profile/schema/durability change, F3 grouping,
carrier, production integration, or commit occurred.

### Terminal custody

The sealed F4-A evidence root contains 63 manifested payloads plus the manifest.
Manifest SHA-256 is
`23e3a74d5015342fda59aad5f6046de488cca6a5d688e9f0e2db8514e2dcfe07`.
An external read-only walk verifies exact modes, bytes, hashes, listed/actual
equality, zero symlinks, and zero owner-writable entries. Its attestation
SHA-256 is
`646d3adaa44d4b23837e13027dcfd887c18bf84b47126f1010c10df54c4513dd`.
The sealed final report / read-only audit SHA-256 values are
`41497414d94b45c55825573d91cec3f765d9043e2a054bb2ce5fc33774a08715` /
`27ca2ccf8473a007f55bc20774a65b16bdeb059b945d053229a3dae558aee46e`.
