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
| Pack bodies | one `bytea` per pack, `STORAGE EXTERNAL` (uncompressed, out of line) for ranged `substring` reads; no `pack_unit` table; beside it `directory`, the pack's header and group directory (`body[0..body_offset)`, `STORAGE PLAIN`), which a CHECK holds to the body's prefix (see [delta chains](#delta-chains)) | same |
| Object locations | narrow rows; no unique key on the id but a B-tree `object_location_prefix` on its first eight bytes, with the full id compared in the heap; a location is inserted only when its id is absent, which is unique because every location insert holds the lane, including a nested one whose lane connection dies (see [the lane barrier](#lane-barrier)); each row also carries `ancestors`, its record's delta chain as a physical list (see [delta chains](#delta-chains)) | [`objects/publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish.rs), [`metadata/reads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs) |
| Short values | bounded `bytea`/`text` columns and both block bodies are `STORAGE PLAIN` (column storage in `CREATE TABLE`, PostgreSQL 16 or newer), so those tables have no TOAST relation | [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| Merge records | `pg_temp` tables of the writer connection: outside the checked schema, seen by that connection only, gone with it; writes are `Stage` transactions | [`merge_records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/merge_records.rs) |
| Schema version | `SCHEMA_VERSION = 3` (2.0's ancestor lists and pack directories, #758); 27 tables, or 30 with acquisition; version 1 is refused (`Integrity`); version 2 is refused as `Refused` ("not supported by this release and not migrated; create a new Store") by the Full, Light and diagnostic checks before any source comparison; nothing is converted | [`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs) |
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
| `postgresql_access(url, schema)` | Server version, one round trip's latency, `current_user`, whether the schema exists, and each privilege a Sandbox's reads and writes need that the role lacks: the schema's `USAGE` and `CREATE`, the database's `TEMP` (merges stage in temporary tables), each table's `SELECT`/`INSERT`/`UPDATE`/`DELETE` and each sequence's `USAGE`. What only a compaction needs (the database's `CREATE`, `pg_read_all_stats`) is not asked: `compaction_missing` checks it when a compaction starts | [`access.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/access.rs) |
| `postgresql_status(url, schemas)` | Server version, one round trip's latency and each named schema's size | [`status.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/status.rs) |

`access` and `status` open one connection for the call and close it, run
read-only queries only and never retry. An authentication or unknown
database error is `Denied`; any other connection failure `Unreachable`,
including a server silent past the 5 s connect-and-login limit
(`POSTGRESQL_HANDSHAKE`). The driver bounds only the socket, so every new
connection is waited for within that limit: a session's ordinary connection
is dialled on its own runtime under a timer, which drops the dial at the
limit; a synchronous client (the lane, `access`, `status`, compaction
recovery) is dialled on its own thread, and a dial left waiting ends when the
server answers or the socket closes. The
diagnostic driver gives its connect and login the same limit under a longer
deadline. `Connector::new` pairs a single `localhost` host (with no
`hostaddr`) with `127.0.0.1` and `::1` via `hostaddr` while keeping
`localhost` for TLS verification, so roots without `/etc/hosts` reach
either IPv4 or IPv6 loopback without DNS. The
session keeps the frozen policy and server version the open check read, so
`PackPersistence::policy` costs no round trip afterwards.
`compaction_missing(session)` checks what a compaction needs Store-wide on
the compaction session in one read-only round trip: the schema's `USAGE` and
`CREATE`, the database's `CREATE` (the scratch schema) and `TEMP` (its
visited set), `pg_read_all_stats` (the recorded owner's `pg_stat_activity`
row and its `pg_terminate_backend`) and ownership of the Store's tables,
before the intent, the mark and the `lfs_gc_<hex>` scratch schema; the first
privilege the role lacks is
`Unsupported`, naming it (AC-44). It is reachable from a session because a
compaction holds one already and never retains the URL.
`PostgresSelection`'s `Debug` redacts the URL.

## Session

```text
 postgresql::Session
   admission: Admission          FIFO turn, as on SQLite
   state: Mutex<State>           client: the ordinary driver
                                 (tokio-postgres on the session's own
                                 current-thread runtime) or the
                                 diagnostic one; statement cache 256,
                                 quarantined, work counters
   lane: Mutex<Option<LaneConnection>>
                                 second connection, opened on first
                                 hold_lane (the synchronous postgres
                                 client), with the id of its open
                                 lane transaction
   settings: schema, synchronous_commit, lock_timeout_ms
   connector: Config + the process's one TLS connector, or none
              for sslmode=disable
```

| Event or counter | Meaning | Source |
| --- | --- | --- |
| New connection answered with an error | `Rejected` | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/connection.rs) |
| No connection made, or none made and logged in within 5 s (the URL's `connect_timeout` when set; LF13) | `Closed` | same |
| Lost reply inside a transaction, or a SQLSTATE without a definite rule (class 58, class XX, ...) | `Unknown`; session quarantined and its connection closed (below) | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs), [`rows.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/rows.rs) |
| A statement refused with a reply in class 21, 22, 23, 42, P0 (PL/pgSQL, such as the `pack_immutable` trigger's `P0001`) or 54 outside the capacity codes | `Integrity`, definite; the transaction is rolled back | [`rows.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/rows.rs) |
| `COMMIT` refused (deferred constraint) | the error; counted as a rollback | same |
| `round_trips` | each exchange that waits for the server; the handshake is not one | same |
| `prepares`, `connects` | statement-cache misses and COPY statements; new connections (session and lane) | same |

The ordinary driver is a tokio-postgres client on a current-thread runtime
the session owns, with its connection as a task on it
([`driver.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/driver.rs));
each call blocks on that runtime, so the socket is served only while a call
waits, as the synchronous `postgres` client (the same client polled inline)
did before. Every call except a pipeline waits for its reply before the
next is sent. `Session::pipeline`
([`pipeline.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/pipeline.rs))
sends several requests (prepared statements with binary parameters, and
simple-protocol text) before awaiting any reply and collects every reply in
order, never stopping at the first error: each request ends in its own
`Sync`, so inside a transaction the requests after a failed one answer
`25P02` and a `COMMIT` rolls it back. It counts one round trip for the
flight, plus one for preparing every statement not yet cached, all in one
flight of their own (`prepares` counts each). A request that cannot be
encoded stops the flight before it, and when earlier requests went out, one
`ROLLBACK` follows. A lost reply quarantines the session; a reply the server
gave is the caller's to classify. No Store call uses it yet (#768 stage 2);
`postgresql_pipeline.rs` proves that a flight is one network wait by
holding every server byte at a proxy until the server shows the whole
flight ran. `sslmode` is used exactly as given: the driver's default `prefer`
costs one extra round trip per connection on a server without TLS, so a
loopback or private route can give `sslmode=disable`; other routes give
`require`, which verifies the server's certificate chain and host name (the
`native-tls` connector built with its defaults). The client reads no
`verify-ca` or `verify-full` and no certificate or key file: a connection
string naming one is refused (`Malformed` here, `InvalidInput` at `store
connect`). The TLS connector loads the system trust roots
once per process, at the first connection whose `sslmode` may negotiate TLS.
A private CA is trusted the platform's way: on Linux `native-tls` is
OpenSSL, which trusts the system's roots and those `openssl-probe` reads
from `SSL_CERT_FILE` (and `SSL_CERT_DIR`) in the environment of the
process that dials, so the variable reaches only that process's calls: a
Sandbox's daemon is started with a cleared environment (`PATH` only) and
reads none, so a CA its Store needs is added to the machine's system
trust store; on macOS it is the Security framework, which reads no such
variable, so the CA must be trusted in the login or system keychain.
Every setting is `SET LOCAL` in the same round trip as `BEGIN`, so a
transaction-pooling proxy never carries one session's settings to another.

A quarantine closes the session's ordinary connection (finding #3), and
the server ends its backend: the client is dropped, its connection task
sends `Terminate` when no reply is outstanding, and the task is aborted
after at most 1 s, which closes the socket. Kept open, that backend stayed until the
process exited: a transaction the server aborted with an error has already
released its row locks, but the session's advisory keys (mount and
compaction presence) survive any abort, and an uncertain outcome that left
the transaction live (a lost reply, or a body error classed uncertain) kept
every row lock, a write's `store_lane` row included, so every other writer
failed `Busy` after its lock wait. Closing ends all of it. Nothing is replayed,
the outcome stays unknown, every later call is refused before it reaches a
connection, and `PostgresStorage::usable` answers `false`, so the caller
opens a new session. GC intent rows are untouched: an intent's owner stays
live by its recorded process, not by this connection. A diagnostic driver
keeps its read-only connection, whose deadline and cancellation outcome are
still read ([`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/connection.rs)).

A mount, a compaction claim and their registrations keep session advisory
keys (Store, project and Branch) across statements
([`admission/session.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission/session.rs)).
The lock and the unlock are each one autocommit `Bare` statement for all of
an intent's keys, outside the generation fence (#776 P2): a fenced
statement would take or drop a key and then answer `Stale`, leaving it held
with no intent row behind it. The lock statement checks the generation
itself and tries no key when it moved. No key outlives a failed lock: a
refused or pooled lock gives back exactly the keys it was granted, and a
lock statement that fails, or whose reply cannot be read, quarantines the
session, whose closed connection gives back every key. An unlock attempts
every key it is given in its one statement; one that fails is handled the
same way ([GC admission](#gc-admission)). A key granted on a
backend other than the session's own is a transaction-pooling proxy:
the call is `Unsupported("transaction-pooled PostgreSQL connection")`, and
mount, merge, Init and the removals refuse it rather than run unadmitted.
Only a catalog with no GC admission at all (`HistoryError::no_admission`)
lets them proceed. A refused shared key reports its exclusive holder (a
compaction or removal row, or else the key's backend as `BusyOp`), never
peer mount rows.

## Transactions

| `Mode` | `BEGIN` | Lane row | Used by |
| --- | --- | --- | --- |
| `Read` | `REPEATABLE READ READ ONLY` | no | history reads, policy, signatures |
| `Write` | `READ COMMITTED` + `synchronous_commit` + `lock_timeout` | `FOR UPDATE` and the lane barrier; nested in the held lane: the barrier key shared and a lane check in the `COMMIT` round trip | history writes, allocation, acquisition writes |
| `Stage` | `READ COMMITTED` + `synchronous_commit` | no | schema create, merge records |
| `Publish` | as `Write` | `FOR UPDATE` and the lane barrier after the pack bodies; nested: as a nested `Write` | one Save publication, up to 16 MiB − 1 |
| `Snapshot` | `REPEATABLE READ` + `synchronous_commit` + `jit` off | no | the garbage-collection mark: reads history, locators and packs from one snapshot and writes only `kept_node` |
| `Immutable` | none; autocommit statements | no | locates, pack reads, inode values |
| `Statement` | none; exactly one autocommit statement | no | `branch_snapshot`, a Commit or Layer by id, a LayerStack by name, `digest_roots` (up to `FILE_DIGEST_WINDOW` = 1,024 contents) |
| `Admit` | as `Publish`: `READ COMMITTED` + `synchronous_commit` + `lock_timeout` | never | a mount's GC registration ([GC admission](#gc-admission)) |
| `Bare` | none; exactly one autocommit statement that may write, sent as written | no | the GC session keys and the owner's release; each carries its own generation check ([GC admission](#gc-admission)) |

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
| Metadata packs the walk decodes | the announced packs of the wave not held yet, up to 256 packs or 8 MiB of pack heads (a head is at most `PACK_LIMIT`, 256 KiB); a pack row is fetched once and kept, at most 32 MiB in two generations | `AHEAD_PACKS`, `AHEAD_BYTES`, `HELD_PACK_BYTES` | [`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs), [`read.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/read.rs) |
| Inode values of a leaf | the blocks holding the ordinals the walk does not hold yet and up to 4,096 ordinals past the last of them, less the blocks this transaction already fetched; a block is fetched once and kept, at most 16 MiB in two generations | `AHEAD_VALUES`, `HELD_VALUES`, `HELD_BLOCK_BYTES` | [`values.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/values.rs), [`reads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs) |
| Locators of delta bases | the bases one level of a pack group's chains names outside the pack, up to 4,096 identities; a locator is kept until 16,384 newer ones were, in two generations | `LOCATE_IDS`, `HELD_LOCATIONS` | [`walk.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/walk.rs) (`expect_bases`), [`reads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs) |

The ranged read takes a pack's ranges sorted and disjoint and joins those at
most 8 KiB apart (`SPAN_FILL`) into spans. The server fetches each span from the pack body
once and cuts the ranges out of it in memory, so the TOAST chunks under a run
of record headers are read once per span and not once per range. The rows
that come back are exactly the ranges asked for: the bytes between two
ranges of a span never leave the server.

A page stored as a delta is rebuilt from its base, and the reader asks for
the base's locator one identity at a time: the base sits in an earlier
Commit's pack, so the walk no longer has its locator at hand. One lookup is
one round trip, and a real Store asked for about 145,000 of them in one mark.
Before the walk rebuilds a record of a pack group it therefore reads the
base each metadata record of the group names, from the group it is about to
decode anyway, and sends the bases outside the pack to the server in one
statement. It then does the same for the bases those records name, and so
on down the chains, stopping at any element the reader's chain memo already
holds, because a rebuild stops there too. A group's lookups cost one
statement a level instead of one a record. This is a hint: a lookup that
was not announced, because the memo dropped an element in between, is still
made on its own (`Mechanics::expect_locations`, told only to an engine whose
`batches_locations` says a lookup costs a round trip).

The kept pack rows, value blocks and locators belong to the transaction and
go with it. They are filled only inside a transaction (one snapshot), and
pack rows only after the walk has announced packs, so an autocommit read
keeps nothing. A kept pack row is a copy of the descriptor and the body's
head: the driver cuts its rows out of the buffers it received them in, so
keeping the driver's row would keep a whole receive buffer for one small
pack, and the bound would not be the memory held. The sizes are chosen so a
whole mark stays near 200 MB on the client: on a real Store of 1.69 million
objects, 96 MiB of pack rows and 64 MiB of value blocks peaked at 290 MB
and sent 14,229 statements; 32 MiB and 16 MiB peaked at 206 and 215 MB in
two samples and send 15,558.

A SQLite Store ignores the pack hint and the locator hint and reads no
block ahead; it returns every value of the blocks it does read
([SQLite: the mark's reads](sqlite.md#the-marks-reads)). Rows read ahead are
the rows the single reads return, so the mark's result and its `mark_bytes`
do not change.

A compaction round's mark, its decision and a rebuild's visited ids are one
repeatable-read transaction
([`compact/op.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/compact/op.rs)
`mark`). After the walk it sends the server the live pack ids
(`gc_live_pack`), read from the mark's own list in ascending order and sent
32,768 a statement, sums their bytes and decides between `none` and `rebuild`
over that same snapshot. Only a rebuild then fills the visited ids
(`gc_visited`): the mark keeps one bit an object over the Store's locator
order, never the identities, so the fill reads the locators again a block
at a time and sends back the reached ones, 8,192 a statement, in ascending
identity order. A locator's rank is its position in the snapshot the walk
read, which is why the fill stays in that transaction: a publication that
lands while the mark runs is in neither the walk nor the fill, and moves the
Store tuple that Phase B checks. A `none` round then writes the `gc_op` row
at phase `mark`, empties `gc_dead` and clears the op, as the specification
asks, without ever sending the ids or computing a dead set it would delete
in the same call; a rebuild fills `gc_dead` with its op row
([`compact/run.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/compact/run.rs)).
Phase A's batches of about 1 GiB of pack bodies are cut by the server, which
returns the first and last pack id and the pack count of each: the client
holds one row a batch and no list of live packs
([`compact/build.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/compact/build.rs)).

The mark's walk holds the same bounded memory as on SQLite
([SQLite: the mark's memory](sqlite.md#the-marks-memory)). Its locator
blocks are read in the order of the `object_location_prefix` index: the
signed eight-byte prefix, then the identity, 512 identities a block and up
to sixteen blocks (8,192 identities) a statement, each after the last one of
the statement before. The table has no unique key on
the identity, so a block takes one row per identity (`DISTINCT ON`, the
least locator); a rank is a position in that order. The mark's sort files
are unnamed files in the client's temporary directory, and so are the
block boundaries and the bits of the locator order beyond the pages the
mark holds of them (32 bytes for every 512 objects and two bits an object,
on disk). The compaction asks the mark for no list: the retained pack ids
are read as a stream from the mark's bounded list
([SQLite: the mark's memory](sqlite.md#the-marks-memory)) and counted from
it, and the swap reads the live heads for its digest with the mark's own
statements, 1,024 rows at a time
([`compact/op.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/compact/op.rs),
[`compact/swap.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/compact/swap.rs)).
So nothing the client holds through a compaction grows with the Store's
objects, packs, Commits or Branches. What it does hold per unit is small and
not of those: one row for each batch of about 1 GiB of kept pack bodies, and
one identity for each project that has a dead row while Phase C deletes them
a project at a time.

One method decides where the sort files go
([`seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs),
`Mechanics::scratch_directory`): the client's temporary directory
(`std::env::temp_dir()`) for a PostgreSQL Store, which has no directory of
its own. Where that directory is a memory file system (`/tmp` on many Linux
hosts) a sort file is memory again, so the bound on the mark's memory holds
there only when `TMPDIR` names a directory on a disk. A name a killed mark
left in that directory is removed by the next mark of any Store that sorts
through files there. The history closure is one recursive statement here: its
working tables are the server's memory, which `work_mem` governs and spills
to the server's own temporary files, not the client's. The client reads
`kept_node` back a page of 1,024 rows at a time and holds no list of it in
a compaction's mark.

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
- **`Bare` statements** are sent as written: each GC admission statement
  that needs the check reads `history_meta` itself and refuses before it
  takes a key or deletes a row ([GC admission](#gc-admission)).
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
   SELECT 1 FROM store_lane ... FOR UPDATE;      (skipped if nested: the
     lane barrier                                 barrier key is held
                                                  shared from BEGIN)
   one statement: INSERT locations ... RETURNING, totals, inode value
     blocks, released ordinals (past the handle's kept 128, #768), signatures
   COMMIT; nested: the lane check rides in it    lost -> ROLLBACK, Closed
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

### Lane barrier

A nested write commits on the session connection while the row lock lives
in the lane connection's transaction. That connection can die mid-lane (an
`idle_in_transaction_session_timeout`, a keepalive, a failover or an
administrator), freeing the row while a nested write is still running, and
the head check and the `object_location` existence probe rely on the row
(finding #10). Two rules keep them exact without a unique key or a second
database ([`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs)):

```text
 every writer, right after it locks store_lane (same round trip, same
 lock_timeout):  pg_advisory_lock(class, key); pg_advisory_unlock(class, key)
 every nested Write/Publish: BEGIN ...; pg_advisory_xact_lock_shared(class, key)
   body
   lane check and COMMIT, one simple-query round trip on the writer (#768):
     SELECT 1/(pg_xact_status('<lane xid8>') IS NOT DISTINCT FROM
               'in progress')::int; COMMIT
     22012 (not in progress, or NULL) or 22023 (an id this server never
     issued) -> the server skipped the COMMIT; close the lane connection;
     ROLLBACK; Closed.  No lane id recorded -> ROLLBACK; Closed, nothing sent
```

The lane transaction holds `store_lane` until it ends, and its id stays
`in progress` exactly until then: the server takes a transaction out of the
running set before it releases its locks, so a check that sees it in
progress saw the row locked. A failing statement in a simple query skips the
rest of the string, so the `COMMIT` after a failed check never runs and the
transaction is left aborted for the `ROLLBACK`. Until #768 the check was its
own round trip on the lane connection
(`pg_current_xact_id_if_assigned()` against the recorded id). Two effects
follow: the lane connection now receives no statement between its `BEGIN`
and its `ROLLBACK`, so an `idle_in_transaction_session_timeout` shorter than
a whole lane hold ends it (a definite `Closed`, never a write without the
row); and the Store role needs `EXECUTE` on `pg_xact_status`, which
PostgreSQL grants to `PUBLIC`. Like the check it replaces, it assumes the
writer and the lane connection reach the same server.

The barrier key is a two-key advisory lock (a fixed class and a hash of the
schema name), a key space apart from the GC keys. A new lane holder takes it
exclusively and lets it go at once, so it cannot read anything until every
nested write of the previous holder that started before the row was freed
has ended; a nested write that starts after that point fails its lane
check, rolls back and answers `Closed` (`OwnershipUnavailable` in history
terms): nothing of it was kept. Its own holder never takes the key
exclusively while its lane is held, so its nested writes never wait for it.

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

### GC admission

A mount registers a shared `gc_intent` row and holds advisory keys on the
writer session for the Workspace's life; its unmount releases both (GC spec
§5.2). Since #768 both run outside the publication lane, and the keys are
taken and given back in one statement each
([`admission.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission.rs) `register`,
[`authority.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission/authority.rs) `release`,
[`session.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission/session.rs)):

```text
 register (mount)                        round trips
   gc_lock_session: every key in one     1   Store, project, Branch; all tried,
     statement (Bare), after its own         none when the generation moved
     generation check
   Admit: BEGIN RC; SET LOCAL ...;       1   no store_lane row, no barrier
     lock_timeout; fence
   Branch probe, conflict scan,          3   register_in, shared with SQLite
     INSERT ... RETURNING
   COMMIT                                1
   keys kept with the op in the session (held_ops)

 release (the owner's)
   gc_intent_release_owner: DELETE ...   1   one fenced autocommit statement;
     RETURNING (Bare)                        refused, deleting nothing, when
                                             the generation moved
   gc_unlock_session: the op's kept      1   only when this session holds
     keys, one statement (Bare)              the op; always after the delete
```

`postgresql_gc_sessions` pins the counts of one warm mount register and
release: register 6 round trips (4 counted statements plus the `BEGIN` batch
and `COMMIT`), release 2 (2 statements, no transaction). Before #768 each was
8 round trips: register took one fenced lock statement per key, then a lane
write; release was a lane write that read the row to rebuild its keys (and
the Branch's project, on every unmount, because a decoded row carries no
project id), then one unlock statement per key. SQLite's counts are
unchanged (register 5 statements, release 3, one `BEGIN IMMEDIATE` write
each).

Why the mount's register and release need no lane:

- Publications never read `gc_intent`. The lane barrier (finding #10) guards
  only the publication head check and the `object_location` existence
  probe, so a write that touches only `gc_intent` neither needs it nor can
  break it.
- Exclusion is the advisory keys. Every registration a mount must see takes
  a key that conflicts with the mount's before it scans: a compaction the
  exclusive Store key, `branch.remove` the exclusive Branch key,
  `project.delete` and `remove_file` the exclusive project key. The mount
  takes its keys before its write begins, so under READ COMMITTED its scan
  sees every such row that committed, and a conflicting registration that
  starts later fails its key. Mounts never refuse each other.
- An Init or merge takes only shared keys and refuses another Init or merge
  by its row alone; off the lane two of them on different daemons could both
  register. They stay on the lane, and so do the rare compaction and removal
  registrations. A mount racing an Init can miss the Init's uncommitted row;
  both then register, which is the outcome of the serial order mount, then
  Init (an Init's scan does not look at mounts).
- The generation fence stays. The `Admit` transaction checks it in its
  `BEGIN` batch and holds `history_meta`'s `AccessShareLock` until `COMMIT`,
  so a swap cannot start under it. The lock statement reads the generation
  in its own snapshot and tries no key when it moved; the release's `DELETE`
  deletes nothing when it moved. A session that holds a registered mount
  still meets the fence on every later call (`a_registered_mount_still_meets_the_generation_fence`).
- `lock_timeout` stays on the `Admit` transaction. The release's single
  autocommit statement runs without it: it can wait only for its own op's
  row (held by another short admission transaction) or for a swap's brief
  `ACCESS EXCLUSIVE` on `history_meta`.
- An operator's `release_done` stays on the lane: its done stamp relies on
  retaining releases being serialized. The owner's delete never writes or
  deletes a `done` row.

Keys and their custody:

- The session keeps each registered op's keys (`held_ops`), so a release
  needs no read. Whether to unlock is decided by the session's own held ops,
  never by the rows the delete returns: an operator-retained `done` row
  returns nothing but its keys are still unlocked, and a row a reconcile or
  another session already deleted no longer leaks them. A failed delete
  unlocks nothing and keeps the op held. A second registration of a held op
  gives back the earlier keys, so the op keeps one level of each.
- The lock statement tries every key even after one fails and returns which
  were granted, so a refusal gives back exactly the granted ones (session
  locks survive a rollback); the refusal names the first refused key in
  Store, project, Branch order as before. A stale session takes none. A lock
  statement that fails, or whose reply cannot be read, leaves the held keys
  unknown: it is `Unknown`, which quarantines the session and closes its
  connection, so the server gives back every key.
  The same holds for the owner release's unlock statement. Its delete has
  committed, so the release still answers success (the unlock's `Unknown`
  is not returned) and the session's next call is refused `UnknownOutcome`.
  Every other mount of that session loses its keys with the connection,
  but its row stays, so a compaction or removal on another session is still
  refused `MountsActive` by the intent-row scan. `postgresql_gc_sessions`
  forces both a failed unlock statement and a lost reply
  (`a_failed_unlock_…`, `a_lost_unlock_reply_…`) and checks this.
- §5.2 item 34 is kept: the lock and unlock statements return
  `pg_backend_pid()`, compared with the session's recorded backend, and a
  transaction-pooled connection is `Unsupported`.

**Not done, and why.** #768 proposed two more cuts that the source does
not support:

- *Skipping the fence on a session that holds the shared Store key.* The
  fence costs no round trip: inside a transaction it is one more statement
  in a `BEGIN` batch that is sent anyway, and an autocommit read carries it
  as a joined column. The key cannot replace it either. A backend never
  conflicts with its own advisory locks, so on the same backend the key
  does not exclude a compaction (the intent-row scan does). Behind a
  transaction pooler the client cannot know which backend holds the key,
  and checking would cost what the fence costs. The fence's
  `AccessShareLock` is also what makes a swap that starts during a write
  fail its `LOCK … NOWAIT`. Exec reads run on Workspace readers, which hold
  no keys, so the rule could never apply to them. The fence stays on every
  call.
- *Dropping a per-mount schema probe.* None runs per mount. `schema::check`
  runs once when a connection opens; the 5-statement read in the measured
  mount was the path-removal marker read on the control reader, which
  [Mount reads](#mount-reads) now replaces. Skipping the open check would
  leave a reader at generation 0, which is unfenced.

The register body (Branch probe, conflict scan and insert) could become one
PostgreSQL-only statement and save 2 more round trips, but it would duplicate
the conflict scan for three scope kinds beside `register_in`, which SQLite's
mount and compaction mark share; it is left as is.

**Waits that remain.** Every Workspace a daemon mounts shares that daemon's
one writer session, whose in-process FIFO admission
([`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs))
a Commit's `hold_lane` keeps for its whole lane hold
([`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/lane/lane.rs)).
So a mount or unmount still waits behind another Workspace's Commit in the
same daemon; only another session's lane (another daemon or process) no
longer delays it. Removing that wait would need a separate admission
connection, a design decision #768 does not take. On SQLite, register and
release still run as `BEGIN IMMEDIATE` writes and wait behind a held lane:
the directory lock is a SQLite writer's only wait (`busy_timeout=0`), and
the same writer lock is the compaction fence
([SQLite: Lane](sqlite.md#lane)).

### Mount reads

A Workspace mount reads only on its own Workspace reader and writes only
its admission on the writer; it never opens or uses the control reader
(#768, [`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs)):

```text
 Workspace reader  branch_snapshot or branch_snapshot_named     1 autocommit statement
                   (a bind by Branch id takes the project name for the
                   intent from the same row)
 writer            gc_register: the mount intent                 admission (history/admission.rs)
 Workspace reader  paths_node_markers: the bound node's serial   1 autocommit statement
                   and every path_removed marker of its project
```

The markers are read after the intent commits, never with the snapshot. A
path removal takes the project key exclusively in its marker transaction and
scans the project's live intents, while the mount holds that key shared from
before its registration (on SQLite the single writer orders the two
transactions instead): a removal that committed before the intent is seen
by the marker read, and one after it is refused `MountsActive`. A marker read
before the registration would miss a removal that commits in between. Both
reads are fenced autocommit statements (`read_statement`).

The mount refuses when `gc_register` refuses, `Unsupported` included: on a
transaction-pooled connection the session key cannot mean presence, so the
mount is `Unsupported("transaction-pooled PostgreSQL connection")` (GC spec
§5.2, #776 C1). Only a catalog with no GC admission at all, whose refusal is
the trait default's (`HistoryError::no_admission`), mounts without an
intent; merge, Init and the removals follow the same rule
([Session](#session)).

| Test | Command | Checks |
| --- | --- | --- |
| [`mount_round_trips.rs`](../../../../engine/crates/layerfs-store-service/tests/mount_round_trips.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-store-service --test mount_round_trips` | Pinned connects, round trips, statements and transactions of a first and a repeated mount by Branch id and a mount by name, per session, on SQLite and PostgreSQL; the control reader is untouched. Also a mounted Workspace's first and second one-file Commit, an UpToDate Commit and the unmount: no Commit reads on the control or Workspace reader (whiteout facts kept from the mount, validation from the built objects). A merge Commit reads the merged root back through the Workspace reader, and the Commit after it reads nothing there |
| [`admission.rs`](../../../../engine/crates/layerfs-store-service/tests/admission.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test admission` | `a_pooled_registration_refuses_the_mount`: a pooled refusal reaches the caller of a mount; a catalog without admission still mounts. `a_pooled_connection_refuses_a_shared_intent`: the same for a merge |

### Commit round trips

A Workspace Commit ([`store/commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs))
does everything that grows with the change before the lane: the capture,
the Save and its publication, the logical count, the binding hints and the
validation. In the lane it checks the head and publishes. Measured with
`log_statement=all` on a one-file change (`mount_round_trips.rs`, warm
statements), the second Commit of a mounted Workspace cost 46 round trips
before #768's Commit cuts, 36 after them and 24 after the stage-1 cuts
(column `stage 1`, notes (f) to (j); derived from the cuts
`mount_round_trips.rs` pins per session, not re-measured with
`log_statement`):

```text
 before the lane                                         before after stage 1
   pack reservation (Write: BEGIN+lane row, UPDATE, COMMIT)  3   3   0  (i)
   file digests, every row below the floor (empty Write)     2   0   0  (a)
   base pack and inode values (construct)                    2   2   0  (j)
   whiteout read (control reader)                            1   1   0  (h)
   head: branch_snapshot                                     1   1   1
   logical.known: the head Commit                            1   1   0  (g)
   finish: locate                                            1   1   1
   finish: the last wave's publication (values only)         4   0   0  (b)
   finish: sealed tail (BEGIN, COPY 2, lane, rows, COMMIT)   6   6   6
   logical.count reads                                       2   2   2
   binding hints (Write: BEGIN+lane row, INSERT, COMMIT)     3   3   3
   validate (Workspace reader)                               5   5   0  (f)
 in the lane
   hold_lane BEGIN batch                                     1   1   1  (c)
   head.lane: branch_snapshot                                1   0   0  (c)
   commit_branch BEGIN batch                                 1   1   1
   Branch probe, epoch read, Branch re-read                  3   1   1  (d)
   serial, Commit insert, parent line, lineage, advance      5   5   5
   epoch FOR SHARE                                           1   1   1
   lane check (lane connection), COMMIT                      2   1   1  (e)
   lane ROLLBACK                                             1   1   1
 total                                                      46  36  24
```

- (a) `record_file_digests` returns before opening a transaction when every
  row is below the digest floor; it wrote nothing and took the lane row.
- (b) `Save::finish` prepares its last wave without publishing it
  (`State::prepare`), so one publication carries that wave's values, the
  sealed packs and the signatures, still split at the transaction row and
  byte limits. A batch holding only values now splits before it overflows,
  and the ordinal release counts as a row.
- (c) `HistoryCatalog::publish_lane_unmoved`: the lane's `BEGIN` batch ends
  with an `EXISTS` over the Branch, with the `branch_snapshot` retirement
  filters, at the head and base the Commit read before the lane. It runs
  after the row lock and the barrier, so it sees what the read it replaces
  would. `true` skips the in-lane read; `false` (moved, absent, SQLite)
  reads as before. It is not generation-fenced: `commit_branch`'s own
  transaction is, and refuses a moved head itself, so a wrong `true` could
  only turn an in-lane merge into `HeadMoved`, never a wrong publication.
  The lane transaction now holds `AccessShareLock` on the history tables it
  read until it ends; nothing runs DDL on them while a Store is open.
- (d) `commit_snapshot_epoch`: the Branch snapshot row plus the project's
  `project_epoch.epoch`, one statement and one snapshot (SQLite too). A
  removal marker bumps the epoch and writes its row in one transaction, so it
  is either wholly visible (the row is filtered: `Missing`) or wholly not
  (the `FOR SHARE` re-check sees the new epoch: `Conflict`). The probe and
  re-read existed only to close the gap between two snapshots.
- (e) [Lane barrier](#lane-barrier).
- (f) `validate_candidate` checks the candidate from the objects its
  construction built (`ConstructedObjects`, through `captured.rs` `Built` and
  a private cache allowance), not read back through the Workspace reader
  (owner decision, #768). An object construction did not build or did not
  keep (a merge's or whiteout step's, an inherited base object, one past the
  staging bound) is read through the ordinary Workspace client as before.
  Same checks: decode, `checked_base`, root inode identity,
  `prepare_base_install`; the prepared install gets the ordinary client
  back, and the built bytes reach the shared cache only after publication.
- (g) `logical.known`: the Workspace keeps one node's recorded logical size,
  the one it last read or published (`BoundWorkspace::known_logical`); a
  Commit over its own last Commit reads nothing. The bind snapshot carries
  no size, so the first Commit after a mount still reads it.
- (h) The whiteout step reuses the bound node's serial and markers that the
  mount read (`KnownRemovals`), and a Commit this Workspace published hides
  nothing: its serial is above every marker's frontier, and while the mount
  admission is registered every marker write refuses `MountsActive`. Any
  other node, or a catalog with no admission, is read as before.
- (i) The Storage handle a Workspace keeps between Commits keeps the unused
  pack ids and the first 128 unused ordinals (`KEPT_ORDINALS`) of its last
  finished Save (`save/reservation.rs`, `IdBlock`); the next Save reserves
  only when a range runs out, so a steady Commit of a small change reserves
  nothing. Finish still releases the ordinal tail past the kept 128 in its
  last publication, as it released the whole tail before. The first Commit
  of a new handle still reserves. A dropped handle loses at most 4,096 pack
  ids and 128 ordinals (ids are never reused; gaps are allowed), and a Save
  that fails or does not finish drops the block.
- (j) A Save reads the metadata pack bodies and inode values earlier Saves
  of its handle published (bounded: 2 MiB of bodies, 512 KiB of values,
  cleared when a Save does not finish), and the pool reader's chain walk
  fetches a chain's groups in one group read and locates its deeper bases
  with its first base (`encoding/pool/read.rs`, `Fetch::locate_ahead`).
  Before, construct read one pack per chain link, so its count grew with
  the base leaves' pooled delta chains (depth up to 8): a diagnostic
  harness of 20 mount, one-file Commit, UpToDate and unmount cycles on one
  Workspace measured 36 37 39 40 … 47 41 round trips per Commit; with this
  cut alone (before (f) to (h)) it measured 31 to 33 throughout.
  `mount_round_trips.rs` pins the second Commit's writer at 25 round trips
  (30 before).

Checked and not done:

- *Reservation off the lane.* Allocation serializes on the `store_policy`
  row, and a compaction's swap locks that table `NOWAIT`, so it needs no lane
  row; but a bare autocommit statement would lose `lock_timeout` and the
  Store's `synchronous_commit`. As one simple-query batch (`BEGIN`, settings,
  a fenced `UPDATE … RETURNING`, `COMMIT`) it would save 2 round trips; left
  for a separate change.
- *Binding hints off the lane.* A hint block's `seq` is `max(seq) + 1` in its
  group under `PRIMARY KEY (grp, seq)`: two Commits naming new entries under
  the same directory would collide without the lane. It stays a lane write.
- *Merging the barrier lock and unlock.* They are already in one batch: no
  round trip to save.
- *The serial with the Commit insert, the parent line with the lineage
  insert, the `FOR SHARE` re-check with `COMMIT`.* Each needs a
  PostgreSQL-only statement form or a server-side assert with its own error
  mapping; about 1 round trip each, not done.

| Test | Command | Checks |
| --- | --- | --- |
| [`postgresql_commit_round_trips.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_commit_round_trips.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql_commit_round_trips` | The lane with a head read and `commit_branch` is 12 round trips (15 before), with the head check in the lane's `BEGIN` 11; a moved head and an absent Branch are never answered unmoved |
| [`mount_round_trips.rs`](../../../../engine/crates/layerfs-store-service/tests/mount_round_trips.rs) | see [Mount reads](#mount-reads) | A real Commit's writer: first 54 round trips (65 before), second 32 (42); SQLite 46/9 and 38/7 statements/transactions (52/11, 44/9) |
| [`postgresql_lane_liveness.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_lane_liveness.rs) | `… --test postgresql_lane_liveness` | A nested write after its lane connection was terminated fails `Closed` and writes nothing: now through `pg_xact_status` |

## Delta chains

\#758. A read of a delta record used to locate each base level by level,
one round trip per level. A PostgreSQL Store records each row's chain when
it publishes the row, so a cold read locates a whole chain in one wave and
reads the groups it needs in one ranged statement per 2 MiB. A chain a read
does not bind to its members (program pages, pooled leaves) costs one more
wave for all its levels: core locates them together from the base ids the
cached records at the members' positions name, each located as an ordinary
authoritative row ([physical storage](../../core/physical-storage/README.md)).

| Piece | What it does | Source |
| --- | --- | --- |
| `object_location.ancestors` | The record's chain, nearest base first, ending at a FULL record: per base `varint(zigzag(pack - previous)) \|\| u8 group \|\| varint(record) \|\| varint(canonical_length - 1)`, `previous` starting at the row's own pack, so a child's list is its base's entry followed by the base's list verbatim. A FULL row stores `''`; `NULL` means unrecorded (never read as a chain). At most 850 bytes (50 entries of at most 17) | [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| `ancestor_varint`, `ancestor_entry`, `ancestor_entries` | SQL functions: the first two build an entry (inlinable SQL, not `STRICT`, operands passed as columns of `OFFSET 0` subqueries so they stay inlined); `ancestor_entries` decodes a list, answering one row with a NULL pack for a malformed list. They serve publication, compaction verification and the doctor; the read path decodes lists in Rust | same |
| Publication | The rows statement ([`publish_rows.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/publish_rows.sql)) derives each new row's list from `ObjectLocation.base`: a base stored earlier gives its stored list (one more prefix probe); a base in the same batch is resolved by a recursive CTE over the batch, children after their base, at most 64 levels. A row whose base is stored nowhere, or whose list would pass 850 bytes, is left `NULL`. A lost row keeps the winner's list. The pack COPY writes `directory` with the body; rows and packs are only inserted, never updated | [`objects/publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish.rs) |
| `locate_chains` | Two autocommit statements. Statement 1 ([`chain_roots.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/chain_roots.sql), `$1` ids `bytea[]`, `$2` prefix keys `bigint[]`) probes each root through `object_location_prefix` as `locate` does and returns the rows with their raw lists, plus the roots' own pack rows with directories by `pack_pkey`. The lists are decoded in Rust ([`metadata/chains.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/metadata/chains.rs)): `NULL`, `''`, malformed lists and lists deeper than the role's depth cap are dropped (the root still locates). Statement 2 ([`chain_packs.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/chain_packs.sql), `$1` member pack ids `bigint[]`) reads, by `pack_pkey`, only the member packs statement 1 did not return; it is skipped when there are none. The session keeps the pack rows both statements returned (`ChainPacks`: rows of immutable packs, per catalog incarnation, within 2 MiB of directories, cleared whole at the bound), so statement 2 reads only member packs the session has not seen; every pack a retained row's list names stays stored. A chain whose member pack is absent is dropped | [`storage/provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/storage/provider.rs) |
| Group reads | `read_groups` answers every read in one statement per 2 MiB of returned bytes through the multi-range `substring` reader (`objects::read::ranges`; ranges under 8 KiB apart are fetched as one span and cut in the statement): a ranged read's ranges, and a whole read's body as one range, authenticated against the descriptor it named; answers in request order, an absent pack `Missing`. `batched_chain_reads` is true, so core pages every demand (reads and Save slices) into such calls, with a handle's own published chains in the same pages | same |
| Locate | the plain locate statement is [`locate.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/locate.sql); the bench's `b8_locate.sql` is the same text | [`metadata/reads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs) |
| Compaction | The rebuild copies `ancestors` and `directory` with their rows, keeping pack ids and positions, so every list stays valid. Before the swap, `verify_ancestors` counts scratch rows whose list is malformed or names a pack or group the scratch Store lacks, and a non-zero count fails the round with `Integrity("compaction ancestor lists")`. A future repack that moves records must rewrite the lists. The table fingerprint includes TOAST reloptions | [`compact/build.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/compact/build.rs), [`compact/op.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/compact/op.rs) |
| Freezing | `pack`, its TOAST relation and `object_location` are insert-only, so they set `autovacuum_freeze_min_age = 0` and `autovacuum_vacuum_insert_scale_factor = 0.05`: inserted pages freeze early instead of in one wraparound pass | [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| Doctor | `store.catalog.chain_lists` (unrecorded lists), and in a full audit `store.catalog.chain_entries` (malformed lists or entries naming no stored pack or group) and `store.catalog.chain_hints` (a first entry naming no matching row, or a list whose rest is not that row's list) | [`diagnostic/catalog.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/catalog.rs), [`diagnostic/scan.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/scan.rs) |

| Test | Command | Checks |
| --- | --- | --- |
| [`postgresql_ancestor_lists.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_ancestor_lists.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql_ancestor_lists` | Lists equal the record-header chain in and out of a batch, for a lost base and under eight concurrent writers; a cold read of a chain costs one wave and three statements on a fresh session, two on a session that holds its member packs' rows; a corrupted entry is a counted stale hint, an error under strict hints; a schema-2 Store is refused |
| [`postgresql_group_directory.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_group_directory.rs) | same, `--test postgresql_group_directory` | `directory` is the body prefix, the CHECK refuses anything else, and the chain locate returns the root and member packs' directories; a demand past 2 MiB reads its groups in ranged pages within the amended A8 (≤ 3 round trips per wave with chains + 1 per 2 MiB) |
| [`postgresql_chain_property.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_chain_property.rs) | same, `--test postgresql_chain_property` | 40 random histories past the depth caps: exact lists, strict reads equal the default port's with no stale hint, one wave per cold file read |
| [`postgresql_history_scaling.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_history_scaling.rs) | same, `--test postgresql_history_scaling` runs stage 1; stages 2–5 by name in the order the file states | The bench workload grown to 800 versions in five staged processes over a kept schema, probed at 50, 200 and 800: equal counted read, chain locate and fresh-session Save costs, bytes per read, catalog bytes per Commit and list bytes per delta row; no row updated; locate, chain and publish statements indexed with no `Seq Scan` |
| [`postgresql_compact_fixtures.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_compact_fixtures.rs) | same, `--test postgresql_compact_fixtures` | A live chain keeps its lists across two compactions and reads strictly with no stale hint |

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
- [Control: Stores and projects](../../control/stores-and-projects/sqlite-and-postgresql.md): how a home connects, updates and checks a PostgreSQL Store.
