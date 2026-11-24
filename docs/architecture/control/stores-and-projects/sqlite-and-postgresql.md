# SQLite and PostgreSQL

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The two Store kinds' own calls: a sqlite Store's file created, moved and
removed across machines, and a PostgreSQL database connected, updated,
checked from a machine and opened, within [Stores and projects](README.md).

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
| `store.create <s>` | an empty Store at `store/<s>/store.sqlite` on the machine whose calls run in this process (`layerfs_machine::in_process`, `native` on macOS and Linux) when no `--machine` is named | `Exists`; `Unsupported` on a host with no such machine |
| `store.remove <s>` | deletes the file wherever it is (here or a worker `store.file remove`), its projects, and any copy an ended move left on the other machine (`op.args.leftover`, so a second remove finishes an interrupted one); clears a gone process's unresolved `op` | `InvalidInput` for PostgreSQL (`store disconnect` keeps its data); `InUse`; `Conflict` if the Store moved meanwhile; a live `op` `Busy` |

## Transfer (`sqlite/transfer/`)

`store.move <s> <machine>` moves a sqlite Store's file from the machine
whose calls run in this process (`native` on macOS and Linux) to a
machine's worker; `store.move <s> native` brings it back. One side is
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

```text
 store.connect(name, url, {auth, synchronous_commit})
   refuse_login(url)          not an address alone, as the client
                              reads it: any `@` or `#`, a user or a
                              password, a URL it does not read
                              (passfile, sslpassword, sslmode=
                              verify-full, sslrootcert), `&` or `#`
                              before a key in its host, database or
                              a value (db&password=) -> InvalidInput
   auth                       required, and saved (`store save-auth`):
                              else InvalidInput / NotFound
   synchronous_commit         off | local (default) | remote_write | on
                              | remote_apply, else InvalidInput
   Exists?                    same name, or (once read) same schema
                              binding, whatever address or login
   schema `layerfs`           empty database: create it, new binding;
                              existing: read its binding
   projects in the schema --> project records; record written under
                              the home lock (re-checked)
