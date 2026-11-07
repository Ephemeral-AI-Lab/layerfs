# Construction

[Architecture](../../README.md) › [Engine](../README.md) › [Commit](README.md) › Construction

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Constructing the captured namespace.

| | |
| --- | --- |
| Component | [Commit](README.md) |
| Source | [`engine/crates/layerfs-workspace/src/construction`](../../../../engine/crates/layerfs-workspace/src/construction), [`engine/crates/layerfs-store-service/src/store/captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs), [`engine/crates/layerfs-store-service/src/store/producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs) |

## Driver

```text
 BoundWorkspace::commit_captured(options)          store/captured.rs
   admit_commit
   commit_admitted( construct = |save, capture, _| {
       acquire_captured_reader, acquire_operation   (two jobs)
       CapturedNamespace::new(workspace, overlay, reader, owner)
           .with_digests(StoreDigests)
           .construct(policy, capacities, save, Staging{save.sink()})
       classify(result, first original cause)
   })
   release_captured  (reader, then operation owner; their reclamation
                      stays queued for reclaim after the answer)
```

| Piece | What it does | Source |
| --- | --- | --- |
| `commit_captured` / `commit_captured_via` | Product constructor; wraps the driver with owner acquisition and release | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| `commit` | Same driver with a caller-supplied constructor (driver tests and `Service::execute`) | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| `Staging` | Forwards each `FinalizedObject` to the Save's sink and offers it to `ConstructedObjects` | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| `classify` | Turns the Content label back into the first original cause (`Owner`, `Content` with provider, `Workspace`) | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |

The objects staged for the shared cache are admitted only in the `published`
callback, after History accepted the publication; on every other path they
are dropped. The construction policy is the Store's
(`self.store.policy().construction()`), the same policy Init uses.

## Namespace construction

```text
 CapturedNamespace::construct                         construction/driver.rs
  1 bind     reader engine == route engine; base rebound to reader.root()
  2 begin    CONTEXT record (unsealed)
  3 names    pass over captured names  --> HEADER per changed parent;
             same-path base files of names bound over a lower binding
  4 values   pass over captured inodes --> VALUE, FRESH; changed files built
  5 seal     CONTEXT record with totals
  6 update   Cursor over sealed records --> Content
             update_filesystem_streamed_backed --> new FilesystemRootId
  7 counts   remember_constructed_counts (cache fill only)
```

| Inode row | Value built | Content entry point | Source |
| --- | --- | --- | --- |
| Fresh file, bytes already saved | Reuses the saved root (#364) | none | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Fresh file | New file root from captured runs; a rename predecessor, else the base file at the same path (#88), is offered as delta base: positionally to chunks, whole to a whole-file result | `construct_runs_with_predecessor` | [`owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs) |
| Changed base file | Indexed edits over the retained `FileView` | `apply_indexed_edits_view_backed` | [`owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs) |
| Fresh symlink | Target from the capture | `emit_symlink` | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Fresh directory | Page built by Content from its header | streamed update | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Base inode, only mtime differs | Base row kept; live row carried past install (`keep_live_row`) | none | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Changed mode or time | Fresh metadata tree, or base tree patched | attribute build or patch | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Non-root row with `nlink` 0 | Skipped (tombstone) | none | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |

Every read goes through the captured reader, the attempt's sealed records or the
immutable base bound to the reader's root. Current active state is never read,
so mutations made after the capture are absent from this root and present in
the next.

## Records

```text
 operation_record scope (owner = OperationOwner, file_scope = 0)
 key = kind (u32) .. serial (u64, big-endian)  -> key order = serial order
 +---------------+---------------------------------------------+
 | CONTEXT  0x434E_0000 | reader identity, base, totals, sealed |
 | HEADER   0x434E_0001 | parent serial -> changed-name count   |
 | VALUE    0x434E_0002 | serial -> typed final inode value     |
 | FRESH    0x434E_0003 | fresh serial -> dense rank            |
 +---------------+---------------------------------------------+
 each changed file: same owner, file_scope = its serial
```

| Rule | Value | Source |
| --- | --- | --- |
| Changes per guarded job | At most `RECORD_ROWS` = 64 (the operation-record window), ordered by kind then serial | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |
| Cursor windows held | 16 per record kind, each at most `PAGE_ROWS` (1,024) rows | [`cursor.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/cursor.rs) |
| Second attempt in the same scope | Refused by the existing `CONTEXT` record | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |
| Digest rows per Store call | `FILE_DIGEST_WINDOW` = 1,024, one captured inode page: one `file_roots` and one `record_file_digests` call per page of up to 1,024 new files | [`digests.rs`](../../../../engine/crates/layerfs-workspace/src/ports/digests.rs) |

Records live in the Workspace's overlay file as operation records
([Overlay: Operation records](../overlay/operation-records.md)). No
namespace-sized collection is held in memory; Content's own filesystem state
uses the same scope through `Backing`
([`records.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/records.rs)).

## Outcome

| Type | Holds | Source |
| --- | --- | --- |
| `CapturedNamespaceAttempt` | `result` (`FilesystemResult` or a Content label) and `custody` | [`outcome.rs`](../../../../engine/crates/layerfs-workspace/src/construction/outcome.rs) |
| `CapturedNamespaceCustody` | `reader`, `operation`, `records`, failing `file`, first `failure`, `work` | [`outcome.rs`](../../../../engine/crates/layerfs-workspace/src/construction/outcome.rs) |
| `CapturedNamespaceWork` | Counted adapter work: rows, pages, headers, values, files, reuse, record jobs, phase times | [`outcome.rs`](../../../../engine/crates/layerfs-workspace/src/construction/outcome.rs) |
| `CapturedConstruction` | Store-side receipt: work, counters, unreleased owners, release error | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs) |

Exactly one of `file`, `failure` and `records.failure` holds the first
failure. `construct` consumes the attempt, so a failed attempt cannot be run
again; the custody value releases nothing on `Drop`.

## One producer

```text
 Commit thread: capture -> begin_save -> construct (one file at a time)
                -> finish -> publish        (no worker pool, no second Save)
 Init:          import -> INIT_WORKERS file workers -> Save -> history
```

| Rule | How the source holds it | Source |
| --- | --- | --- |
| One `Storage` handle per Commit | `Producer::take` returns the Workspace's idle handle, the Store's idle one, or a new one; a taken handle is in no slot | [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs) |
| One file under construction | The values pass builds each changed file in turn | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |
| On the calling thread | Captured ports run on the constructor thread, never a kernel worker | [`captured_namespace_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs) |
| Shared encoder | Content builds objects, Storage `SaveSink` saves them, Persistence writes them | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |

`LAYERFS_CONSTRUCTION_WORKERS` selects nothing in engine source; one producer
holds by construction. Project Init keeps its parallel profile
([Project Init: Construction](../project-init/construction.md)). A Commit never
materializes a native tree or calls Project Init.

## Related

- [Capture](capture.md): the reader this page consumes.
- [Publish and merge](publish-and-merge.md): the driver steps around construction.
- [Core: Files](../../core/files/README.md), [Core: Filesystem trees](../../core/filesystem-trees/README.md): the Content algorithms called here.
- [Core: Physical storage](../../core/physical-storage/README.md): the Save the objects go to.
- [Overlay: Operation records](../overlay/operation-records.md): where records are kept.

---

← [Commit: Capture](capture.md) · [Up: Commit](README.md) · [Commit: Publish and merge](publish-and-merge.md) →
