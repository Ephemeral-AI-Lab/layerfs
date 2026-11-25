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
| Schema version | `SCHEMA_VERSION = 4` (version 3's ancestor lists and pack directories, #758, plus each Commit's attempt, `commit.attempt` with the partial unique index `commit_attempt`, #776 R1); 27 tables, or 30 with acquisition; version 1 is refused (`Integrity`); versions 2 and 3 are refused as `Refused`, each by its own name ("PostgreSQL Store schema version 2 …: not supported by this release and not migrated; create a new Store", "… 3 …: not opened by this release; upgrade it in place with `layerfs store upgrade <store>`") by the Full, Light and diagnostic checks before any source comparison; an open converts nothing (only `store upgrade` converts version 3, see [Format upgrade](#format-upgrade)), and runs no in-place migration first, so a version 2 Store that predates the GC, `kept_node` or position additions gets the same named refusal (#776); the checks read `store_policy.trace_salt` through the row's JSON form, so a Store without that column still reaches the version check | [`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs) |
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
| A body that ignored a statement's refusal and returned success (#768) | The transaction ends with `SELECT 1; COMMIT`: in an aborted transaction the `SELECT` answers `25P02` and the `COMMIT` never runs, so the session sends `ROLLBACK` and answers the first refusal it saw in that transaction (else `Integrity`), definite and counted as a rollback. That holds when the body sent nothing after the refused statement: a statement it sends after it answers `25P02`, which has no definite rule, so the transaction is `Unknown` and the session quarantined. A bare `COMMIT` there would be answered with the tag `ROLLBACK` and no error, which reads as success. A failed `ROLLBACK` quarantines: `Unknown` |
| The guard `SELECT` itself refused with another definite error (a `statement_timeout`, a cancel) | The server skipped the `COMMIT` and left the transaction aborted: the session sends `ROLLBACK` and answers that error, definite. After a refused `COMMIT` (a deferred constraint) the same `ROLLBACK` finds no transaction and only warns. A failed `ROLLBACK` quarantines. Not covered by a test: neither error can be made to hit the instant `SELECT 1` deterministically | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs) |
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
encoded (a value that does not convert, or an `io::Error` while encoding: a
NUL byte in a text, a message past the protocol's length) stops the flight
before it: it and every later request never went out and are refused
definitely (`Integrity`, `sent` false), and when earlier requests went out,
one `ROLLBACK` follows. Only a sent request's lost reply quarantines the
session (an unsent one lost nothing); a reply the server gave is the
caller's to classify. A prepare flight follows the same rule. The fused Workspace Commit uses it
([Fused Commit](#fused-commit), #768 stage 2), sending every request of its
one transaction in one flight; `postgresql_pipeline.rs` proves that a flight is one network wait by
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
| Fused | as `Write`, sent in one pipelined message with the generation fence and a per-attempt token | `FOR UPDATE` and the lane barrier in the transaction itself, never nested in a held lane | a Workspace Commit over its bound head ([Fused Commit](#fused-commit)) |

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
The swap takes the publication lane (the `store_lane` row and the lane
barrier, under `lock_timeout`) before any base-table lock, and reads the
full mark tuple (with the history-row `COUNT` and `MAX(stamp)`), that
digest and the history `SUM(logical)` while it holds the lane alone:
readers are not blocked by these history- and Branch-sized reads, writers
wait for the lane as they do for any holder, and a lane wait that reaches
`lock_timeout` is `Busy` naming the lane holder. Only then does it lock
every base table except `store_lane` (`ACCESS EXCLUSIVE NOWAIT`; the
writers queued behind the lane row hold `RowShare` on it), re-read the
one-row tables, and swap by renames: the old `pack` and `object_location`
and their indexes become `gc_retired_*` and move into the scratch schema
the new tables left, which Phase C drops first, so no payload file is
unlinked under the lock (#776 P5).
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
  `G + 1` makes the transaction `StaleStore`. The REPEATABLE READ ones
  (`Read`, `Snapshot`) first take `LOCK TABLE <schema>.history_meta IN
  ACCESS SHARE MODE` in the same batch. PostgreSQL fixes a REPEATABLE READ
  snapshot when the first statement that needs one is analysed, before that
  statement waits for its `AccessShareLock`; a read that queued behind a
  swap's ACCESS EXCLUSIVE lock would otherwise pass the fence at `G` from a
  snapshot older than the swap's `G + 1` update and then resolve the
  installed tables, so a compacted-away object would read as absent instead
  of `StaleStore`. `LOCK TABLE` takes no snapshot and is allowed in a READ
  ONLY transaction, so the fence read's snapshot follows the swap (#776).
  The READ COMMITTED ones (`Write`, `Stage`, `Publish`) take a new snapshot
  per statement and need no lock first.
- **Autocommit reads** (`Statement`, `Immutable`) join
  `catalog_incarnation` as the trailing column of the read's one statement,
  under a schema-qualified `history_meta` one-row subquery; the trailing
  column is checked and removed before the row reaches the decoder. A read
  that finds no row needs no generation to answer: the swap retains exactly
  the live set (`live_heads_digest` is unchanged), so an absence at `G` is an
  absence at `G + 1`. Its snapshot is taken for its one statement; this
  relies on the server's default isolation being READ COMMITTED, as it is
  unless the database or role sets `default_transaction_isolation` higher
  (an autocommit statement under REPEATABLE READ would have the same
  snapshot-before-lock window).
- **`Bare` statements** are sent as written: each GC admission statement
  that needs the check reads `history_meta` itself and refuses before it
  takes a key or deletes a row ([GC admission](#gc-admission)).
- A session whose open has not set a generation yet (the open check's own
  `Statement` reads) is unfenced, and the Store diagnostic reads through
  `run_observed`: it must be able to read a Store whose authority changed and
  report that change as the `store.identity` failure, not refuse with
  `StaleStore`.

A mismatch is `StaleStore`: nothing was written and the caller reopens the
Store. A transaction whose check reads no generation (no `history_meta` row,
or a value that does not parse) is `Integrity`, and it is rolled back first
like a mismatch: a `Write` holds the lane row from its `BEGIN`, so leaving it
open on a pooled connection would hold every later writer (#776 P7). A
rollback that fails quarantines the connection and answers `Unknown`.

```text
 PostgresStorage::publish(batch)                 one Publish transaction
   validate(batch)                               shared bounds, no SQL
   BEGIN; SET LOCAL ...                          ids already reserved:
   COPY BINARY pack bodies                       no lane needed yet
   SELECT 1 FROM store_lane ... FOR UPDATE;      (skipped if nested: the
     lane barrier                                 barrier key is held
                                                  shared from BEGIN)
   one statement: INSERT locations ... RETURNING, totals, inode value
     blocks, released ordinals (past a keeping handle's kept 128, #768),
     signatures
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
 lock_timeout):  one statement:
   SELECT pg_advisory_unlock(class, key)
     FROM (SELECT pg_advisory_lock(class, key) OFFSET 0) AS barrier
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

The take and the release are one statement, so an error cannot fall between
them. A session-level advisory lock survives `ROLLBACK`, and before #776 P6
they were two statements: a failure after the first left the key held
exclusively on a pooled connection, and every later nested write's shared
key waited for it. A statement cancelled after its lock and before its
release can still leave it, so the `ROLLBACK` of every non-nested `Write` or
`Publish`, and of a failed `hold_lane`, also releases a granted exclusive
barrier of its own backend (no barrier is held past its statement, so such a
lock is always a leftover). A [fused Commit](#fused-commit) runs the same
release on its message's failure path only. `fused_commit.rs`
(`postgresql_cancelled_barrier_is_released`) injects that cancellation into
a fused message through a rewriting proxy; for the other transactions the
suites run the rollback statement on every definite failure.

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
before #768's Commit cuts, 36 after them, 24 after the stage-1 cuts
(column `stage 1`, notes (f) to (j); derived from the cuts
`mount_round_trips.rs` pins per session, not re-measured with
`log_statement`) and 2 after stage 2 (column `s2`, notes (k) to (m);
measured with `log_statement=all` on the private test server, and pinned):

```text
 before the lane                                         before after stage 1  s2
   pack reservation (Write: BEGIN+lane row, UPDATE, COMMIT)  3   3   0   0  (i)
   file digests, every row below the floor (empty Write)     2   0   0   0  (a)
   base pack and inode values (construct)                    2   2   0   0  (j)
   whiteout read (control reader)                            1   1   0   0  (h)
   head: branch_snapshot                                     1   1   1   0  (k)
   logical.known: the head Commit                            1   1   0   0  (g)
   finish: locate                                            1   1   1   1  (m)
   finish: the last wave's publication (values only)         4   0   0   0  (b)
   finish: sealed tail (BEGIN, COPY 2, lane, rows, COMMIT)   6   6   6   0  (k)
   logical.count reads                                       2   2   2   0  (l)
   binding hints (Write: BEGIN+lane row, INSERT, COMMIT)     3   3   3   0  (k)
   validate (Workspace reader)                               5   5   0   0  (f)
 in the lane
   hold_lane BEGIN batch                                     1   1   1   0  (k)
   head.lane: branch_snapshot                                1   0   0   0  (c)
   commit_branch BEGIN batch                                 1   1   1   0  (k)
   Branch probe, epoch read, Branch re-read                  3   1   1   0  (d)
   serial, Commit insert, parent line, lineage, advance      5   5   5   0  (k)
   epoch FOR SHARE                                           1   1   1   0  (k)
   lane check (lane connection), COMMIT                      2   1   1   0  (e)
   lane ROLLBACK                                             1   1   1   0  (k)
 the fused message, BEGIN to COMMIT                          -   -   -   1  (k)
 total                                                      46  36  24   2
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
  other node, or a catalog with no admission, is read as before. The
  markers are kept only while they charge at most `KEPT_REMOVAL_BYTES`
  (256 KiB: each marker's path bytes plus its 40-byte record, about 2,600
  markers of 60-byte paths); a bound node with more keeps nothing, and each
  Commit reads them again on the control reader, as before (1 statement on
  PostgreSQL, 3 on SQLite, measured by `kept_removals_bound.rs`).
- (i) The Storage handle a Workspace keeps between Commits keeps the unused
  pack ids and the first 128 unused ordinals (`KEPT_ORDINALS`) of its last
  finished Save (`save/reservation.rs`, `IdBlock`; such a handle is made
  with `Storage::keeping_ordinals`); the next Save reserves only when a
  range runs out, so a steady Commit of a small change reserves nothing.
  Finish still releases the ordinal tail past the kept 128 in its last
  publication, as it released the whole tail before; a handle that does not
  keep ordinals (Project Init, a Branch merge) releases every unused one
  there, as before. The first Commit of a new handle still reserves. Before
  every orderly drop the kept handle returns its kept ordinals
  (`Storage::release_kept_ordinals`: one publication of the same
  conditional rewind, one attempt, never repeated): a Workspace close that
  finds the Store's idle slot taken (the handle whose kept ordinals end
  later is released, the other takes the slot) or whose overlay close
  fails, a Store
  close (`StoreService::close`, the daemon's `close_bound`) and a Commit
  that failed definitely; after an unknown outcome it is not sent, and a
  quarantined session refuses it.
  Only a crash, an unknown outcome or a refused release loses them (at most
  128 per handle), and a dropped handle loses at most 4,096 pack ids (ids
  are never reused; gaps are allowed). A successful Commit never sends the
  release: measured by `mount_round_trips.rs`, the Store close costs [0, 5,
  2, 1] (cold) and a steady Commit stays [0, 3, 12, 1]; a handle keeping
  pack ids only would send nothing at close but make the second Commit [0,
  6, 13, 2] (it reserves ordinals again). A Commit that failed definitely
  sends it on its exit, before it settles; that write takes the lane row,
  bounded by the lock timeout. A Commit refused for a lane or lock wait
  (`Busy`, a lock timeout, another live operation) sends none and keeps its
  handle and block for the Workspace's next Commit (`failed_save_ids.rs`).
  A Save that fails definitely or does not finish (an in-lane step Save, a
  fallback's merge Save) leaves its unused rest on the handle, which that
  release returns; one whose write had an unknown outcome drops the block.
  Per-call Workspaces on one service, measured (`ordinal_burn` harness,
  both backends alike): two lose no ordinal in steady state in either
  unmount order (`slot_taken_ordinals.rs`; 128 per cycle when the closing
  handle was always the one released); three lose 44 per Commit
  sequentially and 5,460 per Commit when their Commits run at once (new
  handles' concurrent 16,384 reservations; 10,921 before #768). The `u32`
  ordinal space lasts about 97.6 million and 786,000 such Commits; see the
  physical-storage reservation row.
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
- (k) Stage 2: the sealed tail's publication, the binding hints and
  `commit_branch` are one transaction sent as one pipelined message
  ([Fused Commit](#fused-commit)). No head is read before it: the message
  asserts the bound head. A race sends the Commit on the path above, once.
- (l) A Storage handle keeps the inode values its Saves read beside those
  they published (`Fetch::inode_values`, the same 512 KiB bound): a stored
  value never changes, so the logical count reads none of the values
  construction read.
- (m) Kept: `finish`'s membership locate. The publication's objects are
  inserted if absent; a lost identity must be compared with its winner, and
  in the message one already stored refuses the whole transaction as a race.
  Without the locate every Commit that reuses an object stored since the
  handle last looked would fall back. A cold connection adds one round trip
  that prepares every message statement not yet cached:
  `mount_round_trips.rs` pins the second Commit at 3 (the lineage statement
  for a Commit with a parent is first used there), the first Commit at 23
  (its new handle reserves ids, reads its base and prepares the message;
  54 before) and an UpToDate Commit at 2 (its head read, now that
  statement's first use on the session; 1 before).

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
| [`mount_round_trips.rs`](../../../../engine/crates/layerfs-store-service/tests/mount_round_trips.rs) | see [Mount reads](#mount-reads) | A real Commit's writer: first 23 round trips (54 before stage 2), second 3 (25), UpToDate 2 (1); SQLite 35/5 and 16/1 statements/transactions (46/9, 28/5). A merge Commit is exactly one race refusal and one fallback (`Store::fused_counts`); the Commits around it are fused |
| [`fused_commit.rs`](../../../../engine/crates/layerfs-persistence/tests/fused_commit.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test fused_commit` | The message is 1 round trip and 1 transaction warm (2 with its prepares); a moved head, an object stored meanwhile and an existing Commit identity are `Fused::Race`; a generation change `StaleStore`; a lane wait past `lock_timeout` `Busy`; a missing epoch row and a removal committed while the message waits `Conflict`; a lost reply `UnknownOutcome` with the session quarantined; a barrier cancelled between its lock and unlock a definite refusal whose failure path releases the leftover (one more round trip; no advisory lock left on the backend, another writer commits). Each refusal wrote nothing (head, locators, packs and hints checked with admin SQL). SQLite: one transaction per Commit, the race and epoch cases |
| [`postgresql_aborted_commit.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_aborted_commit.rs) | `… --test postgresql_aborted_commit` | A body that ignores a refused statement is answered with that refusal, writes nothing and leaves the session usable. Before the `SELECT 1; COMMIT` guard it answered `Ok(())` |
| [`postgresql_lane_liveness.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_lane_liveness.rs) | `… --test postgresql_lane_liveness` | A nested write after its lane connection was terminated fails `Closed` and writes nothing: now through `pg_xact_status` |

### Fused Commit

A Workspace Commit over its bound head is one write transaction
(`layerfs_persistence::Committer`,
[`history/fused.rs`](../../../../engine/crates/layerfs-persistence/src/history/fused.rs)):
the Save's last publication, held by `Save::finish_prepared`
(`PreparedTail`), the new tree's binding hints and the Commit. On
PostgreSQL its requests go in one pipelined message, one round trip
([`backend/postgresql/history/fused.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/history/fused.rs),
statements in [`sql/postgresql/fused`](../../../../engine/crates/layerfs-persistence/sql/postgresql/fused)):

```text
 1  BEGIN READ COMMITTED; SET LOCAL search_path, synchronous_commit,
    lock_timeout; set_config('layerfs.attempt', <nonce>, true);
    generation fence 1/(catalog_incarnation = G)             simple text
 2  pack bodies, INSERT ... FROM unnest                      only with packs
 3  attempt check; store_lane FOR UPDATE; barrier lock+unlock simple text
 4  the Branch at the expected head and base (branch_snapshot filters), its
    root as expected, project_epoch.epoch kept in layerfs.epoch
 5  the publication's rows (publish_rows, checked)           only with rows
 6  the binding hint blocks                                  only with hints
 7  the next serial and the Commit insert with its attempt, one statement
 8  the lineage, the parent's line read in SQL
 9  the Branch advance, exactly one row
10  project_epoch FOR SHARE, equal to layerfs.epoch
11  COMMIT                                                   simple text
```

Every statement checks the attempt token, a transaction-local setting only
this transaction holds (a blake3 hash of the process, a counter and the
clock): if `BEGIN` failed, a statement that ran on its own in an implicit
transaction fails that check and writes nothing. Each request ends in its
own `Sync`, so after a refusal the rest answer `25P02` and the `COMMIT`
rolls back. Asserts raise from values the planner cannot fold
(`1/(x.n-x.n)` is `22012`, `(32768+x.n)::smallint` is `22003`), and the
first refusal decides:

| Request | Refusal | Outcome |
| --- | --- | --- |
| 1 | `22012` (generation moved) | `StaleStore` |
| 3 | `55P03`, `40P01` (lock wait) | `Busy` |
| 4 | `22012` (Branch moved, absent or retired; root differs) | race |
| 4 | `22003` (no `project_epoch` row) | `Conflict("project epoch")` |
| 5 | `22012` (an object already stored: a lost identity) | race |
| 5 | `22003` (a value block or signature refused) | `Integrity` |
| 7 | `22012` (the Commit identity exists) | race |
| 8, 9 | `22012` (no parent line; the Branch moved) | race |
| 10 | `22012` (the epoch moved or its row went) | `Conflict("project epoch")` |
| any | another SQLSTATE with a definite rule (`rows::sqlstate`) | that refusal |
| any | a request the client never sent (unencodable, or after one that was), the pipeline's `ROLLBACK` of what it sent acknowledged | that client refusal, definite |
| 4, 5, 7, 8, 9 | `22012`, then a later reply of the message lost (the session quarantined) | `Conflict("fused race refusal on a lost session")`, definite, no fallback |
| any | a SQLSTATE without one, `25P02` first, no reply, or an unsent request whose `ROLLBACK` failed | `UnknownOutcome`, session quarantined |

The outcome is decided while the session's turn and lock are still held
(`Session::pipeline_judged`), and an unknown one quarantines the session
before the lock is released: no other caller can use the untrusted
connection in between. A race refusal is the server's reply to a statement
after an acknowledged `BEGIN`, so its transaction aborted there and neither
its `COMMIT` nor the end of the connection can apply anything; when a later
reply of the message was lost, the separate path would start on a writer
that answers every call unknown, so the race is answered as the definite
refusal it is instead.

A message refused or cancelled after its barrier lock and before the unlock
(step 3) leaves the key held at session level, which the message's `COMMIT`
(a rollback after a refusal) does not end. So when any reply is a refusal or
a request was not sent, and no reply was lost, the session runs the
[barrier leftover release](#lane-barrier) before the verdict and before the
session is used again, also before a race's fallback: one more round trip on
the failure path only (appended to the pipeline's `ROLLBACK` when the
message was partly sent), never a `pg_locks` scan on an accepted Commit. A
failed release quarantines the session, so the verdict sees it untrusted. A
lost reply needs none: the quarantine closes the connection, which ends
every session-level lock.

A reply lost before step 3 leaves a backend that may still be inserting the
packs, outside the lane: a later lane pass would not wait for it, and the
`COMMIT` could follow the resolution's read. The open's check reads the
session's backend pid and start with the other meta columns (no extra round
trip), and `Session::pass_lane` on a quarantined session first runs
`pg_terminate_backend(pid, lock_timeout)` on that backend, if it is still
listed, which answers true only once the process has exited; not proven
gone is `Busy`, and a pid a session lock showed on another backend (a
transaction-pooling proxy) is `Unknown`. A start the check could not read
(hidden from a role set with `-c role=`, or not decoded) is also `Unknown`,
with nothing sent: the pid alone may since name another client's backend
(`fused_commit.rs`, `postgresql_lost_backend_without_start_is_not_terminated`).
A `COMMIT` the backend made is then found by the attempt
(`commit_by_attempt`); one it had not made is aborted
(`commit_lost_before_lane.rs`).

The fence statement holds `AccessShareLock` on `history_meta` until
`COMMIT`; the lane row and the barrier are taken by the transaction itself,
on the writer connection (the held-lane connection is not used); the epoch
is read under the lane at step 4 and re-checked `FOR SHARE` last. A race
(`Fused::Race`) wrote nothing: the Store service publishes the held tail
and the hints on their own and continues on the separate path in the same
call, once, from its head read. The tail and hints are written in phase
`Finish` with no Commit intent outstanding, as the separate path's `finish`
and binding hints are: a lost reply there is an unknown Storage or hint
outcome before any publication, folds the capture back like a failed
`finish`, and never leaves the Workspace refusing later Commits `InFlight`
with nothing to resolve (`commit_fault_race_custody.rs`; the writer
session is quarantined all the same, so that service's later Commits fail
until it is reopened). The same holds for an overflow's tail and hints and
for a held tail with no change against the bound root. The separate path
then runs from its head read (`reread_after_fused`: a moved head is merged
with before the lane, in a Save of its own), then `publish_lane_unmoved`
and the rest of that path. The message is never sent again, an epoch
`Conflict` has no fallback, and an unknown outcome never continues: custody
is kept. The pack ids the attempt carried are published unchanged; the
rolled-back transaction wrote nothing and they stay reserved to the handle.
A definite refusal with no fallback (`Conflict`, `Busy`, `Integrity`,
`StaleStore`, a race on a lost session, an unsent request) rolled the whole
message back, the held publication's ordinal release included: the Save
does not finish, but `PreparedTail::refused` puts its unused pack ids and
ordinals (from the first ones it did not assign to its reserved ends) on
the handle, and the failed Commit returns the ordinals
(`release_kept_ordinals`, the conditional rewind; refused by a quarantined
session), except after a `Busy`, when the handle keeps them for the
Workspace's next Commit. The ids the refused publication assigned stay below them and are
never reused. After an unknown outcome the tail is dropped and the Save's
whole remaining block is lost (`fused_refusal_ids.rs`).

While the tail is held, the Commit's own reads (the logical count's file
lengths, the binding hints) must see what the Store does not have yet. The
Storage handle serves the held publication whole, from the batch itself
([`read/held.rs`](../../../../core/crates/layerfs-storage/src/read/held.rs)):
its locations, pack bodies of either domain and inode values are consulted
before the bounded read caches and the Store, and a held location found is
kept in the locator cache as a Store answer would be. A held row may name a
pack an earlier batch of the same finish published (the last publication
split at a transaction bound inside that pack, as happens with files that
compress well): its descriptor comes from the finish's sealed packs and
only its location is served, its body being the Store's already. The batch
is in memory already and bounded by the publication's row and byte bounds. It is released
when it is published (fused or on its own: a lost location's winner is then
read from the Store) or dropped. Before this, the held entries lived only in
the bounded caches (4,096 locators, 2 MiB of metadata bodies, about 7,000
values) and an evicted one was looked up in the Store and missed: one new
directory of 4,000 or more small files, or 400 rewrites over a 20,000-file
tree, could never Commit unforced (`commit_fused_held.rs`), and a first
index of the held batch refused one whose rows named an earlier batch's
pack (`Integrity("held pack missing")`: 40 compressible files of 120 KiB
beside 300 small ones, or 200 of them).

Behaviour that differs from the separate path:

- The epoch re-check (step 10, `FOR SHARE`) runs under the publication's
  `lock_timeout`: a removal marker holding `project_epoch` makes the fused
  Commit `Busy`, where the separate path's `commit_branch` waited for it
  under its own transaction's settings.
- An existing Commit identity (the same request at the same millisecond) is
  a race here, and the separate path then makes the Commit at a new time;
  SQLite's fused transaction runs `commit_branch`, which reuses the verified
  row.
- A retired or missing bound Branch is a race here (step 4), after which the
  separate path's head read answers `Context("bound Branch is absent")`;
  SQLite answers `Missing(Branch)` from `commit_branch`. Neither is reachable
  while the Workspace's mount registration holds the Branch.
- Step 4 does not check the LayerStack's profile and scope as
  `commit_branch` does: the request's profile and scope come from a real
  Branch snapshot, so a mismatch is not reachable from a Workspace Commit.
- `Store::fused_counts` counts as a fallback every `Fused::Race`, including
  those decided before anything is sent: a request the committer does not
  fuse (a second parent, an unchanged root, another required head) and a
  session whose generation fence is not yet set (`incarnation` 0).

Only a Commit whose construction kept its whole built set is fused
(`ConstructedObjects::covers`): its validation reads its own objects from
those bytes, since the Store does not hold the held publication until the
message commits. A Commit without a built set (an SDK `fs.apply`, a
construction a test drives), one whose set dropped an object past its bound,
or one whose removals rebuilt the root goes on the separate path without an
attempt. So does a forced Commit (it reads the head it displaces in the
lane), a Commit that `--expected` names another head for, and a
request too large for one message (`Fused::Overflow`: more hint blocks than
one array, more rows than the transaction row limit, or a publication past
16 MiB − 1): the tail and the hints are published first, then the Commit
alone is attempted fused. So is a Commit whose new tree adds more name
bindings than one hint transaction carries (4,096 rows): the walk that
collects them stops there with nothing written, the held tail is published
on its own, every binding is then recorded in bounded transactions, and
the Commit alone is attempted fused. A hint row is never written before the
objects of the tree it describes (`commit_fused_hint_spill.rs`). A dangling
hint could not have made a reader fail: a hint names inode serials, and
every reader checks it against the tree it reads (`tree.lookup(parent,
name) == child`) before using it, reading only that tree's objects; it
would only have cost a lookup.

A fused Commit's new objects are never read back from the Store before the
Commit becomes visible: validation checks the candidate from the bytes the
Commit built (`ConstructedObjects`), and the Commit's own reads before the
transaction come from the held batch. This is the owner's decision; it
gives up a defence in depth (a Store read-back that would catch a
publication that stored other bytes than were built), not a known defect.
A fused Commit known published whose Save then fails to settle its held
batch locally (`PreparedTail::acknowledged`) keeps its custody as a failed
install after a known publication does (phase `Install`): the next Commit's
resolution installs it.

Not done: the fused transaction and a held lane assume one server, as the
lane barrier does. A writer whose route reaches another server than another
writer's lane connection is the multi-host case #768 records; the fused
transaction does not use the lane connection, so it adds no new pairing.

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
unit ([`acquisition/`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition)). The session's epoch is the
same lock-free value as on SQLite ([Acquisition](sqlite.md#acquisition)): a
`begin` waiting for the lane blocks no live owner's unit.

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

## Format upgrade

`upgrade_postgresql` (#804 §5.2) brings a schema-3 Store to schema 4 in
place ([`backend/postgresql/upgrade/`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/upgrade)).
It uses one raw client from the connector's `connect_raw` (not a Session,
whose open refuses version 3) and simple-protocol text only: one connect
and two messages, so no prepared statement and no lock across a round trip.

| Message | Phase | What runs |
| --- | --- | --- |
| 1 | `check` | one `SELECT`, no transaction, no locks, every `history_meta` and `store_policy` cell read through the row's JSON form (so a version 1 or 2 Store missing a column is still refused by its version name): `server_version_num`, both version cells, the recorded source's sha256, the recorded and live column definitions, `catalog_id`, the binding, `catalog_incarnation` G, the `gc_op` row count, whether `commit_attempt` exists and the two version CHECKs. Rust matches them against the frozen list: version 4 answers `current` (no second message), anything but 3 is `Unsupported` by name, an unsettled `gc_op` is `Busy` |
| 2 | `convert` | one text: `BEGIN`; `search_path`, `lock_timeout` (2 s) and `synchronous_commit=on` for the transaction; `pg_try_advisory_xact_lock` of the Store-wide GC key (compaction's and mounts' key) or 55P03; `LOCK TABLE "commit", history_meta, store_policy IN ACCESS EXCLUSIVE MODE NOWAIT`; `store_lane FOR UPDATE NOWAIT` in a `DO` that refuses a missing lane row (`LFU02`); a `DO` assert that nothing message 1 read changed (`LFU01`), which also stores the heads, Commit count and totals in `layerfs.before`; step 3 → 4 (`ADD COLUMN attempt`, `CREATE UNIQUE INDEX commit_attempt`, each version CHECK dropped, its row updated and the CHECK re-added, `catalog_incarnation + 1` so a session opened before fails `Stale`, the frozen schema-4 source and target definition recorded); a `DO` verify (`LFU02`: before values, live columns equal the frozen target, the schema-4 record, `pg_index` and `pg_constraint` for the index and the three CHECKs); `COMMIT` |

An `Ok` reply is `upgraded`, even if a cancel request was sent meanwhile
(the answer follows the server). A reply with a SQLSTATE means the server
stopped the text and rolled back, and the client closes instead of sending
`ROLLBACK`: 55P03, 40P01 and `LFU01` are `Busy`, 57014 is `Cancelled`,
`LFU02` and any other refusal `Failed`; a connection-class or shutdown
SQLSTATE, or no reply, is `Unknown`. A cancel during message 2 sends the
client's cancel token from a thread nobody joins (its dial is not waited for); before message 2 the flag
refuses it. The run keeps its own counters: 1 connect, 2 round trips and 1
write transaction when upgraded; 1, 1, 0 when current.
