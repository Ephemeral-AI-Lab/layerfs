# Capture

[Architecture](../../README.md) › [Engine](../README.md) › [Commit](README.md) › Capture

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Capturing the published frontier.

| | |
| --- | --- |
| Component | [Commit](README.md) |
| Source | [`engine/crates/layerfs-workspace/src/construction/captured`](../../../../engine/crates/layerfs-workspace/src/construction/captured), [`engine/crates/layerfs-store-service/src/store/captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs), [`engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs), [`engine/crates/layerfs-store-service/src/overlay/captured_run_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_run_port.rs), [`engine/crates/layerfs-overlay/src/lifetime/generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`engine/crates/layerfs-overlay/src/lifetime/captured_reader.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/captured_reader.rs) |

## The published frontier

```text
 before capture                         after capture
 +---------------------------+          +---------------------------+
 | installed floor  (gen F)  |          | installed floor  (gen F)  |
 | active gen G  <- writes   |  ---->   | captured gen G  (sealed)  |
 +---------------------------+          | active gen G+1 <- writes  |
                                        +---------------------------+
 Capture { engine, generation: G, revision, base_root }
```

`Overlay::capture` seals the rows already published in the active generation.
It copies no rows: it moves the generation numbers in the Workspace state and
returns a `Capture` value
([`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs),
[`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs)).

| State field | Before | After | Meaning |
| --- | --- | --- | --- |
| `active` | G | G+1 | Generation later mutations write |
| `captured` | `None` | `Some(G)` | The sealed generation |
| `captured_revision` | `None` | `Some(revision)` | Publication revision frozen with it |
| `dirty_inodes`, `dirty_directory_entries` | counts | 0 | Counted again for G+1 |
| `base_root` | R | R | Unchanged until install |

The frontier is what filesystem requests have published, from any process.
No Exec, command or process identity enters capture, and no row is filtered by
who wrote it ([`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs)).
stdout and stderr are not captured unless a command wrote them to a file in the
Workspace.

### Readiness before the capture

```text
 Commit thread                 Workspace lock (one connection)
 capture job --- lock --->  capture_ready()?
                            | reply attempts pending -> wait (lock released)
                            | consolidating          -> wait
                            | captured already       -> CaptureInFlight
                            v ready
                            Overlay::capture()   one short job
 later Mutation jobs wait behind a waiting capture (finite frontier)
```

| Check | Outcome | Source |
| --- | --- | --- |
| Earlier reply attempts not yet recorded | Wait; a lost reply is recorded as an attempt and never removes published state | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| A failed capture still being folded back | Wait | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| A capture already held | `OverlayError::CaptureInFlight` | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Mutations arriving while a capture waits | Wait behind it (`behind_frontier`) | [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |

A wait is readiness, not a retry: the capture job runs once when ready.

## Captured runs

```text
 CapturedReader { capture, owner, installed }
      |
      +-- captured_inode_page(after)            <= 64 inodes, by serial
      +-- captured_directory_entry_page(after)  <= 64 names, (parent, name)
      +-- captured_directory_entries(parent)    <= 64 names of one parent
      +-- captured_directory_entry(parent,name) one sealed row
      +-- captured_symlink(serial)              target bytes
      +-- captured_inode(serial), captured_run_step(cursor)  file byte runs
      +-- captured_digests, captured_predecessors  <= 64 serials
```

| Port | Implemented by | Source |
| --- | --- | --- |
| `OverlayCapturedNamespace` | `OwnerClient` | [`captured_namespace.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_namespace.rs), [`captured_namespace_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs) |
| `OverlayCapturedRuns` | `OwnerClient` | [`captured_runs.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_runs.rs), [`captured_run_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_run_port.rs) |

Every captured read is checked against the reader (`checked_capture`,
`check_captured_reader`); a page is at most `PAGE_ROWS` = 1,024 rows in key
order, and the next page resumes after the last key. A run step's `Window`
is at most `READ_WINDOW` (128 KiB) of bytes
([Overlay: Payload](../overlay/payload.md#streams)). Later active writes cannot
change a captured page. Each port call is one short job under the Workspace
lock, run on the Commit thread, never on a kernel request worker.

## Custody

```text
 capture --> acquire_captured_reader(capture, request = generation)
         --> acquire_operation(request)
         --> CapturedNamespace::construct ... (outcome known)
         --> drop attempt custody
         --> release_captured_reader --> release_operation_deferred
         --> answer;  then reclaim (the reclamation both releases readied)
```

| Owner | Acquired by | Released when | Source |
| --- | --- | --- | --- |
| Admission flag | `admit_commit` | Success, or a failure that is `locally_settled` with no retained owner | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| `CapturedReader` | `acquire_captured_reader` | After the outcome is known; first in the release order | [`captured_reader.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/captured_reader.rs) |
| `OperationOwner` | `acquire_operation` | After the reader | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| Captured generation | `Overlay::capture` | Install, or `resolve_failed_capture` after a definite nonpublication | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`composition.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/composition.rs) |

Neither release drains the maintenance it readies (owner approval,
2026-11-07): the daemon drains it with `BoundWorkspace::reclaim` after it
sent the Commit's answer, and without that the next job of the Workspace
does; see [Lifetimes and reclamation](../overlay/lifetimes-and-reclamation.md#maintenance-work).

`CapturedConstruction` is the receipt: it names any reader or owner not known
released, the work counted and the release error that stopped the sequence
([`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs)).
Nothing is released on `Drop`. A refused release stops the sequence and the
daemon marks the Workspace `LocalFailure`, so no later Commit is admitted past it
([`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs)).

After a definite nonpublication, `resolve_failed_capture` turns the captured
generation into the consolidating one, and maintenance (`FOLD`) moves its rows
back under the active generation a row at a time; an active row for the same
name or inode wins
([`composition.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/composition.rs)).
An unknown outcome keeps the capture; see [Settle](publish-and-merge.md#settle).

## Related

- [Construction](construction.md): the consumer of the captured reader.
- [Install](install.md): what ends a successful capture.
- [Overlay: Namespace](../overlay/namespace.md), [Overlay: Lifetimes and reclamation](../overlay/lifetimes-and-reclamation.md): generations, rows and their cleanup.
- [Workspace: Ports](../workspace/ports.md): the port traits.

---

← [Engine: Commit](README.md) · [Up: Commit](README.md) · [Commit: Construction](construction.md) →
