# PostgreSQL

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The PostgreSQL backend of the [Store](README.md): one schema in a PostgreSQL database, reached over a connection string, written one transaction at a time behind the `store_lane` row.

| | |
| --- | --- |
| Source | [`backend/postgresql/`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql), [`store/postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/store/postgresql.rs), [`sql/postgresql/`](../../../../engine/crates/layerfs-persistence/sql/postgresql) |

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
 |  store_policy, pack, object_location, inode_value_block,     |
 |  content_signature, store_totals                             |
 |  history_meta, layer_stack, layer, "commit", branch,         |
 |  scope_allocator, binding_hint_block                         |
 |  layer_lineage, commit_lineage, absorbed, reach, file_digest |
 |  init_operation, init_entry, init_host_file   (if created)   |
 +--------------------------------------------------------------+
 | pg_temp of the writer connection: merge_operation,           |
 |  merge_record   (gone when the connection ends)              |
 +--------------------------------------------------------------+
```

| Difference from SQLite | PostgreSQL | Source |
| --- | --- | --- |
| Types and order | `bigint`, `bytea`; history names `COLLATE "C"`, so paging matches SQLite byte order | [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| Pack bodies | one `bytea` per pack, `STORAGE EXTERNAL` (uncompressed, out of line) for ranged `substring` reads; no `pack_unit` table | same |
| Object locations | narrow rows; no unique key on the id but a B-tree `object_location_prefix` on its first eight bytes, with the full id compared in the heap; a location is inserted only when its id is absent, which is unique because every location insert holds the lane | [`objects/publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish.rs), [`metadata/reads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs) |
| Short values | bounded `bytea`/`text` columns and both block bodies are `STORAGE PLAIN` (column storage in `CREATE TABLE`, PostgreSQL 16 or newer), so those tables have no TOAST relation | [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| Merge records | `pg_temp` tables of the writer connection: outside the checked schema, seen by that connection only, gone with it; writes are `Stage` transactions | [`merge_records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/merge_records.rs) |
| Schema version | `SCHEMA_VERSION = 2`; 19 tables, or 22 with acquisition; version 1 is refused (`Integrity`), never converted | [`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs) |
| File operations | none: no seal, snapshot, heads digest, checkpoint or reclamation (`Handles::file` is `None`) | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |

## Authority and access

```text
 PostgresHandles::create(selection, policy, history)    (Init only)
   schema exists in pg_namespace? --> Refused "schema exists", nothing written
   one Stage transaction: CREATE SCHEMA, Store + acquisition tables,
   store_policy row, history_meta row (catalog id, incarnation, binding)
   then open with the Full check

 open_writable / open_read_only / attach_read_only(selection)
   one simple-query round trip, as selection.check says:
     Full : REPEATABLE READ snapshot; exact table set; live column
            definition from information_schema vs the recorded text;
            history_meta and policy rows; server_version
     Light: one autocommit statement; history_meta and policy rows,
            recorded source length, acquisition tables by to_regclass,
            server_version
   wrong binding, catalog id, incarnation, identity format or schema
   version: Integrity; a policy row that does not validate: Malformed
```

| Call | What it answers | Source |
| --- | --- | --- |
| `create` | A new empty Store with Init's acquisition tables; projects reach it only by Init, nothing converts a SQLite Store | [`store/postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/store/postgresql.rs) |
| `Full` check (default) | Init, Store connect and the host's Store service; the only check that finds a column changed by hand | [`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs) |
| `Light` check | A Sandbox daemon's writer, Workspace and control readers, and `attach_read_only`; a hand-changed column fails the statement that uses it instead | [`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs), [`owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs) |
| `binding(selection)` | The binding key a schema records, read in one read-only session, so a home that did not create it can open it | [`store/postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/store/postgresql.rs) |
| `attach_read_only(selection)` | One read-only session under the recorded binding, returned for the caller to compare with its own | same |
| `postgresql_access(url, schema)` | Server version, one round trip's latency, `current_user`, whether the schema exists, and each privilege the role lacks: the schema's `USAGE`, the database's `TEMP` (merges stage in temporary tables), each table's `SELECT`/`INSERT`/`UPDATE`/`DELETE` and each sequence's `USAGE` | [`access.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/access.rs) |
| `postgresql_status(url, schemas)` | Server version, one round trip's latency and each named schema's size | [`status.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/status.rs) |

`access` and `status` open one connection for the call and close it, run
read-only queries only and never retry. An authentication or unknown
database error is `Denied`; any other connection failure `Unreachable`. The
session keeps the frozen policy and server version the open check read, so
`PackPersistence::policy` costs no round trip afterwards.
`PostgresSelection`'s `Debug` redacts the URL.

## Session

```text
 postgresql::Session
   admission: Admission          FIFO turn, as on SQLite
   state: Mutex<State>           client (statement cache 256),
                                 quarantined, work counters
   lane: Mutex<Option<Client>>   second connection, opened on first
                                 hold_lane
   settings: schema, synchronous_commit, lock_timeout_ms
   connector: Config + the process's one TLS connector, or none
              for sslmode=disable
```

| Event or counter | Meaning | Source |
| --- | --- | --- |
| New connection answered with an error | `Rejected` | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/connection.rs) |
| No connection made | `Closed` | same |
| Lost reply inside a transaction | `Unknown`; session quarantined | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs) |
| `COMMIT` refused (deferred constraint) | the error; counted as a rollback | same |
| `round_trips` | each exchange that waits for the server; the handshake is not one | same |
| `prepares`, `connects` | statement-cache misses and COPY statements; new connections (session and lane) | same |

Each statement waits for its reply before the next is sent; nothing is
pipelined. `sslmode` is used exactly as given: the driver's default `prefer`
costs one extra round trip per connection on a server without TLS, so a
loopback or private route can give `sslmode=disable`; other routes give
`require` or `verify-full`. The TLS connector loads the system trust roots
once per process, at the first connection whose `sslmode` may negotiate TLS.
Every setting is `SET LOCAL` in the same round trip as `BEGIN`, so a
transaction-pooling proxy never carries one session's settings to another.

## Transactions

| `Mode` | `BEGIN` | Lane row | Used by |
| --- | --- | --- | --- |
| `Read` | `REPEATABLE READ READ ONLY` | no | history reads, policy, signatures |
| `Write` | `READ COMMITTED` + `synchronous_commit` + `lock_timeout` | `FOR UPDATE`, unless nested in the held lane | history writes, allocation, acquisition writes |
| `Stage` | `READ COMMITTED` + `synchronous_commit` | no | schema create, merge records |
| `Publish` | as `Write` | `FOR UPDATE` after the pack bodies, unless nested | one Save publication, up to 16 MiB − 1 |
| `Immutable` | none; autocommit statements | no | locates, pack reads, inode values |
| `Statement` | none; exactly one autocommit statement | no | `branch_snapshot`, a Commit or Layer by id, a LayerStack by name, `digest_roots` (up to `FILE_DIGEST_WINDOW` = 1,024 contents) |

Source: [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs),
[`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs).
A single statement sees one snapshot, so `Statement` reads a Branch and the
rows it resolves to coherently. Statements outside a transaction name the
schema (`{s}` in [`queries/history`](../../../../engine/crates/layerfs-persistence/sql/postgresql/queries/history)).
Writes are never `SERIALIZABLE` and never retried.

```text
 PostgresStorage::publish(batch)                 one Publish transaction
   validate(batch)                               shared bounds, no SQL
   BEGIN; SET LOCAL ...                          ids already reserved:
   COPY BINARY pack bodies                       no lane needed yet
   SELECT 1 FROM store_lane ... FOR UPDATE       (skipped if nested)
   one statement: INSERT locations ... RETURNING, totals, inode value
     blocks, released ordinals, signatures
   COMMIT
   any failure rolls the whole publication back; ids are never reused
```

The first read of a pack returns its descriptor with the head of the body up
to `PACK_LIMIT` (256 KiB), which `whole_pack_bytes` takes whole, so such a
pack costs one statement; other ranges are one `substring` statement each
([`objects/read.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/read.rs),
[`storage/provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/storage/provider.rs)).

The publication lane is one `READ COMMITTED` transaction on the lane
connection holding `store_lane` `FOR UPDATE` under `lock_timeout`. While it is
held, the thread's writes run on the session connection as nested
transactions. Dropping `LaneHold` rolls the lane transaction back (it wrote
nothing) and releases the turn
([`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/lane/lane.rs)).

| `SynchronousCommit` | Write acknowledged after |
| --- | --- |
| `Off` (default) | the server accepts it, before the WAL flush |
| `Local` | the local WAL flush |
| `RemoteWrite` | standbys receive the WAL |
| `On` | local and synchronous-standby flush |
| `RemoteApply` | synchronous standbys apply the WAL |

Chosen per Store in `PostgresSelection` and applied with `SET LOCAL` to every
`Write`, `Stage` and `Publish` transaction
([`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs)).

## Acquisition

`PostgresAcquisition` implements SQLite's acquisition port with the same row
encodings, charges, refusal order and failures. Each unit is one transaction;
reads are index range scans in one `REPEATABLE READ` snapshot after the owner
check; writes are lane-ordered. A released or foreign operation is `Stale`
before any row is read. A schema without the acquisition tables refuses every
unit ([`acquisition/`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition)).

```text
 release of the last operation                    one Write transaction
   SELECT 1 FROM store_lane ... FOR UPDATE
   DELETE the record; no other record left?
     SAVEPOINT; SET LOCAL lock_timeout 2 s
     LOCK init_operation, init_entry, init_host_file
     TRUNCATE ... CONTINUE IDENTITY; RELEASE SAVEPOINT
     a definite refusal -> ROLLBACK TO SAVEPOINT; dead rows stay
   COMMIT
```

Emptying the tables leaves no dead tuples or emptied B-tree pages after an
Init. A live operation's record is committed before its first row and is
read under the lane row, so compaction never touches it; a refusal keeps the
release's outcome unchanged and is not retried; `CONTINUE IDENTITY` never
reuses an operation id
([`cleanup.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/cleanup.rs)).

| Test | Command | Shows |
| --- | --- | --- |
| [`postgresql_open_check.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_open_check.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --features postgresql --test postgresql_open_check` | `Full` and `Light` refusals |
| [`postgresql_acquisition_compaction.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition_compaction.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --features postgresql --test postgresql_acquisition_compaction` | Compaction after the last release, never touching a live operation |

Without `LAYERFS_TEST_POSTGRES_URL` these tests print `NOT_RUN` and check
nothing.

## Related

- [Store](README.md): the seam, admission, history calls and errors.
- [SQLite](sqlite.md): the tables these mirror.
- [Diagnostics](diagnostics.md): the PostgreSQL audit and its physical checks.
- [Control: Stores and projects](../../control/stores-and-projects/sqlite-and-postgresql.md): how a home connects and attaches a PostgreSQL Store.
