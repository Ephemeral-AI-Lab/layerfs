# Lifetimes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

When [Overlay](README.md) state can be released and how it is reclaimed: generations, reply tickets, orphans, queued maintenance and terminal file disposal.

| | |
| --- | --- |
| Source | [`layerfs-overlay/src/lifetime`](../../../../engine/crates/layerfs-overlay/src/lifetime), [`layerfs-overlay/src/maintenance`](../../../../engine/crates/layerfs-overlay/src/maintenance), [`layerfs-store-service/src/overlay`](../../../../engine/crates/layerfs-store-service/src/overlay) |

## Generations and frontier

```text
                      capture_ready?               install_ready?
 mutations ---> active g ---Capture---> captured g ---install---> retired
 (one ticket     |  needs: no capture,     |  Commit reads     (installed=g,
  each)          |  not consolidating,     |  sealed rows       RETIRE g
                 |  no pending tickets     |                    queued)
                 |                         +--definite failure--> FOLD g
                 v                                (consolidating=g)
             active g+1  <-- later mutations keep going here
```

| `WorkspaceState` field | Meaning | Source |
| --- | --- | --- |
| `active` | Generation new rows go to | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| `captured`, `captured_revision` | The sealed generation and its frozen revision while a Commit holds it | same |
| `installed` | Floor: generations at or below it never enter the view | same |
| `revision` | Local publication count; each published job adds one | same |
| `base_root` | Committed root the Workspace shows | same |
| `dirty_inodes`, `dirty_directory_entries` | Changes since the last capture | same |
| `base_readers` | Held `BaseSource` windows; `install` requires zero | [`workspace.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/workspace.rs), [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| `consolidating` | A failed capture being folded back into active | same |
| `closed` | Terminal close admitted | [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs) |

| Step | Call | Source |
| --- | --- | --- |
| Capture | `capture()`: active g becomes captured, active g+1; dirty counts 0; copies no rows | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Known success | `install(capture, root)`: `installed = g`, `base_root = root`, queue `RETIRE g` | same |
| Definite failure | `resolve_failed_capture`: `consolidating = g`, queue `FOLD g` | [`composition.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/composition.rs) |

The frontier rule lives in the Workspace lock, not the engine: while a
Capture waits, later mutations wait before any effect; while a known install
waits, later base-source acquisitions wait
([`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs),
[`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs)).
These are readiness waits on a condition variable, not retries.

## Tickets

```text
publishing job            replying thread                  Capture
issue(rev) -> pending     reply sent or lost
                          reply_tickets().attempted(pub)
                            -> Pending | Last | Watched    capture_ready:
                                         (wakes waiter)    ticket pending?
```

| Rule | Source |
| --- | --- |
| Every published job issues one reply-attempt ticket; a job that does not commit withdraws it | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs), [`inode.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/inode.rs) |
| Tickets are a `Mutex<BTreeMap>` shared with replying threads; an attempt takes no Workspace job and no SQL | [`tickets.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/tickets.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) `reply_attempted` |
| An attempt, including a lost reply, settles the ticket; it never removes published state or claims delivery | [`tickets.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/tickets.rs) |
| Capture and terminal cleanup wait until no ticket is pending; `pending_publications` pages them for observation | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs), [`frontier.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/frontier.rs) |

## Orphans

```text
last unlink (nlink 0) of serial s
  nothing holds s ----------------> queue SERIAL_RETIRE(s, active)
  a descriptor, reader or lookup holds s:
    orphans[s] = Orphan{root, top=active, floor=installed}
    insert fs_inode(s, generation -1)  (metadata and size, no byte copy)
    descriptor or reader holds it -> queue ORPHAN(s)
ORPHAN steps (held)   move one page of the newest lower layer into
                      generation -1, or release that layer
last reference goes   finish_release: up to one step's page inline,
                      the rest queued; rows deleted at the end
```

| Rule | Source |
| --- | --- |
| An orphan reads at most three layers: its own domain plus at most two retained lower layers, over its own base root | [`orphan.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/orphan.rs) |
| Orphans survive any number of installs and failed captures; their work does not depend on Commit count | [`maintenance/orphan.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/orphan.rs) |
| A lower layer that an unsettled capture or a captured reader still holds parks the orphan; the release that ends the hold wakes it | [`source_wait.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/source_wait.rs) |
| The `orphan_seen` hint false means no orphan row exists, so inode reads stop probing generation `-1` | [`orphan.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/orphan.rs) |

## Maintenance

| `kind` | Name | Queued by | One step does |
| --- | --- | --- | --- |
| 1 | `FOLD` | `resolve_failed_capture` | Moves a page of a failed capture's rows into the active generation |
| 2 | `ORPHAN` | Last unlink under a holder; holder release | One orphan migration or release step |
| 3 | `RETIRE` | `install` | Drops a page of a retired generation's cells, steps, pieces and superseded rows, skipping what an orphan reads |
| 4 | `STALE` | Shrink | Deletes stale cells beyond the boundary |
| 5 | `STEPS` | Shrink | Deletes dominated staircase rows |
| 6 | `OPERATION_RECORD` | `release_operation` | Deletes one envelope of an operation's records |
| 7 | `SERIAL_RETIRE` | Unlink of an unheld file | Retires one serial's layer |
| 8 | `WAKE_ORPHAN` | A capture settled, or a generation wake | Makes up to 64 orphans parked on that layer ready |
| 9 | `WAKE_GENERATION` | Last captured-reader release | Makes parked generation items ready |
| 12 | `STALE_PIECES` | Shrink of a referenced layer | Deletes a page of pieces the shrink hid |

