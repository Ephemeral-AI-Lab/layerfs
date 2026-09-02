# Git and tool workflow

## Status

Draft family 7 contract: 3 timed scenarios and 0 proof-only scenarios. None of
the IDs are registered benchmark evidence.

## Problem statement

The frozen benchmark's `git init + commit 100 files` row is a useful control,
but it measures only repository creation. It does not cover the common agent
loop of changing an existing repository, inspecting it, staging it, committing
it, publishing the Workspace, and reopening the result.

## Goal

Measure one deterministic offline Git workflow at 1, 10, and 100 changed paths
before one LayerFS Commit. Keep the frozen `git init + commit 100 files`
scenario unchanged and separate.

## Files to read

- [v0.1.3 scope](README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Frozen Git control](../../../../benchmark/fs-bench/fs-bench.sh)
- [Workspace execution](../../../../crates/layerfs-workspace/src/execution.rs)
- [FUSE filesystem](../../../../crates/layerfs-fuse/src/filesystem.rs)
- [Workspace capture](../../../../crates/layerfs-workspace/src/capture.rs)
- [Workspace Commit planning](../../../../crates/layerfs-workspace/src/changes.rs)

## Fixed one-genesis-Layer/one-Branch lifecycle boundary

Each scenario sample starts from a fresh deterministic Git repository already
initialized and committed in the imported fixture:

```text
Git fixture -> one genesis Layer -> one Branch -> one real-FUSE Workspace
            -> one composite Git workflow in one fresh process
            -> one LayerFS Commit -> End -> fresh reconnect and verification
```

The Git workflow creates one Git commit. The LayerFS Workspace creates one
unpromoted Commit. Neither is setup for another timed Commit. No Layer is added,
no second Branch is forked, and no network, hook, submodule, LFS, maintenance,
or credential helper is allowed. Repeated-Commit history belongs to v0.1.4.

Fixture generation, Store/Client/container preparation, source sealing, and
report writing stay outside the complete scenario wall and are recorded
separately.

## Exact scenario table

| Scenario ID | Changed paths before the one LayerFS Commit | Composite workflow | Expected outcome |
| --- | ---: | --- | --- |
| `git-tool-1` | First 1 scheduled change | change, status, diff, add, cached check, Git commit, clean status | One Git commit and one `Created` LayerFS Commit |
| `git-tool-10` | First 10 scheduled changes | same exact command sequence | One Git commit and one `Created` LayerFS Commit |
| `git-tool-100` | First 100 scheduled changes | same exact command sequence | One Git commit and one `Created` LayerFS Commit |

The command sequence is fixed:

```text
apply scheduled filesystem changes
git status --porcelain=v1 -z
git diff --no-ext-diff --binary --
git add -A --
git diff --cached --check
git commit --no-gpg-sign --no-verify -m "layerfs v0.1.3 tool workflow"
git status --porcelain=v1 -z
```

Set fixed author/committer names, emails, and timestamps. Disable system and
global configuration, hooks, signing, automatic CRLF conversion, optional
locks, and background maintenance. The row is offline and must use the Git
binary identity retained in the evidence manifest.

## Tier and load rule

The primary load unit is one changed path. With `a = 10`, tiers are `1`,
`a = 10`, and `a^2 = 100` changes. For one seed, the smaller change set and
operation order are exact prefixes of the larger set. All changes and Git
commands happen before one LayerFS Commit.

The imported repository contains deterministic 2,500-byte tracked files for
the modify and delete slots. Add targets are absent. Each ten-operation block
uses this fixed change-kind cycle:

```text
modify, add, delete, modify, add, delete, add, modify, delete, modify
```

`modify` is a ten-byte same-count overwrite at a deterministic offset. `add`
creates a unique deterministic 2,500-byte file. `delete` unlinks one tracked
file. Thus 10 changes contain 4 modifications, 3 additions, and 3 deletions;
100 changes contain 40, 30, and 30 respectively.

Candidate evidence has exactly three fresh timed samples per scenario, one per
fixed seed. There are no warm samples and no proof-only rows.

## Deterministic seeds and random schedule

The three UTF-8 seed labels are exactly:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

Rank master change indices `0..99` by the bytewise digest:

```text
SHA256(seed_label || 0x00 || "git-tool-workflow" || 0x00 || index_le_u64)
```

Break a digest tie by numerical index. Tiers take the first `N` ranked indices;
the change kind comes from scheduled ordinal in the fixed cycle.

Paths are `tracked/modify-{index:03}.dat`,
`added/add-{index:03}.dat`, or `tracked/delete-{index:03}.dat`. Contents and
overwrite offsets use the domain-separated counter stream
`SHA256(seed_label || 0x00 || "git-tool-bytes" || 0x00 || index_le_u64 ||
block_index_le_u64)`. The fixture, baseline Git tree, ordered change manifest,
expected Git tree, and output streams receive retained SHA-256 digests.

## Required metrics and oracles

Record per sample:

- scenario ID, seed, ordered path/change manifest, changed bytes, fixture Git
  tree, resulting Git tree, Git binary/version/config identities, and digests;
- separate apply, first status, diff, add, cached-check, Git commit, final
  status, LayerFS Commit, visibility, End, reopen, and complete scenario wall;
- process count, user/system CPU, peak RSS, swaps, output bytes, Store growth,
  candidate/inserted/reused objects and bytes, transaction maxima, and cleanup;
- FUSE lookup/getattr/open/read/create/write/fsync/rename/unlink/readdir counts
  and transferred bytes; and
- changed paths per second plus same-count edit and payload rates.

The oracle requires the first status/diff to name exactly the scheduled paths,
the cached check to pass, the final status to be empty, one new Git commit with
the expected parent/tree/message/identity/time, and `git fsck --strict` after
fresh reopen. LayerFS must reopen to the expected path bytes, Git tree,
canonical root, and Branch head with no leaked runtime resource.

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
reopen/verification, excluding workload terms. The family wall is the sum of
complete scenario walls for all 3 rows and all 3 seeds: 9 executions.

- Target family wall: **15 seconds**.
- Hard family wall: **30 seconds**.

Crossing the target requires command- and phase-backed disposition. Crossing
the hard budget, using the network, swapping, or violating an oracle blocks
admission.

## Acceptance criteria

- [ ] All 3 exact IDs run with three fresh seed-bound samples.
- [ ] The 1/10/100 changed-path schedules are nested prefixes and complete
  before one LayerFS Commit.
- [ ] The exact modify/add/delete mix and composite command sequence are used.
- [ ] Git runs offline with fixed identity, timestamps, configuration, and
  retained binary identity.
- [ ] Status/diff path sets, clean final status, Git parent/tree, `git fsck`,
  LayerFS canonical root, Branch head, and fresh-reopen bytes are exact.
- [ ] Command phases, FUSE operations, rates, Store/object changes, resources,
  and cleanup are retained for every sample.
- [ ] Applicable payload, path, and same-count-edit floors are at least
  100 MiB/s, 10,000 paths/s, and 100 edits/s, and the fixed lifecycle component
  is at most 500 ms.
- [ ] Family wall meets the 15-second target and never exceeds 30 seconds.
- [ ] No correctness, network, swap, resource-bound, or cleanup failure occurs.
- [ ] The frozen `git init + commit 100 files` control keeps its ID, workload,
  timing boundary, and evidence separate.
