# Construction

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How one captured generation becomes one canonical filesystem root in the Commit's Save, within [Commit](README.md).

| | |
| --- | --- |
| Source | [`layerfs-workspace/src/construction`](../../../../engine/crates/layerfs-workspace/src/construction), [`layerfs-store-service/src/store/captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs), [`pipeline.rs`](../../../../engine/crates/layerfs-store-service/src/store/pipeline.rs), [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs) |

## Driver (`store/captured.rs`)

```text
 BoundWorkspace::commit_captured(options)
   admit_commit
   commit_admitted( construct = |save, capture, _| {
       acquire_captured_reader, acquire_operation      (two owner jobs)
       pipeline::construct(save, Staging{save.sink()}, |objects, sink|
           CapturedNamespace::new(workspace, overlay, reader, owner)
               .with_digests(StoreDigests)              (producer thread)
               .construct(policy, capacities, objects, sink))
       classify(result, first original cause)
   })
   release_captured   (reader, then operation owner; their reclamation
                       stays queued for reclaim after the answer)
```

| Piece | What it does | Source |
| --- | --- | --- |
| `commit_captured`, `commit_captured_via` | Product constructor: the driver plus owner acquisition and release | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| `commit` | Same driver with a caller-supplied constructor (driver tests, `Service::execute`) | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `Staging` | On the Commit thread, forwards each `FinalizedObject` to the Save's sink and offers it to `ConstructedObjects` for the shared cache | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| `classify` | Turns a Content label back into the first original cause (`Owner`, `Content` with provider, `Workspace`) | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| Policy | `store.policy().construction()`: the same policy Init uses | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |

The request number of both owners is the capture's generation number; only
Commit acquires them, one capture at a time, so it is unique.

## Namespace construction (`construction/namespace`, `driver.rs`)

```text
 CapturedNamespace::construct
  1 bind     reader engine == route engine; base rebound to reader.root()
  2 begin    CONTEXT record (unsealed)
  3 names    pass over captured names  -> HEADER per changed parent;
             remember same-path base files (at most 65,536 hints)
  4 values   pass over captured inodes -> VALUE, FRESH; changed files built
  5 seal     CONTEXT record with totals
  6 update   Cursor over sealed records -> Content
             update_filesystem_streamed_backed -> new FilesystemRootId
  7 counts   remember_constructed_counts (cache fill only)
```

| Inode row | Value built | Content entry point | Source |
| --- | --- | --- | --- |
| Fresh file whose digest names a saved root | That root, reused | none | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Fresh file | New file root from captured runs; a rename predecessor, else the base file at the same path, is offered as delta base | `construct_runs_with_predecessor` | [`captured/owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs) |
| Changed base file | Indexed edits over the retained `FileView` | `apply_indexed_edits_view_backed` | [`captured/owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs) |
| Fresh symlink | Target from the capture | `emit_symlink` | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Fresh directory | Page built by Content from its header | streamed update | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Base inode whose only change is mtime | Base row kept; live row carried past install (`keep_live_row`) | none | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Changed mode or time | Fresh metadata tree, or base tree patched | attribute build or patch | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Non-root row with `nlink` 0 | Skipped (tombstone) | none | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |

Every read goes through the captured reader, the attempt's sealed records or
the immutable base bound to the reader's root; current active state is never
read, so mutations made after the capture are absent from this root and
present in the next. A Commit never materializes a native tree or calls
Project Init.

## Records (`namespace/records.rs`, `cursor.rs`)

```text
 operation_record scope (owner = OperationOwner, file_scope = 0)
 key = kind (u32) .. serial (u64, big-endian)  -> key order = serial order
 +---------------------+-------------------------------------------+
 | CONTEXT 0x434E_0000 | reader identity, base, totals, sealed     |
 | HEADER  0x434E_0001 | parent serial -> changed-name count       |
 | VALUE   0x434E_0002 | serial -> typed final inode value         |
 | FRESH   0x434E_0003 | fresh serial -> dense rank                |
 +---------------------+-------------------------------------------+
 each changed file: same owner, file_scope = its serial
```

| Rule | Value | Source |
| --- | --- | --- |
| Changes per guarded job | At most `RECORD_ROWS` = 64, ordered by kind then serial | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs), [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| Cursor windows held | `WINDOWS` = 16 per record kind, each at most `PAGE_ROWS` (1,024) rows | [`cursor.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/cursor.rs) |
| Second attempt in the same scope | Refused by the existing `CONTEXT` record | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |
| Digest lookups | `FILE_DIGEST_WINDOW` = 1,024: one `file_roots` and one `record_file_digests` call per captured inode page | [`digests.rs`](../../../../engine/crates/layerfs-workspace/src/ports/digests.rs) |

