# store family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `store` calls: list and read Stores, create a sqlite Store, connect a PostgreSQL database as a Store with a saved login, move, remove, compact and check a Store, within the [SDK](README.md). `store.doctor` is one of [the doctors](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/store`](../../../../api/crates/layerfs-api/src/store) (one file per call), [`layerfs-store/src`](../../../../control/crates/layerfs-store/src) (`sqlite/`, `postgresql/`, `list.rs`, `get.rs`, `state.rs`, `live.rs`, `stats.rs`, `intent.rs`, `compact.rs`), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) |
| Runs by default | the caller's process (records, locks, leases, saved logins, PostgreSQL connections); a sqlite Store's file steps run in its machine's worker when it is not on a locally executing machine |
| Design | [Stores and projects](../../control/stores-and-projects/README.md) (records, placement, leases), [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md), [Engine: Store](../../engine/store/README.md) |

## Calls

```text
  reads, any time:  list [--with stats]    get [--with live,stats]    doctor
                         |
     sqlite              |                 PostgreSQL
     ------              |                 ----------
     create [--machine]  |                 save-auth <auth> --user U --password-stdin
        |                |                 auths, remove-auth
        v                |                 connect <store> <url> --auth A
     move <store> <machine>                connect ... --machine M (its own address)
     move <store> native (back)            update [--auth A] [--synchronous-commit S]
        |                                  disconnect [--machine M] (data stays)
        v
     remove (file + projects)
     compact (both engines: reclaim dead bytes)
```

