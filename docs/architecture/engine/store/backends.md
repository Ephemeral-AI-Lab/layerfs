# Backends

[Architecture](../../README.md) › [Engine](../README.md) › [Store](README.md) › Backends

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The backend seam shared by SQLite and PostgreSQL.

| | |
| --- | --- |
| Component | [Store](README.md) |
| Source | [`engine/crates/layerfs-persistence/src/backend/seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs), [`engine/crates/layerfs-persistence/src/backend/admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs), [`engine/crates/layerfs-persistence/src/backend/records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs) |

## The backend seam

```text
  shared semantics (engine-neutral)        engine seam            engines
 +----------------------------------+
 | history/  (HistoryProvider:      |--> HistoryStatements --+
 |   Branch, Commit, Layer, merge,  |    history(key,params) |
 |   lineage, allocation)           |    history_arrays(..)  |
 +----------------------------------+                        |
 | metadata/ (policy, allocation,   |                        |  +----------+
 |   locations, values, signatures) |--> Mechanics ----------+->| Engine:: |
 | objects/  (pack reads)           |    policy, locate,     |  | Sqlite   |
 +----------------------------------+    read_packs, ...     |  +----------+
 | storage/  (StorageProvider,      |                        |  +----------+
 |   publication bounds)            |------------------------+->| Engine:: |
 +----------------------------------+                           | Postgres |
                                                                +----------+
       Engine::Sqlite(Arc<sqlite Session>)
       Engine::Postgres(Arc<postgresql Session>)   (feature postgresql)
```

| Part | Role | Source |
| --- | --- | --- |
| `Engine` | The opened session behind a `HistoryProvider`: `Sqlite` or `Postgres`. | [`seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs) |
| `HistoryStatements` | Runs a named history statement (`key`, `Vec<Param>`) in the open transaction. `history_arrays` binds whole columns where the engine can (PostgreSQL); `None` means run the per-row form. | [`seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs) |
| `Mechanics` | Engine statements under shared Metadata and Objects bounds: `policy`, `allocation_cursor`, `advance_allocation`, `allocate` (cursor and advance; PostgreSQL overrides it with one `UPDATE … RETURNING`), `locate`, `inode_values`, `signatures`, `read_packs`. | [`seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs) |
| `Param`, `Value`, `Record` | Owned scalars in and rows out, independent of the driver. | [`records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs) |
| `pack_info` | One checked pack descriptor from stored columns, on either engine. | [`records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs) |

History SQL lives once, as SQLite statements under
[`sql/sqlite/queries/history`](../../../../engine/crates/layerfs-persistence/sql/sqlite/queries/history).
PostgreSQL runs them unchanged except the few it replaces in
[`statements.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/history/statements.rs).

### Errors

| `BackendError` | `PersistenceError` | `HistoryError` | Meaning |
| --- | --- | --- | --- |
| `Busy` | `Busy` | `Busy` | Another writer held the lock; not retried. |
| `Closed` | `Refused "Closed"` | `OwnershipUnavailable` | Store closed before this call's turn. |
| `Rejected` | `Refused "Rejected"` | `OwnershipUnavailable` | PostgreSQL refused a new connection. |
| `Cancelled` | `Refused "Cancelled"` | `OwnershipUnavailable` | A signal interrupted the SQLite writer wait. |
| `ReadOnly` | `Refused "ReadOnly"` | `ContinuityUnavailable` | Write on a read-only session. |
| `Capacity` | `Refused "Capacity"` | `Capacity("persistence")` | A bound or the disk margin was reached. |
| `Integrity` | `Malformed` | `Integrity("persistence")` | Stored or supplied data broke a rule. |
| `Unknown` | `Uncertain` | `UnknownOutcome` | The outcome is not known; the session is quarantined. |

Source: [`records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs).
An `Unknown` result marks the session quarantined: every later call on it
fails `Unknown` without SQL. Nothing is reread, rolled back by guess or
replayed.

## Admission

```text
  thread A (turn)        Admission { queue: Mutex<Queue>, changed: Condvar }
       |                 +--------------------------------------------+
       | enter(kind) --> | active: Some(A)    holder: None            |
       |                 | pending: [t7 (B), t8 (C)]   closed: false  |
  B, C wait on changed   +--------------------------------------------+
       |
  A's Turn drops --> active = None, notify_all --> B (front) takes the turn
```

