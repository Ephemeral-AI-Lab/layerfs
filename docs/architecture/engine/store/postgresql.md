# PostgreSQL

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The PostgreSQL backend of the [Store](README.md): one schema in a PostgreSQL database, reached over a connection string, written one transaction at a time behind the `store_lane` row.

| | |
| --- | --- |
| Source | [`backend/postgresql/`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql), [`store/postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/store/postgresql.rs), [`sql/postgresql/`](../../../../engine/crates/layerfs-persistence/sql/postgresql) |

The backend is always compiled beside SQLite; it adds the `postgres`,
`postgres-native-tls` and `native-tls` dependencies
([`Cargo.toml`](../../../../engine/crates/layerfs-persistence/Cargo.toml)).
There is no `postgresql` cargo feature: every build serves both Store kinds.

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
| `postgresql_access(url, schema)` | Server version, one round trip's latency, `current_user`, whether the schema exists, and each privilege the role lacks: the schema's `USAGE` and `CREATE`, the database's `CREATE` (compaction creates the scratch schema) and `TEMP` (merges stage in temporary tables; compaction reads its visited set from them), `pg_read_all_stats` (compaction reads the recorded owner's `pg_stat_activity` row and calls `pg_terminate_backend`), each table's `SELECT`/`INSERT`/`UPDATE`/`DELETE` and each sequence's `USAGE` | [`access.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/access.rs) |
| `postgresql_status(url, schemas)` | Server version, one round trip's latency and each named schema's size | [`status.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/status.rs) |

`access` and `status` open one connection for the call and close it, run
read-only queries only and never retry. An authentication or unknown
database error is `Denied`; any other connection failure `Unreachable`. The
session keeps the frozen policy and server version the open check read, so
`PackPersistence::policy` costs no round trip afterwards.
`compaction_missing(session)` applies the same five Store-wide checks to the
compaction session in one read-only round trip, before the intent, the mark
and the `lfs_gc_<hex>` scratch schema; the first privilege the role lacks is
`Unsupported`, naming it (AC-44). It is reachable from a session because a
compaction holds one already and never retains the URL.
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
| `Snapshot` | `REPEATABLE READ` + `synchronous_commit` + `jit` off | no | the garbage-collection mark: reads history, locators and packs from one snapshot and writes only `kept_node` |
| `Immutable` | none; autocommit statements | no | locates, pack reads, inode values |
| `Statement` | none; exactly one autocommit statement | no | `branch_snapshot`, a Commit or Layer by id, a LayerStack by name, `digest_roots` (up to `FILE_DIGEST_WINDOW` = 1,024 contents) |

Source: [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs),
[`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs).
A single statement sees one snapshot, so `Statement` reads a Branch and the
rows it resolves to coherently. Statements outside a transaction name the
schema (`{s}` in [`queries/history`](../../../../engine/crates/layerfs-persistence/sql/postgresql/queries/history)).
Writes are never `SERIALIZABLE` and never retried.

The mark runs with the server's JIT compiler off: the planner's estimate for
the history closure crosses the JIT threshold, and compiling it cost about
400 ms for a statement that runs in about 10 ms. Inside its snapshot the mark
also reads ahead, because each statement costs a round trip:

| Read | One statement covers | Bound | Source |
| --- | --- | --- | --- |
| Pack heads and record ranges of payload packs | 256 packs; 1,024 ranges or 512 KiB | `HEAD_PACKS`, `BATCH_RANGES`, `BATCH_BYTES` | [`edges.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/edges.rs) |
| Metadata packs the walk decodes | the next 128 packs of the wave, each up to 32 KiB (a longer pack is read alone) | `AHEAD_PACKS`, `AHEAD_BYTES` | [`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs), [`read.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/read.rs) |
| Inode values of a leaf | the blocks holding up to 4,096 ordinals past the leaf's own; at most 65,536 values kept | `AHEAD_VALUES`, `HELD_VALUES` | [`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs), [`reads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs) |

A SQLite Store ignores the pack hint and reads no values ahead. Rows read
ahead are the rows the single reads return, so the mark's result and its
`mark_bytes` do not change.

### Generation fence

The session keeps the `catalog_incarnation` its open read
(`session.incarnation`), and `run_as` checks it against the Store on the
paths that can observe a compaction swap (spec §5.2, §10):

- **Transactions** (`Read`, `Write`, `Snapshot`, `Stage`, `Publish`) append
  the check to the `BEGIN` batch, so it costs no extra round trip and the
  `AccessShareLock` the transaction takes on `history_meta` holds until
  `COMMIT`: a swap that starts after this point meets `LOCK TABLE ...
  ACCESS EXCLUSIVE NOWAIT` and abandons, and one that already installed
  `G + 1` makes the transaction `StaleStore`.
- **Autocommit reads** (`Statement`, `Immutable`) join
  `catalog_incarnation` as the trailing column of the read's one statement,
  under a schema-qualified `history_meta` one-row subquery; the trailing
  column is checked and removed before the row reaches the decoder. A read
  that finds no row needs no generation to answer: the swap retains exactly
  the live set (`live_heads_digest` is unchanged), so an absence at `G` is an
  absence at `G + 1`.
- A session whose open has not set a generation yet (the open check's own
  `Statement` reads) is unfenced, and the Store diagnostic reads through
  `run_observed`: it must be able to read a Store whose authority changed and
  report that change as the `store.identity` failure, not refuse with
  `StaleStore`.

A mismatch is `StaleStore`: nothing was written and the caller reopens the
Store.

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
| [`postgresql_open_check.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_open_check.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql_open_check` | `Full` and `Light` refusals |
| [`postgresql_acquisition_compaction.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition_compaction.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql_acquisition_compaction` | Compaction after the last release, never touching a live operation |

Without `LAYERFS_TEST_POSTGRES_URL` these tests print `NOT_RUN` and check
nothing.

## Related

- [Store](README.md): the seam, admission, history calls and errors.
- [SQLite](sqlite.md): the tables these mirror.
- [Diagnostics](diagnostics.md): the PostgreSQL audit and its physical checks.
- [Control: Stores and projects](../../control/stores-and-projects/sqlite-and-postgresql.md): how a home connects and attaches a PostgreSQL Store.
