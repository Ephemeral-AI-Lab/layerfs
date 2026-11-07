# Construction

[Architecture](../../README.md) › [Engine](../README.md) › [Project Init](README.md) › Construction

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Building the project's first Commit: file content, directories, the inode
table and the filesystem root, saved through Storage and published as a
genesis LayerStack.

| | |
| --- | --- |
| Component | [Project Init](README.md) |
| Source | [`engine/crates/layerfs-project/src/import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs), [`engine/crates/layerfs-project/src/store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs) |

## Building the first Commit

```text
 init_observed                                  InitStep   timer child
 -----------------------------------------------------------------------
 check_root, outside_source, acquisition.begin   Scan
 Save #1 (prerequisite)
   scan: rows + symlink targets  ---------------           import_scan
   Save #1 finish                                    finish_prerequisite_save
 Save #2 (files)                                 Files
   save_files: construct_stream per identity               import_files
   Save #2 finish                                          import_finish_save
 reserve_inodes(scope, entries) -> start serial  Tree      reserve_inodes
 Save #3 (tree)
   build_namespace: directories, table, root               import_tree
   (Save #3 left open)
 backing.finish: discard rows, release           Publish   discard_acquisition
 Save #3 finish                                            finish_tree_save
 logical_size(root); record_new_bindings                   record_bindings
 initialize_layerstack(genesis_root = root)                initialize_layerstack
 -----------------------------------------------------------------------
 caller (store/): fork first Branch from head layer   Seal
   Init and seal: record_init, seal    live add: close session
```

Timer names carry a `history.` prefix in source
([`init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs)).

| Step | Library call | What it produces | Source |
| --- | --- | --- | --- |
| Files | Content `construct_stream(policy, capacities, file, sink)` | one file root per host identity | [`files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Serials | `HistoryCatalog::reserve_inodes` | one contiguous reservation; serial = `start + position` | [`init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| Directories | Content `build_directory` over each parent's bindings, in name order | one directory root per directory, written back with `set_directory_roots` | [`namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| Empty directory | Content `empty_directory`, once, if any directory bound nothing | the one canonical empty page | [`namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| Inode table | Content `build_table` over entries in position order | the table; regular files read their root from `file_roots` | [`namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| Root | `FilesystemRoot::new(profile_id, scope, root_serial, table)` | a `FilesystemRoot` object referencing the table | [`namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| Genesis | `HistoryCatalog::initialize_layerstack` | the `LayerStackRecord` with `genesis_root` and logical size | [`init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| First Branch | `HistoryCatalog::fork` from `ForkSource::Layer(head_layer)` | `BranchSnapshot` | [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs), [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs) |

Three Saves keep each step reading only finished objects: the tree Save reads
symlink target and file roots that the first two Saves already finished.
Nothing assumes a reader that sees objects the same Save is still producing.
If the tree Save fails, earlier objects may stay unreferenced; no cleanup of
them is attempted
([`namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs)).

The inode scope comes from `scope_for_seed(scope_seed)`; no caller supplies a
serial. Each inode's `namespace_ref_count` is 1 per binding: the root has 0,
other entries 1, and a regular file adds its alias count. The root serial plus
the entry count must stay within `i64::MAX`, or Init fails `Capacity`.

`Initialized` returns `stack`, `root`, `root_serial`, `entries`, Storage
`diagnostics` and `namespace_work`
([`init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs)).
The Init-and-seal route also records `InitStatistics` once per Store
(`record_init`); an added project records none
([`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs)).

## Parallel construction

```text
                 owner thread (calls init; owns Save and backing)
   backing.jobs() --> job feed: sync_channel(512) --try_send--+
        ^                                                     |
        |          +------------+------------+------------+   v
        |          | worker 1   | worker 2   | worker 3   | worker 4
        |          | open file, check stamp, construct_stream,
        |          | re-check length and stamp, deadline
        |          +------------+------------+------------+
        |                       |  ImportBatch (objects, Done)
        |                       v  sync_channel(4)
        |          owner: Save::accept(object); Done -> roots window
        +--------- complete_files every 4096 roots
```

| Property | Init (this page) | Commit |
| --- | --- | --- |
| Producers | 4 file constructors (`INIT_WORKERS`) feeding one Save owner | one construction producer thread feeding one Save owner (owner approval, 2026-11-07) |
| Who writes the Save | the owner thread only | the Commit thread only |
| Queue | 32 batches of at most 256 KiB | the same bounds, plus same-Save read requests |
| Selected by | the constant in [`files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) | design; see [Commit: Construction](../commit/construction.md#one-producer) |

Init keeps this parallel profile by owner rule; Commit and other measured
operations use one producer and run with `LAYERFS_CONSTRUCTION_WORKERS=1`.
A Commit's one producer runs beside its Save on a second thread; it is not a
second producer.
No file under `engine/crates/*/src` reads that variable
([`r5_10_producer_profile.rs`](../../../../engine/crates/layerfs-daemon/tests/r5_10_producer_profile.rs));
Init's worker count is fixed in source.

| Check in each job | Failure | Source |
| --- | --- | --- |
| Deadline before and after construction | `Deadline` | [`files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Opened file matches its scan stamp | `InvalidInput { Changed }` | [`files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Built length equals stamp length; file and path still match | `InvalidInput { Changed }` | [`files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| A worker or channel is gone | `WorkerUnavailable` | [`files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |

The owner waits on batches with a 500 ms receive timeout so it can check the
deadline; that wait is a readiness wait, not a retry. Neither queue grows with
the size of the tree.

## Related

- [Import](import.md): the rows and windows construction reads.
- [Seal and install](seal-and-install.md): what follows the genesis LayerStack.
- [Core: Files: Construction](../../core/files/construction.md): `construct_stream`.
- [Core: Filesystem trees: Updates](../../core/filesystem-trees/updates.md): directory and table builders.
- [Core: Physical storage: Saves](../../core/physical-storage/saves.md): `Save::accept` and `Save::finish`.
- [Core: History model: Allocation](../../core/history-model/allocation.md): inode reservations.
- [Commit: Construction](../commit/construction.md): the one-producer route.

---

← [Project Init: Import](import.md) · [Up: Project Init](README.md) · [Project Init: Seal and install](seal-and-install.md) →
