# Dependent agent work episodes

> **Status:** v0.1.3 implementation plan; no measurements or passing claim.
> **Family ID:** `mixed_load_bearing`. Four timed cases; no separate proofs.
> [Shared testing rules](testing-rules.md) own seeds, preparation, custody,
> common lifecycle, timing, limits, and verification admission.

## Question and membership

Can a complete agent workflow read, modify, reorganize, and publish a Workspace
while unrelated content remains exact? Every tier executes complete dependent
episodes. This replaces raw-operation prefixes, whose tier 1 was only a read
and whose cancelling mutations could disappear from a final-state-only oracle.

| Scenario ID | Complete episodes | Managed workload executions | Final LayerFS Commits |
| --- | ---: | ---: | ---: |
| `agent-episodes-1` | 1 | 1 | 1 |
| `agent-episodes-10` | 10 | 1 | 1 |
| `agent-episodes-100` | 100 | 1 | 1 |
| `agent-episodes-500` | 500 | 1 | 1 |

These are the only four members: **12 performance samples**, three per case,
with separate verification. Repeated public Exec, metadata, symlink errors,
and failure containment belong to
[Workspace reliability](workspace-reliability.md), with no duplicate proof rows
or latency distributions here.

## Bounded fixture

Prepare one fixed 64 MiB background using the shared tree generator, including
wide, deep, and regular placements. Freeze 500 independently named episode cells
outside `background/`. Each initially contains an 8 KiB source, an 8 KiB edit
target and a hard-link alias to it, and an 8 KiB replacement target. Counting
the alias twice, these cells contribute 16,384,000 bytes. N selects prefixes
of one deterministic 500-cell schedule; unused cells remain in the oracle.

Each episode may retain one 8 KiB output and one relative symlink whose target
is at most 256 bytes. It may temporarily hold one 8 KiB replacement file, one
4 KiB scratch file, and a 16-byte append visible through both hard-link names.
Only one episode runs at a time. A conservative whole-workload peak is:

```text
64 MiB background
+ 500 * 32 KiB initial cell path bytes
+ 500 * (8 KiB output + 256 symlink target bytes)
+ 8 KiB replacement temp + 4 KiB scratch + 32 alias append bytes
= 87,729,184 bytes < 128 MiB < 1 GiB
```

Background files are at most 48 KiB; episode files at most 8 KiB + 16 bytes.
The bound includes temporary coexistence and alias path lengths, including files
removed before Commit. Receipts and expected copies stay outside the workload
with separately reported storage.

## One dependent episode

One ordinary filesystem process performs these stages for each selected cell:

1. Read the 8 KiB source and derive deterministic replacement bytes and an
   output token from its content and the frozen episode identity.
2. Overwrite a 4 KiB range of the edit target. Read through its hard-link alias
   and use the observed bytes to derive the next output; do not substitute
   expected bytes without reading the filesystem.
3. Append 16 bytes, read the appended region through the alias, then truncate
   back to 8 KiB. This pair has an intermediate observation despite cancelling
   in the final length.
4. Move the cell directory to a prepared sibling destination, then read the
   edited target through its new path. The alias relation must survive.
5. Write and sync an 8 KiB temporary file, close it, rename it over the
   replacement target, and read the permanent name.
6. Create a relative symlink to the replacement and read through it. Create,
   read, and remove a 4 KiB scratch file.
7. Retain one 8 KiB output derived from the earlier observations. Normalize only
   declared changed-path metadata, complete the declared synchronization, and
   proceed to the next cell.

Freeze names, offsets, bytes, metadata, operation order, and receipt shape
before admission. No per-episode Commit, remount, additional Exec, SDK edit, or
Git command belongs here. Reads that feed later operations stay in performance;
full hashes, manifests, and additional probes are verification-only.

After N episodes, the single managed execution exits with all handles closed.
One unpromoted Commit must return `Created`, followed by End and separately
collected fresh reconnect verification. Dedicated families diagnose component
costs; this family measures their dependence and composition.

## Independent oracle and observations

Construct the expected manifest and retained output bytes from the input
fixture and specified transformations, not candidate-produced roots or reported
success. In verification mode observe each stage's reads, append visibility,
lengths, aliases, directory bindings, replacement bytes, symlink targets, and
scratch existence before removal. Preserve these intermediate receipts so
skipped append/truncate and create/remove pairs cannot pass via final equality.

After fresh reopen compare every path, including all background and unselected
cells: bytes, link classes/counts, symlink targets, modes, timestamps, head, and
qualified canonical root. Check the transient bound at mutation boundaries
and require complete mount/process/spool/reader/Workspace/lease cleanup.

## Metrics, budgets, and grounding

Report episodes, per-class operations and bytes, complete workload and
Create/Commit/End walls, SDK/Exec counts, FUSE operations, candidate and
inserted/reused objects, transaction maxima, RSS, spool/Store growth, sync errors,
and cleanup. Sync success remains passive timing evidence; reliability
explicitly tests its error propagation.

The selected-case 1–5 second goal after cached preparation is provisional.
Freeze qualified per-case and verification walls, including 500 episodes. A
complete family campaign or extended reliability run is not a few seconds.
Keep each episode complete at every tier.

Relevant sources are [`cow_tree.rs`](../../../../crates/layerfs-workspace/src/cow_tree.rs)
for links/rename/unlink, [`file_io.rs`](../../../../crates/layerfs-workspace/src/file_io.rs)
for ordinary writes/truncate, [`filesystem.rs`](../../../../crates/layerfs-fuse/src/filesystem.rs)
for FUSE operations, and [`execution.rs`](../../../../crates/layerfs-workspace/src/execution.rs)
for managed execution. Existing SDK alias and mixed-edit checks in
[Workspace tests](../../../../crates/layerfs-workspace/tests/file_edit.rs)
remain regression coverage; they do not replace this ordinary-tool route.