The schema allows kinds 1 to 12; source uses no 10 or 11. A `FOLD` or
`RETIRE` item whose generation a captured reader holds is set `ready = 0`
instead of run. Source: [`ready.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/ready.rs),
[`garbage.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/garbage.rs),
[`pieces.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/pieces.rs),
[`indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/indexed_operation_record.rs).

```text
job ends: maintenance_pending()      unlock, then OwnerClient::maintain()
  read under the lock ------yes----> on the same caller thread:
                                     loop: lock, db.maintain(cursor),
                                           record step, unlock
                                     until a step returns None, fails,
                                     the Workspace is stopping or a
                                     maintenance hold is live
```

| Rule | Value or effect | Source |
| --- | --- | --- |
| One step is one atomic job over one `cleanup_task` row | `PAGE` = 14 cells, 64 rows; the widest row fits one page | [`ready.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/ready.rs) |
| Order | Ready rows in key order from a cursor; wraps once to the start | same |
| `maintenance_ready` hint | False is exact; true may be a false positive the next turn clears | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| A step fails | Stored as `maintenance_error`; no more steps; the daemon refuses new Mounts | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Inline release | A job that drops an orphan's last reference may finish one step's page itself | [`maintenance/orphan.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/orphan.rs) |
| Deferred jobs | A Commit's two releases (`release_captured_reader`, `release_operation_deferred`) and terminal `close` leave what they ready queued | [`lifetime_operations.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/lifetime_operations.rs) |
| After a Commit's answer | The daemon's connection thread drains the same loop through `reclaim_after_commit` before reading the next call; otherwise the Workspace's next job drains it | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Observations (#679) | The daemon's control Status and the operator's Inspect read `state` through `observe_state`, a deferred job: they leave the queue to `reclaim_after_commit`. A Sandbox probes every Workspace's Status before most calls, an unmount's included, so a draining probe made the first call after a Commit run its reclamation | [`lifetime_operations.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/lifetime_operations.rs), [`status.rs`](../../../../engine/crates/layerfs-daemon/src/control/status.rs), [`inspect.rs`](../../../../engine/crates/layerfs-daemon/src/control/inspect.rs) |
| Terminal unmount (#679) | The daemon's `unmount` takes a maintenance hold for the whole call: a counter on `Shared` outside the lock, so it lands while a drain loop keeps retaking the lock. While any hold is live no step runs, so neither the end of a job, a reply attempt nor `reclaim_after_commit` drains, and close deletes the queue with the file. An unmount that leaves the Workspace bound (a refusal, or a failure before Close) drains the queue itself after dropping the hold | [`hold.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/hold.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) `maintain`, [`unmount.rs`](../../../../engine/crates/layerfs-daemon/src/control/unmount.rs) |
| A failed Commit's fold | `resolve_failed` sets `consolidating` and queues `FOLD`; until it runs, the active dirty counts are 0. The dirty check, like Commit's up-to-date check, counts `consolidating` as a change, and names no paths for it (`truncated`) | [`composition.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/composition.rs), [`dirty.rs`](../../../../engine/crates/layerfs-daemon/src/control/dirty.rs) |

Deferring changes when reclamation runs, not what is queued: the overlay
file holds the same rows at any point, and terminal close deletes whatever
is still queued with the file.

## Terminal disposal

```text
close()             closed = true; no new mutation or acquisition;
                    queued maintenance is not drained
cleanup_state()     Live | Held (kernel, files, readers, lookups, capture,
                    base readers, tickets, owners) | Queued | Gone
retire(namespace)   Backing.terminal(); checked close_database()
                    -> Backing.closed(); last Arc<Backing> dropped
                    -> identity re-checked, remove_file, slot removed
```

| Rule | Source |
| --- | --- |
| Rows are not swept before deletion: the file goes as a whole | [`backing.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/backing.rs) |
| Unlink needs both marks and an unchanged device, inode and link count 1 | same |
| A failed close keeps the connection (`CloseFailure`); a failed unlink keeps the file handle (`OwnerError::Deletion`) and the slot `Failed` | [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs), [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Daemon stop closes every connection with `terminal = false`, so files are not deleted | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) `stop` |

## Related

- [Overlay](README.md): open state, whose released references start reclamation, and the Workspace lock.
- [Payload](payload.md): the cells, steps and pieces reclaimed.
- [Commit](../commit/README.md): the caller of capture, install and failure resolution.
- [Daemon: Control calls](../daemon/control-calls.md): terminal unmount.
