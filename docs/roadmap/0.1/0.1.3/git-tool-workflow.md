# Git workflow in a populated workspace

> **Status:** Current v0.1.3 planning specification; no release candidate or
> measured result is implied.

Family ID: `git_tool_workflow`. This v0.1.3 implementation contract contains
**4 new timed cases and no standalone proofs**. The family adapter is not
implemented yet.

## Purpose and lifecycle

Exercise an agent's ordinary local Git workflow inside a populated repository:
edit files, inspect changes, stage them, create a Git commit, publish a LayerFS
Commit, and reopen the repository. Git uses ordinary POSIX/FUSE operations;
these results make no claims about singular SDK range-edit latency.

Follow the [shared testing rules](testing-rules.md) for seed identities,
preparation reuse, source and Git-binary custody, samples, timing, byte caps,
and independent verification. Each sample starts from a sealed existing Git
repository imported into one genesis Layer and Branch. Public Workspace
Create/managed execution/Commit/End operations surround one offline Git workflow
through real FUSE. There is one new Git commit and one final LayerFS Commit,
followed by a separate fresh-Store and fresh-mount verifier.

## Cases and bounded fixture

Expand `N` over exactly `1, 10, 100, 500` to obtain four scenario IDs:

| Scenario IDs | Changed paths | Expected result |
| --- | ---: | --- |
| `git-tool-{N}` | First N changes from the same seed-bound schedule | One new Git commit and `Created` LayerFS Commit |

Every tier has the same substantial **32 MiB tracked background**, distributed
across 32 shared-profile shards with 6,400 files. Reuse the bounded shared
generator, path manifests, wide-directory layout, and deep-path witness. The
background remains unchanged and must participate in full Git/LayerFS state
verification. A separate 500-slot change schedule uses domain `git-tool-workflow`.
Smaller tiers are exact prefixes of its ordered changes.

Each ten-slot block uses:

```text
modify, add, delete, modify, add, delete, add, modify, delete, modify
```

Modify/delete targets are present in the initial commit; add targets are absent.
All targets contain unique deterministic 2,500-byte files. Every modification
changes ten deterministic bytes at a sealed interior offset without changing
the final length. Number only modify slots with zero-based ordinal `m`:

- Even `m` uses an editor save: read the original 2,500 bytes, apply the planned
  ten-byte change, create a deterministic exclusive temporary sibling, write
  all 2,500 result bytes, set the required mode, `sync_all`, close, and rename
  the temporary file over the original path.
- Odd `m` uses an ordinary in-place ten-byte write to the existing file.

Temporary names are `.{original_basename}.save-{m:03}.tmp`. Saves run serially,
so at most one extra 2,500-byte temporary file exists. Both routes execute
through POSIX/FUSE inside the measured apply phase and yield the same independent
logical ten-byte edit oracle; neither route calls the SDK range-edit API.
Use separate `tracked/modify-*`, `tracked/delete-*`, and `added/add-*` paths.
The 500 tier modifies 200, adds 150, and deletes 150 paths; the initial tracked
change-target payload is 350 × 2,500 = 875,000 bytes. Its modifications contain
100 editor saves and 100 in-place writes. The one-change tier exercises an
editor save, and all routes preserve the shared nested schedule.

The ordinary working tree stays below 34 MiB, including the additional 2,500
bytes while an editor-save temporary file coexists with its original. Budget
the **complete repository at no more than 256 MiB at any intermediate point**,
including `.git` objects,
index, refs, logs, temporary and lock files, and deleted-but-open files. No
individual workload file may exceed 500 MiB, and the shared strictly-under-1-GiB
limit still applies. Freeze a conservative object/index/temporary allocation
bound with the pinned Git version and verify the repository census; never
assume Git compression or CAS sharing makes a larger fixture admissible.
Preparation and verifier copies stay outside the workload namespace and have
separate resource accounting. No pack/repack or automatic maintenance is run.

