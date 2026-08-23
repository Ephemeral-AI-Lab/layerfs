# G5 fast-iteration benchmark contract

Status: **prospective method; no measurement may start before G4 terminal
custody and a versioned G5 lane preregistration are frozen**.

## Objective

Provide rapid causal feedback after each change and a complete stateful
promotion gate without allowing the test matrix to grow into an hours-long
campaign.

The benchmark exposes only three modes:

```text
phase4_history_benchmark --dry-run
phase4_history_benchmark --screen
phase4_history_benchmark --gate
```

Do not add another mode until a measured gap proves it necessary.

## Mode contract

### `--dry-run`

Performs no benchmark operation. It must:

- freeze/verify branch, HEAD, status, source, diff, executable, fixture,
  operation-log, method, and analyzer hashes;
- prove the result root absent;
- construct and compare the exact chronology;
- verify deterministic edit logs and expected manifests;
- identify the global fail-fast lock and any current holder;
- print exact operation counts, size/revision allocation, and timer budgets;
- exit before base preparation or timing on any mismatch.

### `--screen`

Used after an implementation change:

- one candidate and its exact control;
- only the changed lane plus one global protected sentinel;
- 1-MiB and 10-MiB fixtures first;
- at most one 100-MiB falsifier when smaller inputs cannot answer the
  hypothesis;
- complete wall `<=20 s`;
- direct work, CPU, RSS, Q, descriptor, storage, identity, and error evidence;
- exact retain/revise/revert result.

A passing screen is exploratory evidence, not promotion.

### `--gate`

Used once after the candidate is frozen:

- one append-only control/candidate campaign;
- selected 1/10/100-MiB longitudinal workflows;
- no row deletion, replacement, reordering, selective rerun, or historical
  import;
- complete lock-to-terminal wall `<=120 s`;
- primary and independent recomputation;
- exact cleanup, manifest, and read-only terminal verification.

Do not run `--screen` immediately before `--gate` on unchanged frozen source
merely for reassurance.

## Complete `--gate` wall budget

```text
lock, custody, and measured preflight           <=  5 s
isolated base preparation and copy custody      <= 20 s
1-MiB long-history segment                      <= 15 s
10-MiB mechanism-history segment                <= 15 s
100-MiB primary sentinels                       <= 25 s
focused concurrency, cancellation, and faults  <= 10 s
verification, two analyzers, cleanup, manifest <= 20 s
reserve                                         <= 10 s
                                                   -----
complete campaign                               <=120 s
```

The wrapper starts at fail-fast lock acquisition and ends only after the
terminal verification is durably written and verified. Build and full static
validation are separately timed, but no build occurs inside the measured
wrapper and no static result may rescue a failed measured gate.

## Selected stateful gate

The exact chronology is versioned per G5 lane, but its coverage must include:

### 1-MiB segment

- 1,000 same-byte hot-spot edits;
- 1,000 deterministic random same-size edits;
- 1,000 A/B alternations/reverts;
- compact count-changing/append-truncate history if it fits the frozen
  segment budget;
- selected historical roots and reachability/GC checks.

### 10-MiB segment

- 100 same-size random edits;
- 100 count-changing edits;
- materialization every tenth revision;
- reopen every tenth revision;
- selected old/current range and full reads.

### 100-MiB segment

- 100 same-size edits;
- 10 count-changing edits;
- final current-root reconstruction;
- first/full materialization;
- same-open full-byte read with optional digest cost separated;
- clone/no-op and incremental patch;
- reopen followed by head, range, first edit, and first materialization;
- one controlled host-buffer-cold approximation or one explicit
  `Unavailable` record.

### Focused small-fixture semantics

- reader/writer and same-target materializer conflict;
- cancellation before/after publication boundaries;
- lost acknowledgement and fresh reconciliation;
- substitution-safe cleanup;
- corrupt/missing/wrong-role/malformed mapping errors;
- pinned-reader versus GC safety.

## Fixed micro-operation batching

Sub-10-ms operations must not use a single observation to support a strict
relative regression claim. Prospectively freeze a small deterministic batch,
for example 64 operations, and compare total matched work:

```text
control_total_wall = sum(control operation 1..64)
candidate_total_wall = sum(candidate operation 1..64)

ratio = candidate_total_wall / control_total_wall
```

Retain per-operation p50/p95/max and exact work parity. Control and candidate
use the same offsets, roots, cache-state class, preparation, and operation
count. The batch size and gate are fixed before observation; no retry or new
absolute cap may rescue a failed result. A 64-operation batch remains well
below one second for the current range/edit/reopen/clone class and is not a
broad multi-pair campaign.

## Incremental verification

Avoid a full reread after every edit.

Every operation verifies:

- returned/current root;
- expected length, classification, and route;
- transaction/COMMIT and checked work counters;
- exact Q terminal state;
- descriptor/temp/residue state.

At frozen revision checkpoints, additionally capture latency distributions,
work/resource/storage snapshots, selected ranges, and expected roots. Perform
complete reconstruction/digest verification at sequence end and only at
prospectively selected intermediate checkpoints.

This preserves byte/identity authority without turning `N` small edits into
`N` full-file reads.

## Complex coverage without slow iteration

The state and fault matrix may be complex. The execution shape must remain
fast. `<20 s` and `<=120 s` are complete campaign ceilings, not per-row,
per-operation-family, or per-subagent allowances.

Every implementation campaign must:

- build each release executable once outside the measured wrapper;
- hash fixtures, databases, sidecars, expectations, source, diff, and
  executable once in fail-fast preflight;
- keep one long-lived child per stateful arm instead of paying process launch,
  source hashing, database preparation, and seed admission per micro-row;
