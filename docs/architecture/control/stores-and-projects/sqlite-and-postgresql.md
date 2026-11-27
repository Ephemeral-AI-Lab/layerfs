# SQLite and PostgreSQL

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The two Store kinds' own calls: a sqlite Store's file created, moved and
removed across machines, and a PostgreSQL database connected with its
login, attached to machines and opened, within [Stores and projects](README.md).

| | |
| --- | --- |
| Source | [`layerfs-store/src/sqlite/`](../../../../control/crates/layerfs-store/src/sqlite), [`layerfs-store/src/postgresql/`](../../../../control/crates/layerfs-store/src/postgresql), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs), [`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs) |

`sqlite/` and `postgresql/` never import each other; the kind-neutral root
chooses by `StoreKind`. Every sqlite Store opens with the Normal profile
(WAL, synchronous=NORMAL; its record's label is `normal`); Durable execution is
`NOT_RUN — disabled by owner until explicit reauthorization`.

## Create and remove (`sqlite/create.rs`, `sqlite/remove.rs`)

| Call | Does | Refusals |
| --- | --- | --- |
| `store.create <s>` | an empty Store at `store/<s>/store.sqlite` on the machine whose calls run in this process (`layerfs_machine::in_process`, `macos-host` on macOS and `linux-host` on Linux) when no `--machine` is named | `Exists`; `Unsupported` on a host with no such machine |
| `store.remove <s>` | deletes the file wherever it is (here or a worker `store.file remove`), its projects, and any copy an ended move left on the other machine (`op.args.leftover`, so a second remove finishes an interrupted one); clears a gone process's unresolved `op` | `InvalidInput` for PostgreSQL (`store disconnect` keeps its data); `InUse`; `Conflict` if the Store moved meanwhile; a live `op` `Busy` |

## Transfer (`sqlite/transfer/`)

`store.move <s> <machine>` moves a sqlite Store's file from the machine
whose calls run in this process (`macos-host` on macOS and `linux-host` on Linux) to a
machine's worker; `store.move <s> macos-host` (or `linux-host`) brings it back. One side is
always that machine, since the bytes pass through this process: between
two other machines the caller moves the Store to it, then on; each half is
its own operation; on a host with no such machine every move is
`Unsupported`
([`phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs)).

```text
 check     record unblocked; sides; InUse (leases, Sandboxes) under the
           Store lock; both executors reached (stopped -> Unavailable);
           target Stat: no file there (Exists), free >= bytes + WAL +
           max(1 GiB, bytes/10) (Capacity, detail {needed, free});
           write op {from, to}
 snapshot  source: the idle Store's bytes, sha256, heads
 send      Read pieces of <= 8 MiB from source -> Write to target's
           store.sqlite.incoming                     progress {done,total}
 verify    target: the copy's bytes, sha256 and heads are the snapshot's
 install   (op written uncertain first) rename incoming -> store.sqlite
 record    incomplete SQLite project records without an `op` are completed
           read-only from the local `store.sqlite` (`restored::complete_existing`),
           best effort: each one left incomplete is a line of log/repair.jsonl;
           record names the new machine; last_transfer {.., retired:false}
 retire    delete the source's copy; retired: true; op dropped
```

| Step (`StoreStep`) | On the side holding the file | Source |
| --- | --- | --- |
| `Stat` | exists, size, WAL size, partial `incoming` size, free bytes | [`steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `Snapshot` | the idle Store's bytes, sha256 and heads | same |
| `Stage`, `Unstage` | a consistent snapshot of the live Store beside it (`.backup`), for `backup.create` | same |
| `Read`, `Write` | one piece (at most 8 MiB); `Write` offset 0 starts the file again | same |
| `Verify`, `Install` | compare the copy with the snapshot; rename it into place (0600) | same |
| `Retire` | delete a copy the record does not name | same |
| `Remove` | delete the Store's whole directory on the machine | same |

Steps run in the caller's process for the host machine and in the machine's worker
(`store.file`, decoded once into `StepFrame`) for every other machine.
Paths come from the root and the Store's name only. A step that reads,
replaces or deletes the file first closes this process's open copy; a call
running on it is `InUse`
([`step.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/step.rs)).

