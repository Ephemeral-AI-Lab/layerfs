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
                                 quarantined, the connection's backend
                                 (pid, start), the closed connections'
                                 backends not yet ended, work counters
   held_ops, orphans             keys of each held intent; mount intents
                                 whose admission reply was lost (reopen)
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
| `prepares`, `connects` | statement-cache misses and COPY statements; new connections (session and lane, a reopen included) | same |
| `reopens`, `last_reopen_ms` | writer reopens that settled and served (a failed attempt is not counted), and the wall-clock time of the last one in ms since the Unix epoch ([Writer reopen](#writer-reopen)) | [`reopen.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/reopen.rs) |

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
flight ran.

A writer turn (`Session::decided_turn`) can also carry one write unit
(#818 I3b): one transaction spread over several flights of the turn, each
sent once the previous flight's replies are read, so a later flight can use
an earlier one's answers. `DecidedTurn::prepare_unit` prepares the unit's
statements in one flight before the transaction opens, outside any
transaction, and the turn holds their handles, so a statement-cache clear
between flights cannot drop one. `DecidedTurn::send_in_unit` sends one
flight that binds only those handles (any other statement, or a parameter
count that differs, is refused as `Integrity` before anything is sent). The
first flight carries the unit's `BEGIN` with the lane row
(`write_begin(true)`). `open` keeps the transaction open after a flight, and
the last flight ends with the unit's own `COMMIT`. The flights are counted
as ordinary flights; the caller counts the unit's one transaction
(`count_unit`). One attempt: a lost reply quarantines the session, and
nothing is resent. A refused or unsent request is followed by `ROLLBACK`
and the leftover barrier release in one more round trip. A turn that ends
with the unit still open rolls it back the same way and counts it as rolled
back; it never commits. A panic between the unit's flights closes the
connection instead. While a unit is open, the turn's ordinary `send`, a
second `prepare_unit` and a call inside the thread's held lane are refused
as `Integrity`. `postgresql_session_unit.rs` covers a committed unit, a
refused unit, the guards, and drop and kill sweeps at every message of a
two-flight unit: the rows are either the committed result or untouched. No
product path opens a unit yet.

`sslmode` is used exactly as given: the driver's default `prefer`
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
failed `Busy` after its lock wait. Closing ends all of it. Nothing is replayed
and the outcome stays unknown. The closed connection's backend (pid and
start, read when the session opened) is recorded as lost until it is proven
gone. On a session that does not reopen (persistence and compaction
handles), every later call is refused before it reaches a connection, and
`PostgresStorage::usable` answers `false`, so the caller opens a new
session. The Store service's writer and readers reopen instead: the next
call opens one new connection and settles what the closed one left before
serving ([Writer reopen](#writer-reopen)). GC intent rows are
untouched: an intent's owner stays live by its recorded process, not by
this connection. A diagnostic driver
keeps its read-only connection, whose deadline and cancellation outcome are
still read ([`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/connection.rs)).

A mount, a compaction claim and their registrations keep session advisory
keys (Store, project and Branch) across statements
([`admission/session.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission/session.rs)).
The lock and the unlock are each one autocommit `Bare` statement for all of
an intent's keys (since #768 steps B and C a Branch mount's lock is a
statement of its admission message and an owner release's unlock rides in
the release's message, [GC admission](#gc-admission)), outside the
generation fence (#776 P2): a fenced
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

### Writer reopen

The Store service's writer session reopens after a quarantine
([`reopen.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/reopen.rs),
enabled by `PostgresStorage::enable_reopen` in the service's
[`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs)),
and so do its readers (the control reader and every Workspace reader).
Persistence and compaction handles do not: they keep the refusal above.
Before the reopen, one lost reply refused every later Commit, mount,
unmount release and kept-ordinal rewind of that service until it closed,
and a reader whose connection closed failed every later read of its
Workspace (a lost control-reader reply failed every later resolution of
the service, any Workspace's).

The next call that takes the session's turn outside a held lane opens
exactly one new connection before its own work: a new session for a new
operation, never a resend of the one whose outcome stays unknown. A
connection the server already closed (a terminated backend, a server
restart, a dropped route) is seen before anything is sent, with no round
trip, and is quarantined first: nothing of that call had gone out, so it
loses nothing and proceeds on the new connection. The reopen settles, in
order, before the new connection serves anything:

| Step | What it does | A failure |
| --- | --- | --- |
| 1. End the old backends | One statement ends every recorded lost backend by pid and start (`pg_terminate_backend` bounded by `lock_timeout`, 30 s when unset) and reads which are gone. One whose start is unknown (the role could not read it) is never ended: the pid alone may name another client's backend | A backend not proven gone stays recorded and the reopen proceeds; a later `pass_lane` ends it before any resolution concludes "not committed". Only a lost reply fails the reopen |
| 2. Read the new backend | The Store's generation, `pg_backend_pid()` and the backend start in one statement | a moved generation `Stale` |
| 3. Take held keys again | The advisory keys of every intent the session still holds (`held_ops`: its live mounts) went with the old connection. They are taken again, all or none, in the statement a registration uses (`gc_lock_session`, generation and backend checked) | a refused key `Busy`, a moved generation `Stale`, a key on another backend (a transaction-pooling proxy) `Integrity`: the new connection is closed, the writer stays quarantined and the call fails. The intent rows were never touched, so they kept refusing removals and compaction |
| 4. Release orphaned rows | Two kinds of intent row no caller will release: a mount whose admission reply was lost (it may have committed its row and never reached its caller), and an op whose release found the writer quarantined and could not be sent (its owner has closed; the closed connection gave its keys back, so it left `held_ops` and step 3 does not take them again). Each was recorded inside the turn that found it; its row is deleted by the owner release's generation-checked statement, by op id (token `reopen`). This step runs only when step 1 proved every closed connection's backend gone, so no admission `COMMIT` can land after the delete; otherwise every orphan waits for a later reopen | An orphan whose delete was answered is forgotten (a definite refusal leaves the row). One whose delete may not have run (a lost reply, its prepare's included) stays, with those not yet attempted, and the reopen fails. The delete names one op and is a no-op once the row is gone, so sending it at a later reopen never touches another op's row |
| 5. Prepare again | The daemon's mount statements prepared at open (`prepare_mounts`) are prepared on the new connection | as any prepare |

**Custody never straddles a reopen.** Every change to `held_ops` and the
orphans happens inside a writer turn, together with the statement that
changes what the connection holds, and a reopen reads them inside its own
turn, so a reopen never sees an op half registered or half released. A
mount's one-message admission holds its op, or records it as an orphan,
before its turn ends. A release takes the op from `held_ops` after the
turn is ready (a reopen in that turn has taken its keys again, so the
unlock gives back exactly that level) and puts it back, when the unlock
gave nothing back, before the turn ends. A registration statement by
statement (removal, Init, merge, compaction and a mount's fallback) takes
its keys in one turn and writes its row in a second turn pinned to the
connection that holds them (`Session::ready_pinned`, by the connection's
epoch, `State::closed`): once that connection has closed, the pinned turn
is refused `Unknown` before anything is sent and the row is never written
keyless. Its op is held inside that second turn. Every unlock of keys that
are not in `held_ops` (a refused lock's grants, a failed registration's
keys, an earlier level of a registered op, a compaction claim) is pinned the
same way and sends nothing once that connection has closed, since the
current one holds other ops' levels of the same keys. Only that pin refusal
counts as unlocked; any other refusal before the unlock is sent (a closed
session, same-thread re-entry, a poisoned lock) is answered as it is.

Pooler detection, the generation fence and `lock_timeout` apply to the new
connection as to the first; it is opened with the same settings and
connector. **One reopen attempt per public call.** A Commit, a mount, an
unmount, a resolution and an SDK Store call each run inside
`layerfs_persistence::one_call` (the Store service's `BoundWorkspace` and
`methods::call` entry points, and each daemon control request as a whole,
so a Commit request's custody resolution and its own Commit share one
budget): each session attempts at most one reopen in it. Sessions are told
apart by a process-unique identity, never reused, so a session closed and
opened again within a call has a budget of its own. The scope is the
calling thread's: a thread spawned inside a call starts unscoped (one
attempt per turn) unless it scopes its own work; no product call hands a
session's work to such a thread (a Commit's construction thread takes no
session turn). A reopen that fails answers its error, the session stays quarantined,
and a later call may try once more; a connection that closes again later in
the same call is refused `Unknown`, not opened a second time. Outside a
call scope the budget is one per turn. There is no loop or backoff. Calls
queued for the writer's turn see the new connection in turn, so several
calls waiting when the connection died reopen it once. A call inside a held
lane never reopens: its lane transaction relies on the closed connection.

A reader holds no keys and writes nothing, so its reopen settles nothing
beyond steps 1 and 2. Its store-service session no longer latches an
uncertain read as a permanent failure (`StoreReader::reopening`): the read
whose reply was lost fails, and the next read reopens.

Retained uncertain Commits are not resolved inside the reopen: the Store
service keeps no Workspace registry, and an owner job run from inside the
writer's turn could wait on that turn. Each is resolved by its Workspace's
next Commit (`resolve_retained`, [Fused Commit](#fused-commit)): the
`pass_lane` that resolution runs first ends any lost backend still recorded,
then the attempt lookup installs it or folds it back. Nothing is resent.
`SqlWork.reopens` counts reopens that served and `last_reopen_ms` records
the last one; neither is in the observation wire format.

**Known limit: a lost backend that can never be ended.** A closed
connection's backend whose start the role could not read, or one a session
lock showed on another backend (a transaction-pooling proxy), cannot be
ended by pid and start, so it stays recorded for the session's life. Every
later `pass_lane` on that writer then answers `Unknown` without sending, so
each later resolution of a retained Commit of that service stays
undetermined, and step 4 never runs (orphaned rows stay until the daemon
exits and the owner is settled). Concluding "not committed" without proving
the backend gone could install a Commit twice, so the record is kept. A
role that can read its own backends' starts (the default) never meets it.

## Transactions

| `Mode` | `BEGIN` | Lane row | Used by |
| --- | --- | --- | --- |
| `Read` | `REPEATABLE READ READ ONLY` | no | history reads, policy, signatures |
| `Write` | `READ COMMITTED` + `synchronous_commit` + `lock_timeout` | `FOR UPDATE` and the lane barrier; nested in the held lane: the barrier key shared and a lane check in the `COMMIT` round trip | history writes, allocation, acquisition writes |
| `Stage` | `READ COMMITTED` + `synchronous_commit` | no | schema create, merge records |
| `Publish` | as `Write` | `FOR UPDATE` and the lane barrier after the pack bodies; nested: as a nested `Write` | one Save publication, up to 16 MiB − 1 |
| `Snapshot` | `REPEATABLE READ` + `synchronous_commit` + `jit` off | no | the garbage-collection mark: reads history, locators and packs from one snapshot and writes only `kept_node` |
| `Immutable` | none; autocommit statements | no | locates, pack reads, inode values |
| `Statement` | none; exactly one autocommit statement | no | Init's acquisition reads (owner joined), `branch_snapshot`, a Branch by name, a Commit or Layer by id, a LayerStack by name, a project's `project.remove_file` markers (alone or with a node's serial), a read call's context (`read_context_*`), `digest_roots` (up to `FILE_DIGEST_WINDOW` = 1,024 contents) |
| `Admit` | as `Publish`: `READ COMMITTED` + `synchronous_commit` + `lock_timeout` | never | a mount's GC registration ([GC admission](#gc-admission)) |
| `Bare` | none; exactly one autocommit statement that may write, sent as written | no | the GC session keys and the owner's release; each carries its own generation check ([GC admission](#gc-admission)) |
| Fused | as `Write`, sent in one pipelined message with the generation fence and a per-attempt token | `FOR UPDATE` and the lane barrier in the transaction itself, never nested in a held lane | a Workspace Commit over its bound head ([Fused Commit](#fused-commit)) |

Source: [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs),
[`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs).
A single statement sees one snapshot, so `Statement` reads a Branch and the
rows it resolves to coherently. A read call's Branch-by-name and marker reads
are each one such statement (#818 R1): one round trip, where a `Read`
transaction costs three (its `BEGIN` and fence, the statement, `COMMIT`).
Each still sees a snapshot of its own, as the separate transactions did, and
the markers are still read after the head resolves, so a later read can only
hide more. A read call naming `main`, a Branch or a full node identity
reads all of that in one statement instead (#818 R5, `read_context_named` or
`read_context_node`): its stack, Branch, node record with its serial, the
Commit's recorded Branch and the project's markers come from one snapshot,
one context row then one row per marker (records carry no arrays). The
markers are those `path_removals` already loads; nothing caps them. A
history walk (`fs.blame`, `fs.revision`) reads its first-parent Commits in
windows of 16, 32, 64 then 128, each one such statement
(`lineage_chain_window`, #818 H4): it follows the Commits' run segments
through the `commit_runs` (run, depth) index, a run being one first-parent
path whose first Commit's parent lies in another run, and returns each
Commit's row and `history_serial` with its segment and depth, so the order
is checked in Rust. A row is that Commit's read: one that fails to decode
refuses only when the walk reaches it, and a gap ends the window so the walk
reads the next Commit itself. SQLite reads no window (the catalog's default,
one Commit and serial at a time, is not sent there). Statements outside a transaction name the
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
taken and given back in one statement each. Since #768 step B a Branch
mount's whole registration is one pipelined message
([Mount admission in one message](#mount-admission-in-one-message)); the
statement-by-statement registration below remains for every other intent
and decides each refusal that message cannot report exactly
([`admission.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission.rs) `register_separately`,
[`authority.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission/authority.rs) `release`,
[`session.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission/session.rs)):

```text
 register_separately (a mount's         round trips
   fallback; other intents on the lane)
   gc_lock_session: every key in one     1   Store, project, Branch; all tried,
     statement (Bare), after its own         none when the generation moved
     generation check
   Admit: BEGIN RC; SET LOCAL ...;       1   no store_lane row, no barrier
     lock_timeout; fence
   Branch probe, conflict scan,          3   register_in, shared with SQLite
     INSERT ... RETURNING
   COMMIT                                1
   keys kept with the op in the session (held_ops)

 release (the owner's), one pipelined message since #768 step C
   gc_intent_release_owner: DELETE ...   1   one fenced autocommit statement;
     RETURNING; records the attempt's        refused, deleting nothing, when
     token in `layerfs.released` when it     the generation moved
     ran at the session's generation
   mount/unlock.sql: the op's kept       (same  only when this session holds
     keys, one statement, given back      flight) the op; after the delete, in
     only when it finds that token               its own Sync; gives nothing
                                                 back after a refused or stale
                                                 delete
```

The release's two statements go in one flight
([`release.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/history/release.rs)).
Each has its own `Sync`, so the unlock runs whatever the delete answered.
The delete, when it completes at the session's generation, sets the session
setting `layerfs.released` to this attempt's token (rolled back with the
statement if it fails), and the unlock gives the keys back only when it
finds that token. So it gives them back exactly when the
statement-by-statement release answered success and sent its unlock: after
a delete that ran or found the row already gone (another session, a
reconcile, an operator's retained `done` row), even when the generation
moves between the delete and the unlock. (The first step-C text checked the
generation again in the unlock; a generation that moved in between then
left an `Ok` unmount's keys held with no later release to give them back,
a review finding, fixed by the token.) After a
stale or refused delete it gives nothing back and the session keeps the
keys with the op, so a later release still can. A lost delete reply is
`UnknownOutcome` and quarantines the session; a refused or lost unlock
quarantines it too, while the release answers its delete, as before. Such
an op has left `held_ops` inside the release's own turn, so no writer
reopen takes its keys again ([Writer reopen](#writer-reopen)). A release
whose turn finds the writer quarantined, and whose reopen fails or is
refused, sends nothing: its op leaves `held_ops` and its row becomes an
orphan the next reopen deletes. So does a release whose flight quarantined
the writer before its delete was sent (the reply to a prepare in that
flight lost, after a full statement cache cleared its statements): the op
left `held_ops` before the flight, so it is recorded as an orphan directly
(`writer_reopen_release.rs`). A release whose delete was sent and whose
outcome is unknown is not an orphan: the row stays, marked uncertain by
`release_or_mark`, and no reopen deletes it a second time (the documented
limit below).

`postgresql_gc_sessions` pins the counts of one warm mount register and
release: register 1 round trip since step B (the one message; 6 before it,
statement by statement as above: 4 counted statements plus the `BEGIN` batch
and `COMMIT`), release 1 since step C (2 statements in one flight, no
transaction; 2 round trips before it). Before #768 each was
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
- `lock_timeout` stays on the `Admit` transaction. The release's
  autocommit statements run without it: it can wait only for its own op's
  row (held by another short admission transaction) or for a swap's brief
  `ACCESS EXCLUSIVE` on `history_meta`. Known limit (accepted, predates
  #768's release changes): it also waits, without `lock_timeout`, for any
  lock an operator or another session holds on `gc_intent` (an independent
  check saw an unmount wait 2 s behind such a lock).
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
- An unmount releases the intent on the writer even when closing the
  Workspace's reader failed (`BoundWorkspace::close`, #768 review): the
  healthy writer deletes the row and gives back the keys instead of keeping
  them until the service exits. The release runs once, after the local
  close succeeded; nothing with an unknown outcome is sent again. A reader
  whose backend was terminated no longer fails: its next read reopens it
  ([Writer reopen](#writer-reopen)), so its reads and the unmount succeed
  (`tester768_stale_reader` `a_mounted_workspace_reader_terminated`).

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

The register body (Branch probe, conflict scan and insert) is one
PostgreSQL-only statement since step B, for a Branch mount only, and it
does not duplicate the rule: it composes the shipped `branch_branch_by_id`
and `gc_intent_ops_branch` texts that `register_in` runs
([Mount admission in one message](#mount-admission-in-one-message)).
Project- and Store-scoped intents keep `register_in`.

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

Since #768 step B a PostgreSQL mount sends two messages, both on the
writer, and none on a reader; it never opens or uses the control reader
([`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs),
`Opened::with_mount_on_writer`, set by `bootstrap.rs` for PostgreSQL only):

```text
 writer  branch_snapshot or branch_snapshot_named            1 RT  one fenced autocommit
         (a bind by Branch id takes the project name for            statement
         the intent from the same row)
 writer  gc_register_mount: BEGIN batch, the session keys,    1 RT  one pipelined message
         the asserting insert, COMMIT, then paths_node_markers       (Mount admission in one
         (the bound node's serial and every path_removed             message)
         marker of its project)
```

The Workspace reader is still opened at a first mount (or prefilled after
readiness) for the Workspace's own reads; a first mount on a new reader
pays its open and the root's first demand there, a repeated mount nothing.
SQLite keeps the earlier route: the snapshot and the markers on the
Workspace reader, the registration on the writer, as below:

```text
 Workspace reader  branch_snapshot or branch_snapshot_named     1 autocommit statement
 writer            gc_register: the mount intent                 admission (history/admission.rs)
 Workspace reader  paths_node_markers: the bound node's serial   1 autocommit statement
                   and every path_removed marker of its project
```

The snapshot moved to the writer, so it now also waits behind another
Workspace's Commit holding the same daemon's writer admission, as the
registration already did ([Waits that remain](#gc-admission)).

The markers are read after the intent commits, never with the snapshot. A
path removal takes the project key exclusively in its marker transaction and
scans the project's live intents, while the mount holds that key shared from
before its registration (on SQLite the single writer orders the two
transactions instead): a removal that committed before the intent is seen
by the marker read, and one after it is refused `MountsActive`. A marker read
before the registration would miss a removal that commits in between. Both
reads are fenced autocommit statements (`read_statement`); on PostgreSQL
the marker read is the message's last request, sent after its `COMMIT` on
the same connection, so it reads in a snapshot taken after the intent
committed.

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

### Mount statements prepared at open

A statement is prepared once per connection, on its first use, at one round
trip each (`State::statement`). Since #768 step A the mount's statements are
prepared before the mount, in one flight each
([`mount.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/history/mount.rs),
[`prepare.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/prepare.rs)):

- **The writer**, when the daemon opens its Store
  ([`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs)
  `OpenedStore::prepare_mounts`, called by the daemon's owner after
  `open_store_observed`): the mount admission's and the owner release's
  statements. Some name tables without the schema, as the transactions that
  run them resolve them through `SET LOCAL search_path`, so the flight is
  one read transaction: its `BEGIN` batch, the prepares and `COMMIT`, one
  round trip. Step A ran it in every Store open; since the review only the
  daemon, which mounts, pays it: a Store opened for an SDK call, an install
  or a restore does not (`mount_round_trips` `store.open` [1, 1, 1, 1] and
  `store.prepare` [0, 1, 0, 1]; the open was [1, 9, 5, 1] until #776
  deleted the four in-place migration probes, 2 round trips each). A mount on a service that did not prepare
  them prepares the message's statements at first use, in the message's
  own turn ([Mount admission in one message](#mount-admission-in-one-message)).
- **A Workspace reader**, when it opens (at the mount that opens it, or
  after readiness for the idle reader, `Store::prefill_reader`): the
  snapshot by Branch id and by name, the markers, and the root's first
  demand (the chain locate, the pack ranges, the inode value blocks), each
  in its fenced autocommit form. The control reader prepares none.
- **Since step B** the snapshot (by id and by name) and the markers run on
  the writer, so the writer's open flight prepares them with the admission
  message's lock and insert statements and the separate path's texts; a
  Workspace reader prepares only the root's first demand.

A prepare changes nothing on the server: a refused one fails the open as
its one attempt, and one whose reply is lost quarantines the session.

```text
                                  before step A    after step A
 Store open (writer, daemon)      9 RT, 1 tx       10 RT, 2 tx
 idle reader (prefill)            1 RT             2 RT
 first mount, own reader          23 RT            15 RT   (writer 10 -> 6, reader 13 -> 9)
 first mount, prefilled reader    23 RT            14 RT   (writer 10 -> 6, reader 13 -> 8)
 repeated mount                   9 RT             9 RT
 unmount, first on the session    4 RT             2 RT
```

The counts are `mount_round_trips.rs`'s session counters (connects are not
round trips); `postgresql_open_cost.rs` sees the same +1 on the wire (a
daemon Store open 10 -> 11, which that test now makes with
`prepare_mounts` as the daemon does, a reader open 2 -> 3); its host open
pays no prepare.

### Mount admission in one message

Since #768 step B a PostgreSQL Branch mount's registration is one pipelined
writer message, one round trip, after the snapshot read
([`history/mount.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/history/mount.rs),
[`admission/mount.rs`](../../../../engine/crates/layerfs-persistence/src/history/admission/mount.rs),
`HistoryCatalog::gc_register_mount`):

```text
 1  BEGIN READ COMMITTED; SET LOCAL search_path, synchronous_commit,
    lock_timeout; set_config('layerfs.attempt', nonce, true); the
    generation fence                                 (simple text, as Fused)
 2  mount/lock.sql: every key tried; any refused, the granted ones given
    back in the same statement, which then fails (all or none)
 3  mount/insert.sql: INSERT ... SELECT ... WHERE the Branch exists
    (branch_branch_by_id) AND no live operation conflicts
    (gc_intent_ops_branch) ON CONFLICT DO NOTHING; fails unless one row
 4  COMMIT
 5  paths_node_markers, fenced autocommit, only when the mount asks
```

Statements 2 and 3 check the attempt token, so if `BEGIN` failed and a
statement ran on its own it takes no key and writes no row. Each request
ends in its own `Sync`: after a refusal the rest of the transaction answers
`25P02` and its `COMMIT` rolls back. The first refusal decides; the keys
are session locks, so they outlive the rolled-back transaction:

| Refused at | SQLSTATE | Keys | Outcome |
| --- | --- | --- | --- |
| 1 | `22012` | none taken | `StaleStore` |
| 1 | `55P03` (the lock timeout, waiting on `history_meta`) | none taken | `Busy`, as before step B: the fallback's key statement runs without `lock_timeout` and would wait as long as the lock is held |
| 1 | another definite one | none taken | the statement-by-statement registration, once |
| 2 | `22012` (a key refused) | none held | the statement-by-statement registration, once |
| 2 | `22003` (another backend) | none taken | `Unsupported("transaction-pooled PostgreSQL connection")` |
| 3, 4 | a definite one (`rows::sqlstate`) | held | one unlock statement (+1 RT), then the statement-by-statement registration, once |
| 5 | a definite one | held with the op | admitted; the markers are read separately |
| any | none, `25P02` first, or a SQLSTATE without a definite rule | unknown | the session is quarantined (its connection closes, so the server holds none of its keys); `UnknownOutcome`, never sent again |
| none | — | held with the op | admitted (`held_ops`), with the markers |

The message admits only what the statement-by-statement registration admits
with nothing to settle: it refuses whenever any live operation row
conflicts, dead or not, and never revives or replaces a row of its own op
(a retained `done` row, or a live re-registration). Each such refusal is
decided by `register_separately`, which settles provably dead rows,
classifies the holders (`MountsActive`, `BusyOp`, `Unresolved`) or
registers. So for every state the message reads with all its replies
received, the fast path changes the cost of an admission, not its answer;
the exceptions are a lost reply (below) and the cases it reports itself
(`StaleStore` and a pooled connection, which that registration would also
report, and a lock timeout in the `BEGIN` batch, which answers `Busy`
within about the session's `lock_timeout`). Any other definite error of
the fast path, a statement that does not prepare included, falls back the
same way, once; an unknown outcome
still quarantines the session and is never sent again. A statement the
message needs and the connection has not cached (evicted from the
256-entry statement cache, or a service that did not prepare it at open)
is prepared in the message's own turn inside a read transaction with the
Store's `search_path` (`Session::decided_turn`), so its unqualified
names resolve and nothing can evict it before the flight.
`postgresql_mount_pipeline.rs` checks this over seeded random sets
of intents (every kind; Store, project and Branch scopes of the mount's
project and another; live, uncertain and done; with and without owners):
the message admits exactly when `validate_mount` and `conflicts_for`
refuse nothing, and the whole registration answers exactly as
`register_separately` alone, holders included; a missing and a retired
Branch and both same-op rows are checked the same way. No refusal leaves a
key held.

A refusal on a session that also lost a later reply is a definite
`Conflict`: the transaction aborted at the refusal, and the quarantined
session cannot run the fallback.

**A reply lost after the message's `COMMIT`.** If the connection is lost
after the `COMMIT` went out, the markers' reply included, the mount answers
`UnknownOutcome` and the writer is quarantined, but the intent may have
committed. The mount never reached its caller, so its op id is recorded,
inside the turn that lost the reply, and the writer's reopen deletes that
row with the owner release's generation-checked statement once the old
backend is proven gone ([Writer reopen](#writer-reopen), step 4;
`mount_fault_*` "unknown, row released at reopen"). Until then the row
refuses `branch.remove` and compaction (`MountsActive`). This stays within
the one-attempt rule: the mount is not resent, and the delete is a new
operation on a row no caller holds. Before step B the markers were
read on the Workspace reader, so a lost markers reply left a healthy writer
that could release the row.

**Known limit: an unmount's release whose reply is lost.** The release may
have deleted the row or not; the reopen does not delete it a second time
and takes none of its keys again (`mount_fault_*` "unknown, row kept").
A row left there stays until the daemon exits and the owner is settled
(`gc_mark_uncertain`, an owner reconcile, or an operator's release).

```text
                                  after step A    after step B
 Store open (writer, daemon)      10 RT, 2 tx     10 RT, 2 tx  (the message's texts join the open flight)
 first mount, own reader          15 RT           9 RT    (writer 6 -> 2, reader 9 -> 7: open and root reads)
 first mount, prefilled reader    14 RT           7 RT    (writer 6 -> 2, reader 8 -> 5: the root's reads)
 repeated mount                   9 RT            2 RT    (writer 6 -> 2, reader 3 -> 0)
 mount after a Commit             9 RT            2 RT    (as a repeated mount; no content chain)
 UpToDate Commit                  2 RT            1 RT    (its Branch read was prepared by the mount's
                                                           snapshot at open)
 gc_register (postgresql_gc_sessions)  6 RT       1 RT
```

Step C then made the owner release one message (the delete and the
conditional unlock, above): an unmount 2 RT -> 1 (`mount_round_trips`
`unmount`, `cycle.unmount`: writer [0, 2, 2, 0] -> [0, 1, 2, 0];
`postgresql_gc_sessions` release 2 -> 1). A per-tool-call cycle on a
prefilled reader is now: the daemon's Store open 10 RT (9 plus the
prepare), the idle reader's prefill 2, mount 7 (2 on the writer), unmount
1; a later mount, after a Commit or not, 2.

**The idle reader's check** (`PostgresStorage::usable`, called when a mount
takes a pooled reader) is local first: two runtime turns, then whether the
client or its connection task has ended, which catches a connection the
server closed (a restart, a terminated backend, an idle timeout) with no
round trip. A reader whose connection has been silent at least
`READER_IDLE_CHECK_MS` (30 s, the selection's `reader_idle_check_ms`) must
also answer an empty query within
`READER_CHECK_TIMEOUT` (5 s, one counted round trip), as every reuse did
before step B: a path that ends an idle connection without telling the
client (a load balancer or NAT that resets it at the next packet, a cloud
path that drops it silently, where a read would wait for the TCP
retransmission timeout) is found here, and the mount gets a fresh reader
instead of a failed or stalled first read. This is a readiness check
before the reader serves anything, not a retry. Silence counts from the
connection's last reply from the server (a result or a server error, kept
in the session, `Driver::silent`), not from the reader's return to the
pool: a Workspace that held the reader for minutes without reading leaves
it as silent as one that sat in the pool, and its next checkout checks it.
Back-to-back per-tool-call mounts reuse a reader that heard the server well
under 30 s ago and send nothing for it (step B first made the check local
only; the 30 s query and the reply clock are review fixes).
`postgresql_reader_idle.rs` checks a fresh reader (nothing sent), an aged
one whose path resets (replaced), one that went silent while a Workspace
held it and was handed back just before the next mount (checked, replaced)
and an aged one whose backend was terminated behind a silent path
(replaced after the 5 s bound).

**Known limit: a reader close that fails leaves its session entry.**
`ReadService::close` (`read_service.rs`) returns the error of
`session.release()?` before it removes the Workspace's session entry, so a
reader whose close fails stays listed under that Workspace (its counters
are not retired either). The unmount still releases its intent (step C's
custody fix in the daemon); only the read service's bookkeeping keeps the
entry. Recorded by review, not fixed in #768.

| Test | Checks |
| --- | --- |
| [`postgresql_mount_pipeline.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_mount_pipeline.rs) | the random equivalence and edge cases above; a mount racing a compaction or a project removal on another session is never both admitted; two sessions mounting one Branch are both admitted; a backend terminated while the insert waits on a table lock is an unknown outcome with no row and no key, and the session refuses further calls; an insert reaching the lock timeout gives its keys back and the fallback reports `Busy`; a release whose delete the server refuses keeps its row and keys, and the next release frees both in one round trip; a release whose generation moves after its delete gives its keys back; a `BEGIN` batch that meets the lock timeout answers `Busy` within about the lock timeout, with no row and no key; a `BEGIN` batch cancelled while it waits (`57014`) falls back once and the mount is admitted when the lock goes |
| [`postgresql_reader_idle.rs`](../../../../engine/crates/layerfs-store-service/tests/postgresql_reader_idle.rs) | the idle reader's check: under the threshold nothing is sent; past it, a reset path and a silently dead backend both get the mount a fresh reader, the second within the 5 s bound; a reader silent past the threshold while checked out is checked at its next checkout, however short its time in the pool |
| [`mount_fault_drop.rs`](../../../../engine/crates/layerfs-store-service/tests/mount_fault_drop.rs), [`mount_fault_kill.rs`](../../../../engine/crates/layerfs-store-service/tests/mount_fault_kill.rs) | a dropped connection, and a terminated backend, at every server message of a repeated mount and of an unmount (`support/mount_sweep.rs`): an answered mount holds its row and three keys (unless the backend ended after every reply); an answered unmount left neither; a failed call whose service still works left nothing (a mount) or kept both (an unmount); an unknown one closed the writer's connection, so no key, at most its row, and the next call reopened the writer (at most once): a mount's row is released by the reopen, an unmount's row is kept, and the old backend has ended; a backend terminated after every reply of a mount loses its keys, which the reopen takes again; a fresh service mounts; no backend is left in a transaction |
| [`writer_reopen.rs`](../../../../engine/crates/layerfs-store-service/tests/writer_reopen.rs) | [Writer reopen](#writer-reopen): a terminated writer backend and a dropped writer connection reopen at the next Commit, which lands (`reopens` 1, the old backend gone); an uncertain fused Commit that the server committed is installed once (`resolve_retained` `Committed`, then `UpToDate`) and one cut before its `COMMIT` folds back and lands once; a mount kept across the reopen keeps its row and its keys move to the new backend (removal and compaction still `MountsActive`, the unmount releases both); a key another session holds exclusively refuses the reopen (a definite refusal, no reopen counted, the row kept), the writer stays quarantined and a later call reopens once the key is free; a mount whose reply was lost after its `COMMIT` leaves no row after the reopen; a reopen against a server that refuses connections fails once and the next call reopens; two Commits queued when the writer died reopen it once |
| [`testerwr_races.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr_races.rs), [`testerwr_orphan_release.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr_orphan_release.rs), [`testerwr_reopen_sweep.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr_reopen_sweep.rs), [`testerwr_resolve_sweep.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr_resolve_sweep.rs), [`testerwr_control_reader.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr_control_reader.rs) | The independent tester's: queued and racing calls reopen once and keep every key level; an unmount whose reopen is refused leaves no keys held and its row is released by the next reopen; an orphan whose delete was never executed (its prepare's reply lost) is still released by a later reopen; a drop or kill at every message of a reopen and its Commit (each half of the flight in its own test) keeps every live mount's row and all four keys on at most one backend, never resends, and leaves no orphaned row at the end; every message of an uncertain Commit's resolution; after a server restart, and after one lost control-reader reply, a retained Commit still resolves, installed once |
| [`testerwr2_custody.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr2_custody.rs), [`testerwr2_lost_backend.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr2_lost_backend.rs), [`testerwr2_one_call.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr2_one_call.rs), [`testerwr2_reader_resolve.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr2_reader_resolve.rs), [`testerwr2_audit_cost.rs`](../../../../engine/crates/layerfs-store-service/tests/testerwr2_audit_cost.rs) | The second independent round: a registration's second turn and a refused admission's unlock never run on a reopened connection; a lost release or mount reply leaves no owner-less keys and no orphaned row; an orphan is not released while its old backend is stopped, not proven gone (needs `LAYERFS_TEST_POSTGRES_CONTAINER`, NOT_RUN without it); the shapes of the one-call reopen budget; a server restart during a retained Commit's resolution never folds it back or publishes it twice, and one after the lookup leaves the custody retained so a later resolution installs it; the allocator audit's cost (a measurement) |
| [`writer_reopen_release.rs`](../../../../engine/crates/layerfs-store-service/tests/writer_reopen_release.rs) | A release whose flight prepares its statements and loses the prepare's reply quarantines the writer with nothing sent; its row is an orphan the next reopen deletes (keys gone, compaction admitted) |

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
  Commit over its own last Commit reads nothing. Its Store keeps one more,
  the size any of its Workspaces last read or published
  (`Store::last_logical`, #818 W3), so a Workspace bound to the Commit
  another one just published (sequential SDK file calls on one Branch)
  reads nothing either. The bind snapshot carries no size, so the first
  Commit after a mount over any other node still reads it.
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
  quarantined session that does not reopen refuses it (the Store service's
  writer reopens first, [Writer reopen](#writer-reopen)).
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

Construction's base directory reads are grouped (#810 F3,
[Construction](../commit/construction.md#base-directory-reads-810-f3)): the
base root pages a Commit rewrites are read 256 to a demand, the Save locates
a demand's delta chains one level at a time for all its objects, and the
remembered directories' new roots are one demand. A change that touches
many directories therefore costs a few round trips per 256 directories
rather than one or two per directory: on the campaign's shape kit the
"add 10,000 files into 3,942 directories" cell (C4s4) fell from 8,514 to
2,021 construct round trips and "delete `node_modules`" (B2) from 1,681 to
281, with the same roots and stored bytes; what remains there is mostly
inode value-block reads and the base-read batches (32) of the inode lookups.

| Test | Command | Checks |
| --- | --- | --- |
| [`postgresql_commit_round_trips.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_commit_round_trips.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test postgresql_commit_round_trips` | The lane with a head read and `commit_branch` is 12 round trips (15 before), with the head check in the lane's `BEGIN` 11; a moved head and an absent Branch are never answered unmoved |
| [`mount_round_trips.rs`](../../../../engine/crates/layerfs-store-service/tests/mount_round_trips.rs) | see [Mount reads](#mount-reads) | A real Commit's writer: first 23 round trips (54 before stage 2), second 3 (25), UpToDate 2 (1); SQLite 35/5 and 16/1 statements/transactions (46/9, 28/5). A merge Commit is exactly one race refusal and one fallback (`Store::fused_counts`); the Commits around it are fused |
| [`commit_batched_base_files.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_batched_base_files.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-store-service --test commit_batched_base_files` | #810 F5: a Commit's Workspace reader asks the lengths of one captured inode page's changed base files in one demand and fetches their roots in one bounded batch, and reads nothing for a file whose bytes the capture left ([Construction](../commit/construction.md#base-files-of-one-inode-page-810-f5)). 1,100 `chmod`+`touch` files: 0 reader round trips and 0 object or length demands (72–76 round trips and 1,100 + 1,100 demands before); 1,100 in-place edits over two pages: 3 reader round trips, 2 + 2 demands (72–73, 1,100 + 1,100 before); writer unchanged; roots equal to the base's on both backends. `hidden_changes_are_committed`: same size with other bytes, truncate-and-extend and a shrink, each with the mtime set back, are committed; mode- and mtime-only files kept |
| [`commit_fault_base_files_drop.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_fault_base_files_drop.rs), [`commit_fault_base_files_kill.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_fault_base_files_kill.rs) | `… --test commit_fault_base_files_drop`, `… --test commit_fault_base_files_kill` | A dropped connection and a terminated backend at every server message (88) of a Commit that edits six base files and changes four files' mode only (`support/base_file_sweep.rs`): its Workspace reader's one message is the batched prefetch. A second sweep (98 messages) has another Workspace state the files first, so the lengths are remembered in the shared cache and the changes' stats demand nothing on this reader: its Commit sends the batched length demand (one locate) and the prefetch, both swept. The Branch is at its old head or one Commit past it with the intended tree; a reader lost mid-window refuses the Commit and its follow-up; audit, locators and transactions clean |
| [`fused_commit.rs`](../../../../engine/crates/layerfs-persistence/tests/fused_commit.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test fused_commit` | The message is 1 round trip and 1 transaction warm (2 with its prepares); a moved head, an object stored meanwhile and an existing Commit identity are `Fused::Race`; a generation change `StaleStore`; a lane wait past `lock_timeout` `Busy`; a missing epoch row and a removal committed while the message waits `Conflict`; a lost reply `UnknownOutcome` with the session quarantined; a barrier cancelled between its lock and unlock a definite refusal whose failure path releases the leftover (one more round trip; no advisory lock left on the backend, another writer commits). Each refusal wrote nothing (head, locators, packs and hints checked with admin SQL). SQLite: one transaction per Commit, the race and epoch cases |
| [`postgresql_aborted_commit.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_aborted_commit.rs) | `… --test postgresql_aborted_commit` | A body that ignores a refused statement is answered with that refusal, writes nothing and leaves the session usable. Before the `SELECT 1; COMMIT` guard it answered `Ok(())` |
| [`postgresql_lane_liveness.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_lane_liveness.rs) | `… --test postgresql_lane_liveness` | A nested write after its lane connection was terminated fails `Closed` and writes nothing: now through `pg_xact_status` |

### Similar files publish once per wave

A Save used to publish a similar whole file's ring proposal before trying
it, when the proposal was an object the same Save had not published yet: one
write transaction, a reservation and a pack per similar file, about 6 round
trips each, plus a pack body read for every later trial against those packs
(#810). The Save now decides first and publishes such a base only with its
preparation wave
([encoding](../../core/physical-storage/encoding.md)). Measured at
e11d8c8d0f with the #810 shape kit, one sample per cell on this Mac's
Docker loopback (`log_statement` off; host load 10 to 43, so times are
indicative): the Commit's PostgreSQL round trips and packs, with records,
roots and depth histograms unchanged:

| Cell | Change | Round trips before → after | Packs before → after | Stored bytes before → after | Time before → after |
| --- | --- | --- | --- | --- | --- |
| B1 | npm install, 10,002 files | 5,309 → 1,405 | 663 → 141 | 18,651,828 → 18,635,820 | 16.7 s → 3.4 s |
| B6e | overwrite 1,000 ~60 B near-identical files | 4,508 → 891 | 605 → 3 | 106,581 → 87,317 | 13.6 s → 1.8 s |
| B6n | add 1,000 ~60 B similar files | 3,863 → 21 | 552 → 4 | 80,512 → 62,976 | 4.2 s → 0.36 s |
| B4 | add 1,843 build-output files | 222 → 157 | 243 → 241 | 29,264,275 → 29,264,219 | 13.7 s → 1.7 s |
| B6 | add 1,000 similar 0.1–4 KB files | 28 → 28 | 6 → 6 | 239,281 → 239,281 | 0.45 s → 0.43 s |
| A2 | add ten 50 MB files | 395 → 395 | 2,200 → 2,200 | 525,798,458 → 525,798,458 | 7.7 s → 13.3 s (load) |

The round trips left in B1 and B6e are other per-file reads (#810 F1, F3–F5).
[`commit_similar_tiny_files.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_similar_tiny_files.rs)
pins three small shapes' round trips, publications, records and payload
bytes on both backends;
[`commit_fault_similar_drop.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_fault_similar_drop.rs)
and [`commit_fault_similar_kill.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_fault_similar_kill.rs)
drop the connection or kill the backend at each of the 100 server messages of
a warm Commit whose Save holds similar files for that publication.

### Captured Save registration

Captured construction on an admitted Workspace can carry positive inode leaf
and branch locators, compact ancestor coordinates, and validated directories
from its Workspace reader to its Save. One capture owns at most 196 KiB of
retained metadata. The Save uses that metadata only for a complete bounded
group at the same catalog incarnation, after its own pending and held objects
are considered. Every decoded object passes the ordinary authentication.
An incomplete group uses the ordinary locator path. The same Workspace reader
can also pair two ready bounded ordinal requests on its existing connection.
Terminal close disables retained locators before releasing the mount admission.
The retention and pairing end when the captured construction ends.

During captured construction, an eligible Save batch sends one fenced
transaction on the existing writer connection. It binds up to 4 MiB of pack
bodies, with catalog arrays subject to the existing publication row limit.
The flight checks generation, inserts packs, acquires the Store lane, registers
rows, checks the transaction token, and commits. The Commit command is a
separate request in the same flight, so it also closes an aborted transaction.

The row statement checks value and signature integrity on the server and
returns inserted identities. The Save compares any lost identity with the
first writer's bytes before dependent work continues. A lost acknowledgement
keeps the outcome uncertain and quarantines the session. Large batches and
nested lane calls use ordinary publication, selected before sending.
A captured Commit whose binding blocks overflow one transaction may also
publish its held Save tail through this flight before recording the
bindings. That tail settles through the ordinary Save path. Untrusted
completion or rollback and lane cleanup withhold unused allocations. Save
finish and final fused history publication use their respective existing
registration paths.

**Namespace Init's Saves (#818 I3c).** Init's three Saves (prerequisite,
files, tree) are built with `Save::publishing_in_flights`, a flag of their
own apart from captured output: every publication, finish included, goes
through the same flight where eligible (`publish_captured_tail`: an
untrusted completion is uncertain, the ordinary publication's custody) and
through ordinary publication otherwise. No payload is kept, as for any
non-captured Save. A publication then costs one round trip instead of
`BEGIN`, the pack `COPY` (two), the lane and the rows and `COMMIT` (Init
10k: 8 publications, 36 round trips fewer).

After the tail acknowledges, each eligible overflow binding batch can send
its existing sorted block append in one fenced write flight. The continuation
waits for each batch's outcome before advancing and sends final history
publication separately. The ordinary binding encoder and sequence allocation
serve both paths.

[Captured Save flight](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish_flight.rs)

### Commit reads before the lane (#810)

A wide change used to pay one round trip per changed directory, file or
inode page it read before the lane. The reads are now collected first and
sent a window at a time (campaign #810; Lane A fixes). Each demand runs once.
A failed construction window read ends the Commit before history publication
and returns the provider's original error. Rows from earlier acknowledged Save batches
remain in the Store. Each batch takes the lane in its own transaction
before history publication. See
[construction read failures](../commit/construction.md#base-directory-reads-810-f3).
The drop and kill sweeps of a wide Commit
(`commit_fault_wide_{drop,kill}_{0..3}.rs`, `Flight::Wide`) fault every
server message of its flight:

- **Binding hints (F1).** The inode difference walk notes the directories
  whose listing changed (`ChangedDirectories`); their difference then runs
  128 directories at a time, both root pages of each window read in one
  demand (`directory_bindings`, at most 256 pages or 2 MiB), so a window is one
  locate, its delta chains' levels and one pack read instead of one locate
  per page and chain level. A directory the left root lacks is diffed
  against the empty listing, with the same bindings in the same order.
  Logical counting retains up to 4,096 changed directories. Larger changes
  stream another inode difference in bounded directory windows. If hints
  exceed one transaction, the retained prefix is recorded after the Save
  publishes, and the next read starts at the first remaining directory.
  C4s4 (10,000 new files over 3,942 directories): `bindings` 11,403 -> 2,056
  round trips, the same root and stored bytes.

- **One walk, read ahead past the other side (F4).** The logical count and
  the binding hints walked the same two inode tables apart, and the walk's
  read-ahead gave up whenever the other side still held a page above the
  level it opened: every changed leaf was then a point read and a value
  lookup, twice (C4: 1,190 and 1,326 round trips). Now `logical.count`
  walks once (`logical_size_and_directories`) and notes the changed
  directories for `bindings`; and each frontier page carries the largest
  key its parent records, so the walk reads ahead every following page at
  the level whose largest key is at most the key before the other side's
  first unopened higher page (no page of the other side can share its
  identity), both sides in one demand when both expand. It still reads no
  page it would skip, and holds at most 128 pages (1 MiB).
- **The Save's leaf reads (F4).** A Save read of several pooled inode
  leaves (construction's base pages) rebuilt and looked up their values one
  leaf at a time; selection then read each new leaf's stored base again,
  one value lookup each (`pooled_bases`). The Save now rebuilds the leaves
  of a demand ahead, their chains located a level at a time for all of them
  (`read/prefetch.rs`), and looks up their values in one lookup per window
  (`PoolReader::prepare_leaves`: at most 128 leaves, 1 MiB of bodies and the
  unchanged 512 KiB `POOLED_VALUE_CACHE_BYTES` of values); and before a
  wave page's inode leaves are admitted, their stored bases the same way.
  Only leaves already sealed or located are prepared, so preparing asks the
  Store nothing the demand would not.

Measured with the shape kit (PostgreSQL round trips, base `e11d8c8d0f` ->
F1+F4; every root, `stored`, `inserted`, `reused` and `packs` identical on
both backends): C4 4,848 -> 950 (writer 4,122 -> 224; its 726 Workspace
reader round trips are #810 F5's), C4s4 26,734 -> 7,585 (`construct`'s
7,246 are mostly the directory rewrites, #810 F3), B1 5,309 -> 4,621
(publication waves, #810 F2), C1 454 -> 56, C3 823 -> 417 and B5e 897 ->
467 (their remaining are mostly Workspace reader reads, F5); B3 37 -> 13;
A3 18, A5 15, C2 8 and D2 3 unchanged.

The pins are in
[`commit_batched_reads.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_batched_reads.rs):
each scenario's writer round trips and statements for `construct`,
`finish`, `logical.count` and `bindings`, and its Save's `inserted`,
`reused` and `packs`.

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
reply of the message was lost, the separate path would start on a
quarantined writer (a reopen is a new session, never this call's
continuation), so the race is answered as the definite refusal it is
instead.

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
trip), and `Session::pass_lane` first runs
`pg_terminate_backend(pid, lock_timeout)` on every closed connection's
backend still recorded as lost (the session quarantined, or reopened while
one was not yet proven gone), if it is still listed, which answers true
only once the process has exited; a backend proven gone is no longer
recorded, and one not proven gone is `Busy`, and a pid a session lock showed on another backend (a
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
session is quarantined all the same, and that service's next writer call
reopens it, [Writer reopen](#writer-reopen)). The same holds for an overflow's tail and hints and
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
session that does not reopen), except after a `Busy`, when the handle keeps them for the
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
| Captured Commit joined metadata read (#815 Stage 1 experiment) | The Save canonical-object read can append [a whole-pack selection](../../../../engine/crates/layerfs-persistence/sql/postgresql/chain_whole_packs.sql) to a root locate for two or more uncached object IDs. A candidate pack has only requested FULL inode/directory metadata rows, uses ordinary sized or pages framing, and has no resident groups or body. PostgreSQL returns at most 256 KiB of deduplicated whole bodies per root statement. The client verifies each descriptor and whole-pack digest, validates the requested group framing, and checks decoded canonical identities as in the original body path. Pack formats outside the selection, remaining bodies and pooled inode values use their ordinary reads. A failed joined statement ends the attempt. The same Save read barrier and existing connection are used. | [Storage admission](../../../../core/crates/layerfs-storage/src/read/joined.rs) |
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
encodings, charges, refusal order and failures. Each write unit is one
lane-ordered transaction that matches the owner first. Each read unit is one
fenced autocommit statement (`Statement` mode, #818 I3a): its index range
scan is joined `LATERAL` to the owner's live row, so a released or foreign
operation reads no row and is `Stale`, and an empty window answers one row
with a NULL marker; a path window (jobs, directories) sums its rows' sizes
over the rows it read and keeps the leading rows that fit in the same
statement, where a sizing read and a second read did before. One round trip
per read instead of four or five. A schema without the acquisition tables
refuses every unit ([`acquisition/`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition)). The session's epoch is the
same lock-free value as on SQLite ([Acquisition](sqlite.md#acquisition)): a
`begin` waiting for the lane blocks no live owner's unit.

**Write units with no wait (#818 I3b, part C).** `complete_files`,
`set_directory_roots`, `advance`, `discard` and `dispose` send their
transaction as one pipelined flight: the `BEGIN` text (the lane row and
barrier as before) with the generation fence and an attempt token, the owner
check, the unit's statements, the token check and `COMMIT` as a request of
its own. Every check the unit made between statements is a server assertion
before `COMMIT`: a division by zero for `Stale` (a missing owner or charge
row) and for a root window that missed an input position (the client reads
`Changed`, the first input position missing from that window's returned
positions); a `smallint` overflow where the unit read `Integrity` (an unknown
phase, a negative count). A removal job is one statement (data-modifying
CTEs: the entry key, the entry delete, the host-file key, the host-file
delete and the credit); `dispose` adds the release, which deletes the record
only at zero held rows and flags the last record, and the compaction below as
a `DO` block that swallows its own refusal. The first refusal decides; a lost
reply, or a committed reply that does not validate, is `Uncertain` and
quarantines the session; nothing is resent. A session without a generation,
a write nested in the held lane, or a position that does not convert to
signed runs the ordinary transaction instead
([`flights.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/flights.rs)).
A warm unit is one round trip instead of five to thirteen.

**Write units that wait (#818 I3b, part A).** `put_entries` and
`place_children` read a reply before they know what to send next: the D17
dependency check of each window, and a dependent row's `BIND_HOST_FILE`
(its canonical position is the entry's parameter). Each is one transaction
over several flights of one writer turn (`DecidedTurn::prepare_unit`,
`send_in_unit`): the unit's statements are prepared before `BEGIN` in one
flight (the first unit prepares every acquisition write text, so later unit
kinds find theirs cached); each wait ends a flight; every other statement is
queued and sent, in order, at the head of the next flight. A queued
placement (`PLACE_CHILDREN`, `PLACE_ENTRY`) is decoded when its flight
returns, in request order before any later reply, so its `Changed` comes
where the per-statement loop found it. The first flight opens as a flight
above does (lane row, barrier, fence, attempt token, owner assertion); the
last sends what is queued, `CHARGED`, the token check and `COMMIT`
(`place_children` first reads its placements back, since its charge depends
on them). A refusal from the client (`Bounds`, a malformed reply) first sends
what is queued, so an earlier server refusal still wins; a refusal or lost
reply in a flight rolls back (or quarantines) as the session's unit rules
say, and a unit left open is rolled back by the turn's end. Same refusal
order, rows and charges as the ordinary transaction, which still runs when
the session is not eligible
([`unit.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/unit.rs)).
An independent window is two round trips (dependency check; inserts, charge
and `COMMIT`) instead of six; a placement unit of one set window is three.

**Wide-directory placement (#818 I2a).** A `place_children` unit (one write
window: at most 4,096 children or 1 MiB) binds and places its children in
set windows instead of two statements per child. The window rule is shared
with SQLite (`sqlite/acquisition/placement.rs`, I2b) and mirrors `put_entries`'
`independent`: a window ends before a repeated name, position or
(device, inode), and before a child a statement could refuse alone (a
position that does not convert to signed, a host path out of bounds, a bound
child at position 0). A one-child window runs the per-row `BIND_HOST_FILE`
and `PLACE_ENTRY`. A longer window first runs one indexed check for its first
child a stored row could refuse (a stored identity at its canonical position,
a stored directory at its position; D17), which then runs per row after the
children before it; the rest go in one statement (`PLACE_CHILDREN`: the
upsert and the update as data-modifying CTEs over `unnest` arrays, answering
each slot's binding and kind). The first slot with no binding, no placed row
or a kind that disagrees is the loop's `Changed`, and the unit rolls back as
before. A flat directory of 10,000 files places in 3 units of 3 round trips
(part A) instead of 20,000 ([`placement.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/placement.rs)).

```text
 release of the last operation                    one Write transaction
                                                  (a flight: `dispose`)
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
| [`acquisition_placement.rs`](../../../../engine/crates/layerfs-persistence/tests/acquisition_placement.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test acquisition_placement` | Set-window placement gives the per-row loop's first refusal, rows and charges (repeated names and positions, unsigned positions, stored-row dependencies, all-hardlink and large mixed directories), both backends |
| [`postgresql_placement_fault.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_placement_fault.rs) | same, `--test postgresql_placement_fault` | Drop and kill at every server message of a placement unit (72 + 72): the unit is atomic, never replayed |
| [`postgresql_save_flight_fault.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_save_flight_fault.rs) | same, `--test postgresql_save_flight_fault` | A Save publishing in flights stores the ordinary Save's rows in fewer round trips; drop and kill at every server message of its finish (68 + 68): atomic, a dropped reply never a success, never replayed |
| [`postgresql_acquisition_flights.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition_flights.rs) | same, `--test postgresql_acquisition_flights` | The no-wait write units give SQLite's answers and charges (recorded, repeated, missing, filled and unconvertible root positions, every phase, removal budgets, disposal with another operation live and of the last one, stale owners); a warm unit is one round trip and one transaction; drop and kill at every server message of each unit kind (251 + 251): atomic, a dropped reply never a success, never replayed |
| [`postgresql_acquisition_units.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition_units.rs) | same, `--test postgresql_acquisition_units` | `put_entries` over flights gives SQLite's answers, charges and rows (independent windows, aliases in and across units, the first refusal of every tail order: `Changed`, `Bounds`, a stored key, a repeated position; stale owners); a warm independent window is two round trips and one transaction; drop and kill at every server message of three unit kinds (156 + 156): atomic, never replayed |
| [`postgresql_acquisition_reads.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition_reads.rs) | same, `--test postgresql_acquisition_reads` | Every read window, paged under row and byte limits, equals SQLite's; empty, released, missing and foreign owners stay distinct; drop and kill at every server message of each read kind (78 + 78): never a wrong answer, no row changed, never replayed |

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
