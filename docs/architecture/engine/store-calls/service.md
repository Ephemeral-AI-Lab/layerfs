# Service

[Architecture](../../README.md) › [Engine](../README.md) › [Store calls](README.md) › Service

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The StoreService: opening, binding and read handles.

| | |
| --- | --- |
| Component | [Store calls](README.md) |
| Source | [`engine/crates/layerfs-store-service/src/store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs), [`engine/crates/layerfs-store-service/src/bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs), [`engine/crates/layerfs-store-service/src/store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs), [`engine/crates/layerfs-store-service/src/store/bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs), [`engine/crates/layerfs-store-service/src/store/read_handle.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_handle.rs), [`engine/crates/layerfs-store-service/src/store/read_service.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_service.rs), [`engine/crates/layerfs-store-service/src/store/recognizer.rs`](../../../../engine/crates/layerfs-store-service/src/store/recognizer.rs), [`engine/crates/layerfs-store-service/src/store/statistics.rs`](../../../../engine/crates/layerfs-store-service/src/store/statistics.rs) |

## StoreService

```text
StoreService
+-------------------------------------------------------------+
| opened:   OpenedStore   (store, engine version, writer and  |
|                          reader SQL observations)           |
| store:    Arc<Store>                                        |
|   +-------------------------------------------------------+ |
|   | writer:  Arc<dyn PackPersistence>   (the one writer)   | |
|   | history: Arc<dyn HistoryCatalog>    (catalog, lane)    | |
|   | readers: ReadSet                                       | |
|   |   session 0 = control reader (read-only)               | |
|   |   session n = one per bound Workspace                  | |
|   |   idle      = up to 4 kept readers for private binds   | |
|   | cache:   CanonicalCache (shared immutable objects)     | |
|   | recognizer, serial pool, idle producer, merge scratch  | |
|   +-------------------------------------------------------+ |
| owned/owner: overlay Owner and its OwnerClient              |
| capacity, bound: private Workspaces admitted (max 64)       |
| retained: Vec<Retained>  (uncertain Commits, kept)          |
+-------------------------------------------------------------+
```

| Field | Meaning | Source |
| --- | --- | --- |
| `opened` | The opened Store plus SQL session observations (`sql_sessions`, `diagnostics`). | [`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs) |
| `store` | The shared `Store`: one writer, a history catalog, the read set and the immutable cache. | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |
| `owner` | The overlay owner client private Workspaces open through. | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| `capacity`, `bound` | Admission count for private Workspaces; `bind` refuses `Capacity` at the limit. | same |
| `retained` | Private Workspaces whose Commit outcome is uncertain, with their failure. | same |

`StoreService` serves the SDK's Store calls in the caller's process. A daemon
uses the same `Store` type for its mounted Workspaces; it does not use
`StoreService` for them.

## Open and bind

```text
open_direct(store, binding, cursor_key, template)
  1 kernel_guard::check(store)          --> Unsupported (nothing opened)
  2 store.is_file()                     --> Missing
  3 open_store_observed(sqlite, Disposable, CALLS=64, cache 64 KiB)
       SqliteHandles::open_writable     (the writer session)
       factory(0) -> control reader     (read-only session)
       Store::new(..).with_merge_scratch(..)
                                        --> Failed("Store open: ..")
  4 Owner::start(template, pager 1024 KiB, namespaces 64)
                                        --> Failed("overlay template: ..")
```

| Constant | Value | Meaning |
| --- | --- | --- |
| `CALLS` | 64 | Private Workspaces one process may hold: one per SDK binding worker thread. |
| `CACHE_BYTES` | 64 KiB | The Store's shared immutable-object cache. |
| `PAGER_KIB` | 1024 | Each private overlay's page cache. |

`open_direct` selects `SqlitePersistenceProfile::Disposable` explicitly; it
adds no busy handler, retry or fsync. `open_postgres` (cargo feature
`postgresql`) skips the kernel guard, since a PostgreSQL Store is reached over
a connection, not a shared file. With the feature off, a PostgreSQL selection
fails `BackendUnavailable` ([`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs)).
Backend details are in [Store: Lifecycle](../store/lifecycle.md) and
[Store: Backends](../store/backends.md).

The kernel guard ([`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs))
allows ext4, xfs and btrfs on Linux and apfs or hfs on macOS (for the native
Sandbox, where the SDK and daemon share one macOS kernel). Every other platform
is `Unsupported`.

```text
StoreService::bind(workspace, branch)          Store::bind_private
  overlay maintenance stopped? -> refuse         phase Snapshot:
  bound < capacity?            -> else Capacity    readers.mount(ws, pooled)
  store.bind_private(owner, request) ----------->  reader.snapshot(branch)
                                                 phase Root:
                                                   checked_base(client, snap)
                                                 phase Open:
                                                   owner.open(.., root)
                                                 -> BoundWorkspace
  on refusal: bound -= 1                         on error: close_reader
```

`checked_base` reads only the root inode, the root directory page and its
attributes; it never enumerates the namespace
([`store/bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs)).
`release` closes the Workspace, decrements `bound` and retires its overlay
namespace. `retain` keeps it and answers `Unknown` with the Workspace id and
namespace in the detail.

## Read handles

```text
ReadSet (one Mutex<State>)
  sessions: { None -> session 0 (control), Some(ws) -> session n }
  idle:     [(index, StoreReader)] <= IDLE_READERS = 4
  pooled:   Workspaces whose reader returns to idle at close

ReadSession (one Mutex per session; held for one demand)
  attempt(call):  lock -> failure set? -> reader present? -> call
                  unknown outcome -> failure is kept (quarantine)
```

