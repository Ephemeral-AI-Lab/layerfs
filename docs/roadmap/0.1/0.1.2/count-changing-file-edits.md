# Count-changing file-edit performance family

> **Status:** Archived; retained for historical evidence only.

This document records the superseded `edit_count_changing` POSIX/FUSE family
from issues #15/#19. Its 31 IDs, partial 1/10/100 MiB scaling supplement,
container workload process, direct POSIX mutations, and temp-copy/fsync/rename
paths remain immutable reproducibility evidence. They are not active v0.1.2
admission, a baseline, a paired arm, or a release claim.

Issue [#20](https://github.com/Ephemeral-AI-Lab/layerfs/issues/20) replaces this
model with the 32-ID
[`edit_length_changing`](sdk-only-edit-benchmark-rebuild.md#family-2-edit_length_changing)
family: insert, delete, append, prepend, grow, shrink, truncate, and zero
extension, each at exact 1/10/100/500 MiB. Every new row is one singular public
SDK edit followed by one Commit and one End. Composite sparse/batch performance
is outside issue #20.

Historical paths remain unchanged:

```text
benchmark/fs-bench-pro/families/edit_count_changing.rs
benchmark/fs-bench-pro/run-edit-count-changing.sh
benchmark-results/fs-bench-pro/edit-count-changing/
```

Active paths are distinct:

```text
benchmark/fs-bench-pro/families/edit_length_changing.rs
benchmark/fs-bench-pro/run-edit-length-changing.sh
benchmark-results/fs-bench-pro/edit-length-changing/
```

Canonical extent-count preservation/increase/decrease is no longer deferred;
it is the separate complete 12-ID `edit_canonical_chunk_count` family owned by
#20.
