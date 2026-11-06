# PostgreSQL

[Architecture](../../README.md) › [Engine](../README.md) › [Store](README.md) › PostgreSQL

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The PostgreSQL backend.

| | |
| --- | --- |
| Component | [Store](README.md) |
| Source | [`engine/crates/layerfs-persistence/src/backend/postgresql`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql), [`engine/crates/layerfs-persistence/src/store/postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/store/postgresql.rs) |

The backend is compiled only with the cargo feature `postgresql`, which adds
the `postgres`, `postgres-native-tls` and `native-tls` dependencies
([`Cargo.toml`](../../../../engine/crates/layerfs-persistence/Cargo.toml)).
Without it, selecting a PostgreSQL Store fails `BackendUnavailable`.

## Schema

```text
 one PostgreSQL database
 +--------------------------------------------------------------+
 | schema <selection.schema>      (one Store; [a-z_][a-z0-9_]*) |
 |  store_lane (id=1)   <-- the row every writer locks          |
 |  store_policy, pack, object_location, inode_value,           |
 |  content_signature, store_totals                             |
 |  history_meta, layer_stack, layer, "commit", branch,         |
 |  scope_allocator, binding_hint                               |
 |  layer_lineage, commit_lineage, absorbed, reach, file_digest |
 |  init_operation, init_entry, init_host_file   (if created)   |
 +--------------------------------------------------------------+
 | pg_temp of the writer connection: merge_operation,           |
 |  merge_record   (gone when the connection ends)              |
 +--------------------------------------------------------------+
```

| Difference from SQLite | PostgreSQL | Source |
| --- | --- | --- |
| Types | `bigint` for integers, `bytea` for BLOBs | [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| Name order | history names `COLLATE "C"`, so paging matches SQLite byte order | same |
| Pack bodies | one `bytea` per pack, `STORAGE EXTERNAL` (uncompressed, out of line) for ranged `substring` reads | same |
| Pack units | absent: group rows are a SQLite physical layout | [`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs) |
| Lane | `store_lane`, one row | [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| Merge records | temporary tables of the writer connection | [`merge_records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/merge_records.rs) |
| Schema version | `SCHEMA_VERSION = 1`, engine-scoped | [`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs) |

`PostgresHandles::create` checks `pg_namespace` and refuses an existing
schema (`Refused "schema exists"`) before writing. It then creates the
schema, the Store and acquisition tables, the policy row and the
`history_meta` row in one transaction. Open never creates or migrates a
schema; it checks the schema in one unprepared simple-query round trip, as
the selection's `PostgresCheck` says (#63):

| Check | Reads | Opened with it |
| --- | --- | --- |
| `Full` (default) | one `REPEATABLE READ` snapshot: the exact table set (19 tables, or 22 with acquisition) and the live column definition from `information_schema`, compared with the recorded source and definition; then the `history_meta` and policy rows and `server_version` | Init (`PostgresHandles::create`, always), Store connect and the host's Store service |
| `Light` | one autocommit statement: the `history_meta` and policy rows, the recorded source's length, the three acquisition tables by name (`to_regclass`) and `server_version`; no `information_schema` scan | a Sandbox daemon's writer and every Workspace and control reader (#64), and a selection that asks for it (`with_check`), and `attach_read_only` |

Both refuse a wrong binding, catalog id, incarnation, identity format or
schema version (`Integrity`), and a policy row that does not validate
(`Malformed`). Only `Full` finds a column changed by hand after the schema
was checked; under `Light` the statement that uses it fails instead. The
session keeps the frozen policy and the server version the check read, so
`PackPersistence::policy` costs no round trip afterwards
([`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs),
[`postgresql_open_check.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_open_check.rs)).

## Connection and session

```text
 postgresql::Session
 +--------------------------------------------------------------+
 | admission: Admission          FIFO turn, as on SQLite        |
 | state: Mutex<State>           client (statement cache 256),  |
 |                               quarantined, work counters     |
 | lane: Mutex<Option<Client>>   second connection, opened on   |
 |                               first hold_lane                |
 | settings: schema, synchronous_commit, lock_timeout_ms        |
 | connector: Config + the process's one TLS connector          |
 |            (sslmode from the connection string, as given)    |
 +--------------------------------------------------------------+
```

| Event | Result | Source |
| --- | --- | --- |
| Server answers a new connection with an error | `Rejected` | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/connection.rs) |
| No connection made | `Closed` | same |
| Lost reply inside a transaction | `Unknown`; session quarantined | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs) |
| `COMMIT` refused (deferred constraint) | the error; counted as a rollback | same |