```

| Call | Does | Refusals | Source |
| --- | --- | --- | --- |
| `store.connect` | records the database as a Store in the fixed `layerfs` schema: the URL exactly as given (an address alone: no user or password), schema, `synchronous_commit`, its saved login `auth`, binding, a cursor key of this home's own, no machine. On a connected Store the URL becomes the address every machine dials (`readdress`); with `--machine <m>` it is that machine's own, `machines.<m>: {url}`, checked from the side that dials (this process, or the worker's `store.reach`): the server answers, its schema holds this Store, and the role has what a Sandbox's reads and writes need (the schema's `USAGE` and `CREATE`, the database's `TEMP`, each table's and sequence's privileges; never what only a compaction needs, which `store compact` checks itself), and the machine's platform must run PostgreSQL Sandboxes. Both refuse `--auth` and `--synchronous-commit` | `InvalidInput`, `Exists`, `NotFound` (no such saved login), `Unavailable`, `Unsupported` (`--machine` on a platform that runs no PostgreSQL Sandbox), `Denied`, `Conflict` | [`connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `connect` |
| `store.disconnect` | forgets the Store; its data stays. With `--machine <m>`, removes that machine's own address, which then dials the Store's, and forgets its reach | `InvalidInput` for sqlite (`store remove`); `InUse` while leased or a Sandbox uses it; `NotFound` when the machine has neither an address of its own nor a reach | same |
| `store.update <s> {auth, synchronous_commit}` | another saved login or `synchronous_commit` (one is required); a new login is checked once from this process on the Store's address, which must hold this Store, and saved again under the home lock; the write drops old fields (`StoreRecord::without_old_fields`) and never rewrites a URL: a record keeping a URL that is not an address alone is refused with its fix, nothing written (`login::update_fix`) | `InvalidInput`; `Unavailable` (not reached, or another Store's schema); `Conflict` if the record changed meanwhile | [`update.rs`](../../../../control/crates/layerfs-store/src/postgresql/update.rs) |
| Store calls | open the schema in this process with the URL resolved now | — | [`open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| Host read session | one read-only session per process for Sandbox host operations: a create's reach check and a status's head read, on every route; opened with the light check on first read, checked with one empty query before each use (a dead one is replaced, not retried), closed after 60 s unused | — | [`reader.rs`](../../../../control/crates/layerfs-store/src/postgresql/reader.rs) |

A PostgreSQL Store is on no machine. Its Sandboxes run on every machine
whose platform runs PostgreSQL Sandboxes (#536 C4), and `sandbox.create`
checks from the machine that it reaches this Store, so no call comes
between `store.connect` and the first Sandbox. `machines` holds only the
machines with a connection of their own (H9). A record holding what 2.0 no
longer writes (a `password`, a URL that is not an address alone, a
`machines.<m>` entry without `url` or with more, such as an old `checked`)
reads `broken` `outdated` (`StoreRecord::outdated_fields`) with a fix the
user runs; nothing migrates it (owner, 2026-11-19: no backward
compatibility in 2.0). A URL is never cleaned, stripped or rewritten (the
e0aa26e149 review's Decision, owner-confirmed 2026-11-24: five rounds of a grammar
of LayerFS's own each mishandled a new shape). A record keeps a URL that
may hold a login, and any password in it, on disk byte for byte until its
fix runs (the one exception to "no password in a Store record", for
records written before 2.0's rules); only a connect of an address alone
replaces it. The fixes (`login::fix_of`, `login::update_fix`) dial only
with the login they name, never with the one the record names, so they
work when that login no longer authenticates, and name the address
`<url>` and the user `<user>`, never text of a URL:

| The record | Its fix |
| --- | --- |
| only old fields, every URL an address alone | `store update <s> --auth <auth>` (the record's own login, or `<auth>` with `save-auth` first when it names none or one not saved); its write drops them (`StoreRecord::without_old_fields`) |
| a machine's own URL is not an address alone | `store disconnect <s> --machine <m>` (dials nothing) `&& store update <s> --auth <auth> && store connect <s> <url> --machine <m>` (checked from that machine, which must be running and run PostgreSQL Sandboxes) |
| the Store's URL is not an address alone | `store disconnect <s> && store connect <s> <url> --auth <auth> --synchronous-commit <recorded>`, then `store connect <s> <url> --machine <m>` for every machine with an address of its own: the Store is forgotten and connected again (its data stays; refused `InUse` while a Sandbox of it exists; its projects are found again, while its backup links and reach observations are not kept) |

The `outdated` fix names `<auth>` when it replaces a URL (the record's own
login, which `store get` names, or another that authenticates). `store
update <s> --auth <new>` (or `--synchronous-commit` alone) refuses a record
keeping a URL that is not an address alone and writes nothing; its fix is
the one above naming `<new>` and carrying a given `--synchronous-commit`
(review e0aa26e149 S3 and N1). A `store connect` refused for a URL holding
a login names the saved login first: `save-auth <auth> --user <user>
--password-stdin`, then on a connected Store the steps above that leave it
naming `<auth>` (`store update <s> --auth <auth>` when every URL is an
address alone), then `store connect <s> <url>` (`--machine <m>`) unless a
step already connects it (`login::login_fix`). No write but `store
update` drops an old field, and none rewrites a URL: `store connect`
writes the address given, `store connect --machine`, `store disconnect
--machine` and `machine remove` change only that machine's entry
(`StoreRecord::with_own_connection`), every other entry byte for byte.

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
`machine.remove` forgets the machine's entry and its reach in every Store.

### Who connects (spec v23)

What was seen when a side dialled the database is kept beside the record,
in `store/<s>/observed.json` with the `at` a live read writes:
`here` (this computer) and `reach.<machine>`, each `{ at, ok, latency_ms,
server_version, error }`
([`observed.rs`](../../../../control/crates/layerfs-store/src/observed.rs)).
Writers: `store.connect --machine` (its check), `store.doctor` (this
computer, measured with the same check, then every machine with an address
of its own, a Sandbox, or `--machine`) and a Sandbox create on a machine
(`{ at, ok: true }`, on the host after the reply,
[`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)
`reach`). `native` with no address of its own dials as this computer and is
recorded as `here`. A writer runs after its Store I/O, under the Store's
lock, and writes nothing once the record is gone; a write never fails its
call and moves no `generation`. `connect.rs` `connections` reads it back:
this computer, then every machine with an address of its own, a reach or a
Sandbox (handed in by `layerfs-control`), each `{ machine, url, own, server,
reached, reach }`. Nothing converts a sqlite Store into PostgreSQL: a project
reaches PostgreSQL only by an Init into it.

