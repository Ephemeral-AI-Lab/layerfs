# Lifetimes and reclamation

[Architecture](../../README.md) › [Engine](../README.md) › [Overlay](README.md) › Lifetimes and reclamation

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

When overlay state can be released, and how.

| | |
| --- | --- |
| Component | [Overlay](README.md) |
| Source | [`engine/crates/layerfs-overlay/src/lifetime`](../../../../engine/crates/layerfs-overlay/src/lifetime), [`engine/crates/layerfs-overlay/src/maintenance`](../../../../engine/crates/layerfs-overlay/src/maintenance), [`layerfs-store-service/src/overlay`](../../../../engine/crates/layerfs-store-service/src/overlay) |

## Generations and frontier

```text
                      capture_ready?               install_ready?
 mutations ---> active g ---Capture---> captured g ---install---> retired
 (publish,       |  needs: no capture,     |  Commit reads     (installed=g,
  one ticket     |  not consolidating,     |  sealed rows       RETIRE g
  each)          |  no pending tickets     |                    queued)
                 |                         +--definite failure--> FOLD g
                 v                                (consolidating=g)
             active g+1  <-- later mutations keep going here
```

| `WorkspaceState` field | Meaning |
| --- | --- |
| `active` | Generation new rows go to |
| `captured`, `captured_revision` | The sealed generation and its frozen revision, while a Commit holds it |
| `installed` | Floor: generations at or below it never enter the view |
| `revision` | Local publication count; each published job adds one |
| `base_root` | Committed root the Workspace shows |
| `dirty_inodes`, `dirty_directory_entries` | Changes since the last capture |
| `closed` | Terminal close admitted |
| `base_readers` | Held `BaseSource` windows |
| `consolidating` | A failed capture being folded back into active |

The frontier rule is in the owner, not the engine: while a Capture waits,
later mutations wait before any effect; while a known install waits, later
base-source acquisitions wait. So sustained traffic cannot extend what a
waiter awaits. These are readiness waits on a condition variable, not
retries. Source: [`workspace.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/workspace.rs),
[`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs),
[`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs),
[`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs).

## Tickets

```text
publishing job             replying thread                 Capture
issue(rev) -> pending      reply sent or lost
                           reply_tickets().attempted(pub)
                             -> Pending | Last | Watched   capture_ready:
                                          (wakes waiter)   ticket pending?
```

| Rule | Source |
| --- | --- |
| Every published job issues one reply-attempt ticket; a job that does not commit withdraws it. | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs), [`inode.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/inode.rs) `settle` |
| Tickets are a `Mutex<BTreeMap>` shared with replying threads; a reply attempt takes no Workspace job and no SQL. | [`tickets.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/tickets.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) `reply_attempted` |
| An attempt, including a lost reply, settles the ticket; it never removes published state or claims delivery. | [`tickets.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/tickets.rs) |
| Capture and terminal cleanup wait until no ticket is pending. `pending_publications` pages them for observation. | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs), [`frontier.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/frontier.rs) |

## Orphans

```text
last unlink (nlink 0) of serial s
  nothing holds s ----------------> enqueue SERIAL_RETIRE(s, active)
  a descriptor/reader/lookup holds s:
    orphans[s] = Orphan{root, top=active, floor=installed}
    insert fs_inode(s, generation -1)  (metadata, size, no copy)
    descriptor or reader holds it -> enqueue ORPHAN(s)
ORPHAN steps (held)   move one cell of the newest lower layer into
                      generation -1, or release that layer
last reference goes   finish_release: up to one step's page inline,
                      the rest queued; rows deleted at the end
```

| Rule | Source |
| --- | --- |
| An orphan reads at most three layers: its own domain plus at most two retained lower layers, with its own base root. | [`orphan.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/orphan.rs) |
| Orphans survive any number of installs and failed captures; work does not depend on Commit count. | [`maintenance/orphan.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/orphan.rs) |
| A lower layer that an unsettled capture or a captured reader still holds parks the orphan; the release that ends the hold wakes it (`WAKE_ORPHAN`, `WAKE_GENERATION`). | [`source_wait.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/source_wait.rs) |
| `orphan_seen` false means no orphan row exists, so inode reads stop probing generation `-1`. | [`orphan.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/orphan.rs) |

## Garbage collection

