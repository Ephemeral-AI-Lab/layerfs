# Directory construction and traversal

## Status

Draft family 6 contract: 6 timed scenarios and 0 proof-only scenarios. None of
the IDs are registered benchmark evidence.

## Problem statement

The frozen `10x10x10` shell rows cover one regular tree shape and exclude
Workspace lifecycle, publication, fresh reopen, and production receipts. They
cannot show whether directory cost tracks the paths actually constructed or
visited, nor whether depth changes that cost.

## Goal

Measure directory construction and deterministic traversal at 1, 10, and 100
irregular-depth subtree operations before one Commit.

## Files to read

- [v0.1.3 scope](README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Frozen upstream directory meanings](../../../../benchmark/fs-bench/fs-bench.sh)
- [FUSE filesystem](../../../../crates/layerfs-fuse/src/filesystem.rs)
- [FUSE inode table](../../../../crates/layerfs-fuse/src/inode_table.rs)
- [Workspace copy-on-write tree](../../../../crates/layerfs-workspace/src/cow_tree.rs)
- [Workspace Commit planning](../../../../crates/layerfs-workspace/src/changes.rs)

## Fixed one-genesis-Layer/one-Branch lifecycle boundary

Each scenario sample uses a fresh Store, Client, fixture, and candidate runtime:

```text
fixture -> one genesis Layer -> one Branch -> one real-FUSE Workspace
        -> N subtree operations in one fresh process -> one Commit -> End
        -> fresh Store reconnect and exact verification
```

Construction requires one `Created` Commit. Traversal is read-only and requires
an `UpToDate` Commit with an unchanged root. No Commit is promoted to a Layer;
there is no second Branch, Layer addition, or repeated Commit. Repeated-history
behavior belongs to v0.1.4.

Fixture generation, Store/Client/container preparation, source sealing, and
report writing stay outside the complete scenario wall and are recorded
separately.

## Exact scenario table

| Scenario ID | Timed workload before the one Commit | Genesis state | Expected outcome |
| --- | --- | --- | --- |
| `directory-construct-1` | `mkdir` the first 1 scheduled irregular chain | Chain absent | `Created`; exact new directory set |
| `directory-construct-10` | `mkdir` the first 10 scheduled irregular chains | Chains absent | `Created`; exact new directory set |
| `directory-construct-100` | `mkdir` the first 100 scheduled irregular chains | Chains absent | `Created`; exact new directory set |
| `directory-traverse-1` | Walk the first 1 scheduled subtree | All 100 subtrees present | `UpToDate`; root unchanged |
| `directory-traverse-10` | Walk the first 10 scheduled subtrees | All 100 subtrees present | `UpToDate`; root unchanged |
| `directory-traverse-100` | Walk the first 100 scheduled subtrees | All 100 subtrees present | `UpToDate`; root unchanged |

One construction operation creates one chain by issuing ordinary `mkdir` for
each missing component in root-to-leaf order. One traversal operation walks one
scheduled subtree with `open`/`readdir` plus `lstat` for every returned entry,
recursing in bytewise filename order. It does not read file payloads.

## Tier and load rule

The primary load unit is one affected subtree operation. With `a = 10`, tiers
are `1`, `a = 10`, and `a^2 = 100`. For a seed, each smaller schedule is an
exact prefix of the larger schedule. All `N` operations happen before one
Commit.

Each subtree is a unique directory chain of depth `1..10`, inclusive. The
master depth cycle is:

```text
1, 4, 2, 8, 3, 10, 5, 7, 6, 9
```

and repeats by scheduled ordinal. Thus the 10- and 100-unit tiers contain the
same irregular depth distribution while the random schedule changes concrete
path ownership. Report both operation units and actual directory entries
created or visited.

Candidate evidence has exactly three fresh timed samples per scenario, one per
fixed seed. There are no warm samples and no proof-only rows.

## Deterministic seeds and random schedule

The three UTF-8 seed labels are exactly:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

Rank master subtree indices `0..99` by the bytewise digest:

```text
SHA256(seed_label || 0x00 || "directory-construction-traversal" || 0x00
       || index_le_u64)
```

Break a digest tie by numerical index. Tiers take the first `N` ranked indices.

Subtree `index` starts at `tree/u{index:03}`. Its component at depth `d` is
`d{d:02}-{first-8-hex(SHA256(seed_label || 0x00 || "directory-component"
|| 0x00 || index_le_u64 || depth_le_u64))}`. The depth comes from the fixed
cycle by scheduled ordinal. Record the ordered schedule, depth vector, complete
expected path manifest, and digest. Never rely on host enumeration order or
unrecorded randomness.

## Required metrics and oracles

Record per sample:

- scenario ID, seed, subtree operations, depth vector, directories created or
  visited, path components resolved, maximum depth, and fixture digest;
- Workspace create, inner workload, Commit API, visibility, End, reopen
  verification, and complete scenario wall;
- user/system CPU, peak RSS, swaps, Store growth, candidate/inserted/reused
  objects and bytes, transaction maxima, and cleanup state;
- FUSE `mkdir`, `lookup`, `getattr`, `open`, `readdir`, and `release` counts;
  and
- subtree operations and actual visited/created paths per second.

Construction oracles compare the exact directory manifest, modes, Branch head,
canonical root, and fresh-reopen root. Traversal must return every expected
entry exactly once, in the harness's sorted oracle, with no mutation, no Store
growth, an `UpToDate` outcome, and the genesis root unchanged. Both paths prove
no leaked mount, process, container, spool, or lease.

## Target and hard family time budget

The planning model is:

```text
0.5 s fixed lifecycle
+ payload bytes / 100 MiB/s
+ actual paths / 10,000 paths/s
+ same-count edits / 100 edits/s
+ count-changing edits / 50 edits/s
```

The fixed lifecycle is Workspace Create + Commit/`UpToDate` + End + fresh
reopen/verification, excluding workload terms. The family wall is the sum of
complete scenario walls for all 6 rows and all 3 seeds: 18 executions.

- Target family wall: **10 seconds**.
- Hard family wall: **20 seconds**.

Crossing the target requires a phase-backed disposition. Crossing the hard
budget, swapping, or violating a resource/correctness oracle blocks admission.

## Acceptance criteria

- [ ] All 6 exact IDs run with three fresh seed-bound samples.
- [ ] The 1/10/100 schedules are nested prefixes with irregular depths 1..10,
  and all operations precede one Commit.
- [ ] Construction creates exactly the scheduled chains after fresh reopen.
- [ ] Traversal visits every expected entry exactly once and returns
  `UpToDate` with no root or Store change.
- [ ] Actual path counts, depth, path rate, lifecycle phases, FUSE operations,
  object reuse, resource peaks, and cleanup are retained for every sample.
- [ ] Path processing is at least 10,000 paths/s and the fixed lifecycle
  component is at most 500 ms.
- [ ] Family wall meets the 10-second target and never exceeds 20 seconds.
- [ ] No correctness, swap, resource-bound, or cleanup failure occurs.
- [ ] Existing registered directory scenarios keep their IDs and meanings
  unchanged.
