# Workspace change locality

> **Status:** v0.1.3 implementation plan; no measurements or passing claim.
> **Family ID:** `workspace_change_locality`. Sixteen timed cases; no separate
> proofs. [Shared testing rules](testing-rules.md) govern common setup, seeds,
> custody, timing, verification, resource bounds, and admission.

## Question and fixture

Measure total Workspace size and dirty-work size independently. A clean Commit,
one small namespace change, sparse owner SDK edits, and dense ordinary writes
exercise different paths. Each is a four-tier curve, not a Cartesian matrix.

Reuse [`workspace-shards-v1`](testing-rules.md#shared-workspace-fixture), including
its exact file placement and metadata. One shard has 200 regular files:
`128*1 KiB + 64*8 KiB + 8*48 KiB = 1 MiB`. The frozen layout includes the wide
directory, regular siblings, 128-component spine, and empty `dest/`. Do not
create a second almost-identical fixture or label these as the original
single-file hashes. Prepare only the selected input size; N-shard inputs are
nested prefixes of the 500-shard generator.

| N | Files in an N-shard tree | Logical bytes in an N-shard tree |
| ---: | ---: | ---: |
| 1 | 200 | 1,048,576 |
| 10 | 2,000 | 10,485,760 |
| 100 | 20,000 | 104,857,600 |
| 500 | 100,000 | 524,288,000 |

All files are at most 48 KiB; each case stays at or below 500 MiB of aggregate
logical file bytes throughout. Owner SDK edits preserve length. Dense writes
replace files in place without a coexisting tree copy. Receipts stay outside
the workload with separately reported storage.

## Exact member expansion

| Scenario ID | Fixture | Measured dirty work | Expected Commit |
| --- | --- | --- | --- |
| `workspace-clean-commit-1` | 1 shard | None; untouched Workspace | `UpToDate` |
| `workspace-clean-commit-10` | 10 shards | None; untouched Workspace | `UpToDate` |
| `workspace-clean-commit-100` | 100 shards | None; untouched Workspace | `UpToDate` |
| `workspace-clean-commit-500` | 500 shards | None; untouched Workspace | `UpToDate` |
| `workspace-fixed-move-1` | 1 shard | Move one fixed 1 KiB file | `Created` |
| `workspace-fixed-move-10` | 10 shards | Move the same fixed 1 KiB file | `Created` |
| `workspace-fixed-move-100` | 100 shards | Move the same fixed 1 KiB file | `Created` |
| `workspace-fixed-move-500` | 500 shards | Move the same fixed 1 KiB file | `Created` |
| `workspace-distributed-sdk-edit-1` | Fixed 500 shards | 1 singular SDK 4 KiB overwrite | `Created` |
| `workspace-distributed-sdk-edit-10` | Fixed 500 shards | 10 singular SDK 4 KiB overwrites | `Created` |
| `workspace-distributed-sdk-edit-100` | Fixed 500 shards | 100 singular SDK 4 KiB overwrites | `Created` |
| `workspace-distributed-sdk-edit-500` | Fixed 500 shards | 500 singular SDK 4 KiB overwrites | `Created` |
| `workspace-dense-rewrite-1` | Fixed 500 shards | Rewrite 200 files / 1 MiB | `Created` |
| `workspace-dense-rewrite-10` | Fixed 500 shards | Rewrite 2,000 files / 10 MiB | `Created` |
| `workspace-dense-rewrite-100` | Fixed 500 shards | Rewrite 20,000 files / 100 MiB | `Created` |
| `workspace-dense-rewrite-500` | Fixed 500 shards | Rewrite 100,000 files / 500 MiB | `Created` |

There are **16 timed cases and 48 performance samples**, three per case, with
separate verification. No fault, metadata, or link subcases are hidden here.

## Operations and attribution

**Clean Commit:** create the ready real-FUSE Workspace and immediately Commit,
without Exec, listing, stat, payload reads, or mutations. Measure Create,
Commit, and End separately. Require unchanged Branch/head/root, no Commit
insertion, no write transaction, and unchanged Store counts. Whole-tree scan
families instead visit namespace or payload; their clean Commit does not replace
this untouched control.

**Fixed move:** one fresh managed process moves the same prepared 1 KiB file,
`regular/s000/f064.dat`, to `dest/moved.dat` using one ordinary rename. Changed
file count, payload size, and endpoint directories remain constant while the
background grows. Complete the declared sync, close all handles, and exit
before Commit. Do not enumerate the tree merely to locate this known path.

**Distributed SDK edits:** select one eligible file with index j >= 128 in
each selected shard of the fixed 500-shard tree. The deterministic N-element
prefix spreads targets across directories; each target is edited once. Invoke
`Client::edit_workspace_file_range` N times, replacing exactly 4 KiB in place
with independently generated different bytes. No managed Exec remains active.
Report aggregate SDK edit wall and one Commit wall separately. The public batch
API is same-file only; do not invent a multi-file batch. This measures a dirty
file frontier across a Workspace; inherited SDK families own single-file size
and edit geometry.

**Dense rewrite:** one managed process rewrites all 200 files of each selected
shard through ordinary FUSE open/write/close calls. Preserve each logical length
and use independently generated new content. Select N prefixes of one frozen
500-shard order. This is a dense ordinary-filesystem workload, not 100,000 SDK
edit calls. Freeze exact open/truncate/write/sync behavior and count every call
in the workload. Finish all writes and exit before one Commit.

All curves use one Branch, one final unpromoted Commit attempt, and End. Cached
preparation may reuse an identical input Store but cannot precompute output.
Metadata normalization needed for the oracle is explicit measured work limited
to affected paths; the clean curve performs none. Report it separately without
removing it from the operation's wall.

## Independent verification and meaningful gates

Replay transformations against the frozen input manifest outside performance.
Compare every reopened path, bytes, length, type, mode, timestamp, and topology.
The move changes one binding; SDK oracles splice explicit 4 KiB replacements;
dense oracles replace exactly the selected files. Check unchanged files and
subtree identities, not just changed-file hashes. The clean control preserves
the complete input root. Candidate-produced roots are not their own oracle.

Record total and dirty files/bytes separately, SDK calls, FUSE operation/byte
counts, metadata/payload reads, candidate and inserted/reused objects,
transaction maxima, all phase walls, RSS, spool/Store growth, and cleanup.
Sparse dirty work must be distinguishable from full-tree capture or hashing.
Dense work may scale with all selected files/bytes.

The selected-case 1–5 second goal after cached preparation is provisional.
Dense 100,000-file rewriting and full verification may need larger qualified
budgets. Freeze per-case target/hard walls before admission; do not claim the
entire 48-sample campaign is a few seconds or silently skip large tiers.

## Source grounding and completion

- [`sdk_edit_common.rs`](../../../../benchmark/fs-bench-pro/families/sdk_edit_common.rs)
  owns the released four labels and bounded generator infrastructure.
- [`lifecycle.rs`](../../../../crates/layerfs-workspace/src/lifecycle.rs),
  `Workspace::commit` and `Workspaces::edit_workspace_file_ranges`, distinguish
  clean admission, owner edits, publication, and presentation.
- [`changes.rs`](../../../../crates/layerfs-workspace/src/changes.rs),
  `try_build_localized_candidate`, `base_manifest`, and `final_manifest`,
  motivates measuring whole-tree and dirty-frontier dependence.
- [`file_io.rs`](../../../../crates/layerfs-workspace/src/file_io.rs) owns ordinary
  dirty file state; [`client.rs`](../../../../crates/layerfs-sdk/src/client.rs)
  defines the public singular SDK entrypoint.

Completion requires all 16 members, full-tree proofs, qualified fixture/source
identities, bounded samples, cleanup, and pre-admission budgets. Product changes
follow measured causes rather than a prescribed optimization.