| Call | Engine | Short answer | MCP |
| --- | --- | --- | --- |
| [`store.list`](#storelist) | both | `{ items: [{ store, engine }] }` | `admin` |
| [`store.get`](#storeget) | both | `{ engine, state }` | `admin` |
| [`store.create`](#storecreate) | sqlite | `{ ok }` | `admin` |
| [`store.connect`](#storeconnect) | PostgreSQL | `{ projects }`, or `{ ok }` | `admin` |
| [`store.disconnect`](#storedisconnect) | PostgreSQL | `{ ok }` | `admin` |
| [`store.save_auth`](#storesave_auth) | PostgreSQL | `{ ok }` | `admin` |
| [`store.auths`](#storeauths) | PostgreSQL | `{ items: [{ auth, user, state }] }` | `admin` |
| [`store.remove_auth`](#storeremove_auth) | PostgreSQL | `{ ok }` | `admin` |
| [`store.update`](#storeupdate) | PostgreSQL | `{ ok }` | `admin` |
| [`store.move`](#storemove) | sqlite | `{ moved_bytes }` | `admin` |
| [`store.remove`](#storeremove) | sqlite | `{ ok }`, or `{ left }` | `admin` |
| [`store.repair_mount_holders`](#storerepair_mount_holders) | both | holders, one settled/released op, or a bounded done-audit page | `admin` |
| [`store.compact`](#storecompact) | both | `{ freed_bytes }` | `admin` |
| [`store.doctor`](#storedoctor) | both | the doctor report | `admin` |

A sqlite Store is opened with the Normal profile (WAL,
`synchronous=NORMAL`), the only SQLite Store profile; its record carries the
label `normal`, and a record written before 2026-11-23 that says `disposable`
answers `normal` (it is never rewritten for it). Durable execution is `NOT_RUN — disabled by owner until
explicit reauthorization`. A PostgreSQL Store has no profile: its write
setting is `synchronous_commit`. A Store name is 1 to 63 of
`a-z 0-9 _ . -`, starting and ending with a letter or digit, not a Windows
device name ([`names.rs`](../../../../control/crates/layerfs-home/src/names.rs)).
A folder scope refuses every `store` call.

**A Store's state** ([`state.rs`](../../../../control/crates/layerfs-store/src/state.rs))
comes from its records only, with no probe, first match wins: `busy` while
an operation it records (`op { name, phase, started_at }`: a move, a remove,
a create on a machine, a compaction, a backup or a restore) has a live
owner; `broken` `unresolved` once a gone owner left that operation's outcome
unknown, with `fix` naming the call that clears it (`store move` again for a
move, `store remove` for a remove, `store compact` for a compaction,
`store get <store> --with live` for a backup or a restore); `blocked` when a
sqlite Store's machine is not ready, with `blocked_by { machine, reason }`;
`broken` with the reason the last live read found (`missing`,
`unreachable`) or `unsupported` for a record kind this build does not read;
else `ready`. A move or remove holds the Store exclusively; a compaction,
backup or restore does not refuse reads. This is the one records-only
derivation `store list`, `store get` and the Store nodes of
`home get --with tree` use
([`home_tree.rs`](../../../../control/crates/layerfs-control/src/home_tree.rs)).

### store.list

Every Store, from the home's records.

```text
CLI     layerfs store list [--machine M] [--with stats]
Node    await layerfs.store.list({ machine, with })
Python  layerfs.store.list(*, machine=None, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` option | name | only the sqlite Stores on this machine |

| Answer | Keys |
| --- | --- |
| short | `items` |
| long | `items` |
| `items[]` short | `store`, `engine`; when set: `state`, `machine`, `auth`, `connections`, `projects`, `projects_count`, `backups`, `op`, `blocked_by`, `reason`, `fix`, `observed_at` |
| `items[]` long | `store, engine, state, machine, auth, binding, connections, projects, projects_count, backups, op, blocked_by, reason, fix, observed_at, profile, synchronous_commit, sqlite_version, created_at, last_transfer` |
| `items[].projects[]` short | each a `project` |
| `items[].connections[]` short | `url`; when set: `machine`, `reached` |
| `items[].connections[]` long | `machine, url, own, server, reached, reach` |
| `items[].backups[]` short | `bucket` |
| `items[].backups[]` long | `bucket, file, sha256, heads_sha256, last, attempt` |
| `--with stats` | adds `size` |
| call | writes one call-log line; MCP `admin` |

Reads every `store/*/record.json`; a machine's state is derived once for
all its Stores. Each item is as `get` answers it, with `store` first.
`--machine M` keeps the sqlite Stores on `M`. `--with stats` adds each
item's `size` (file plus write-ahead log) from one `stat` on its machine,
starting nothing; null for a PostgreSQL Store and for a machine that does
not answer. It changes nothing and is not paged
([`list.rs`](../../../../control/crates/layerfs-store/src/list.rs)).

```console
$ layerfs store list
{"items":[{"store":"shop","engine":"sqlite","machine":"native","projects":["api"]}]}
```

### store.get

A Store's record and state.

```text
CLI     layerfs store get <store> [--with live,stats]
Node    await layerfs.store.get(store, { with })
Python  layerfs.store.get(store, *, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |

| Answer | Keys |
| --- | --- |
| short | `engine`, `state`; when set: `machine`, `auth`, `connections`, `projects`, `projects_count`, `backups`, `op`, `blocked_by`, `reason`, `fix`, `observed_at` |
| long | `engine, state, machine, auth, binding, connections, projects, projects_count, backups, op, blocked_by, reason, fix, observed_at, store, profile, synchronous_commit, sqlite_version, created_at, last_transfer` |
| `projects[]` short | each a `project` |
| `connections[]` short | `url`; when set: `machine`, `reached` |
| `connections[]` long | `machine, url, own, server, reached, reach` |
| `backups[]` short | `bucket` |
| `backups[]` long | `bucket, file, sha256, heads_sha256, last, attempt` |
| `--with live` | adds `holders`, `phases`, `behind`, `server_version`, `latency_ms` |
| `--with stats` | adds `size`, `logical_bytes`, `canonical_bytes`, `stored_bytes`, `metadata_bytes`, `file_bytes`, `reusable_bytes`, `dedupe`, `compression`, `init` |
| call | writes one call-log line; MCP `admin` |

A Store's record and records-only state
([`get.rs`](../../../../control/crates/layerfs-store/src/get.rs)). A sqlite
Store answers `machine`, `profile` and `sqlite_version`; a PostgreSQL Store
`auth` (its saved login, by name), `binding` (long form: the first 8 bytes
of the id in its schema as 16 hex, the open-Store cache's key),
`connections` and `synchronous_commit`; the other engine's keys are null.
`connections` (spec v23,
[`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs)
`connections`) lists every side that connects to the database: this computer
first (`machine` null, the Store's address), then by name each machine with
an address of its own, a reach seen from it, or a Sandbox of the Store (the
Sandbox records come from `layerfs-control` through
`layerfs_store::get::Sandboxes`); `native` with no address of its own dials
as this computer and is its row. A row is `{ machine, url, own, server,
reached, reach }`: the URL it dials (never a login), whether it is the
machine's own, the `{ host, port, database }` it names, and the last reach
seen with its word (`ok`, `failed`, null). Reaches live in the Store's
observation file, `store/<store>/observed.json` (`here` and
`reach.<machine>`, each `{ at, ok, latency_ms, server_version, error }`;
[`observed.rs`](../../../../control/crates/layerfs-store/src/observed.rs)):
`store connect --machine` records its check, `store doctor` each reach
(this computer's measured with the same check), and a Sandbox create on a
machine `{ at, ok: true }` after its reply. A writer takes the Store's lock
after its Store I/O and writes nothing once the record is gone; a live
read's `at` merges without a lock, as a read takes none. None of it moves
the home's `generation`. A PostgreSQL Store naming a login the home does not
hold is `broken` `missing`; one whose record holds a `password`, a URL that
is not an address alone or a `machines.<m>` entry with anything but `url`
is `broken` `outdated`, fixed by `store update <store> --auth <auth>`. No
URL is ever cleaned or rewritten: a URL that is not an address alone (one
that may hold a login, or a raw `#`) is replaced by the user's fix, which
dials only with the login it names: `store disconnect <store> --machine M
&& store update <store> --auth <auth> && store connect <store> <url>
--machine M` for a machine's own, `store disconnect <store> && store
connect <store> <url> --auth <auth> --synchronous-commit <its value>` and
each machine's own connected again for the Store's; until then the record
keeps that URL, and any password in it, as it was written
([`state.rs`](../../../../control/crates/layerfs-store/src/state.rs),
[`login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs)). `projects` shows at
most 20 names, `projects_count` only when there are more. A `backups` row's
long form is `bucket, file, sha256, heads_sha256, last, attempt`: `last` the
last transfer that worked (`{ call, ended_at, size }`), `attempt` the last
one that did not (`{ call, ended_at, error }`). `last_transfer` is the last
move.

**`--with live`** ([`live.rs`](../../../../control/crates/layerfs-store/src/live.rs))
reads the Store where it lives, starting nothing: a sqlite Store's file,
compaction record and heads through its machine (in this process on
`native`, else its recorded worker), a PostgreSQL Store's server from this
computer. `blocked` comes first, with nothing asked, while a sqlite Store's
machine is not ready; then `broken` `missing` (no file where the record
says), `broken` `unreachable` (the server does not answer), `busy` or
`broken` `unresolved` from a compaction's own record beside the file (a run
live there is `busy` even when this home's record holds it unresolved after
a lost reply), or `unknown` with when LayerFS last knew, when the machine or
worker does not answer. The record is written only when what the read found
wrong changed (which moves the home's `generation`); `observed_at` goes to
`store/<store>/observed.json` (keeping the reaches there), which does not. A backup or restore whose
owner is gone with its outcome unknown is settled here: the `op` is dropped
and what was tried is written to the bucket row's `attempt`. The `live`
section: `holders` (pids of other live processes holding the Store's lease;
[`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs)),
`phases` (a running compaction's phases), `behind` (the buckets whose backup
heads differ from the Store's; null for PostgreSQL or unread heads),
`server_version` and `latency_ms` (PostgreSQL only).

**`--with stats`** ([`stats.rs`](../../../../control/crates/layerfs-store/src/stats.rs))
opens the Store, as a Store call does (refused while a move or remove holds
it), and reads its statistics totals in constant work: `size`,
`logical_bytes`, `canonical_bytes`, `stored_bytes`, `metadata_bytes`,
`file_bytes`, `reusable_bytes` (null for PostgreSQL), `dedupe` and
`compression` (ratios as text, six decimals) and `init` (the first project
init's figures, or null). `logical_bytes` sums saved trees over every
Commit, so it is not a deduplication numerator.

Run on macOS native with a debug build:

```console
$ layerfs store get shop
{"engine":"sqlite","state":"ready","machine":"native","projects":["api"]}
$ layerfs store get shop --with live,stats
{"engine":"sqlite","state":"ready","machine":"native","projects":["api"],"observed_at":"2026-11-18T22:05:32Z","stats":{"size":307200,"logical_bytes":199,"canonical_bytes":6815,"stored_bytes":3354,"metadata_bytes":278242,"file_bytes":307200,"reusable_bytes":28672,"dedupe":"0.029200","compression":"2.031902"}}
```

`store.status` and `store.stats` were merged into this call, with no alias.

### store.create

An empty sqlite Store.

```text
CLI     layerfs store create <name> [--machine M]
Node    await layerfs.store.create(name, { machine })
Python  layerfs.store.create(name, *, machine=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | name | the new Store's name |
| `machine` option | name | make it on this machine; this computer (native) when left out |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, store, engine, machine, profile, sqlite_version, created_at` |
| call | writes one call-log line; MCP `admin` |

A new empty sqlite Store, ready for `project init`: on `native`, or with
`--machine M` made on that machine directly through its worker (a running
machine), recording a `create` operation while it runs. On `native`, under
the home lock, the call makes `store/` and `store/<name>/` (`0700`), checks
the folder with the kernel guard, builds the empty file as
`store.sqlite.creating` (`0600`) and renames it to `store.sqlite`. A failure
before the file exists removes `store/<name>/` only when this call made it
([`sqlite/create.rs`](../../../../control/crates/layerfs-store/src/sqlite/create.rs)).
The long form is `ok, outcome ("created"), store, engine, machine, profile,
sqlite_version, created_at`.

Refusals: `Exists` (a Store of that name, of either engine, or a file at the
target); `Unsupported` (the kernel guard refuses `store/<name>/`: not on a
local lock-carrying filesystem, or an existing folder not owned by this user,
a symlink, or with group or other access); `Unavailable` (the machine is
stopped, or a lock is held too long).

```console
$ layerfs store create shop
{"ok":true}
$ layerfs store create shop
{"error":"Exists","message":"a Store named shop exists"}
```

### store.connect

Connect a PostgreSQL database as a Store, or set an address of a connected one.

```text
CLI     layerfs store connect <store> <url> [--auth A] [--machine M] [--synchronous-commit local]
Node    await layerfs.store.connect(store, url, { auth, machine, synchronous_commit })
Python  layerfs.store.connect(store, url, *, auth=None, machine=None, synchronous_commit=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | the Store's name in this home |
| `url` | text | a `postgresql://` address: host, port, database and TLS settings, with no user or password |
| `auth` option | name | the saved login; required on a Store's first connect, refused afterwards |
| `machine` option | name | on a connected Store: the address is this machine's own |
| `synchronous_commit` option | text | off, local, remote_write, on or remote_apply; first connect only; default `local` |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `projects`, `projects_count` |
| long | `ok, projects, projects_count, outcome, store, engine, auth, connections, synchronous_commit` |
| call | writes one call-log line; MCP `admin` |

Connects a PostgreSQL database as a Store, or sets an address of a
connected one ([`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs)).
The record keeps the URL exactly as given, with no user and no password,
and names its saved login (`auth`), put into the URL only when the Store is
dialled; a URL that may hold a login is `InvalidInput`: one holding an `@`,
one the PostgreSQL client reads a user or a password in, one it does not
read, or one whose percent-decoded text holds a login key (`user`,
`password`, `passfile`, `sslkey`, `sslcert`, `sslpassword`,
`oauth_client_secret`) and `=` anywhere, in any case
(`postgres://h/db&password=p`, `postgres://h/db?application_name=me?password=p`);
so is one holding a `#` (written `%40` or `%23` when part of the address).
A URL is read only as that client reads it and shown exactly as written,
or as `postgresql://<hidden: not an address alone>` when it may hold a
login; none is ever rewritten. The client takes `sslmode`
`disable`, `prefer` or `require` and no certificate or key file:
`sslmode=require` verifies the server's certificate chain and host name, and
`sslmode=verify-full`, `verify-ca`, `sslrootcert`, `sslcert` and `sslkey` are
refused, the refusal naming the one it met. A private CA is trusted where
the Store is dialled: on Linux, where `native-tls` is OpenSSL, in the
system's trusted roots (`SSL_CERT_FILE` reaches only calls in the process
that reads it: a Sandbox's daemon starts with a cleared environment); on
macOS, where it is the Security framework, in the login or system
keychain.

On a Store's first connect `--auth` is required and must be saved;
`--synchronous-commit` defaults to `local`. It creates the `layerfs` schema
in an empty database or reads the Store's id from it; an existing
database's projects become this home's project records. Short: `projects`
(the first 20 names, `projects_count` only past those), or `{"ok":true}`
for an empty database or when only an address changed. On a connected Store
`connect` checks the new address against the same Store's id and takes no
`--auth` or `--synchronous-commit` (`InvalidInput`, naming `store update`).
With `--machine M` the address is `M`'s own, checked from `M` with the
Store's login; a machine never has its own login. No default machine is
recorded: `sandbox create --machine M` chooses where a Sandbox of it runs.

| Refusal | When |
| --- | --- |
| `InvalidInput` | a URL that is not PostgreSQL, names a user or holds a password, holds an `@` or `#`, or that the PostgreSQL client does not read; a bad `--synchronous-commit`; `--auth` missing on a first connect; `--auth` or `--synchronous-commit` on a connected Store |
| `NotFound` | the named login is not saved (the message names `store save-auth`) |
| `Exists` | another Store of this home has that name or that database's id |
| `Unavailable` | no session (no database, a refused login, no server); the machine is stopped |
| `Unsupported` | the machine's platform runs no PostgreSQL Sandboxes here |

Run on macOS native against a local `postgres:17`, with a debug build:

```console
$ layerfs store connect shop-pg 'postgresql://postgres:pw@127.0.0.1:55231/shop'
{"error":"InvalidInput","message":"the URL names a user or holds a password (or a password file); give the address alone and the login with --auth (`layerfs store save-auth` saves one); an `@` or `#` that is part of the address is written %40 or %23"}
```

### store.disconnect

Forget a PostgreSQL Store, or one machine's own address; its data stays.

```text
CLI     layerfs store disconnect <store> [--machine M]
Node    await layerfs.store.disconnect(store, { machine })
Python  layerfs.store.disconnect(store, *, machine=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |
| `machine` option | name | remove only this machine's own address |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, store, machine` |
| call | writes one call-log line; MCP `admin` |

Forgets a PostgreSQL Store in this home; the database keeps every byte and
`store connect` can record it again. With `--machine M` it removes only
`M`'s own address and forgets what was seen of `M`'s reach (`NotFound` when
it has neither). No server is contacted and no outcome is uncertain. The
long form is `ok, outcome ("disconnected"), store, machine`.

Refusals: `InvalidInput` (a sqlite Store: `store remove` deletes one);
`InUse` (a Sandbox or Workspace uses the Store, another process holds its
lease, or a call in this process is running on it;
[`open.rs`](../../../../control/crates/layerfs-store/src/open.rs));
`NotFound` (a machine with no address of its own).

### store.save_auth

Save a PostgreSQL login in the home; saving again rotates it.

```text
CLI     layerfs store save-auth <auth> --user U --password-stdin
Node    await layerfs.store.save_auth(auth, { user, password })
Python  layerfs.store.save_auth(auth, *, user, password)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `auth` | name | a saved login's name |
| `user` option | text | the PostgreSQL role; required; required |
| `password` option | content | the password; required; the shell reads it from stdin with --password-stdin; required; shell `--password-stdin` |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, auth, user, stores, stores_count` |
| call | writes one call-log line; MCP `admin` |

Saves a PostgreSQL login in the home's secret folder,
`secret/auth/<auth>.json` (mode `0600`, in a `0700` folder), and moves the
home's `generation` ([`postgresql/login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs)).
The shell reads the password from stdin with `--password-stdin` and no
other way; Node and Python pass `password`. It is never answered, logged or
traced. Saving again under the same name rotates it: every Store using it
dials with the new one from its next connection. The long form is `ok,
outcome (saved or rotated), auth, user, stores, stores_count`.

### store.auths

The saved PostgreSQL logins and the Stores that use each.

```text
CLI     layerfs store auths
Node    await layerfs.store.auths()
Python  layerfs.store.auths()
```

| Answer | Keys |
| --- | --- |
| short | `items` |
| long | `items` |
| `items[]` short | `auth`, `user`; when set: `state`, `reason`, `fix`, `stores`, `stores_count` |
| `items[]` long | `auth, user, state, reason, fix, stores, stores_count, created_at, updated_at` |
| call | writes one call-log line; MCP `admin` |

The saved logins and the Stores that use each (`stores`, at most 20;
`stores_count` only past 20); never a password. Not paged: a home holds a
handful. The long row adds `created_at` and `updated_at`. A login file that
cannot be read is its own row, `broken` `failed` with the save that writes
it again (`user` null), and the other logins are listed as usual (spec v23).

### store.remove_auth

Remove a saved PostgreSQL login no Store uses.

```text
CLI     layerfs store remove-auth <auth>
Node    await layerfs.store.remove_auth(auth)
Python  layerfs.store.remove_auth(auth)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `auth` | name | a saved login's name |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, auth` |
| call | writes one call-log line; MCP `admin` |

Removes a saved login no Store names. Refused `InUse` while a Store names
it (`stores` lists them; the message names `store update --auth` and
`store disconnect`), `NotFound` when there is none. The long form is `ok,
outcome ("removed"), auth`.

### store.update

Change a PostgreSQL Store's saved login or its synchronous_commit.

```text
CLI     layerfs store update <store> [--auth A] [--synchronous-commit S]
Node    await layerfs.store.update(store, { auth, synchronous_commit })
Python  layerfs.store.update(store, *, auth=None, synchronous_commit=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |
| `auth` option | name | another saved login |
| `synchronous_commit` option | text | off, local, remote_write, on or remote_apply |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, store, auth, synchronous_commit` |
| call | writes one call-log line; MCP `admin` |

Changes a PostgreSQL Store's saved login or its `synchronous_commit`; at
least one is required ([`postgresql/update.rs`](../../../../control/crates/layerfs-store/src/postgresql/update.rs)).
A new login is checked once from this process on the Store's address, before
it is saved: the server must be reached with it and hold this Store. The
record is written under the home lock and the Store's; this process's open
sessions on it are closed. The login is checked again under the home lock,
which `store remove-auth` takes, and the write drops every field 2.0 no
longer writes (`StoreRecord::without_old_fields`) and never rewrites a URL:
a record keeping a URL that is not an address alone is refused, nothing
written, with the fix that replaces it naming the login given and carrying
a given `--synchronous-commit` (`login::update_fix`). A running Sandbox keeps the
connection it started with. An address change is a `store connect`; a machine's own address is
removed with `store disconnect --machine`. The long form is `ok, outcome
("updated"), store, auth, synchronous_commit`.

| Refusal | When |
| --- | --- |
| `InvalidInput` | a sqlite Store; nothing to update; a bad `synchronous_commit` |
| `NotFound` | no such Store, or the login is not saved |
| `Unavailable` | the new login does not reach the server, or the database holds another Store |
| `InUse` | a call of this process is running on the Store |
| `Conflict` | the Store was disconnected or connected to another database meanwhile |

### store.move

Move a sqlite Store between this computer and a machine.

```text
CLI     layerfs store move <store> <machine> [--progress]
Node    await layerfs.store.move(store, machine, { on_progress })
Python  layerfs.store.move(store, machine, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |
| `machine` | name | the machine it moves to; `native` moves it to this computer |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `moved_bytes` |
| long | `moved_bytes, outcome, store, from, to, heads_sha256` |
| call | sends progress; writes one call-log line; MCP `admin` |

Moves a sqlite Store's file between this computer and a machine, checked
byte for byte. One side is always `native`: a Store on a machine moves to
another by `store move <store> native` first (refused `InvalidInput` naming
that call; [`sqlite/transfer/resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)).
`moved_bytes` is the bytes moved; the long form adds `outcome` (`moved`),
`store`, `from`, `to` and `heads_sha256`, the digest of the Store's history
heads, checked on the target. Progress phases: `check`, `snapshot`, `send`,
`verify`, `install`, `record`, `retire`; `done` and `total` are bytes while a
phase moves bytes, else null. Each step runs once and a lost worker answer
is never resent; every refusal's message starts `store move, <phase>:` and
carries `detail.phase`
([`phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs)).
A move whose owner is gone is cleared by the same move or by `store
remove`. It replaces `store.push` and `store.pull`, removed with no alias.

| Refusal | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store; the Store is already on `machine`; neither side is `native` |
| `NotFound` | no such Store or machine; no Store file at snapshot |
| `InUse` | a live process holds the Store, or a Sandbox of one of its projects is recorded (`detail.related`); the next field `sandboxes` names them, and when no process holds it the `fix` removes them (`layerfs sandbox remove a && layerfs sandbox remove b`; [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle`) |
| `Exists` | the target already holds a Store file of that name |
| `Capacity` | the target's free disk is below the bytes plus max(1 GiB, 10%) |
| `Unavailable` | the machine is stopped or its worker gone; `fix: layerfs machine start <machine>` |
| `Busy` | a compaction runs on the Store's machine |
| `Failed` | a `-wal` no open-and-close could checkpoint; a digest mismatch at `verify`; I/O |
| `Unknown` | a worker's answer was lost, or a failure of `record` (both copies exist): both copies are kept and the `op` stays uncertain; move again, which first deletes the copy the record does not name |

```bash
layerfs store move shop dev --progress    # (not run)
```

### store.remove

Delete a sqlite Store with every project in it.

```text
CLI     layerfs store remove <store>
Node    await layerfs.store.remove(store)
Python  layerfs.store.remove(store)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |

| Answer | Keys |
| --- | --- |
| short | `ok`; `left` when only its records went |
| long | `ok, outcome, store, left` |
| call | writes one call-log line; MCP `admin` |

Deletes a sqlite Store, wherever its file is, with every project in it; also
the call that clears an unresolved operation on the Store. What goes: the
file with its `-wal` and `-shm`, `store/<name>/` in the home (record and
project entries), its folder under a worker's root, and a partial copy a
move left elsewhere. What stays: its backups in their buckets
([`sqlite/remove.rs`](../../../../control/crates/layerfs-store/src/sqlite/remove.rs)).
The long form is `ok, outcome ("removed"), store, left`. Two cases take
the Store's records alone and answer `left`, each sentence about what may
remain: a record this build does not read (its kept Store file, with the
`rm -f` line that removes it), and a Store on a machine whose record has
no 2.0 name (its file stays on that machine, which no call reaches; RV3
FAIL 1).

| Refusal | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store (`store disconnect` forgets one) |
| `NotFound` | no Store of that name |
| `Busy` | another live process's operation runs on the Store |
| `InUse` | a live process, or this one, holds it; a Sandbox of one of its projects is recorded: the next field `sandboxes` names them, and when no process holds it the `fix` removes them ([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle`) |
| `Conflict` | the Store moved to another machine meanwhile; nothing removed |
| `Unknown` | the worker's answer was lost; the `op` stays uncertain; remove again |

```console
$ layerfs store remove nope
{"error":"NotFound","message":"no Store named nope"}
```

### store.repair_mount_holders

Explicit, operator-only repair of the mount admissions held at one Branch.
Nothing is automatic: preview and audit change no live intent, the doctor only
reports the same rows without releasing them, and no action deletes a row.

```text
CLI     layerfs store repair-mount-holders <name> <project> <branch> <action> [--op HEX] [--legacy] [--operator NAME] [--page N] [--limit N]
```

The action is one of:

| Action | Effect |
| --- | --- |
| `list` | lists the typed holder view (op, project, Branch, Workspace, Sandbox, machine, place authority, state, legacy, `safe`, `reason`); read-only, writes nothing |
| `preview` | lists the same typed view and writes one retained `done` row with `action=preview;operator=NAME` in `evidence`, `op` null, the operator label and no owner or place columns; it changes no live intent |
| `settle` | marks exactly `--op` uncertain; a later call reports `Unresolved`, never `Busy` |
| `release` | needs the preceding settle; retains the row in state `done` with `action=release;operator=NAME;op=HEX` in `evidence` and clears the owner columns, so the unique owner key cannot block a remount |
| `settle_legacy` | marks only rows with missing owner or place evidence |
| `audit` | one bounded page of the named project/Branch's retained `done` rows, oldest first (`--page`, `--limit`, clamped to 1000); changes nothing. The scope is the recorded project and Branch names, so a removed Branch's retained rows stay readable |

Every holder row carries `safe` and `reason`: an uncertain (settled) row is
safe to release and says so; a live row still holds the admission and must be
settled first. Release answers `{ action: "release", op, audit: "gc_intent" }`;
preview answers the holders plus `{ action: "preview", op: null, audit:
"gc_intent" }`; list answers the holders alone; settle and `settle_legacy` are
in the home's `trace/calls.jsonl` call log only. `--operator` is the local name
the caller gives (at most 128 characters, no control characters; empty is
`local`): it is not an authenticated principal. On a SQLite Store the call runs
on the machine that holds the file through its worker; a PostgreSQL Store runs
in the calling process.

| Error | When |
| --- | --- |
| `InvalidInput` | an unknown action; a missing or malformed `--op`; a bad operator label |
| `NotFound` | no mount holder with that op at the Branch (for a release: already released) |
| `Conflict` | release before settle |
| `Unavailable`, `Unknown` | the owning worker is not reachable or its answer was lost |

### store.compact

Reclaim dead bytes from one Store.

```text
CLI     layerfs store compact <store> [--progress]
Node    await layerfs.store.compact(store, { on_progress })
Python  layerfs.store.compact(store, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `freed_bytes` |
| long | `freed_bytes, outcome, store, generation_before, generation_after, dead_bytes, discarded_bytes, reusable_bytes, kept_waste, delta_base_references, delta_base_references_count, peak_temp_bytes, residue_bytes, residue_artifacts, residue_reason, estimate, zero_reasons` |
| call | sends progress; writes one call-log line; MCP `admin` |

Reclaims one Store's dead bytes by rebuilding it from its live history: the
only byte-reclaiming call, with no confirm, force or dry run and no
automatic collection ([`compact.rs`](../../../../control/crates/layerfs-store/src/compact.rs)).
It records a `compact` operation in the Store's record while it runs and
writes its own operation record beside the Store before the first
checkpoint; it verifies the rebuilt Store and installs it by rename. A
definite failure before the rename keeps the old Store and its generation;
an installed-but-uncertain reply is reported, never deleted. A Store with
less than 10% dead bytes is `unchanged`: the run marks only and writes
nothing. On SQLite the dead bytes include the dead records inside packs the
run keeps, and the rebuild writes each such pack again without them;
`kept_waste` (`{dead_in_retained_packs_bytes, delta_bases_bytes,
inode_blocks_bytes}`) reports what the rebuild still kept. A sqlite Store on
another machine is compacted by that machine's worker, which sends the same
phases; the call starts no machine. A returned run's record left unresolved
is adopted and settled by the next `store compact`
([`report.rs`](../../../../control/crates/layerfs-store/src/report.rs)).
Progress phases, sent as each starts with null counts: `mark`, `build`,
`verify`, `install` (SQLite; an `unchanged` run sends `mark` only), `mark`,
`build`, `swap`, `cleanup` (PostgreSQL).

| Refusal | When | Next |
| --- | --- | --- |
| `Unavailable` | the Store's machine is stopped, records no worker, or its worker does not answer (the Store is `blocked`) | `fix: layerfs machine start <machine>` |
| `Unavailable` | the machine's worker is of another build | `fix: layerfs machine stop <machine> && layerfs machine start <machine>` (`--force` on the stop when Sandboxes are on it) |
| `Unavailable` | the machine's record has no `vm_name` (the Store is `blocked`, `outdated`), or a live read found its VM gone (`blocked`, `missing`; LF14); it carries `blocked_by {machine, reason}` and nothing is dialled (LF10) | `fix`: the machine's own, `layerfs store remove <store> && … && layerfs machine remove <machine>` (UI2-8) |
| `Busy` | a machine operation holds the machine, or a compaction of the Store runs here or in the worker; it carries `op` (`{name: compact, phase, started_at}`) | |
| `InUse` | Workspaces of the Store are mounted (raised `MountsActive`) | |
| `Unknown` | the reply was lost after the request went out; the Store is `broken` `unresolved` | `fix: layerfs store compact <store>` |
| `Unresolved` | an earlier run left its outcome unknown | `fix: layerfs store compact <store>` |
| `Capacity` | a real allocation refusal during the rebuild; the old Store is kept | |
| `Conflict` | the sealed state changed at install | |

Run on macOS native with a debug build:

```console
$ layerfs store compact shop --progress
{"phase":"mark","done":null,"total":null}
{"freed_bytes":0}
```

### store.doctor

Check a Store, then each of its projects.

```text
CLI     layerfs store doctor <store> [--machine M] [--progress]
Node    await layerfs.store.doctor(store, { machine, on_progress })
Python  layerfs.store.doctor(store, *, machine=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |
| `machine` option | name | also probe a PostgreSQL Store's connection from this machine |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `findings`, `not_checked` |
| long | `ok, findings, not_checked, status, counts, observed_at, tree` |
| `findings[]` short | `<thing>`, `check`, `severity`, `message`; when set: `fix`, `from` |
| `findings[]` long | `<thing>, check, severity, message, fix, from, evidence` |
| `not_checked[]` short | `<thing>`; when set: `check`, `blocked_by`, `reason` |
| `not_checked[]` long | `<thing>, check, blocked_by, reason` |
| call | sends progress; writes one call-log line; MCP `admin` |

Checks a Store, then each of its projects shallowly. Its checks, in order:
`store.record` (its record reads and names it; a PostgreSQL record that
alone makes the Store `broken`, `outdated` or `missing`, fails with the `fix`
`store list` gives), `store.operation` (an
operation whose owner is gone with its outcome unknown is an error whose
`fix` is the call that clears it; for a sqlite Store it also reads the
compaction record beside the file through its machine), `store.connection`
(a sqlite Store's file is where its record says, read through its machine; a
PostgreSQL Store's server answers this computer with the saved login),
`store.identity` (the Store's id is the recorded one), `store.schema` (its
schema is this build's) and, for a sqlite Store, `store.backups` (a notice
per bucket whose backup heads differ, `fix: layerfs backup create <store>
<bucket>`). No Store data is scanned: `project doctor --full` reads a
project's data. A PostgreSQL Store is then probed from each machine that has
a Sandbox of it and from `--machine`, each as one more `store.connection`
row with `from`; a sqlite Store refuses `--machine`. A Store whose machine
is not ready shows `store.connection` as `not_checked` with `blocked_by`, and
nothing after it is tried. It changes nothing and exits 0 once its report is
made. It replaces the shell's `layerfs doctor store`, removed with no alias.

```console
$ layerfs store doctor shop
{"ok":true}
```

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name or option; a folder scope | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NotFound` | no Store of that name | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) |
| `Busy`, `Unresolved` | another operation's `op`; an unresolved one is shown by `store get` and cleared by the call its `fix` names | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | a stopped machine; a machine whose record has no `vm_name` or whose VM is gone, with `blocked_by {machine, reason: outdated | missing}` and the machine's fix (LF10, LF14); a server that does not answer | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs), [`postgresql/open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| `Unsupported` | new work on a removed platform's machine | [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs), [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| `Unknown` | a step's answer was lost; an uncertain move or remove appends to `log/uncertain.jsonl`, and nothing is cleaned up on a guess | [`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) |

## Related

- [Stores and projects](../../control/stores-and-projects/README.md), [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md).
- [Engine: Store](../../engine/store/README.md), [Store calls](../../engine/store-calls/README.md).
- [backup family](backup.md), [machine family](machine.md), [Doctors](../../control/doctor/README.md).