Records live in the Workspace's overlay file as operation records
([Overlay](../overlay/README.md)). No namespace-sized collection is held in
memory; Content's own filesystem state uses the same scope through `Backing`
([`records.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/records.rs)).

## Outcome (`outcome.rs`)

| Type | Holds | Source |
| --- | --- | --- |
| `CapturedNamespaceAttempt` | `result` (`FilesystemResult` or a Content label) and `custody` | [`outcome.rs`](../../../../engine/crates/layerfs-workspace/src/construction/outcome.rs) |
| `CapturedNamespaceCustody` | `reader`, `operation`, `records`, failing `file`, first `failure`, `work` | [`outcome.rs`](../../../../engine/crates/layerfs-workspace/src/construction/outcome.rs) |
| `CapturedNamespaceWork` | Counted work: rows, pages, headers, values, files, reuse, record jobs, phase times | [`outcome.rs`](../../../../engine/crates/layerfs-workspace/src/construction/outcome.rs) |
| `CapturedConstruction` | Store-side receipt: work, counters, unreleased owners, release error | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs) |

Exactly one of `file`, `failure` and `records.failure` holds the first
failure. `construct` consumes the attempt, so a failed attempt cannot run
again; the custody value releases nothing on `Drop`.

## One producer (`store/pipeline.rs`, `store/producer.rs`)

```text
 Commit thread: capture -> begin_save -> spawn producer --.  (holds the Save)
                accept batches, answer reads  <-----------+-- queue: 32 msgs
                join -> finish -> publish -> install       |
 producer:      names -> values (one file at a time) -> update   (no Save)
```

| Rule | How the source holds it | Source |
| --- | --- | --- |
| One `Storage` handle per Commit | `Producer::take` returns the Workspace's idle handle, else the Store's idle one, else a new one; a taken handle is in no slot. A completed Commit keeps its handle for the next | [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs) |
| One file under construction | The values pass builds each changed file in turn on the one producer thread | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |
| Objects in emission order | One FIFO queue; the Commit thread, which owns the `!Send` Save, accepts each batch in order | [`pipeline.rs`](../../../../engine/crates/layerfs-store-service/src/store/pipeline.rs) |
| Same-Save reads see every earlier object | A read hands over the pending batch, then asks on the same queue and waits | [`pipeline.rs`](../../../../engine/crates/layerfs-store-service/src/store/pipeline.rs) |
| Bounded memory | `QUEUE_SLOTS` = 32 messages; a batch holds at most `BATCH_BYTES` 256 KiB and `BATCH_OBJECTS` 512, or one larger object alone: 8 MiB in flight at the default small-file threshold, 32 MiB at its largest, as for Init | [`pipeline.rs`](../../../../engine/crates/layerfs-store-service/src/store/pipeline.rs) |
| Captured ports off kernel workers | The producer thread runs the captured ports, each one short owner job; it is never a kernel receive or request-service worker | [`captured_namespace_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs) |
| No lock across Store I/O | Neither thread holds the Workspace lock between jobs | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| Shared encoder | Content builds objects, Storage `SaveSink` saves them, Persistence writes them | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |

| Failure | What happens |
| --- | --- |
| The Save refuses an object | The Commit thread stops accepting and closes the queue; the producer's next hand-over is refused. The Save's error is the first cause (`CommitError::Construction` with the producer's result as `original`); later objects are dropped |
| A same-Save read fails | The Save keeps the read's error; the producer gets the read's Content error |
| Construction fails | The producer ends with its first original cause; the Commit thread drops the Save unfinished |
| The producer panics | Its sender closes, the Commit thread stops waiting, and the panic resumes on the Commit thread |
| The thread cannot be started | `CommitError::Context`, before any construction |

Nothing is retried or replayed. Each wait polls its queue for `POLL` (200 µs)
before it parks, because a small Commit makes only about a dozen hand-overs.
Commit has no deadline or cancellation of its own; the no-change `UpToDate`
shortcut runs before any thread is spawned. Traced, the producer's own
`construct` tree is attached below `captured.namespace`, beside `save.accept`
(the Commit thread's time accepting objects and answering reads).

`LAYERFS_CONSTRUCTION_WORKERS` selects nothing in engine source: one producer
holds by construction, as
[`r5_10_producer_profile.rs`](../../../../engine/crates/layerfs-daemon/tests/r5_10_producer_profile.rs)
checks. Project Init keeps its own parallel profile
([Project Init](../project-init/README.md)).

## Related

- [Commit](README.md): capture before, publish and install after.
- [Overlay](../overlay/README.md): captured generations and operation records.
- [Core: Files](../../core/files/README.md), [Core: Filesystem trees](../../core/filesystem-trees/README.md): the Content algorithms called here.
- [Core: Physical storage](../../core/physical-storage/README.md): the Save the objects go to.
