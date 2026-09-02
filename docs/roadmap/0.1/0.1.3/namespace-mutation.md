# Namespace mutation

## Status

Draft v0.1.3 family contract: 3 timed cases and 0 proof-only cases.

## Problem statement

Namespace initialization and tiny-file churn do not prove that one Workspace
can publish a mixed structural edit frontier without rebuilding unrelated
namespace state. Rename, cross-directory move, subtree creation, and subtree
deletion also exercise different path-count and ancestor-rewrite outcomes.

## Goal

Measure deterministic nested prefixes of 1, 10, and 100 mixed namespace
mutations before one Commit. The stream must contain count-neutral, growing,
and shrinking outcomes and reopen to the exact path tree.

## Files to read

- [v0.1.3 shared contract](README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [Namespace initialization, scale, and CAS/CDC deduplication](namespace-initialization-scale.md)
- [Tiny-file churn](tiny-file-churn.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Workspace change planner](../../../../crates/layerfs-workspace/src/changes.rs)

## Fixed topology and lifecycle boundary

Each timed sample starts from one LayerStack, one genesis Layer, one Branch,
and a fresh real-FUSE Workspace. One fresh process applies the scheduled
prefix, syncs its ordinary filesystem work, exits, and is followed by one
Commit, End, fresh Store reconnect, and exact Branch verification. The Commit
is not promoted into another Layer. Repeated Commit history belongs to v0.1.4.

## Timed scenarios

| Scenario ID | Scheduled mutations | Required outcome |
| --- | ---: | --- |
| `namespace-mutation-1` | 1 | First nested mutation publishes exactly |
| `namespace-mutation-10` | 10 | First full mixed cycle publishes exactly |
| `namespace-mutation-100` | 100 | Ten mixed cycles publish exactly with unrelated subtrees reused |

## Proof-only scenarios

There are no proof-only cases in this family.

## Tier/load rule and deterministic schedule

The primary load unit is one scheduled semantic namespace mutation and
`a = 10`, giving nested 1/10/100 prefixes. A semantic subtree create or delete
may issue multiple ordinary filesystem calls; report both scheduled mutations
and affected paths rather than pretending they are the same count.

Every ten-operation cycle uses ten independent fixture cells and this fixed
operation sequence:

| Slot | Mutation | Count outcome |
| ---: | --- | --- |
| 0 | Rename a regular file within its directory | neutral |
| 1 | Move a regular file to a sibling directory | neutral |
| 2 | Create a subtree containing 10 regular files | grow by 11 paths |
| 3 | Delete a prepared subtree containing 10 regular files | shrink by 11 paths |
| 4 | Rename a directory containing one marker file | neutral |
| 5 | Move a directory containing one marker file to a sibling parent | neutral |
| 6 | Create a second 10-file subtree | grow by 11 paths |
| 7 | Delete a second prepared 10-file subtree | shrink by 11 paths |
| 8 | Rename a second regular file | neutral |
| 9 | Move a second regular file to a sibling directory | neutral |

The 10-operation cycle has zero net path-count change while still containing
grow and shrink operations. The 100-operation tier repeats the cycle on new
cells, never on paths mutated by an earlier cycle.

### Frozen seeds and nested prefixes

Use the three seed labels frozen in the shared contract. A
`v0.1.3/namespace-mutation` SHA-256 counter stream selects a permutation of 100
prepared cells; operation type remains fixed by ordinal modulo 10. Names,
marker bytes, modes, and mtimes derive from the seed label, cell, and ordinal.

For each seed, the 1-operation case is the first element of the 10-operation
case and the 10-operation case is the first ten elements of the 100-operation
case. Freeze the initial and expected-final manifest digests before candidate
collection.

## Required metrics and oracles

Record complete workflow and workload time, CPU, peak RSS, swaps, scheduled
mutation count, actual filesystem calls, affected paths, neutral/grow/shrink
counts, path count before and after, candidate/inserted/reused objects and
bytes, transaction maxima, Store growth, sync evidence, and cleanup state.

Verification must prove exact path names and parents, file bytes, modes, mtimes,
path count, canonical root, Branch head, fresh-reopen digest, absence of every
deleted subtree, presence of every created subtree, and no leaked mount,
process, spool, Workspace, or lease.

## Expected-rate assumptions and family budget

Applicable work must sustain at least 10,000 affected paths/s. The fixed
Create + Commit/acknowledgement + End + fresh-reopen/verification component is
at most 500 ms after subtracting the path term.

The complete family campaign—three fresh samples for each timed case—targets
10 seconds and has a hard ceiling of 20 seconds. Fixture and environment
preparation, sealing, and report generation are excluded and reported
separately.

## Acceptance criteria

- [ ] Exactly the three timed scenario IDs above are registered; no proof or
  control row is added by this family.
- [ ] All three seeds use exact nested 1/10/100 prefixes of the same stream.
- [ ] The stream covers rename, move, subtree create, and subtree delete and
  records neutral, grow, and shrink results separately.
- [ ] One Commit publishes the whole prefix and the fresh reopen matches the
  frozen final manifest and canonical root.
- [ ] Unrelated cells and persistent subtrees are reused rather than rebuilt.
- [ ] Path throughput, fixed lifecycle, and 10/20-second family budgets pass
  without dropping a valid sample.
- [ ] Sync/barrier remains passive evidence, not another scenario or family.
- [ ] No repeated Commit, competing Branch, prepend, range-copy, or SDK-call
  microbenchmark enters this family.