| Handle | Opened | Closed | Cache |
| --- | --- | --- | --- |
| Control reader (session 0) | At Store open; with shared readers, at its first use (`ReadSet::control`) | Whole-Store `stop_reads` | None: each demand reads through a fresh reader |
| Mounted Workspace reader | `mount_reader` at mount; with shared readers, reuses an idle reader first | `close_reader` at unmount; with shared readers, returned to idle (up to 4) if it served without failure | `KeptReads` kept while mounted, dropped after a failed demand or above `KeptReads::ORDINARY_BYTES` |
| SDK private reader | `bind_private`; reuses an idle reader first | Returned to idle (up to 4) if it served without failure | As a mounted reader |

A PostgreSQL Store opens with shared readers (`Store::with_shared_readers`,
#64); a SQLite Store keeps the rows above without them. Its readers open
with the light check ([PostgreSQL](../store/postgresql.md)), so a new one costs
a connection and one round trip. An idle reader is checked before a mount
takes it (`StoreReader::usable`: one empty query, outside the `ReadSet`
lock); one that is quarantined or does not answer is dropped and a new
reader opens, so no operation runs on it and none is retried. A control
reader opened at first use is that operation's one attempt: a failure is
returned to it and the next use opens again. With shared readers a daemon
holds at most the writer, its lane connection (opened in the first Commit),
the control reader (in practice opened by the first Commit, which reads the
published tree through it), one reader per mounted Workspace and
`IDLE_READERS` = 4 idle ones. The writer serves no Workspace reads (its
admission is the one writer's, and a read's unknown outcome would
quarantine it), so a fresh daemon needs a reader for its first mount. It
opens that one eagerly (#64, owner decision 2026-11-06): after it reported
ready (a `Hello` answered ControlReady was sent), a thread of its own calls
`Store::prefill_reader`, which opens one reader into the idle readers,
once per Store and only with shared readers (PostgreSQL). It never delays
readiness. A failure is written to the daemon's log and counted
(`ReuseWork::prefill_failures`), and nothing opens it again: the first mount
then opens its own reader. A mount that arrives before the idle reader is
open also opens its own; the late one joins the idle readers under their
cap. So an idle PostgreSQL daemon holds 2 server connections after create
(its writer and the idle reader; before #64's eager reader, 1), and its
first mount opens none. At most it holds the writer, the lane connection,
the control reader, one reader per mounted Workspace and 4 idle ones, as
before.

All SDK published-history lookups (`Store::published`) go through session 0,
the read-only control session, so they never queue behind a Commit that holds
the publication lane
([`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs)).
A session whose demand had an unknown outcome keeps that failure; it refuses
later demands and is not returned to the idle pool
([`store/read_handle.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_handle.rs)).
An unknown open keeps its failure under the Workspace id and never permits a
second open for it ([`store/read_service.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_service.rs)).

| Read admission error | When |
| --- | --- |
| `Capacity` | More sessions than the Store's Workspace capacity. |
| `Stopped` | The read set is stopping, or the Workspace already has a session. |
| `NoReaders` | No session for this Workspace. |
| `Poisoned` | The read set's lock was poisoned. |

## Recognizer

```text
Recognizer(Mutex<Table>)    RECOGNIZER_SLOTS = 64, never grows
  Slot { route, serial, run: WriteRun, used }
  take(route, serial)  -> run out of the table (no lock while hashing)
  put(route, serial, run, work) -> back in; LRU slot evicted when full
  published(route, serial, [offset, end), revision)
```

The recognizer is the fixed table of write-recognizer slots shared by every
Workspace of one daemon. A slot holds one file's write state and at most one
carried chunk, never a byte of a write. A file without a slot starts over,
which costs only a missed chunk claim
([`store/recognizer.rs`](../../../../engine/crates/layerfs-store-service/src/store/recognizer.rs)).
How Workspace writes use it is in [Workspace: Mutation](../workspace/mutation.md).

## Statistics

| Function | Answer | Source |
| --- | --- | --- |
| `Store::logical_size(node)` | A Commit's `meta.statistics.logical` or a Layer's `logical`, read from history; used as the known size when a merge counts its new logical size. | [`store/statistics.rs`](../../../../engine/crates/layerfs-store-service/src/store/statistics.rs) |
| `Store::work`, `reuse_work`, `read_work`, `save_counters`, `merge_record_work` | Demand, reuse, read-session, Save and merge-record counters. | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |
| `SqlSessions::totals` | Writer and reader SQL work, summed under one registry turn. | [`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs) |
| `telemetry::attribute_sql`, `phase_sql` | While a thread records, each `phase` (Commit's `capture`, `construct`, `finish`, `logical.count`, `bindings`, `lane`, `head.lane`, `validate`, `publish`, …) reads the caller's SQL probe before and after; `phase_sql` returns each phase's inclusive difference. Independent of the timing tree's node bound. Used by the round-trip count test ([`postgresql_round_trips.rs`](../../../../engine/crates/layerfs-store-service/tests/postgresql_round_trips.rs)). | [`telemetry.rs`](../../../../engine/crates/layerfs-store-service/src/telemetry.rs) |

These counters are observations; they are not performance claims.

## Related

- [Store: Lifecycle](../store/lifecycle.md) and [Store: Backends](../store/backends.md) — what `open_store_observed` opens.
- [Store: History publication](../store/history-publication.md) — the publish lane that reads avoid.
- [Reads](reads.md), [Writes](writes.md), [Branches](branches.md) — the calls that use the service.
- [Overlay](../overlay/README.md) — the private overlay a bind opens.
- [Daemon](../daemon/README.md) — the other user of `Store`.

---

← [Engine: Store calls](README.md) · [Up: Store calls](README.md) · [Store calls: Reads](reads.md) →
