# Phase 4 fast test contract

Status: **working contract; review before runner implementation**

## 1. Objective

Optimize the user-facing Phase-4 workflows through short, causal experiments
without weakening canonical correctness, authentication, bounded memory,
SQLite durability, or typed failures.

The primary current target is the accepted 100-MiB durable full-write path.
One candidate changes one mechanism. An inconclusive candidate is never used as
the base for the next experiment.

## 2. Operations

```text
durable-full-write
edit-same
edit-plus1
materialize-warm
materialize-fresh
read-range
reopen
```

`durable-full-write` ends only after successful durable COMMIT acknowledgement.
`edit-same` reports edit latency and exact changed work, not source-size divided
by edit wall. `edit-plus1` is an honest suffix-cost guard under the current
fixed-ordinal mapping. `materialize-fresh` uses a fresh process and connection;
OS page-cache state is reported honestly and is not called cold disk.

## 3. Fixtures

| Fixture | Exact bytes | Role |
|---|---:|---|
| S1-1 | 1,048,576 | correctness and fixed-overhead smoke |
| S1-10 | 10,485,760 | fast scaling and candidate screen |
| S1-100 | 104,857,600 | controlling performance result |

Each source is generated from a frozen seed outside all timers. Record the
generator, seed, SHA-256, raw fingerprint, reference count, and ordered CDC
fingerprint. Generate each source once per experiment root; never copy it per
row or retain duplicate copies in the report directory.

Correctness tests also use tiny structural cases such as empty, one, K-1, K,
K+1, F-1, F, F+1, partial-final, boundary-crossing, malformed, tampered,
missing, duplicate, cycle, overflow, rollback, and ambiguous-COMMIT cases.

## 4. Four execution tiers

### Tier 1 — affected-test loop

Target: at most 20 seconds.

Run only the focused unit or integration tests for the component changed. Do
not build release, generate a large fixture, or run a performance workflow.

### Tier 2 — fast semantic gate

Target: at most 60 seconds.

Run core tests, compact engine parity, the affected adversarial tests,
formatting, and diff checks. Do not run performance campaigns or retain
databases.

### Tier 3 — candidate performance screen

Target: at most five minutes total and 120 seconds per command.

Run one 1-MiB smoke, three 10-MiB candidate samples, and three balanced
100-MiB control/candidate pairs for the affected primary operation. Run each
protected operation once at 100 MiB. Screening evidence is directional and is
not an accepted performance claim.

```text
warmup: A B
pair 1: A B
pair 2: B A
pair 3: A B

A = frozen control
B = candidate
```

Screening passes only when the affected-operation median improves by at least
5%, the candidate wins at least two of three pairs, the predicted direct
counter moves, and correctness/resources/protected operations pass.

### Tier 4 — retained checkpoint

Target: at most ten minutes unless prospectively amended.

Only a screening winner receives one warmup and five balanced adjacent pairs
for the primary 100-MiB operation. A retained performance claim requires at
least 5% arm and paired-median improvement, at least four of five wins, the
predicted counter movement, exact identities, and all protected gates.

Run full workspace regression once per retained checkpoint. Run 512 MiB only
when the changed mechanism may scale differently and only after the 100-MiB
candidate passes. Do not run a multi-profile or multi-operation campaign by
default.

## 5. Correctness and authority gate

Every relevant performance row must preserve or independently prove:

- source fingerprint and ordered frozen CDC sequence;
- canonical bytes and raw/canonical object IDs;
- file mapping, workspace root, transition, delta, and closure;
- reconstruction and requested ranges;
- incumbent authentication and exact typed errors;
- one transaction, one COMMIT dispatch/return, and durable acknowledgement;
- fresh reopen and complete-head comparison where applicable;
- exact checked counters and terminal live Q equal to zero; and
- unchanged metadata/schema/profile unless a separately approved specification
  explicitly changes them.

Any identity, closure, malformed-input, transaction, durability, provenance,
or exact-Q failure forces `REVERT` regardless of speed.

## 6. Measurement and resources

For the affected operation, retain disjoint timers, paired wall deltas, user
and system CPU, exact Q, RSS/footprint, SQL/BLOB/pager/COMMIT counters, object
and canonical/mapping bytes, and logical/apparent/allocated endpoint storage.

Unsupported physical observations are `Unavailable`. Logical, pager, or
apparent bytes must never be relabeled as physical-media I/O.

A wall improvement without the preregistered direct-counter movement is
`INCONCLUSIVE`. Noise does not authorize adding samples after seeing results;
the next attempt must be a new checkpoint with a prospective schedule.

## 7. Retention and cleanup

Use a uniquely created temporary directory for fixtures, databases, authority
files, outputs, and row working state. After each successful row, perform the
required fresh verification, emit one strict JSON record, close all handles,
and delete the transient row state.

Retain only:

- one Markdown checkpoint report;
- one compact raw JSONL file;
- source, diff, executable, fixture, command, and environment hashes;
- direct counters and correctness dispositions; and
- at most one representative database for a final accepted checkpoint when
  explicitly preregistered.

Ordinary screening evidence must remain below 10 MiB. Do not retain duplicate
fixtures, successful per-row databases, copied authorities, copied
expectations, release executables, or a manifest over transient state.

## 8. Stop rules

Stop and report rather than expand the experiment when:

- a command reaches 120 seconds;
- a screening wave reaches five minutes;
- a checkpoint wave reaches ten minutes;
- the direct counter does not move as predicted;
- the candidate loses two screening pairs;
- any correctness or exact-Q gate fails;
- the artifact root would exceed its cap; or
- another operation/profile/size would be added only after seeing an
  unfavorable result.

The decision vocabulary is `BASELINE`, `SCREEN-PASS`, `RETAIN`, `REVISE`,
`REVERT`, or `INCONCLUSIVE`.
