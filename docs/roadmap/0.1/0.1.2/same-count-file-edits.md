# Same-count file-edit performance family

> **Status:** Archived; retained for historical evidence only.

This document records the superseded `edit_same_count` POSIX/FUSE family from
issues #13/#19. Its 14 timed IDs, A/A policy, workload process, FUSE writes,
and retained results remain immutable and reproducible. They are not active
v0.1.2 admission, a baseline, a paired arm, or a release claim.

Issue [#20](https://github.com/Ephemeral-AI-Lab/layerfs/issues/20) replaces this
model with the 12-ID
[`edit_length_preserving`](sdk-only-edit-benchmark-rebuild.md#family-1-edit_length_preserving)
family: head, middle, and tail 4 KiB overwrites, each at exact
1/10/100/500 MiB. Every new row uses one singular
`Client::edit_workspace_file_range` call, one Commit, and one End, with zero
edit-caused FUSE payload and zero Workspace spool.

Historical paths remain unchanged:

```text
benchmark/fs-bench-pro/families/edit_same_count.rs
benchmark/fs-bench-pro/run-edit-same-count.sh
benchmark-results/fs-bench-pro/edit-same-count/
```

Active paths are distinct:

```text
benchmark/fs-bench-pro/families/edit_length_preserving.rs
benchmark/fs-bench-pro/run-edit-length-preserving.sh
benchmark-results/fs-bench-pro/edit-length-preserving/
```
