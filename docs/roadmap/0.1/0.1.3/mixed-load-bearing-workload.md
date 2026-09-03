# Mixed load-bearing workload

## Status

Draft v0.1.3 family contract: 3 timed cases and 3 proof-only cases.

## Problem statement

Single-operation families isolate regressions but do not prove that ordinary
reads, same-count writes, length changes, namespace edits, links, metadata,
sync, Commit, and reopen compose in one Workspace. The release needs one mixed
semantic stream without turning sync or metadata correctness into invented
latency families.

## Goal

Measure deterministic nested prefixes of 1, 10, and 100 mixed semantic
operations before one Commit. Add one proof each for chmod, mtime, and xattr
semantics. Record ordinary sync/barrier behavior passively; it is not another
family, timed row, or proof row.

## Files to read

- [v0.1.3 shared contract](README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [Completed v0.1.2 same-count edits](../0.1.2/same-count-file-edits.md)
- [Completed v0.1.2 count-changing edits](../0.1.2/count-changing-file-edits.md)
- [Namespace mutation](namespace-mutation.md)
- [Link/inode topology](link-inode-topology.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [FUSE filesystem adapter](../../../../crates/layerfs-fuse/src/filesystem.rs)

## Fixed topology and lifecycle boundary

Each timed sample uses one LayerStack, one genesis Layer, one Branch, and one
fresh real-FUSE Workspace. One fresh process runs the scheduled prefix and its
ordinary final sync before one Commit. End, fresh Store reconnect, and exact
verification follow. Proof cases use the same topology but receive one
execution each and no latency distribution. No Commit becomes a Layer and no
case performs a second Commit.

## Timed scenarios

| Scenario ID | Scheduled semantic operations | Required outcome |
| --- | ---: | --- |
| `mixed-load-bearing-1` | 1 | First nested read operation yields `UpToDate` and verifies exactly |
| `mixed-load-bearing-10` | 10 | One complete semantic cycle yields `Created` and reopens exactly |
| `mixed-load-bearing-100` | 100 | Ten semantic cycles yield `Created` and reopen exactly |

## Proof-only scenarios

| Scenario ID | Operation | Required proof |
| --- | --- | --- |
| `mixed-metadata-chmod-proof` | `chmod` a prepared file to `0640` | Mode is exact before Commit and after fresh reopen |
| `mixed-metadata-mtime-proof` | Set mtime to `1700000013.123456789` | Seconds and nanoseconds are exact before Commit and after fresh reopen |
| `mixed-metadata-xattr-proof` | Set `user.layerfs-v013=mixed-proof` | Value round-trips, or the documented stable unsupported result leaves no mutation |

The xattr proof must record the exact syscall result and capability. It may
accept `EOPNOTSUPP` only when that is the documented v0.1.x behavior; the
benchmark must not add a benchmark-only xattr API or silently report an
unsupported operation as a successful round trip.

## Tier/load rule and deterministic schedule

The primary load unit is one scheduled semantic operation and `a = 10`, giving
nested 1/10/100 prefixes. Every ten-operation cycle uses a new fixture cell:

| Slot | Semantic operation | Declared load term |
| ---: | --- | --- |
| 0 | Read a deterministic 1 MiB range | 1 MiB payload |
| 1 | Overwrite 10 bytes without changing file length | 1 same-count edit |
| 2 | Append 10 bytes | 1 count-changing edit |
| 3 | Truncate those 10 bytes | 1 count-changing edit |
| 4 | Create one 2,500-byte regular file | 1 affected path + payload |
| 5 | Move and rename that file to a sibling directory | 2 affected path bindings |
| 6 | Unlink the moved file | 1 affected path |
| 7 | Create a hard link to a prepared source | 1 affected path |
| 8 | Create a relative symlink to another prepared source | 1 affected path |
| 9 | `stat` and read the symlink target | 2 path observations |

The process performs its ordinary final sync after the selected prefix and
before returning. Sync count, duration, bytes, and error are passive fields on
the timed case. They create no schedule unit or registry entry.

### Frozen seeds and nested prefixes

Use the three seed labels frozen in the shared contract. A
`v0.1.3/mixed-load-bearing` SHA-256 counter stream chooses independent fixture
cells, read ranges, overwrite offsets, payload bytes, and names. Operation type
is fixed by ordinal modulo 10, so the load-term counts remain identical across
seeds.

For each seed, the 1-operation and 10-operation schedules are exact prefixes
of the 100-operation schedule. Freeze the initial fixture, scheduled-operation
receipt, metadata values, and expected-final manifest digest before candidate
collection.

## Required metrics and oracles

Record complete workflow and workload time, per-operation-class time and
count, payload bytes, affected paths, same-count and count-changing edits, CPU,
peak RSS, swaps, FUSE operation and byte counts, passive sync/barrier evidence,
candidate/inserted/reused objects and bytes, transaction maxima, Store growth,
and cleanup state.

Verification must replay the deterministic oracle and prove exact reads,
writes, lengths, created/deleted/moved paths, link topology, symlink targets,
metadata proof outcomes, Branch head, canonical root, fresh-reopen digest, and
absence of leaked mounts, processes, output readers, spools, Workspaces, or
leases.

## Expected-rate assumptions and family budget

Apply every shared floor represented by the stream: at least 100 MiB/s payload,
10,000 affected paths/s, 100 same-count edits/s, and 50 count-changing edits/s.
The fixed Create + Commit/acknowledgement + End + fresh-reopen/verification
component is at most 500 ms after subtracting those terms.

The complete family campaign—three fresh samples for each timed case plus one
execution of each metadata proof—targets 15 seconds and has a hard ceiling of
30 seconds. Fixture and environment preparation, sealing, and report
generation are excluded and reported separately.

## Acceptance criteria

- [ ] Exactly the three timed and three proof-only scenario IDs above are
  registered by this family.
- [ ] All three seeds use exact nested 1/10/100 prefixes of the declared
  semantic stream.
- [ ] One Commit publishes every selected prefix and fresh reopen matches the
  frozen semantic receipt, manifest digest, and canonical root.
- [ ] Chmod and mtime round-trip exactly; xattr records either an exact round
  trip or the documented stable unsupported result without mutation.
- [ ] Payload, path, same-count-edit, count-changing-edit, fixed-lifecycle, and
  15/30-second family gates pass without dropping a valid sample.
- [ ] Sync/barrier evidence remains passive and adds no family, timed case,
  proof case, or separate target.
- [ ] No repeated Commit, Branch fan-out, conflict workflow, or new
  owner-side Workspace file-range-edit member enters this family.
