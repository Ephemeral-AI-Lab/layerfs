# Updates

[Architecture](../../README.md) › [Core](../README.md) › [Filesystem trees](README.md) › Updates

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Updating an existing tree.

| | |
| --- | --- |
| Component | [Filesystem trees](README.md) |
| Source | [`core/crates/layerfs-content/src/filesystem/update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs), [`core/crates/layerfs-content/src/filesystem/state`](../../../../core/crates/layerfs-content/src/filesystem/state), [`core/crates/layerfs-content/src/filesystem/input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`core/crates/layerfs-content/src/filesystem/statistics.rs`](../../../../core/crates/layerfs-content/src/filesystem/statistics.rs) |

## Updating a tree

```text
 input: base root (or none), sorted DirectoryUpdates, InodeUpdates,
        new serials, FilesystemResources
   |
   v
 [validate]     load + check base root, scope, root serial;
   |            new serials absent from base; topology, no cycle
   v
 [directories]  per changed parent, in parent order (base_read_batch rows):
   |              apply_bindings --> new directory tree root
   |              observe(before, after) --> reducer (add before remove)
   v
 [references]   reducer.finish: derived counts + final values, base
   |            records read in bounded waves; zero-count dirs released
   v
 [inodes]       apply_inode_values: sorted merge of final rows into table
   v
 [cleanup]      ordering backing released once (failure fails the op)
   v
 [root.encode]  base.with_inode_table(new table) or a new FilesystemRoot
   |            --> FinalizedObject(FilesystemRoot) to the consumer
   v
 FilesystemResult { root, value, counters }
```

| Entry point | Base | Input | Source |
| --- | --- | --- | --- |
| `build_filesystem`, `build_filesystem_timed` | none | resident `PreparedRows` | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |
| `update_filesystem`, `update_filesystem_timed` | required | resident `PreparedRows` | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |
| `build_filesystem_streamed[_timed]`, `update_filesystem_streamed[_timed]` | none / required | `PreparedDirectoryStreams` | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |
| `build_filesystem_streamed_backed[_timed]`, `update_filesystem_streamed_backed[_timed]` | none / required | streams plus caller `IndexedConstructionBacking` for serial state | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |

A build with a base, or an update without one, is refused with
`InvalidRecord`. The phase names in brackets are the scopes recorded through
[`FilesystemPhases`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs) when the caller enables timing; disabled
phases run the same code.

| Caller | Entry used | Source |
| --- | --- | --- |
| Workspace Commit construction | `update_filesystem_streamed_backed` | [`construction/driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs) |
| Branch merge | `update_filesystem_streamed_backed` | [`merge/build.rs`](../../../../core/crates/layerfs-content/src/merge/build.rs) |
| Project Init | the primitives `build_directory`, `build_table` and `FilesystemRoot::new` directly | [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |

A directory whose change list is empty keeps its stored root. A content-only
change to a file touches only the inode table, never a directory page. A change
that restates an existing binding leaves the root ID unchanged
(`a_same_name_no_op_keeps_the_root_identity` in
[`filesystem_updates.rs`](../../../../core/crates/layerfs-content/tests/filesystem_updates.rs)).

## Update state and input

```text
 PreparedRows (resident)                 PreparedDirectoryStreams (streamed)
 +---------------------------------+     +-----------------------------------+
 | directories: [DirectoryUpdate]  |     | directory_headers(): parent order |
 |   parent, [(name, Some/None)]   |     |   DirectoryHeader {parent, rows}  |
 | inodes:      [InodeUpdate]      |     | directory_changes(parent): cursor |
 | new_inodes:  [serial]           |     | inodes, new serials: cursors      |
 | base, scope, root_serial,       |     | base, scope, root_serial,         |
 | resources                       |     | resources                         |
 +---------------------------------+     +-----------------------------------+
        each cursor is a full pass, replayable from the start
```

| Input field | Meaning | Source |
| --- | --- | --- |
| `DirectoryUpdate.changes` | Final binding of each changed name, strictly sorted and unique; `None` means the name is absent | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| `InodeUpdate.value` | Typed final value; its count is replaced by the derived count | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| `new_inodes` | Serials the caller's allocator just created; checked absent from the base | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| `FilesystemResources.scratch_bytes` | Sorted engine scratch; default 4 MiB, at least 1,024 | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| `maximum_pending_records`, `merge_buffer_bytes`, `ordering_bytes` | Reducer memory, merge buffer and the one ordering byte ceiling | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`references/runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs) |
| `base_read_batch` | Base records one read wave may request (capped at 4,096) | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`objects.rs`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs) |

Declared row totals are checked against what the cursors return
([`rows/check.rs`](../../../../core/crates/layerfs-content/src/filesystem/rows/check.rs)); an under-declared source is refused. A
receive path can hold rows in a charged file spool with fixed 32-byte slots
([`rows/spool.rs`](../../../../core/crates/layerfs-content/src/filesystem/rows/spool.rs)). With backed serial state the
operation keeps counts, dropped parents and rebuilt directory roots in the
caller's records ([`state/store.rs`](../../../../core/crates/layerfs-content/src/filesystem/state/store.rs),
[`state/roots.rs`](../../../../core/crates/layerfs-content/src/filesystem/state/roots.rs)), so no resident container grows with
the change and no total is refused against the ordering budget.

A directory this operation allocates but never binds is dropped: no page is
built for it, and its bindings are still counted
(`unreachable_parents` in [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs)).

## Statistics

```text
 logical_size(root)          = sum over inode rows of
                               file length x namespace_ref_count
                               (regular files only)

 logical_size_after_with(before, after, known, lengths)
   = known + sum over changed inode rows of
     (+ after length x count) (- before length x count)
     lengths asked in windows of LENGTH_WINDOW = 1024 roots
```

| Function | Reads | Used by | Source |
| --- | --- | --- | --- |
| `logical_size` | every inode leaf, one window at a time, and each file's state | Project Init | [`statistics.rs`](../../../../core/crates/layerfs-content/src/filesystem/statistics.rs), [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| `logical_size_after_with` | only inode pages that differ between the two tables | Commit and Branch history | [`statistics.rs`](../../../../core/crates/layerfs-content/src/filesystem/statistics.rs), [`store/commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |

A hard-linked file counts once per binding. Directories and symlinks count
zero. These are logical bytes, independent of encoding and storage.

Each operation also returns [`FilesystemUpdateCounters`](../../../../core/crates/layerfs-content/src/filesystem/update.rs): object
reads and emits, sorted directory and inode work (`pages_read`,
`pages_created`, `pages_reused`, `untouched_subtrees`, `peak_scratch_bytes`),
reference and release work, bindings added and removed, and validation work.
They count work done; they are not a persistence claim.

## Related

- [Representation](representation.md) — the pages the merge rewrites.
- [Attributes and links](attributes-and-links.md) — how counts are derived.
- [Validation](validation.md) — the checks in the first phase.
- [Engine: Commit](../../engine/commit/README.md), [Engine: Project Init](../../engine/project-init/README.md) — the callers.
- [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md).

---

← [Filesystem trees: Attributes and links](attributes-and-links.md) · [Up: Filesystem trees](README.md) · [Filesystem trees: Validation](validation.md) →