| `cleanup_task.kind` | Name | Queued by | One step does |
| --- | --- | --- | --- |
| 1 | `FOLD` | `resolve_failed_capture` | Moves a page of a failed capture's rows into the active generation |
| 2 | `ORPHAN` | last unlink under a holder; holder release | One orphan migration or release step |
| 3 | `RETIRE` | `install` | Drops a page of a retired generation's cells, steps, pieces and superseded rows, skipping what an orphan reads |
| 4 | `STALE` | shrink | Deletes stale cells beyond the boundary |
| 5 | `STEPS` | shrink | Deletes dominated staircase rows |
| 6 | `OPERATION_RECORD` | `release_operation` | Deletes one envelope of an operation's records |
| 7 | `SERIAL_RETIRE` | unlink of an unheld file | Retires one serial's layer |
| 8 | `WAKE_ORPHAN` | a capture settled (install, failure, closed release) or a generation wake | Makes up to 64 orphans parked on that layer ready |
| 9 | `WAKE_GENERATION` | last captured-reader release | Makes parked generation items ready |
| 12 | `STALE_PIECES` | shrink of a referenced layer | Deletes a page of pieces the shrink hid |

The schema allows kinds 1–12; source uses no kind 10 or 11. A `FOLD` or
`RETIRE` item whose generation a captured reader holds is set
`ready = 0` instead of run. Source: [`ready.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/ready.rs),
[`garbage.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/garbage.rs),
[`pieces.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/pieces.rs),
[`composition.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/composition.rs).

## Maintenance work

```text
job ends: maintenance_pending()      unlock, then owner.maintain()
  read under the lock ------yes----> on the same caller thread:
                                     loop: lock, db.maintain(cursor),
                                           record step, unlock
                                     until a step returns None, fails
                                     or the Workspace is stopping
```

| Rule | Value or effect | Source |
| --- | --- | --- |
| One step is one atomic job over one `cleanup_task` row | `PAGE` = 14 cells, 64 rows; the widest row fits one page | [`ready.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/ready.rs) |
| Order | Ready rows in key order from a cursor; wraps once to the start | [`ready.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/ready.rs) |
| `maintenance_ready` hint | False is exact; true may be a false positive the next turn clears | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| A step fails | Stored as `maintenance_error`; no more steps; new Mounts are refused | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Inline release | A job that drops an orphan's last reference may finish at most one step's page itself | [`maintenance/orphan.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/orphan.rs) |
| Deferred jobs | `call_deferred` jobs leave what they ready queued: a Commit's two releases (`release_captured_reader`, `release_operation_deferred`) and terminal `close` | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`lifetime_operations.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/lifetime_operations.rs) |
| After a Commit's answer | The daemon's connection thread runs `BoundWorkspace::reclaim`, the same loop, before it reads the next call; otherwise the next job of the Workspace drains it | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs), [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |

A Commit's own reclamation follows its answer (owner approval, 2026-11-07).
Releasing the captured reader wakes the `RETIRE` of the generation it held,
one step per retired row: about 16,000 steps (0.4 s) for a new 500 MiB
file, which the Commit used to pay before it answered. The queued rows and
their order are unchanged, so the overlay file holds the same state at any
crash point as before; terminal close deletes whatever is still queued with
the file.

Terminal disposal is separate from live reclamation:

```text
close()             closed = true; no new mutation or acquisition;
                    queued maintenance is not drained
cleanup_state()     Live | Held (kernel, files, readers, lookups, capture,
                    base readers, tickets, owners) | Queued | Gone
retire(namespace)   Backing.terminal(); checked close_database()
                    -> Backing.closed(); last Arc<Backing> dropped
                    -> identity re-checked, remove_file, slot removed (Gone)
```

Rows are not swept before the file is deleted: the file goes as a whole.
A failed close keeps the connection (`CloseFailure`); a failed unlink keeps
the file handle (`OwnerError::Deletion`). Source:
[`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs),
[`backing.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/backing.rs),
[`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs).

## Related

- [Open state](open-state.md): the references whose release starts reclamation.
- [Namespace](namespace.md) and [Payload](payload.md): the rows reclaimed.
- [Operation records](operation-records.md): `OPERATION_RECORD` cleanup.
- [Commit: Install](../commit/install.md): the caller of `install`.
- [Daemon: Control calls](../daemon/control-calls.md): terminal unmount.
- [Glossary](../../glossary.md).

---

← [Overlay: Open state](open-state.md) · [Up: Overlay](README.md) · [Overlay: Operation records](operation-records.md) →
