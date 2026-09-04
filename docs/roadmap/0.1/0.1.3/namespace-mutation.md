# Populated subtree mutation

> **Status:** v0.1.3 implementation plan; no measurements or passing claim.
> **Family ID:** `namespace_mutation`. Four timed cases; no separate proofs.
> [Shared testing rules](testing-rules.md) own infrastructure, seeds, custody,
> timing, verification, size limits, and campaign admission.

## Question and scope

What does moving or deleting a large populated subtree cost when a substantial
untouched Workspace must survive exactly? Scale affected subtree size while
holding the background fixed. A directory rename remains one namespace syscall
even when it relocates 100,000 files; do not call that 100,000 rename operations.

This replaces the earlier four small independent mutation mixtures. Creation
belongs to the tiny-file and directory families; single-file moves belong to
[Workspace change locality](workspace-change-locality.md). Link lifetime and
invalid mutations belong to [Workspace reliability](workspace-reliability.md).

## Fixture and exact membership

Use a fixed untouched `background/` of 100,000 independently generated files,
each 2,500 bytes: exactly 250,000,000 logical bytes. Each of `source/tree-a/`
and `source/tree-b/` contains N shards, each with 200 independently generated
1 KiB files. Prepare empty `destination/` before timing. Freeze directory names,
placement, metadata, and whole-tree manifests; N uses nested shard prefixes.

| Scenario ID | N | Files in each affected tree | Initial regular files | Peak logical file bytes |
| --- | ---: | ---: | ---: | ---: |
| `namespace-subtree-relocate-delete-1` | 1 | 200 | 100,400 | 250,409,600 |
| `namespace-subtree-relocate-delete-10` | 10 | 2,000 | 104,000 | 254,096,000 |
| `namespace-subtree-relocate-delete-100` | 100 | 20,000 | 140,000 | 290,960,000 |
| `namespace-subtree-relocate-delete-500` | 500 | 100,000 | 300,000 | 454,800,000 |

The primary unit is **affected subtree shards**, not operation count. These are
the only four members. All files are at most 2,500 bytes; the initial state is
the peak, strictly below 1 GiB. The bound includes both affected trees, including
the one subsequently deleted. No temporary copied tree is permitted.

## Measured operation

One real-FUSE Workspace and one fresh managed workload execution perform:

1. Rename `source/tree-a` to `destination/moved-a`, changing parent and basename
   with one native directory rename. Do not copy or traverse A in the workload.
2. Remove all files and directories under `source/tree-b` using a frozen
   deterministic postorder traversal and ordinary unlink/rmdir calls.
3. Complete the declared synchronization and exit before one LayerFS Commit.

Record move and delete phase walls separately, then Commit and End separately.
Use one Branch and one final unpromoted Commit. The expected outcome is
`Created`; fresh reconnect and full verification are outside performance.
Any metadata normalization needed for the oracle is a declared operation in
the workload phase; it may touch only the declared affected paths.

## Independent verification

Derive the final manifest by renaming every A path, removing every B path, and
retaining every other path from the frozen input manifest. Compare the full
reopened Workspace: paths/types, bytes, lengths, modes, normalized timestamps,
and directory structure. Check the moved tree immediately after rename in
verification mode. Verify absence of both old tree bindings and all deleted
descendants, and exact equality of every background file.

Final regular-file count is `100000 + 200*N`; final logical bytes are
`250000000 + 204800*N`. Derive directory counts from the frozen manifests.
Check published head and canonical root against independently qualified
expectations. Report payload object reuse and unchanged subtree identities;
zero inserted payload is the expected result of namespace-only work, not a
claim of zero metadata work. Require complete runtime cleanup.

## Measurements, budgets, and grounding

Report actual rename/unlink/rmdir counts, affected/untouched paths, payload
reads/writes, metadata/object reads, candidate and inserted/reused objects/bytes,
transaction maxima, RSS, spool/Store growth, and all phase walls. Moving A and
deleting B have different scaling behavior and remain separately attributed.

The selected-case 1–5 second goal after cached preparation is provisional.
Large deletion and full verification can exceed it; qualify and freeze each
phase's target/hard wall before admission. The complete family contains
**12 performance samples**, three per case, plus separate verification.

- [`changes.rs`](../../../../crates/layerfs-workspace/src/changes.rs):
  `try_build_localized_candidate`, `base_manifest`, and `final_manifest`
  expose the risk of whole-tree work for directory mutations.
- [`cow_tree.rs`](../../../../crates/layerfs-workspace/src/cow_tree.rs):
  `rename`, `unlink`, and `replace_path_prefix` own live namespace semantics.
- [Existing mutation tests](../../../../crates/layerfs-workspace/tests/file_edit.rs):
  `group_3_rename_parent_replace_unlink_and_final_alias_reclamation_are_inode_exact`
  retains basic correctness without another timed micro-family.

Completion requires all four members, exact full-tree proofs and transient
bounds, unchanged background, cleanup, complete source/fixture/oracle identities,
and qualified pre-admission budgets.
