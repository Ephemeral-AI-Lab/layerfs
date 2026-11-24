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
| `store.create <s>` | an empty Store at `store/<s>/store.sqlite` on `native` | `Exists` |
| `store.remove <s>` | deletes the file wherever it is (here or a worker `store.file remove`), its projects, and any copy an ended move left on the other machine (`op.args.leftover`, so a second remove finishes an interrupted one); clears a gone process's unresolved `op` | `InvalidInput` for PostgreSQL (`store disconnect` keeps its data); `InUse`; `Conflict` if the Store moved meanwhile; a live `op` `Busy` |

## Transfer (`sqlite/transfer/`)

`store.move <s> <machine>` moves a sqlite Store's file from `native` to a
machine's worker; `store.move <s> native` brings it back. One side is always `native`: between two
machines the caller moves the Store to `native`, then on; each half is its
own operation
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

Steps run in the caller's process for `native` and in the machine's worker
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
| `store.connect` | records the database as a Store in the fixed `layerfs` schema: the URL exactly as given (an address alone: no user or password), `user`, schema, `synchronous_commit`, binding, a cursor key of this home's own, no machine; the password goes to the home's secrets as kind `login` under the Store's name. On a connected Store it changes the address, user, password or `synchronous_commit`, an omitted user or password keeping the saved one; the result is checked from this process against the same binding before the record and password are written, and the Store's open connections in this process are closed | `InvalidInput`, `Exists`, `Unavailable`, `Denied`, `Conflict` | [`connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `connect` |
| `store.disconnect` | forgets the Store, its saved password and its attachments; its data stays | `InvalidInput` for sqlite (`store remove`); `InUse` while leased or a Sandbox uses it | same |
| `store.attach <s> --machine <m>` | checks the Store from the machine with the saved URL and login (in this process on a locally executing machine, else through the worker's `store.reach`): the server answers, its schema holds this Store, and the role has what a Sandbox's reads and writes need (the schema's `USAGE` and `CREATE`, the database's `TEMP`, each table's and sequence's privileges; never what only a compaction needs, which `store compact` checks itself); the machine's platform must run PostgreSQL Sandboxes. Only then is `<m>` added to the record's `attached` list and its reach recorded; an attached machine is checked again (`checked`) | `InvalidInput` (sqlite), `NotFound`, `Unsupported`, `Unavailable`, `Denied`, `Conflict` | [`check.rs`](../../../../control/crates/layerfs-store/src/postgresql/check.rs) `attach`, [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `attach` |
| `store.detach <s> --machine <m>` | removes `<m>` from `attached` and forgets its reach, under the home lock and the Store's, once no Sandbox of the Store's projects is on it | `InUse` (counted and named, with the removals as `fix`), `NotFound` (not attached), `InvalidInput` | [`check.rs`](../../../../control/crates/layerfs-store/src/postgresql/check.rs) `detach`, [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle_on` |
| Store calls | open the schema in this process with the URL resolved now | — | [`open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| Host read session | one read-only session per process for Sandbox host operations: a create's reach check and a status's head read, on every route; opened with the light check on first read, checked with one empty query before each use (a dead one is replaced, not retried), closed after 60 s unused | — | [`reader.rs`](../../../../control/crates/layerfs-store/src/postgresql/reader.rs) |

A PostgreSQL Store is on no machine. Its Sandboxes run on every attached
machine whose platform runs PostgreSQL Sandboxes (#536 C4). This computer
is the control process (home, records, secrets, the saved connection); it
is never a machine. Machine native is the built-in machine whose platform
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
client reads a user or a password in it, when its client does not read it
(what it does not read cannot be told apart), or when its percent-decoded
text holds, in any case, a login key followed by `=` anywhere: `user`,
`password` (the client's), `passfile`, `sslkey`, `sslcert`,
`sslpassword` and `oauth_client_secret` (libpq 18's keys of a password, a
password file, a client key or certificate and its password, and an OAuth
client secret). The key list is fixed, with no grammar guessed:
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
machine native's reach is `reach.native`, like any machine's, and neither
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
      |  native: same process; else worker sandbox.host, whose decode
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
| The daemon runs `--config <setup file>`; the setup is in neither its arguments nor its environment; `DaemonStore` prints `url` as `[redacted]` | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs), [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs) |

The URL is never rewritten for a machine; which URLs a machine refuses is
[Reach](../machines/workers-and-reach.md#reach-reachrs).

## Related

- [Stores and projects](README.md): records, routes and leases.
- [Machines: workers and reach](../machines/workers-and-reach.md): `store.file`, `store.reach` and the loopback refusal.
- [Backups](../backups/README.md): `Stage` and `Unstage` for `backup.create`.
- [Store: SQLite](../../engine/store/sqlite.md), [Store: PostgreSQL](../../engine/store/postgresql.md): what the engine opens.
- [API: store](../../api/sdk/store.md).