| Case | What is kept | Cleared by |
| --- | --- | --- |
| a phase fails certain | whatever exists; the refusal names the phase | — |
| `Unknown`, or any failure at `record` (both copies exist) | both copies; `op` uncertain; `log/uncertain.jsonl` | a new `store move`, which first deletes the copy the record does not name |
| a retire that did not finish | the source's copy; `last_transfer.retired: false` | the next `store move` |
| a move that failed certain at `send`, `verify` or `install` | the target's `store.sqlite.incoming`; the record's `incoming_on` names the machine | the next `store move` or `store remove` (doctor reports its bytes) |

The copy the record names is never touched, and nothing is deleted while
another operation holds the Store
([`resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)).

## PostgreSQL (`postgresql/`)

This computer is the one place a PostgreSQL Store's address and login are
saved (owner, 2026-11-24). Machines are attached to the Store and dial with
both; no machine has an address or a login of its own.

```text
 store.connect(name, url, {user, password, synchronous_commit})
   refuse_login(url)          not an address alone, as the client
                              reads it: any `@` or `#`, a user or a
                              password, a URL it does not read
                              (passfile, sslpassword, sslmode=
                              verify-full, sslrootcert), `&` or `#`
                              before a key in its host, database or
                              a value (db&password=) -> InvalidInput
   no record                  new: user and password required, else
                              InvalidInput; synchronous_commit off |
                              local (default) | remote_write | on |
                              remote_apply, else InvalidInput
     Exists?                  same name, or (once read) same schema
                              binding, whatever address or login
     schema `layerfs`         empty database: create it, new binding;
                              existing: read its binding
     projects in the schema   --> project records; record written under
                              the home lock (re-checked), then the
                              password saved            outcome connected
   sqlite record              Exists
   PostgreSQL record          update: user/password given or saved; the
                              server must hold the same binding; record
                              and password written     outcome updated
```

| Call | Does | Refusals | Source |
| --- | --- | --- | --- |
| `store.connect` | records the database as a Store in the fixed `layerfs` schema: the URL exactly as given (an address alone: no user or password), `user`, schema, `synchronous_commit`, binding, a cursor key of this home's own, no machine; the password goes to the home's secrets as kind `login` under the Store's name. On a connected Store it changes the address, user, password or `synchronous_commit`, an omitted user or password keeping the saved one; the result is checked from this process against the same binding, and a new address, user or password then from each attached machine (`check_from`; one that fails refuses `Conflict` with `machines` and a detach `fix`), before the record and password are written, and the Store's open connections in this process are closed. `dry_run` makes the checks only and answers `tested` (`here`, `machines`); every refused check names its `step` | `InvalidInput`, `Exists`, `Unavailable`, `Denied`, `Conflict` | [`connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `connect` |
| `store.disconnect` | forgets the Store, its saved password and its attachments; its data stays | `InvalidInput` for sqlite (`store remove`); `InUse` while leased or a Sandbox uses it | same |
| `store.attach <s> --machine <m>` | checks the Store from the machine with the saved URL and login (in this process on a locally executing machine, else through the worker's `store.reach`): the server answers, its schema holds this Store, and the role has what a Sandbox's reads and writes need (the schema's `USAGE` and `CREATE`, the database's `TEMP`, each table's and sequence's privileges; never what only a compaction needs, which `store compact` checks itself); the machine's platform must run PostgreSQL Sandboxes. Only then is `<m>` added to the record's `attached` list and its reach recorded; an attached machine is checked again (`checked`). `dry_run` checks, takes no lock and records nothing (`tested`); a refused check names its `step` and `machine` | `InvalidInput` (sqlite), `NotFound`, `Unsupported`, `Unavailable`, `Denied`, `Conflict` | [`check.rs`](../../../../control/crates/layerfs-store/src/postgresql/check.rs) `attach`, [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `attach` |
| `store.detach <s> --machine <m>` | removes `<m>` from `attached` and forgets its reach, under the home lock and the Store's, once no Sandbox of the Store's projects is on it | `InUse` (counted and named, with the removals as `fix`), `NotFound` (not attached), `InvalidInput` | [`check.rs`](../../../../control/crates/layerfs-store/src/postgresql/check.rs) `detach`, [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle_on` |
| Store calls | open the schema in this process with the URL resolved now | — | [`open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| Host read session | one read-only session per process for Sandbox host operations: a create's reach check and a status's head read, on every route; opened with the light check on first read, checked with one empty query before each use (a dead one is replaced, not retried), closed after 60 s unused | — | [`reader.rs`](../../../../control/crates/layerfs-store/src/postgresql/reader.rs) |

A PostgreSQL Store is on no machine. Its Sandboxes run on every attached
machine whose platform runs PostgreSQL Sandboxes (#536 C4). This computer
is the control process (home, records, secrets, the saved connection); it
is never a machine. The host machine (`macos-host` or `linux-host`) is the built-in machine whose platform
runs Sandboxes directly on the host OS; what it allows (a shared loopback,
a local executor) comes from its platform, not its name, and it runs a
Store's Sandboxes only once attached. `store connect` attaches nothing.
With no `--machine`, `sandbox.create` uses the one attached machine (none
attached is refused with the attach as its fix; several are listed in
`next.machines` and one must be named). `sandbox.create` on a machine
that is not attached is refused
`InvalidInput` with `layerfs store attach <s> --machine <m>` as its fix, and
on an attached one still checks from the machine that it reaches this Store.
A record is read as it is: there is no migration and no state for a record
written by an earlier build (owner, 2026-11-19 and 2026-11-24: no backward
compatibility in 2.0). A record without the fields above fails through the
normal path: a call that dials it needs its `user` and the saved password
(`login::dial`), and is refused `InvalidInput` with `layerfs store connect
<s> <url> --password-stdin` as its fix when this home holds none. A URL is
never cleaned, stripped or rewritten (the e0aa26e149 review's Decision,
owner-confirmed 2026-11-24): `store connect` writes the address given, and
`store attach`, `store detach` and `machine remove` change only the
`attached` list.

A Store's URL is read only as its client reads it
([`postgresql/address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs),
`postgres::Config`, tokio-postgres 0.7's `UrlParser`, which
`layerfs-persistence` re-exports as `PostgresConfig`); LayerFS keeps no
parser of its own and no rewriting. A URL holds a login
(`address::holds_login`) when it has any `@` after its scheme, when its
client reads a user or a password in it, or a host holding an `@` (a login
written percent-encoded, `postgres://alice%3Ap%40db/app`, which the client
reads as the host `alice:p@db`; no real host holds an `@`), when its client
does not read it (what it does not read cannot be told apart), or when its
percent-decoded text holds, in any case, a login key followed by `=`
anywhere: `user`, `password` (the client's), `passfile`, `sslkey`,
`sslcert`, `sslpassword`, `oauth_client_secret`, `scram_client_key` and
`scram_server_key` (libpq 18's keys of a password, a password file, a
client key or certificate and its password, an OAuth client secret, and a
SCRAM client or server key; a ClientKey alone authenticates as the role).
The key list is fixed, with no grammar guessed:
`postgres://h/db?application_name=me?password=p` and
`postgres://h:5432/db/password=p@ss` hold one. Such a URL is shown as
`postgresql://<hidden: not an address alone>` with no `server`, never
dialled and never rewritten. Any other URL is shown and stored exactly as
written (`address::shown`), and `target` (a row's `server`, a refusal's
host) is its client's hosts, ports and database. Text the client reads as
part of a host, database or value and that names no login key is the
client's: `postgres://h/db&sslmode=require` is the database
`db&sslmode=require`, accepted, and refused by the server if there is no
such database.
`machine.remove` detaches the machine from every Store and forgets its reach.

### Who connects

What was seen when a side dialled the database is kept beside the record,
in `store/<s>/observed.json` with the `at` a live read writes:
`here` (this computer) and `reach.<machine>`, each `{ at, ok, latency_ms,
server_version, error }`
([`observed.rs`](../../../../control/crates/layerfs-store/src/observed.rs)).
Writers: `store.connect` (this computer's, from its own check once saved,
first connect or update), `store.attach` (its check), `store.doctor` (this computer, measured
with the same check, then every attached machine, or `--machine` alone) and
a Sandbox create on a machine (`{ at, ok: true }`, on the host after the
reply, [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)
`reach`). `here` is this computer, the process that holds the connection;
the host machine's reach is `reach.macos-host` or `reach.linux-host`, like any machine's, and neither
is read as the other. A
writer runs after its Store I/O, under the Store's lock, and writes nothing
once the record is gone; a write never fails its call and moves no
`generation`. `connect.rs` reads it back: `reach` (this computer's) and
`machines`, one `{ machine, reached, reach }` per attached machine
(`store get`; `store list` and `home get --with tree` give the names).
Nothing converts a sqlite Store into PostgreSQL: a project reaches
PostgreSQL only by an Init into it.

### Credentials

A PostgreSQL Store's record holds its `user`; its password is a home secret
of kind `login` under the Store's name (`secret/login/<store>.json`,
`{ password, changed }`, mode `0600` in a `0700` folder;
[`login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs)).
`login::dial` puts both into the address it dials (`address::with_login`),
from this computer and for every attached machine alike. Only an address
alone is dialled (`refuse_login`, below). `with_login` reads the URL it
makes back with the client and refuses it, undialled, unless the client
reads exactly the user and password and the address's hosts, host
addresses, ports and database. A record that names no `user` is refused
`InvalidInput` naming the missing field; a password this home does not hold
is refused `InvalidInput` with `layerfs store connect <store> <url>
--password-stdin` as its fix (`login::password_fix`). `store get` answers
`password_saved` and `password_changed_at`, never the password.

```text
 Store record: url postgres://h/db  user <user>  attached [m, ...]
 secret/login/<store>.json: password, changed (0600)
      |  each call, in the caller only
      v
 address::with_login -> <user>:<percent-encoded password>@h/db  (memory,
                         read back by the client before it is dialled)
      v
 machine reach(spec, url) -> postgresql field of ONE request
      |  host machine: same process; else worker sandbox.host, whose decode
      |  drops url and password from the Store record it keeps
      v
 DaemonStore::Postgres in the daemon setup -> <sandbox>/config/daemon.setup
                                              0600 in a 0700 folder
```

| Rule | Source |
| --- | --- |
| The login is a saved one, and only an address alone is dialled or recorded: `refuse_login` refuses `InvalidInput` a URL that holds a login (`holds_login`, above: an `@`, a user or password the client reads, a login key and `=`, or one the client does not read, such as an unknown key like `passfile`, `sslpassword`, `sslrootcert`, `sslkey` or `sslcert`, an `sslmode` other than `disable`, `prefer` or `require`, or a bad value) or holding a `#` (the client reads it as text, other readers end the URL there); an `@` or `#` of the address itself is written `%40` or `%23`, which the client decodes. A refusal of a URL the client does not read names what it refuses only by a key or value from fixed lists (`sslmode=verify-full`, `sslrootcert`, the value of `port`), never by other URL text (a value the client would read but for a `#` in it, `sslmode=require#f`, is named by that `#`; `sslrootcert` adds where a private CA is trusted: on Linux, where `native-tls` is OpenSSL, in the system's trusted roots, or in `SSL_CERT_FILE`, which `openssl-probe` reads in the process that dials only: a Sandbox's daemon starts with a cleared environment; on macOS, where it is the Security framework, which reads no such variable, in the login or system keychain); `sslmode=require` verifies the server's certificate chain and host name (the engine's `native-tls` connector, with its defaults), so `verify-ca` and `verify-full` are refused rather than dialled; any refusal of a URL holding a `#` names `%23`. The refusal's fix (`connect::refuse_url`) names `<url>` and `<user>`, never URL text: a URL that holds a login gets `layerfs store connect <s> <url> --user <user> --password-stdin` on a first connect or when it holds a login, and one refused only for a `#` on a connected Store gets the connect of the address alone. A password not saved is `InvalidInput` with `login::password_fix` | [`postgresql/address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs), [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs) `dial` |
| Every byte of the user and password outside `A-Z a-z 0-9 - . _ ~` is percent-encoded | [`postgresql/address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs) `percent` |
| Put in again on each call; the full URL is never written to a record, log, trace or `Debug` output | same; [`host/frame.rs`](../../../../control/crates/layerfs-sandbox/src/host/frame.rs) `decode` |
| The daemon runs `--config <setup file>`; the setup is in neither its arguments nor its environment; `DaemonStore` prints `url` as `[redacted]` | [`shared/daemon.rs`](../../../../control/crates/layerfs-sandbox-backend/src/shared/daemon.rs), [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs) |

#### Held sessions in the host worker (#819 P-B)

A Store record written by `store connect` (new or updated) carries
`login_revision`: 16 random bytes in hex, not a secret, new on every such
write and never reused. The CLI's `file.read`, `file.stat` and `file.list`
send the saved password with that revision to the host worker
(`store.postgresql_read`). Under the Store's lock the worker re-reads the
record and refuses `Conflict` unless the revision matches, the record is
unblocked and the host is admitted. It then takes a held read-only session
([`postgresql/pool.rs`](../../../../control/crates/layerfs-store/src/postgresql/pool.rs)),
probing it if it was idle, and only with the session in hand takes the
call's lease, under the Store's lock with the record checked again; it runs
the call once and drops the lease when the call ends. A read waiting for a
session, or for its probe, holds no lease, so `store disconnect` and
`store upgrade` are never held off by a read that has not started.

| Rule | Value |
| --- | --- |
| Session identity | Store key, login revision and the address dialled with the sent password. Revisions have no order; the record does: a call naming another revision than the sessions' is compared, under the Store's lock, with the record's current revision. The current one replaces the old identity (idle sessions close, lent ones close when they come back); any other is refused `Conflict` with nothing run and the sessions untouched, so a late call admitted before a rotation never displaces the newer login |
| Retire | `store connect` (update) and `store disconnect` send `store.postgresql_retire { store, revision }` naming the revision they replaced or removed, past their own locks, to a running worker only. The worker never retires the record's current revision; a lost retire changes nothing, since the record comparison refuses the old revision anyway |
| Before reuse | one Light check within 5 s on one held session ([engine](../../engine/store/postgresql.md)). If it fails, every idle session of the Store closes and the call opens exactly one new session. This is a route decision taken before any byte of the call's operation is sent, as a mount reader's `usable()` is: nothing is retried |
| After the call | `Unknown` or `StaleStore` drops the session; any other outcome keeps it. A guard gives the session's slot back on every path, a panic included |
| Bounds | 4 sessions per Store and 8 per worker process, idle, lent and reserved (owner, 2026-11-27); an idle session of another Store closes to make room; idle ones close after the open-Store reaper's idle time, and on drain. A read starts only once it holds one of its Store's slots (a reservation, taken without waiting); nothing waits inside the pool. The slot is taken before the read's record is checked, so a read refused there may have closed another Store's idle session, which that Store's next read replaces with one login |
| Threads | an ordinary thread reads the request, checks it and hands a PostgreSQL read to the worker's own PostgreSQL lane (8 threads, up to 64 reads handed over and unanswered), so PostgreSQL reads never hold an ordinary thread or slot. A read its request alone refuses (another worker generation, an invalid request ID, a method the route does not serve, arguments `serve` would refuse) is answered on the ordinary thread with the refusal the lane would give, and takes no lane place or slot; a handed-over request keeps only the fields a host request is read by, so each is small ([per-lane memory](../machines/workers-and-reach.md#host-worker-exchange-20-implementation-plan)). A lane thread starts the oldest queued read whose Store has a free slot. A read of a busy Store waits in the queue, on no thread, and a Store whose sessions are stuck never holds the threads another Store's reads need. Lane threads sleep until a read is queued or one ends; nothing wakes them on a timer. Queued reads whose caller has closed its socket (end of file, hang-up or error; bytes the caller sends are not a close) are dropped unrun before any queued read starts and whenever a read is handed over |
| Full lane | at 64 unanswered reads the ordinary thread answers the read at once `Unavailable` (`detail { state: postgresql-reads-full, reads: 64 }`), naming the load, and never waits on the PostgreSQL lane. The answer comes after the request was read; §8.5 allows that because nothing of the read ran |
| Password lifetime | the worker reads no secret from its environment or the home and writes none. One helper ([`layerfs_home::wipe`](../../../../control/crates/layerfs-home/src/wipe.rs)) overwrites a buffer's whole allocation, spare capacity included, between compiler fences; control forbids `unsafe` code, so these are ordinary writes that `black_box` makes likely to stay, not guaranteed as `write_volatile` would be. Overwritten before they are freed: a host frame that carries a login (`args.password`), once decoded or sent, and the CLI's trial frame that checks the request fits, in the worker and in the CLI; the decoded request in the worker once answered, refused or dropped unrun, and the CLI's request value on every path; the address dialled with the password, built in a buffer sized so it never grows, once the call ends or when it is refused; the pool's copy of a Store's address when its identity is replaced or its last session closes; the URL handed to a session's open once it is open; the saved login `store connect` kept to put back, the password a connect was given and the address it dialled, the saved password a call read, and the copy and text of a saved login as it is written or read. Still holding a copy, not overwritten: the PostgreSQL client's parsed settings (the check that reads the dialled address back, and each open session's, kept while it lives, as reconnects within it need them) and its authentication exchange; JSON printing and parsing internals; and freed allocator memory of those. A core dump cannot hold any of them (below) |
| Core dumps | the host worker sets `RLIMIT_CORE` to 0 before it reads its first request; on Linux it also clears `PR_SET_DUMPABLE` as soon as it decodes its first PostgreSQL request, before any session holds the password, so another process of the user cannot read or trace it from then on. The request frame holding that first password is decoded just before; setting the flag at start would hide every worker's environment from the check that proves it cleared. macOS has no non-dumpable flag: there only the core file limit is set, and a debugger of the same user can still attach |
| Upgrade | `store upgrade` renews the login revision when it writes its `op` (§8.7) and sends the worker `store.postgresql_retire` for the revision it replaced, past its locks. While `op` stands the worker admits no PostgreSQL read; a call admitted at the old revision is refused `Conflict`, and no session held before the upgrade serves after it. A record with no revision gets none and stays direct |
| Ping | the worker's ping and `machine doctor`'s ping evidence report `postgresql { stores, idle, lent, writes, reads, queued }`: `lent` counts slots reserved or lent, `writes` the write slots among them, `reads` the calls (reads and single-change writes) handed over and unanswered, `queued` those not yet started |
| Login order | `store connect` saves a new password before it writes the record with the new revision, so a revision is never visible without its password. When the record cannot be written, an update puts the saved login it replaced back and a first connect removes the one it saved; a put-back or removal that fails is said in the refusal |
| Records without a revision | 2.0 keeps no compatibility path for them: such a record stays on the direct route (P-A) until `store connect` is run again |

#### Held write sessions (#819 P-B writes)

The CLI's file writes (`write`, `edit`, `mkdir`, `rm`, `cp`, `mv`, `apply`)
on a PostgreSQL Store whose record holds a login revision go to the host
worker as `store.write`, the request the SQLite host route sends, with the
record's `login_revision` and the saved password added
([`host_write.rs`](../../../../control/crates/layerfs-store/src/host_write.rs)).
The route is chosen before anything is sent, under the same conditions as
a read: the CLI, the host machine not stopped, the record unblocked and
the request within the host envelope; any other write stays direct (P-A).
The worker admits a write as it admits a read (the record re-read under
the Store's lock, the revision compared, the lease taken only once a
session is in hand) and runs it once on a held write session
([`host_postgresql.rs`](../../../../control/crates/layerfs-store/src/host_postgresql.rs)
`serve_write`).

| Rule | Value |
| --- | --- |
| Session | a writable Store service (`StoreService::open_postgres`): its writer (reopen enabled, #817), its lane connection, its control reader and the Workspace readers its calls' private Workspaces open and hand back, with its own private overlay directory. A session is of one kind: a read session never serves a write, nor a write session a read. It is lent to one call at a time; closing it stops its overlay owner and removes its directory |
| Bounds | the same 4 sessions per Store and 8 per process, both kinds counted, idle, lent and reserved. Write slots lent at once are at most 2 per Store and 4 per process (half of each bound), so reads always keep the other half; an idle session of the other kind, of this Store or another, closes to make room |
| Before reuse | the same one Light check, on the write session's writer (`postgres_session_current`). A failure closes every idle session of the Store, both kinds, and the call opens one new write session: a route decision before any byte of the write is sent |
| After the call | `Unknown` or `StaleStore` drops the write session; any other outcome keeps it. An uncertain outcome is recorded before its answer, as on the SQLite route ([host worker exchange](../machines/workers-and-reach.md#host-worker-exchange-20-implementation-plan)) |
| Single-change writes | read by an ordinary thread, their header checked there (`host_write::header`), and handed to the PostgreSQL lane, as a read is: they never run on an ordinary thread, so one waiting on a held Store writer (the publication lane, up to its `lock_timeout`, 30 s) holds no ordinary thread. The lane starts one only when its Store and the process have a free write slot, so at most 4 lane threads wait on writers and the other 4 keep serving reads |
| Streamed writes | `write` and an unscoped `apply` come in on the write socket and wait for a write thread as on the SQLite route; a write thread starts the oldest waiting write that can start: a SQLite write, or a PostgreSQL write whose Store has a free write slot, taken then. One that has none waits with nothing sent but its request, and later writes start meanwhile |
| Outcome boundary | unchanged from the SQLite route: a worker lost before the input's end is sent leaves the write `Unavailable`, nothing committed; after the end, `Unknown`. A session lost in flight (a server restart, a terminated backend) is `Unknown` for that write alone; waiting writes run once on new sessions, or are refused before anything is sent when no session can be opened. Nothing is resent |
| Password copies | the CLI's request value and trial frame as for a read; the encoded request frame once sent on the write socket; the worker's decoded request and each held write's arguments once answered or dropped. A write session's reader factory keeps one copy of the dialled address for the readers it opens, while the session lives |
| Connections | one write session holds its writer and, while used, its lane connection and control reader, plus the readers its calls open and keep idle; up to 4 write sessions per process (8 with idle ones) on top of the read sessions. All of them count against the server's `max_connections` |

Memory: a write session keeps its Store service's caches (64 KiB of
immutable objects) and, while a call runs, that call's private overlay
(4 MiB page cache); a write's streamed input is read in bounded frames as
on the SQLite route. No buffer grows with the file, the list of changes or
the number of writes.

The URL is never rewritten for a machine; which URLs a machine refuses is
[Reach](../machines/workers-and-reach.md#reach-reachrs).

## Related

- [Stores and projects](README.md): records, routes and leases.
- [Machines: workers and reach](../machines/workers-and-reach.md): `store.file`, `store.reach` and the loopback refusal.
- [Backups](../backups/README.md): `Stage` and `Unstage` for `backup.create`.
- [Store: SQLite](../../engine/store/sqlite.md), [Store: PostgreSQL](../../engine/store/postgresql.md): what the engine opens.
- [API: store](../../api/sdk/store.md).