- prepare a deterministic root/edit sequence once and give control/candidate
  the identical operation log;
- check root, transition, length, route, transaction/COMMIT, direct work, and
  operation-local Q after every operation;
- use checkpoint ranges/digests and one final complete verification instead of
  rereading the complete file after every edit;
- run the full semantic/fault matrix on 1/10-MiB fixtures and admit 100 MiB
  only for a direct performance/scaling mechanism;
- retain compact numeric timing/work sidecars and one sequence record rather
  than verbose prose per operation; and
- stop on the first hard failure while preserving the failed attempt.

Verification is tiered:

| Tier | Included in fast G5 work | Treatment |
|---|---|---|
| Changed trust/projection mechanism | trusted/verified transition, corruption distinction, seed rotation, pending-chain composition, projection lag, SQLite contention, native faults, repeated resource bounds | mandatory now |
| Protected shared paths | full create, same-open edits, reopen/head, range, reconstruction, full materialization/fallback | compact sentinels only |
| G5 closure | retained history, random/hotspot/revert, historical roots, multiple readers/one writer, shutdown/restart, reachability | G5-3 gate, not first screen |
| Later optimization | append/truncate specialization, arbitrary middle native count change, persistent cross-process seed, multi-file projection, GC, controlled-cold, 500-MiB scale, hostile writers, cross-platform backend | excluded from current candidate |

No operation is admitted merely to make the matrix look complete. It must
either falsify the changed mechanism, protect a shared path, or satisfy the
current stage's closure rule. Performance evidence never transfers implicitly
across a different operation shape, size, route, cache state, process
boundary, or concurrency state.

For projection work, the fast gate uses three separate stateful sequences:

1. exact-every-root same-size operations for additive latency and seed
   rotation;
2. latest-following same-size operations for no-lag pipeline behavior; and
3. a precommitted count-changing enqueue storm that deterministically outruns
   the worker, followed by one 100-MiB final-fallback/convergence sentinel.

The third sequence is the controlling coalescing mechanism. The same-size
projection stage is already faster than the same-size canonical producer and
may generate no natural queue pressure.

## Fast test ladder

Do not run full static closure after every edit. Advance through the smallest
test that can falsify the current change:

1. compile the touched target and run only the existing focused tests extended
   for the changed trust/projection function;
2. run the runner/analyzer schedule assertion and `--dry-run` with zero measured
   rows;
3. run one `<20 s` mechanism screen;
4. repair or revert before expanding if any focused/screen gate fails;
5. after candidate source is frozen, run workspace tests, clippy with warnings
   denied, formatting, tracked/untracked diff/whitespace, and manifest custody
   once; and
6. run the `<=120 s` gate only after static closure passes.

Passing full workspace/static commands are not repeated during unchanged-source
evidence work. A runner/analyzer-only repair reruns its own focused checks and
terminal custody, not unrelated product tests. Semantic faults use the
smallest fixture that exercises the boundary; a 100-MiB fixture is never used
merely to test an error enum or cleanup branch.

## Raw schema

A long sequence produces one compact append-only sequence record containing:

- sequence ID, fixture/profile/edit-log hashes;
- operation count and first failing index if any;
- checkpoint records at 1/10/100/1,000;
- total, p50, p95, and maximum latency;
- direct work and storage deltas;
- CPU/RSS/Q/descriptor/buffer/queue high-water;
- final root/digest/reachability result;
- cleanup and residue result.

Compact numeric per-operation timing sidecars may be retained for independent
recomputation. Do not duplicate owned explanatory prose in every row.

## Fail-fast rules

Stop the current attempt immediately on:

- identity, byte, topology, exact-error, authority, durability, or old-or-new
  failure;
- unexpected fast/fallback route;
- counter overflow or impossible subtraction;
- terminal Q, descriptor, permit, lock, temp, journal, seed, or cache leak;
- capacity/storage bound violation;
- campaign bucket or complete-wall overrun;
- schedule/custody/analyzer disagreement; or
- unsupported state relabeled as observed.

Preserve the failed attempt. Repair the smallest shared root cause and create a
new versioned attempt. Never rerun unchanged source until noise produces a
PASS, weaken a gate after observation, or expand the timeout.

## What the fast gate cannot prove

The `<=120 s` gate does not claim:

- multi-day endurance;
- production cache hit rates;
- thousands of clients;
- multi-terabyte GC;
- true controller/device-cold behavior;
- rare hardware power-loss semantics; or
- cross-platform/application integration.

Use deterministic simulation, formal invariants, small-fixture fault tests,
and later dedicated integration/soak evidence for those questions. They must
not slow the core candidate loop unless the candidate directly changes that
mechanism.

## G5-1 H11 terminal amendment

H11 v2 used the separately frozen [v2 preregistration](../../../../implementation-detail/phase-4/experiments/g5-foundation-h11/v2/PREREGISTRATION-v2.md), not the prospective `<=120 s` full-gate ceiling in this document. Its observed wall was `8.551146875 s` under the hard `<=20 s` timing gate, but final audit sets `H11_REVISE_EXACT_BLOCKER` because benchmark-owned allocations were absent from Q. Timing success cannot waive that resource failure.

The prospective G5 two-sample latency materiality test is:

```text
candidate_sum * 100 > control_sum * 105
AND candidate_sum - control_sum >= 2,000,000 ns
```

It applies only to latency disposition. Bytes/identities, topology, exact errors, SQL/query/row/BLOB/work, transaction/COMMIT count, authority, durability, reconciliation, cleanup, Q/RSS/buffers/descriptors, storage, chronology, custody, timing buckets, analyzer agreement, and observability labels remain hard.