### Credentials

A PostgreSQL Store's record names a saved login (`auth`), which
`postgresql/login.rs` `dial` puts into the address it dials
(`address::with_login`); a machine's own address (`machines.<m>.url`) is
dialled with the same login. Only an address alone is dialled
(`refuse_login`, below); a recorded URL that is not one (an older
record's) is refused `InvalidInput` with the record's fix, never dialled.
`with_login` reads the URL it makes back with the client and refuses it,
undialled, unless the client reads exactly the saved user and password and
the address's hosts, host addresses, ports and database. A record that names no `auth` (written before
saved logins) is `broken` `outdated`, and every call that dials it is
refused `InvalidInput` with the fix `layerfs store save-auth <auth> --user
<user> --password-stdin && layerfs store update <store> --auth <auth>`; its
old `password` reference is not read. A record naming a login the home does
not hold is `broken` `missing` with the save of that login and the same
update as its fix
([`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs)
`dial`, `no_auth_fix`; [`state.rs`](../../../../control/crates/layerfs-store/src/state.rs)).

```text
 Store record: url postgres://h/db  auth <auth>
 secret/auth/<auth>.json: user, password (0600)
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
| The login is a saved one, and only an address alone is dialled or recorded: `refuse_login` refuses `InvalidInput` a URL that holds a login (`holds_login`, above: an `@`, a user or password the client reads, a login key and `=`, or one the client does not read, such as an unknown key like `passfile`, `sslpassword`, `sslrootcert`, `sslkey` or `sslcert`, an `sslmode` other than `disable`, `prefer` or `require`, or a bad value) or holding a `#` (the client reads it as text, other readers end the URL there); an `@` or `#` of the address itself is written `%40` or `%23`, which the client decodes. A refusal of a URL the client does not read names what it refuses only by a key or value from fixed lists (`sslmode=verify-full`, `sslrootcert`, the value of `port`), never by other URL text (a value the client would read but for a `#` in it, `sslmode=require#f`, is named by that `#`; `sslrootcert` adds where a private CA is trusted: on Linux, where `native-tls` is OpenSSL, in the system's trusted roots, or in `SSL_CERT_FILE`, which `openssl-probe` reads in the process that dials only: a Sandbox's daemon starts with a cleared environment; on macOS, where it is the Security framework, which reads no such variable, in the login or system keychain); `sslmode=require` verifies the server's certificate chain and host name (the engine's `native-tls` connector, with its defaults), so `verify-ca` and `verify-full` are refused rather than dialled; any refusal of a URL holding a `#` names `%23`. The refusal's fix (`refuse_login_with_fix`) names `<url>` and `<user>`, never URL text: a URL that holds a login gets `save-auth` + `--auth` on a first connect, and on a connected Store `save-auth` and the steps that name the saved login before the address alone (`login::login_fix`); one refused only for a `#` holds no login, so its fix is the connect of the address alone, saving none. A named login not saved is `NotFound`, one with no user or password `Failed` | [`postgresql/address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs), [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs) `read` |
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
