# Count-changing file edits

## Status

Draft family 8 contract: 6 timed scenarios and 4 proof-only scenarios. None of
the IDs are registered benchmark evidence.

## Problem statement

LayerFS has strong evidence for same-count overwrites and prepend, but it lacks
one compact public-FUSE family that separates append, shrink, sparse growth,
and a bounded mixture of same-count and count-changing edits. Middle and full
rewrites also need exact correctness proof without pretending POSIX exposes a
native insert/delete-range operation.

## Goal

Measure five bounded count-changing workloads before one Commit, and prove four
middle/full grow/shrink rewrites exactly, without adding repeated Commit history
or changing the public filesystem contract.

## Files to read

- [v0.1.3 scope](README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Existing edit workload](../../../../benchmark/fs-bench-pro/workload.rs)
- [v0.1.2 capture scope](../0.1.2/capture-large-mixed-edit-resilience.md)
- [Workspace file I/O](../../../../crates/layerfs-workspace/src/file_io.rs)
- [Workspace capture](../../../../crates/layerfs-workspace/src/capture.rs)
- [Persistent content edits](../../../../crates/layerfs-content/src/file/rope/edit.rs)
- [Workspace Commit planning](../../../../crates/layerfs-workspace/src/changes.rs)

## Fixed one-genesis-Layer/one-Branch lifecycle boundary

Each scenario execution uses a fresh Store, Client, fixture, and candidate
runtime:

```text
fixture -> one genesis Layer -> one Branch -> one real-FUSE Workspace
        -> scheduled edits in one fresh process -> one Commit -> End
        -> fresh Store reconnect and exact verification
```

Every row creates at most one unpromoted Commit. No row adds a Layer, forks a
second Branch, or performs a second Commit. Repeated Commit/rebase and history
growth belong to v0.1.4.

Fixture generation, Store/Client/container preparation, source sealing, and
report writing stay outside the complete scenario wall and are recorded
separately.

## Exact scenario table

| Scenario ID | Class | Workload before the one Commit | Exact size effect |
| --- | --- | --- | --- |
| `append-1` | Timed | Append one deterministic 4 KiB block to one 8 MiB file | `+4 KiB` |
| `truncate-1` | Timed | Truncate one 8 MiB file smaller by 4 KiB | `-4 KiB` |
| `sparse-grow-1` | Timed | `ftruncate` one 8 MiB file larger by 8 MiB | `+8 MiB` logical zeros |
| `mixed-1` | Timed | First 1 scheduled edit on the first scheduled 1 MiB file | Exact schedule below; count-growing append |
| `mixed-10` | Timed | First 10 scheduled edits on 10 distinct 1 MiB files | Exact schedule below |
| `mixed-100` | Timed | First 100 scheduled edits on 100 distinct 1 MiB files | Exact schedule below |
| `middle-grow` | Proof | Insert 4 KiB at the midpoint of one 8 MiB file by temp-copy, fsync, rename | `+4 KiB` |
| `middle-shrink` | Proof | Delete 4 KiB at the midpoint by temp-copy, fsync, rename | `-4 KiB` |
| `full-grow` | Proof | Fully rewrite one 8 MiB file as deterministic 12 MiB | `+4 MiB` |
| `full-shrink` | Proof | Fully rewrite one 8 MiB file as deterministic 4 MiB | `-4 MiB` |

`KiB` and `MiB` are binary. A same-count edit preserves logical file length. A
count-changing edit changes it through append, shrink, sparse extension, or an
explicit application-level rewrite. Proof rows exercise ordinary public POSIX
workflows; they do not claim a native middle insert/delete primitive.

## Tier and load rule

The primary load unit is one edit operation. With `a = 10`, the mixed tiers are
`1`, `a = 10`, and `a^2 = 100`; the three focused rows each use one operation.
The 1- and 10-edit schedules are exact prefixes of the 100-edit schedule. Every
operation happens before the one Commit.

Each ten-operation mixed block uses this fixed kind cycle:

```text
append, overwrite, truncate, overwrite, sparse-grow,
overwrite, append, overwrite, truncate, overwrite
```

Thus each block has 5 ten-byte same-count overwrites, 2 appends of 4 KiB, 2
shrinks of 4 KiB, and 1 sparse growth of 64 KiB. Operations target distinct
files, so 10 and 100 operations affect exactly 10 and 100 paths.

