# Tiny-file churn

## Status

Draft family 5 contract: 9 timed scenarios and 0 proof-only scenarios. None of
the IDs are registered benchmark evidence.

## Problem statement

The frozen benchmark has one 1,000-file shell workload, but it combines fixture
creation with some operations and does not measure LayerFS publication or fresh
reopen. LayerFS therefore lacks a small, nested-prefix curve for creating,
stating, and unlinking diverse tiny files through the complete product path.

## Goal

Measure create, `lstat`, and unlink at 1, 10, and 100 affected paths before one
Commit, with exact filesystem, canonical-root, resource, and cleanup oracles.

## Files to read

- [v0.1.3 scope](README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Frozen upstream tiny-file meanings](../../../../benchmark/fs-bench/fs-bench.sh)
- [FUSE filesystem](../../../../crates/layerfs-fuse/src/filesystem.rs)
- [Workspace capture](../../../../crates/layerfs-workspace/src/capture.rs)
- [Workspace Commit planning](../../../../crates/layerfs-workspace/src/changes.rs)

## Fixed one-genesis-Layer/one-Branch lifecycle boundary

Each scenario sample uses a fresh Store, Client, fixture, and candidate runtime:

```text
fixture -> one genesis Layer -> one Branch -> one real-FUSE Workspace
        -> N operations in one fresh process -> one Commit -> End
        -> fresh Store reconnect and exact verification
```

The `stat` rows require an `UpToDate` Commit; create and unlink require one
`Created` Commit. No Commit is promoted to a Layer. No row adds a Layer, forks a
second Branch, or performs a second Commit. Repeated-Commit history belongs to
v0.1.4.

Fixture generation, Store/Client/container preparation, source sealing, and
report writing stay outside the complete scenario wall and are recorded
separately.

## Exact scenario table

| Scenario ID | Timed workload before the one Commit | Genesis state | Expected outcome |
| --- | --- | --- | --- |
| `tiny-create-1` | Create the first 1 scheduled path and its bytes | Target absent | `Created`; 1 added path |
| `tiny-create-10` | Create the first 10 scheduled paths and their bytes | Targets absent | `Created`; 10 added paths |
| `tiny-create-100` | Create the first 100 scheduled paths and their bytes | Targets absent | `Created`; 100 added paths |
| `tiny-stat-1` | `lstat` the first 1 scheduled path | All targets present | `UpToDate`; root unchanged |
| `tiny-stat-10` | `lstat` the first 10 scheduled paths | All targets present | `UpToDate`; root unchanged |
| `tiny-stat-100` | `lstat` the first 100 scheduled paths | All targets present | `UpToDate`; root unchanged |
| `tiny-unlink-1` | Unlink the first 1 scheduled path | All targets present | `Created`; 1 removed path |
| `tiny-unlink-10` | Unlink the first 10 scheduled paths | All targets present | `Created`; 10 removed paths |
| `tiny-unlink-100` | Unlink the first 100 scheduled paths | All targets present | `Created`; 100 removed paths |

`lstat` means metadata lookup without opening or reading file contents. Fixture
setup is never hidden inside a `stat` or unlink workload.

## Tier and load rule

The primary load unit is one affected path. With `a = 10`, the tiers are
`1`, `a = 10`, and `a^2 = 100` operations. For a given seed, the 1-path
schedule is a prefix of the 10-path schedule, which is a prefix of the 100-path
schedule. All operations happen before the single Commit.

Candidate evidence has exactly three fresh timed samples per scenario, one per
fixed seed. There are no warm samples and no proof-only rows.

Tiny-file sizes repeat this exact ten-entry cycle by scheduled item ordinal:

```text
0, 1, 7, 31, 127, 511, 1,024, 2,500, 4,096, 8,192 bytes
```

Paths are spread across ten deterministic top-level prefixes. Create bytes are
unique per path; identical contents must not turn this into a dedup-only case.

## Deterministic seeds and random schedule

The three UTF-8 seed labels are exactly:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

For each seed, rank master item indices `0..99` by the bytewise digest:

```text
SHA256(seed_label || 0x00 || "tiny-file-churn" || 0x00 || index_le_u64)
```

Break a digest tie by numerical index. The ranked list is the random schedule,
and each tier takes its first `N` items. Scheduled ordinal `j` and master index
`i` map to `tiny/p{j mod 10}/f{i:03}.dat`.

File content concatenates the counter stream
`SHA256(seed_label || 0x00 || "tiny-file-bytes" || 0x00 || i_le_u64 ||
block_index_le_u64)` and truncates it to the scheduled size. Record the schedule
and fixture digest; do not use wall time, host randomness, directory
enumeration order, or an unrecorded seed.

## Required metrics and oracles

Record per sample:

- scenario ID, seed, scheduled paths, size vector, logical bytes, and fixture
  digest;
- Workspace create, inner workload, Commit API, required visibility, End,
  reopen verification, and complete scenario wall;
- user/system CPU, peak RSS, swaps, Store growth, candidate/inserted/reused
  objects and bytes, transaction maxima, and cleanup state;
- FUSE `create`, `write`, `fsync`, `lookup`/`getattr`, `unlink`, and transferred
  byte counts; and
- affected paths per second and payload bytes per second where applicable.

The oracle verifies the exact path set, size and SHA-256 of every affected
file, unchanged bytes and metadata on unaffected files, expected Commit
outcome, Branch head, canonical root, fresh-reopen root, and absence of leaked
mounts, processes, containers, spools, or leases. `stat` must cause zero
filesystem, Branch-head, and Store-growth changes.

## Target and hard family time budget

The planning model is:

```text
0.5 s fixed lifecycle
+ payload bytes / 100 MiB/s
+ affected paths / 10,000 paths/s
+ same-count edits / 100 edits/s
+ count-changing edits / 50 edits/s
```

The fixed lifecycle is Workspace Create + Commit/`UpToDate` + End + fresh
reopen/verification, excluding workload terms. The family wall is the sum of
complete scenario walls for all 9 rows and all 3 seeds: 27 executions.

- Target family wall: **10 seconds**.
- Hard family wall: **20 seconds**.

Crossing the target requires a phase-backed disposition. Crossing the hard
budget, swapping, or violating a resource/correctness oracle blocks admission.

## Acceptance criteria

- [ ] All 9 exact IDs run with three fresh seed-bound samples and no hidden
  fixture work.
- [ ] The 1/10/100 schedules are nested prefixes and all operations precede one
  Commit.
- [ ] The exact diverse size cycle and unique deterministic bytes are used.
- [ ] Create adds, `stat` preserves, and unlink removes exactly the scheduled
  paths after fresh reopen.
- [ ] `stat` returns `UpToDate` with an unchanged canonical root and zero Store
  growth.
- [ ] Path and payload rates, lifecycle phases, FUSE operations, object reuse,
  resource peaks, and cleanup are retained for every sample.
- [ ] Applicable payload and path floors are at least 100 MiB/s and 10,000
  paths/s, and the fixed lifecycle component is at most 500 ms.
- [ ] Family wall meets the 10-second target and never exceeds 20 seconds.
- [ ] No correctness, swap, resource-bound, or cleanup failure occurs.
- [ ] Existing registered scenarios keep their IDs and meanings unchanged.