## Exact measured workflow

Apply the scheduled ordinary filesystem changes, then run:

```text
git status --porcelain=v1 -z
git diff --no-ext-diff --binary --
git add -A --
git diff --cached --check
git commit --no-gpg-sign --no-verify -m "layerfs v0.1.3 tool workflow"
git status --porcelain=v1 -z
```

Pin the Git binary, author/committer identity and timestamps. Disable system
and global configuration, hooks, signing, optional locks, automatic line-ending
conversion, credential helpers, network access, and background maintenance.
Retain the effective configuration. Required index/ref locks remain part of
Git's ordinary writes. The process must complete before LayerFS Commit.

First status includes the exact modified/added/deleted target set; expand
untracked-directory entries or configure `status.showUntrackedFiles=all` so
porcelain does not collapse added files. Before staging, ordinary `git diff`
contains only the tracked modify/delete changes: it must not be incorrectly
required to include untracked added files. The independent staged-tree oracle
and final Git commit must include all scheduled changes.

## Timing and verification

Record separate apply, first status, diff, add, cached-check, Git commit, final
status, LayerFS Commit, visibility, End, and complete lifecycle walls. Account
for subprocess startup and output handling inside the workload. Fixture Git
initialization, fixture import, source sealing, and exhaustive verification
remain outside operation timers and have separately reported walls.
Report editor-save and in-place counts and apply-phase costs separately.
Git's own object hashing remains authentic tool work; extra reporting digests
and full repository/transcript verification run outside performance timing.

Record changed/unchanged path counts, bytes, output digests, FUSE operations,
Git processes, CPU, memory, swap/OOM, logical/physical I/O, Store/object growth,
repository byte peaks, and cleanup under the shared schema. Do not claim that
N changed paths equals N total filesystem calls: Git also reads metadata and
writes its own repository state.

The separate verifier uses an independent expected source-tree manifest and
reference Git tree/commit prepared outside product timing. Verify exact first
status, tracked diff paths/content, staged tree, cached check, empty final
status, and one new commit with the expected parent/tree/message/identity/time.
In verification mode, observe each editor save after temporary sync and after
rename: require the exact replacement bytes, bounded coexistence, and removal
of the temporary pathname. Verify the in-place routes against the same logical
edit oracle, and require no leftover save file in any final repository state.
In the verification execution, seal a complete repository receipt before
LayerFS Commit and compare it after fresh Store reconnect and real-FUSE remount,
before running any Git command that might refresh the index. Then run
`git fsck --strict` and repeat clean-status/tree checks. The source-tree manifest,
Git tree, commit, and required object identities have independent expected
values. The receipt additionally proves persistence of every repository file,
its bytes, mode, and timestamp; it is custody evidence, not an independent
content oracle. A native reference's index bytes are not an equality oracle
because inode/stat-cache fields legitimately differ. Authenticate the LayerFS
canonical root and Branch head, unchanged background, size envelope, and
cleanup. No semantic expectation is inferred solely from the product receipt.

## Execution and completion

Three fresh performance samples per new case produce **12 timed executions**;
every case has separate independent verification. Cache the sealed initial
repository and prepared input Store, then use existing sample-clone isolation.
Every measured sample still performs its own edits, Git workflow, and LayerFS
Commit. Reuse the benchmark binary, workload helper, runner, custody, and
report machinery; no new benchmark framework is needed.

Prospective selection is `git-tool-1` with one seed and its focused verifier;
this selector is not implemented yet. A 1–5-second selected-run target is
provisional and depends on the baseline. Larger tiers, full families, and
complete `fsck`/manifest verification use the longer lane with baseline-derived
budgets. Completion requires all four case identities and samples, exact Git
and LayerFS proofs, bounded resources, and clean teardown.

The historical `git init + commit 100 files` shell control keeps its original
identity, meaning, timing, and evidence separately; it is not a fifth case.
