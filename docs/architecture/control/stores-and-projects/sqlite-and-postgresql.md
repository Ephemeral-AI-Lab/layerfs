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
| `store.connect` | records the database as a Store in the fixed `layerfs` schema: URL with no user or password, schema, `synchronous_commit`, its saved login `auth`, binding, a cursor key of this home's own, no machine. On a connected Store the URL becomes the address every machine dials (`readdress`); with `--machine <m>` it is that machine's own, `machines.<m>: {url}`, checked from the side that dials (this process, or the worker's `store.reach`): the server answers, its schema holds this Store, and the role has what a Sandbox's reads and writes need (the schema's `USAGE` and `CREATE`, the database's `TEMP`, each table's and sequence's privileges; never what only a compaction needs, which `store compact` checks itself), and the machine's platform must run PostgreSQL Sandboxes. Both refuse `--auth` and `--synchronous-commit` | `InvalidInput`, `Exists`, `NotFound` (no such saved login), `Unavailable`, `Unsupported` (`--machine` on a platform that runs no PostgreSQL Sandbox), `Denied`, `Conflict` | [`connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) `connect` |
| `store.disconnect` | forgets the Store; its data stays. With `--machine <m>`, removes that machine's own address, which then dials the Store's, and forgets its reach | `InvalidInput` for sqlite (`store remove`); `InUse` while leased or a Sandbox uses it; `NotFound` when the machine has neither an address of its own nor a reach | same |
| `store.update <s> {auth, synchronous_commit}` | another saved login or `synchronous_commit` (one is required); a new login is checked once from this process on the Store's address, which must hold this Store, and saved again under the home lock; the write is clean (`StoreRecord::cleaned`) | `InvalidInput`; `Unavailable` (not reached, or another Store's schema); `Conflict` if the record changed meanwhile | [`update.rs`](../../../../control/crates/layerfs-store/src/postgresql/update.rs) |
| Store calls | open the schema in this process with the URL resolved now | — | [`open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| Host read session | one read-only session per process for Sandbox host operations: a create's reach check and a status's head read, on every route; opened with the light check on first read, checked with one empty query before each use (a dead one is replaced, not retried), closed after 60 s unused | — | [`reader.rs`](../../../../control/crates/layerfs-store/src/postgresql/reader.rs) |

A PostgreSQL Store is on no machine. Its Sandboxes run on every machine
whose platform runs PostgreSQL Sandboxes (#536 C4), and `sandbox.create`
checks from the machine that it reaches this Store, so no call comes
between `store.connect` and the first Sandbox. `machines` holds only the
machines with a connection of their own (H9). A record holding what 2.0 no
longer writes (a `password`, a URL that is not an address alone, a
`machines.<m>` entry without `url` or with more, such as an old `checked`)
reads `broken` `outdated` (`StoreRecord::outdated_fields`), fixed by `store
update <s> --auth <auth>` (`login::outdated_fix`; a URL with no clean form,
below, is replaced instead: a machine's own with `store connect <s> <url>
--machine <m>`, the Store's with `store connect <s> <url>`; when the record
names no login, which `store connect --machine` needs to dial, a machine's
own goes first with `store disconnect <s> --machine <m>`, or with the whole
Store's `store disconnect <s>` when the Store's address has no clean form,
and is connected again as the fix's last step; `store connect --machine`
checks from that machine, which must be running, while `store disconnect
--machine` dials nothing; `store update <s> --auth <new>` refuses such a
record and writes nothing, with a fix built from the record as it is
(`login::update_fix`), so each step dials with a login that exists: when
the record's own login is saved, those replacements while it dials with
it, then `store update <s> --auth <new>` (unless it names `<new>`
already); when it names none, or one not saved, the steps of a record
naming none with `<new>` and no `save-auth`: `store disconnect <s>
--machine <m> && store update <s> --auth <new> && store connect <s> <url>
--machine <m>`, or `store disconnect <s> && store connect <s> <url> --auth
<new>` and the machines' connects when the Store's address has no clean
form); every PostgreSQL record write (`connect`,
`update`, `save_own`, `forget_machine`) drops those fields
(`StoreRecord::cleaned`, spec v23 V23-6), and no read does. Such a record
keeps its URL, and any password in it, on disk byte for byte until its fix
runs: a URL with no clean form is never rewritten (the one exception to "no
password in a Store record", for records written before 2.0's rules). A 2.0
record whose URL holds a raw `#` (`postgres://h/d#x`), accepted before
these rules, now reads `outdated` and is refused at dial until `store
update <s> --auth <auth>` rewrites it with `%23`.
A Store's URL is read only as its client reads it
([`postgresql/address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs),
`postgres::Config`, tokio-postgres 0.7's `UrlParser`, which
`layerfs-persistence` re-exports as `PostgresConfig`); LayerFS keeps no
parser of its own. What is shown or written down is a URL's clean form
(`address::clean`): the text after the client's user part (to the first
`@` anywhere), without the query pairs the client reads as a login (each
pair asked of the client alone), each `#` written `%23`. It is kept only
when the client reads it with no user and no password and the same hosts,
host addresses, ports and database as the original, and no piece of the
client's password (any three bytes, or all of a shorter one) is in it, as
written or percent-decoded, in its `target` or in the client's user. A URL
with no clean form (the client does not read it; its client user part
holds one of `? & # ; ,`, or a user holding one of `/ ? & = #`, so the
login cannot be told from an address or a query, as in
`postgres://h/db&password=p@ss` or `postgres://h:5432&password=p@ss`,
whose client user is `h`; its client's hosts, database, `options` or
`application_name` hold `&` or `#` before a key the client reads or
`passfile`/`sslpassword`, as in `postgres://h/db&password=p`, whose
database is `db&password=p`; an `@` is left; or a check fails) is shown as `postgresql://<hidden: not an address alone>` with no
`server`, and a write keeps it as it was, never rewritten: the record stays
`outdated`. `without_login` (rows, fix lines), `url_user` (a fix's
`--user`: the client's user of a URL with a clean form, unless it holds
one of `/ ? & = #`, else `<user>`;
a fix's address is `<url>` when there is no clean form) and `target` (a
row's `server`, a refusal's host: the clean form's hosts, ports and
database as the client reads them) all read that form, so
`postgres://h/d#x?password=y` is kept as `postgres://h/d%23x` (database
`d#x`), and `postgres://u:pw@h/db?application_name=me@laptop` is hidden,
never rewritten to the host `laptop`.
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
| The login is a saved one, and only an address alone is dialled or recorded: `refuse_login` refuses `InvalidInput` a URL holding any `@` (the client ends a user part at the first, wherever it is) or `#`, one the client does not read (an unknown key such as `passfile`, `sslpassword`, `sslrootcert`, `sslkey` or `sslcert`, an `sslmode` other than `disable`, `prefer` or `require`, or a bad value), and one whose client reads a user or a password (`user=`, `password=` in any spelling the client decodes), or whose client reads `&` or `#` (or `%26`, `%23`) right before a key it reads or `passfile`/`sslpassword` and an `=` in a host, the database, `options` or `application_name` (a fixed list, no grammar guessed: `postgres://h/db&password=p` is the database `db&password=p` to the client, refused with fixed words and a fix whose address is `<url>`, saving a login first when the key is a login's); an `@` or `#` of the address itself is written `%40` or `%23`, which the client decodes. A refusal of a URL the client does not read names what it refuses only by a key or value from fixed lists (`sslmode=verify-full`, `sslrootcert`, the value of `port`), never by other URL text (a value the client would read but for a `#` in it, `sslmode=require#f`, is named by that `#`; `sslrootcert` adds where a private CA is trusted: on Linux, where `native-tls` is OpenSSL and `openssl-probe` reads it, in `SSL_CERT_FILE`, on macOS, where it is the Security framework, which reads no such variable, in the system keychain); `sslmode=require` verifies the server's certificate chain and host name (the engine's `native-tls` connector, with its defaults), so `verify-ca` and `verify-full` are refused rather than dialled; any refusal of a URL holding a `#` names `%23`. The refusal's fix gives the clean form (`refuse_login_with_fix`: `save-auth` + `--auth` on a first connect, `save-auth` + `store update --auth` before the address alone on a connected Store), `<url>` and `<user>` when there is none; a URL refused only for a `#` or a TLS setting holds no login (`address::holds_login`), so its fix is the connect of the address alone, saving none, unless the client does not read its hosts and path alone (`postgres://alice:secret/db?sslmode=verify-full`, an `@host` left out) or they or a value hold `&` or `#` before a login key. A named login not saved is `NotFound`, one with no user or password `Failed` | [`postgresql/address.rs`](../../../../control/crates/layerfs-store/src/postgresql/address.rs), [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs) `read` |
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