| Exit from `enter` | When | Source |
| --- | --- | --- |
| `Ok(Turn { nested: false })` | No active turn and nobody ahead; or this ticket reaches the front. | [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |
| `Ok(Turn { nested: true })` | The caller already holds the publication lane: its own SQL runs inside that turn. | same |
| `Closed` | `Admission::close` ran before the turn; the turn in progress finishes. | same |
| `Integrity` | The caller's thread already holds the turn outside a lane (would deadlock). | same |
| `Unknown` | The queue mutex was poisoned. | same |

Each session (one connection) has its own admission: a first-come,
first-served queue with no time limit. Waiting is readiness, not a retry;
nothing is attempted before the turn. The queue is bounded by the threads
that can block on it, not by a count. A holder that unwinds still releases
its turn. Time queued is added to the thread's `Writer` or `Reader` wait
total. `Session::diagnostics` reads counters under the state lock only, so
it never queues behind a held lane.

## One writer

```text
 SQLite write: Session::run(true, body)           any process, one host
   1 admission.enter        in-process FIFO turn on this session
   2 state mutex            the connection
   3 Writer::acquire        Linux/macOS: flock(LOCK_EX) on the Store's
                            parent directory     (skipped if nested)
   4 BEGIN IMMEDIATE        SQLite's write lock; busy_timeout 0 -> Busy
   5 body; COMMIT           or ROLLBACK; Unknown -> quarantine
   6 finish_writer          release the flock
   7 Turn drop              next queued caller

 PostgreSQL write: Session::run_as(Write, body)
   1 admission.enter
   2 state mutex
   3 BEGIN READ COMMITTED; SET LOCAL search_path, synchronous_commit;
     SET LOCAL lock_timeout; SELECT 1 FROM store_lane WHERE id=1
     FOR UPDATE                                      (skipped if nested)
   4 body; COMMIT           lost reply -> quarantine
```

| Rule | Enforced by |
| --- | --- |
| One write transaction at a time per session | `Admission` turn: [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |
| One writer across processes on one Linux or macOS host | Directory `flock`: [`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs) |
| One writer in the SQLite file | `BEGIN IMMEDIATE` with `busy_timeout=0`: [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs) |
| One writer across PostgreSQL clients | `store_lane` row `FOR UPDATE`: [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs) |
| No busy handler, no retry | `busy_timeout` reads back 0; `Busy` is returned as is |

The lock is taken on a separate descriptor of the Store's directory, never on
the main, WAL or SHM file: closing a main-file descriptor would drop SQLite's
own POSIX locks. Several Store files in one directory share this lock. Linux
and macOS both use the directory descriptor, so neither takes an advisory
lock on SQLite's own files. Reads (`BEGIN`, or one
implicit snapshot) take the turn but never the directory lock or the lane.
The publication lane holds steps 1 and 3 across several transactions; see
[History publication](history-publication.md#the-publish-lane).

SQLite connection lifecycle also takes this directory owner: before opening
and applying/checking the profile (including the Disposable background
connection), and around the last Session connection's close. WAL startup and
last-close checkpoint/sidecar teardown can otherwise meet another process's
writer before its ordinary transaction admission. This is a short lifecycle
hold, not a hold over a Store call or the lifetime of an open Session. Read-only
and diagnostic connection initialization uses the same hold; their subsequent
reads remain independent of the writer lane.

The background connection closes while the Session's main connection is still
alive, so it is never the last connection. It must not reacquire the lane while
the Session joins its thread: a quarantined Session can already retain that
owner. The main connection closes before its retained/new lifecycle owner is
released. If a destructor's single admission wait is interrupted or fails,
it retains the connection until process exit instead of performing unadmitted
teardown or retrying. Checked `seal` retains its explicit close/error result.
Source: [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs),
[`lifecycle.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/lifecycle.rs),
[`background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs).

## Related

- [SQLite](sqlite.md#sessions) and [PostgreSQL](postgresql.md#connection-and-session): each engine's session.
- [History publication](history-publication.md): the history side of the seam.
- [Metadata and objects](metadata-and-objects.md): the Mechanics side of the seam.
- [Core: Physical storage, ports](../../core/physical-storage/ports.md): the `PackPersistence` port the provider implements.
- [Glossary](../../glossary.md).

---

← [Store: Lifecycle](lifecycle.md) · [Up: Store](README.md) · [Store: SQLite](sqlite.md) →