`Session::diagnostics` returns the session's `SqlWork`. Besides the shared
statement, transaction and wall counters it fills three PostgreSQL-only
counters (zero on SQLite):

| Counter | Counts | Source |
| --- | --- | --- |
| `round_trips` | each exchange that waits for the server: a prepare, a statement execution, a simple-query script (`BEGIN` with its `SET LOCAL`s, `COMMIT`, `ROLLBACK`, schema scripts), a binary COPY's start and finish, the open's check (one), and the lane's `BEGIN … FOR UPDATE` and `ROLLBACK` | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs), [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/connection.rs), [`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/lane/lane.rs) |
| `prepares` | statement-cache misses and each COPY's unnamed statement | same |
| `connects` | new connections: the session's own and its lane connection | same |

A connection's handshake (TCP, the SSL probe of `sslmode=prefer`, startup
and authentication) is one `connects`, not part of `round_trips`.

The connection string's `sslmode` is used exactly as given; nothing changes
it. The driver's default, `prefer`, sends an SSLRequest first and costs one
more round trip per connection on a server without TLS. On a loopback or
private network route (a server on the same host, a Docker network) give
`sslmode=disable` to skip it; on any other route give `require` or
`verify-full`. The TLS connector, which loads the system's trust roots, is
built once per process. The Store
service reports the writer session and every Workspace reader session,
live and retired, as on SQLite
([`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs)).

## Transactions

| `Mode` | `BEGIN` | Lane row | Used by | Source |
| --- | --- | --- | --- | --- |
| `Read` | `REPEATABLE READ READ ONLY` | no | history reads, policy, signatures | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs) |
| `Write` | `READ COMMITTED` + `synchronous_commit` | `FOR UPDATE`, unless nested in the held lane | history writes, location rows, allocation | same |
| `Stage` | `READ COMMITTED` + `synchronous_commit` | no | schema create, merge records | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs) |
| `Publish` | `READ COMMITTED` + `synchronous_commit` + `lock_timeout` | `FOR UPDATE` after the pack bodies, unless nested in the held lane | one Save publication wave, up to 16 MiB − 1 (`publication_bytes`; SQLite keeps 4 MiB − 1) | same |
| `Immutable` | none; autocommit statements | no | locates, pack reads, inode values | same |
| `Statement` | none; exactly one autocommit statement (a second is refused before it is sent) | no | `branch_snapshot`, `file_roots` (`digest_roots`, one statement for up to `FILE_DIGEST_WINDOW` = 1,024 contents) | same |

A single PostgreSQL statement sees one snapshot, so `Statement` reads a Branch
and the rows it resolves to coherently without `BEGIN`/`COMMIT`. Statements run
outside a transaction name the schema (`{s}` in
[`queries/history`](../../../../engine/crates/layerfs-persistence/sql/postgresql/queries/history)),
since no `SET LOCAL search_path` applies to them.

Every setting is `SET LOCAL` in the same round trip as `BEGIN`, so a
transaction-pooling proxy never carries one session's settings to another.
Writes are never `SERIALIZABLE` and never retried.

```text
 PostgresStorage::publish(batch)                  one Publish transaction
   validate(batch)                                (shared bounds)
   BEGIN; SET LOCAL ... lock_timeout              1 round trip
   COPY BINARY pack bodies                        2 (prepare cached); ids were
                                                  reserved: no other writer
                                                  touches them, so no lane yet
   SELECT 1 FROM store_lane ... FOR UPDATE        1 (skipped if nested)
   WITH ins AS (INSERT locations ... RETURNING),  1: one data-modifying
        totals, inode values, released ordinals,     statement; totals count
        signatures  SELECT inserted ids               the inserted locations
   COMMIT                                         1
   any failure -> the whole publication rolls back; ids are never reused
```

The lane row is held only for the rows statement and the COMMIT; the body
upload happens before it.

Source: [`objects/publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish.rs),
[`storage/provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/storage/provider.rs).

The publication lane is one `READ COMMITTED` transaction on the lane
connection holding `store_lane` `FOR UPDATE` with `lock_timeout`. While it
is held, the thread's writes run on the session connection as nested
transactions without locking the row again. Dropping `LaneHold` rolls the
lane transaction back (it wrote nothing) and releases the turn. Source:
[`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/lane/lane.rs).

## Acquisition

`PostgresAcquisition` implements the same acquisition port as SQLite over the
session: each unit is one transaction, reads in one `REPEATABLE READ`
snapshot, writes lane-ordered. The live owner is matched against its row
first, so a released or foreign operation is `Stale` before any row is read.
A schema without the acquisition tables refuses every unit. Source:
[`acquisition/`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition).

Deleted working rows stay behind as dead tuples until autovacuum, and B-tree
pages are never returned, so on its own a Store would be several MB larger
right after an Init than a minute later, and keep the emptied indexes for
good. The transaction that releases the last operation therefore empties the
three tables (#87):

```text
 release (or the dispose job that removes the last row)   one Write transaction
   SELECT 1 FROM store_lane ... FOR UPDATE                  every acquisition write
   WITH gone AS (DELETE the record) SELECT no other record   one statement
   if no other record:
     SAVEPOINT; SET LOCAL lock_timeout 2 s
     LOCK init_operation, init_entry, init_host_file          ACCESS EXCLUSIVE, record first
     TRUNCATE ... CONTINUE IDENTITY; RELEASE SAVEPOINT        one script, one round trip
     a definite refusal -> ROLLBACK TO SAVEPOINT; keep the dead rows
   COMMIT
```

| Concern | Why it holds |
| --- | --- |
| A live operation of another session | Its record is committed before its first row. The check runs after the lane row is held and reads every committed record. |
| An Init starting concurrently | `begin` is a lane-ordered write, so it waits for the lane row until this transaction commits. Its record goes into the emptied table. |
| Readers | Only a live operation's units read working rows, and each unit reads the record first. The locks are taken in that order, so a reader that already holds a table blocks the compaction rather than deadlocking with it. |
| Autovacuum on a table | It is cancelled after `deadlock_timeout` (1 s by default). The 2 s wait covers that. |
| A refusal (lock wait, privilege) | The savepoint keeps the release's outcome unchanged. The dead rows are left for autovacuum, as before. Nothing is retried. A lost reply stays uncertain. |
| Operation identity and epochs | `CONTINUE IDENTITY`: an id is never reused. |

So a Store's size right after an Init is a never-used Store's size for the
acquisition tables, and autovacuum later finds nothing there to reclaim
([`postgresql_acquisition_compaction.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition_compaction.rs)).
Constructed file roots are still recorded by an `UPDATE` of each host-file
row, which doubles that table's pages while an Init runs. Removing that `UPDATE`
needs a change to the acquisition schema.

`postgresql_status(url, schemas)` reports a server's version, one
round trip's latency and each named schema's size, on a connection opened and
closed for the call, with no retry
([`status.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/status.rs)).

## synchronous_commit

| `SynchronousCommit` | Setting | Write is acknowledged after |
| --- | --- | --- |
| `Off` (default) | `off` | the server accepts it, before the WAL flush |
| `Local` | `local` | the local WAL flush |
| `RemoteWrite` | `remote_write` | standbys receive the WAL |
| `On` | `on` | local and synchronous-standby flush |
| `RemoteApply` | `remote_apply` | synchronous standbys apply the WAL |

Source: [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs).
The value is chosen per Store in `PostgresSelection` and applied with
`SET LOCAL` to every `Write`, `Stage` and `Publish` transaction. The SQLite Disposable
profile is unaffected.

## Related

- [Backends](backends.md): the seam and the shared admission.
- [Lifecycle](lifecycle.md): `StoreSelection` and `Handles`.
- [SQLite](sqlite.md): the tables these mirror.
- [History publication](history-publication.md#the-publish-lane): what runs inside the lane.
- [Connect: Network](../../connect/network/README.md): how a machine reaches the server.

---

← [Store: SQLite](sqlite.md) · [Up: Store](README.md) · [Store: History publication](history-publication.md) →
