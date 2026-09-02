# Link/inode topology

## Status

Draft v0.1.3 family contract: 3 timed cases and 0 proof-only cases.

## Problem statement

Path-count benchmarks do not prove inode topology. Hard links add directory
entries without adding a file inode, while symbolic links add independent
link nodes with exact target bytes. A mixed stream must preserve those
differences through one Commit and fresh reopen.

## Goal

Measure nested prefixes of 1, 10, and 100 deterministic hard-link and symbolic-
link creations before one Commit, with exact path, inode, link-count, target,
and canonical-root oracles.

## Files to read

- [v0.1.3 shared contract](README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [Namespace mutation](namespace-mutation.md)
- [LayerFS namespace model](../../../versioned/0.1.0/specification.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Workspace change planner](../../../../crates/layerfs-workspace/src/changes.rs)

## Fixed topology and lifecycle boundary

Each timed sample uses one LayerStack, one genesis Layer, one Branch, and one
fresh real-FUSE Workspace. One fresh process creates the scheduled links,
performs its ordinary sync, and exits before one Commit. End, fresh Store
reconnect, and exact verification follow. The Commit is not promoted into a
Layer and no case performs a second Commit.

## Timed scenarios

| Scenario ID | Scheduled link operations | Required outcome |
| --- | ---: | --- |
| `link-inode-topology-1` | 1 | One hard-link entry shares its source inode |
| `link-inode-topology-10` | 10 | Five hard links and five symlinks publish exactly |
| `link-inode-topology-100` | 100 | Fifty hard links and fifty symlinks publish exactly |

## Proof-only scenarios

There are no proof-only cases in this family.

## Tier/load rule and deterministic schedule

The primary load unit is one link creation and `a = 10`, giving nested
1/10/100 prefixes. Even ordinals create a hard link to a prepared regular file;
odd ordinals create a relative symbolic link to a different prepared regular
file. Each operation uses a new destination and changes path count by exactly
one.

For a prefix of `n` operations:

```text
added_paths        = n
added_hard_links   = (n + 1) / 2
added_symlinks     = n / 2
added_file_inodes  = 0
added_link_inodes  = n / 2
```

`added_link_inodes` follows LayerFS's canonical symlink-node model. The oracle
also records native inode/link-count observations without treating host inode
numbers as canonical identities.

### Frozen seeds and nested prefixes

Use the three seed labels frozen in the shared contract. A
`v0.1.3/link-inode-topology` SHA-256 counter stream selects a permutation of
100 prepared source files and destination cells. Operation parity fixes link
type, so each seed has the same hard-link/symlink counts while source and
destination paths differ deterministically.

For each seed, the 1-operation and 10-operation schedules are exact prefixes
of the 100-operation schedule. Freeze source bytes, relative symlink targets,
initial topology, and expected final manifest digests before candidate
collection.

## Required metrics and oracles

Record complete workflow and workload time, CPU, peak RSS, swaps, link
operations, affected paths, path count, canonical inode count, regular-file
inode count, symlink-node count, hard-link count, source link counts,
candidate/inserted/reused objects and bytes, transaction maxima, Store growth,
sync evidence, and cleanup state.

Fresh-reopen verification must prove every hard-link pair shares one logical
inode and identical bytes, later path lookup is exact, every symlink retains
its exact relative target bytes, path and inode equations hold, the canonical
root and Branch head match the frozen oracle, and no runtime resource leaks.

## Expected-rate assumptions and family budget

Applicable work must sustain at least 10,000 affected paths/s. The fixed
Create + Commit/acknowledgement + End + fresh-reopen/verification component is
at most 500 ms after subtracting the path term.

The complete family campaign—three fresh samples for each timed case—targets
8 seconds and has a hard ceiling of 15 seconds. Fixture and environment
preparation, sealing, and report generation are excluded and reported
separately.

## Acceptance criteria

- [ ] Exactly the three timed scenario IDs above are registered; no proof or
  control row is added by this family.
- [ ] All three seeds use exact nested 1/10/100 prefixes with the stated
  hard-link/symlink and path/inode equations.
- [ ] One Commit publishes each prefix and fresh reopen preserves hard-link
  identity, link count, file bytes, symlink target bytes, and canonical root.
- [ ] Link creation does not copy immutable source payload solely because a
  new hard-link path exists.
- [ ] Path throughput, fixed lifecycle, and 8/15-second family budgets pass
  without dropping a valid sample.
- [ ] Sync/barrier remains passive evidence, not another scenario or family.
- [ ] No unlink churn, repeated Commit, Branch fan-out, or SDK-operation timer
  enters this family.
