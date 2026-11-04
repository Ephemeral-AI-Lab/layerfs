# Ports

[Architecture](../../README.md) › [Engine](../README.md) › [Workspace](README.md) › Ports

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

What kernel routes and Commit call into.

| | |
| --- | --- |
| Component | [Workspace](README.md) |
| Source | [`engine/crates/layerfs-workspace/src/ports`](../../../../engine/crates/layerfs-workspace/src/ports) |

Each port is a trait that the Workspace calls and some other crate
implements. Every method is one short overlay job: it does no provider,
content or Store I/O.

```text
  layerfs-workspace (calls)              implemented by
  +------------------------------+
  | OverlayRead, OverlayJobs     |-----> layerfs_overlay::Overlay  (direct)
  | OverlayFileRead              |-----> OwnerClient (store-service:
  | OverlayCapturedRuns          |        under the Workspace's lock)
  | OverlayCapturedNamespace     |
  | OverlayOperationRecords      |
  +------------------------------+
  | FileLengths                  |-----> CanonicalClient, StorePorts
  | FileDigests                  |-----> StoreDigests (store-service)
  +------------------------------+
```

| Implementation | Source |
| --- | --- |
| Direct `Overlay` impls | [`ports/`](../../../../engine/crates/layerfs-workspace/src/ports) |
| `OwnerClient` read and job ports | [`read_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/read_port.rs), [`file_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/file_port.rs), [`captured_namespace_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs), [`operation_record_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/operation_record_port.rs) |
| One lock per Workspace connection | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| `StoreDigests`, `StorePorts` (lengths) | [`store/digests.rs`](../../../../engine/crates/layerfs-store-service/src/store/digests.rs), [`store/ports.rs`](../../../../engine/crates/layerfs-store-service/src/store/ports.rs) |
| `OwnerClient` captured runs | [`captured_run_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_run_port.rs) |

## Overlay port

[`overlay.rs`](../../../../engine/crates/layerfs-workspace/src/ports/overlay.rs)

| Trait | Method | One job returns |
| --- | --- | --- |
| `OverlayRead` | `inode(source, serial)` | The local inode row, or none |
| | `directory_entry(source, parent, name)` | The local name row (bound or whiteout), or none |
| | `directory_entries(source, parent, after)` | One `DirectoryEntryWindow`: active and captured rows, at most 64 each |
| | `cell(source, serial, generation, offset)` | One cell at an exact generation |
| | `read(source, serial, offset, length)` | The local layers of one read window, or none (wholly inherited) |
| `OverlayJobs` | `namespace(job)` | One `JobOutcome`: evaluation and its single publication in the same job |

```text
  OwnerClient::namespace(job)
     lock Workspace connection  (wait behind a pending capture/install)
       job.perform(db)          evaluate + at most one apply
     unlock
```

An error after the attempt is the exact outcome; the port never retries.

## Captured namespace and runs

Commit reads a sealed capture through these ports while the live Workspace
keeps changing. A `CapturedReader` fixes the root and installed floor.

```text
  live rows (active generation)      keep changing, not read here
  ---------------------------------------------------------------
  captured rows (sealed generation)  <-- CapturedReader
     captured_inode_page(after)      inodes by serial, <= 64 per page
     captured_directory_entry_page   names by (parent, name), whiteouts
     captured_directory_entries      one parent's names
     captured_run_step(cursor)       sparse payload runs, one unit
```

| Trait | Method | Purpose | Source |
| --- | --- | --- | --- |
| `OverlayCapturedRuns` | `captured_inode`, `captured_run_step` | One inode point; one bounded step over a file's captured runs | [`captured_runs.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_runs.rs) |
| `OverlayCapturedNamespace` | `captured_inode_page`, `captured_directory_entry_page`, `captured_directory_entries`, `captured_directory_entry` | Replayable pages of captured changes, not a merged namespace | [`captured_namespace.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_namespace.rs) |
| | `captured_symlink` | A captured symlink target | same |
| | `keep_live_row` | Keeps an mtime-only inode's live row past install | same |
| | `captured_digests`, `captured_predecessors` | Whole-content digests and rename predecessors, at most 64 serials | same |

An empty page ends a sequence. Replaying a sealed page is reading, not
retrying a failed operation. How Commit uses these is in
[Commit: Capture](../commit/capture.md).

## Files and lengths

| Trait | Method | Rule | Source |
| --- | --- | --- | --- |
| `OverlayFileRead` | `file_read(FileRead, offset, length)` | Read through an open descriptor; works after the last name is gone | [`files.rs`](../../../../engine/crates/layerfs-workspace/src/ports/files.rs) |
| | `captured_read(reader, serial, offset, length)` | Read the sealed bytes at the capture's own root | same |
| `FileLengths` | `file_length(id)`, `file_lengths(ids)` | Store metadata for a content root; no payload fallback | [`lengths.rs`](../../../../engine/crates/layerfs-workspace/src/ports/lengths.rs) |
| `FileDigests` | `record_file_digests`, `file_roots` | BLAKE3 of a file's bytes to its saved root; `FILE_DIGEST_WINDOW` = 64 rows per call | [`digests.rs`](../../../../engine/crates/layerfs-workspace/src/ports/digests.rs) |

The caller of a descriptor or captured read keeps its token until every
consumer of the bytes has finished.

## Operation records

`OverlayOperationRecords` is a bounded key-value service in the Workspace's
overlay file. Commit construction keeps its scratch records there, and SDK
recursive writes keep their hard-link maps there
([`operation_record.rs`](../../../../engine/crates/layerfs-workspace/src/ports/operation_record.rs)).

| Method | Returns |
| --- | --- |
| `operation_record_contains`, `operation_record_get` | One key |
| `operation_record_get_many` | At most 64 keys, in the order asked |
| `operation_record_apply` | `Applied`, or `NotApplied { index, key, actual, original }`: the first failed guard |
| `operation_record_keys_after` | At most 64 sorted keys; a short window means the kind ends |
| `operation_record_entries_after` | At most 64 (key, value) rows within one record envelope |

Each result is one original attempt. A refused input returns
`OperationRecordInputRefusal` with the original changes. The record tables are
described in [Overlay: Operation records](../overlay/operation-records.md).

## Related

- [Mutation](mutation.md): the job sent through `OverlayJobs`.
- [Operations](operations.md): the reads sent through `OverlayRead` and `OverlayFileRead`.
- [Overlay](../overlay/README.md): the database behind every port.
- [Commit: Capture](../commit/capture.md) and [Commit: Construction](../commit/construction.md): callers of the captured and record ports.

---

← [Workspace: Mutation](mutation.md) · [Up: Workspace](README.md) · [Workspace: State](state.md) →
