# Updates

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Building or updating a tree from sorted final rows, deriving reference counts
and logical size, within [Filesystem trees](README.md).

| | |
| --- | --- |
| Source | [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs), [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`rows/`](../../../../core/crates/layerfs-content/src/filesystem/rows), [`state/`](../../../../core/crates/layerfs-content/src/filesystem/state), [`references/`](../../../../core/crates/layerfs-content/src/filesystem/references), [`statistics.rs`](../../../../core/crates/layerfs-content/src/filesystem/statistics.rs) |

## update.rs

```text
 input: base root (or none), sorted DirectoryUpdates, InodeUpdates,
        new serials, FilesystemResources
 [validate]     check base root, scope, root serial; new serials absent
   |            from base; topology, no cycle            (validation.md)
 [directories]  per changed parent, in parent order:
   |              apply_bindings --> new directory tree root
   |              observe(before, after) --> reducer (add before remove)
 [references]   reducer.finish: derived counts + final values, base
   |            records read in bounded waves; zero-count dirs released
 [inodes]       apply_inode_values: sorted merge of final rows into table
 [cleanup]      ordering backing released once (a failure fails the op)
 [root.encode]  base.with_inode_table(new table) or a new FilesystemRoot
   v
 FilesystemResult { root, value, counters }
```

| Entry point | Base | Input |
| --- | --- | --- |
| `build_filesystem[_timed]` | none | resident `PreparedRows` |
| `update_filesystem[_timed]` | required | resident `PreparedRows` |
| `build_filesystem_streamed[_timed]`, `update_filesystem_streamed[_timed]` | none / required | `PreparedDirectoryStreams` |
| `build_filesystem_streamed_backed[_timed]`, `update_filesystem_streamed_backed[_timed]` | none / required | streams plus a caller `IndexedConstructionBacking` for serial state |

A build with a base, or an update without one, is `InvalidRecord`. The
bracketed phases are the scopes recorded through `FilesystemPhases`
([`objects.rs`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs))
when timing is on; disabled timing runs the same code.

| Caller | Entry | Source |
| --- | --- | --- |
| Workspace Commit construction | `update_filesystem_streamed_backed` | [`construction/driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs) |
| Branch merge | `update_filesystem_streamed_backed` | [`merge/build.rs`](../../../../core/crates/layerfs-content/src/merge/build.rs) |
| Project Init | the primitives `build_directory`, `build_table`, `FilesystemRoot::new` | [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |

A directory with no changes keeps its stored root. A content-only file change
touches only the inode table. Restating a binding keeps the root ID. A
directory the operation allocates but never binds is dropped (no page is
built; its bindings are still counted, `unreachable_parents`).

`FilesystemUpdateCounters` counts object reads and emits, sorted work
(`pages_read`, `pages_created`, `pages_reused`, `untouched_subtrees`,
`peak_scratch_bytes`), reference and release work, bindings and validation.
They count work; they are not a persistence claim.

## input.rs and rows/

```text
 PreparedRows (resident)              PreparedDirectoryStreams (streamed)
 directories: [DirectoryUpdate]       directory_headers(): parent order
   parent, [(name, Some/None)]        directory_changes(parent): cursor
 inodes: [InodeUpdate]                inodes, new serials: cursors
 new_inodes: [serial]                 base, scope, root serial, resources
 base, scope, root serial, resources  every cursor replays from the start
```

| Field | Meaning | Source |
| --- | --- | --- |
| `DirectoryUpdate.changes` | final binding of each changed name, strictly sorted and unique; `None` = absent | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| `InodeUpdate.value` | typed final value; its count is replaced by the derived count | same |
| `new_inodes` | serials the caller's allocator just created; checked absent from the base | same, [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| `scratch_bytes` | sorted-engine scratch; default 4 MiB, at least 1,024 | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |
| `maximum_pending_records`, `merge_buffer_bytes`, `ordering_bytes` | reducer memory (default 4,096 rows), merge buffer (16 KiB), the one ordering ceiling (64 MiB) | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs), [`references/runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs) |
| `base_read_batch` | base records per read wave, capped at 4,096 | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`objects.rs`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs) |

Declared row totals are checked against what the cursors return; an
under-declared source is refused
([`rows/check.rs`](../../../../core/crates/layerfs-content/src/filesystem/rows/check.rs)).
A receive path can hold rows in a charged file spool of fixed 32-byte slots
([`rows/spool.rs`](../../../../core/crates/layerfs-content/src/filesystem/rows/spool.rs)).
On the resident route more than `ordering_bytes / 16` rows is
`ObjectLimitExceeded`.

## state/

With backed serial state the operation keeps counts, dropped parents and
rebuilt directory roots in the caller's records instead of memory
([`state/store.rs`](../../../../core/crates/layerfs-content/src/filesystem/state/store.rs),
[`state/roots.rs`](../../../../core/crates/layerfs-content/src/filesystem/state/roots.rs)).
No resident container grows with the change, and no total is refused against
the ordering budget.

## references/

"Reference" here means a name binding counted against an inode; object
references belong to [Objects](../objects/README.md).

```text
 directory merge --observe(before, after)--> reducer
                                               | pending rows (bounded)
                                               v
                             spill: sorted run --> tiered runs
                                               |
                                               v
                  final rows (serial, count, value) --> inode table merge
```

| Part | What it holds | Source |
| --- | --- | --- |
| Ordering record | fixed 96 bytes: serial, version, tag (1 new-inode count, 2 effect total), length, value flag, tally, optional 73-byte value, reserved zeros | [`record.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/record.rs) |
| Pending map | at most `maximum_pending_records` rows in memory | [`runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs) |
| Runs | sorted spilled batches merged into tiers; the newer row wins on equal keys; at most 32 tiers | [`runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs), [`merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/merge.rs) |
| Ordering backing | caller-supplied run storage with one byte account, released exactly once; `FileBacking` is the local implementation | [`backing.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/backing.rs) |
| Indexed state | with backed serial state, the same meanings in caller records instead of runs | [`indexed.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/indexed.rs), [`operation.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/operation.rs) |

## statistics.rs

```text
 logical_size(root) = sum over inode rows of
                      file length x namespace_ref_count  (regular files)
 logical_size_after_with(before, after, known, lengths)
   = known + sum over changed inode rows of
     (+ after length x count) (- before length x count)
     lengths asked in windows of LENGTH_WINDOW = 1024 roots
```

| Function | Reads | Used by |
| --- | --- | --- |
| `logical_size` | every inode leaf, one window at a time, and each file's state | Project Init ([`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs)) |
| `logical_size_after_with` | only inode pages that differ between the two tables | Commit and Branch history ([`store/commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs)) |

A hard-linked file counts once per binding; directories and symlinks count
zero. These are logical bytes, independent of encoding and storage.

## Related

- [Filesystem trees](README.md): pages, attributes and links.
- [Validation](validation.md): the first phase and every limit.
- [Compare and merge: Merge](../compare-and-merge/merge.md): a merge builds
  its result through this backed update.
- [Engine: Commit](../../engine/commit/README.md),
  [Engine: Project Init](../../engine/project-init/README.md): the callers.