Each timed scenario has exactly three fresh samples, one per fixed seed. Each
proof runs exactly once with `layerfs-v0.1.3-seed-1`; proof executions receive
no latency distribution but are included once in the family wall.

## Deterministic seeds and random schedule

The three UTF-8 seed labels are exactly:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

Rank master edit indices `0..99` by the bytewise digest:

```text
SHA256(seed_label || 0x00 || "count-changing-file-edits" || 0x00
       || index_le_u64)
```

Break a digest tie by numerical index. Mixed tiers take the first `N` indices;
edit kind comes from scheduled ordinal in the fixed cycle. The target is
`files/f{index:03}.dat`.

Fixture and edit bytes use the counter stream
`SHA256(seed_label || 0x00 || scenario_id || 0x00 || index_le_u64 ||
block_index_le_u64)`. Hash-derived values choose valid overwrite offsets;
count-changing deltas remain the exact table and cycle values. Record the
ordered edit schedule, pre/post size manifest, and pre/post digest oracle. Do
not use wall time or unrecorded randomness.

## Required metrics and oracles

Record per execution:

- scenario ID, seed, ordered edit manifest, same-count/count-changing counts,
  affected paths, changed bytes, logical zero bytes, and pre/post digests;
- Workspace create, inner workload, Commit phases, visibility, End, reopen
  verification, and complete scenario wall;
- write/truncate calls and bytes, dirty and charged interval counts/peaks/
  metadata bytes, spool logical/allocated/peak bytes, and zero-buffer peak;
- user/system CPU, peak RSS, swaps, Store growth, candidate/inserted/reused
  objects and bytes, transaction maxima, and cleanup state; and
- payload throughput plus same-count and count-changing edits per second.

The byte oracle independently applies the ordered schedule, then verifies every
file's exact size and SHA-256, all sparse gaps as zero, unaffected paths,
canonical root, Branch head, and fresh-reopen root. Proof rows additionally
verify prefix/suffix preservation across the middle splice or complete expected
replacement across the full rewrite. No row may leak a mount, process,
container, spool, or lease.

## Target and hard family time budget

The planning model is:

```text
0.5 s fixed lifecycle
+ payload bytes / 100 MiB/s
+ affected paths / 10,000 paths/s
+ same-count edits / 100 edits/s
+ count-changing edits / 50 edits/s
```

The fixed lifecycle is Workspace Create + Commit + End + fresh
reopen/verification, excluding workload terms. The family wall sums complete
scenario walls for the 6 timed rows at all 3 seeds plus each of the 4 proof rows
once: 22 executions. Fixture/setup, sealing, and report work remain excluded.

- Target family wall: **18 seconds**.
- Hard family wall: **35 seconds**.

Crossing the target requires an edit- and phase-backed disposition. Crossing
the hard budget, allocating in proportion to a sparse gap, swapping, or
violating an oracle blocks admission.

## Acceptance criteria

- [ ] All 6 timed IDs run with three fresh seed-bound samples; all 4 proof IDs
  run once with seed 1.
- [ ] `mixed-1` and `mixed-10` are exact prefixes of `mixed-100`, and all edits
  precede one Commit.
- [ ] Append, truncate, sparse-grow, and mixed rows produce the exact size,
  bytes, zero ranges, canonical root, and fresh-reopen result.
- [ ] Middle/full grow/shrink proofs preserve exactly the required bytes and do
  not claim an unsupported native splice operation.
- [ ] Dirty/charged ranges, spool/zero memory, payload and edit rates, Commit
  phases, object reuse, resource peaks, and cleanup are retained.
- [ ] Sparse growth does not allocate memory or physical spool bytes
  proportional to the logical zero gap.
- [ ] Applicable payload, path, same-count-edit, and count-changing-edit floors
  are at least 100 MiB/s, 10,000 paths/s, 100 edits/s, and 50 edits/s, and the
  fixed lifecycle component is at most 500 ms.
- [ ] Family wall meets the 18-second target and never exceeds 35 seconds.
- [ ] No correctness, swap, resource-bound, unreachable-object, or cleanup
  failure occurs.
- [ ] Repeated Commit history is absent and remains owned by v0.1.4.
