# store family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `store` calls: list and read Stores, create a sqlite Store, connect a PostgreSQL database as a Store with its login, attach machines to it, move, remove, compact and check a Store, within the [SDK](README.md). `store.doctor` is one of [the doctors](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/store`](../../../../api/crates/layerfs-api/src/store) (one file per call), [`layerfs-store/src`](../../../../control/crates/layerfs-store/src) (`sqlite/`, `postgresql/`, `list.rs`, `get.rs`, `state.rs`, `live.rs`, `stats.rs`, `intent.rs`, `compact.rs`), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) |
| Runs by default | the caller's process (records, locks, leases, saved passwords, PostgreSQL connections); a sqlite Store's file steps run in its machine's worker when it is not on a locally executing machine |
| Design | [Stores and projects](../../control/stores-and-projects/README.md) (records, placement, leases), [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md), [Engine: Store](../../engine/store/README.md) |

## Calls

```text
  reads, any time:  list [--with stats]    get [--with live,stats]    doctor
                         |
     sqlite              |                 PostgreSQL
     ------              |                 ----------
     create [--machine]  |                 connect <store> <url> --user U --password-stdin
        |                |                 connect <store> <url> (change address, login, setting)
        v                |                 attach <store> --machine M (checked from M)
     move <store> <machine>                detach <store> --machine M
     move <store> native (back)            disconnect (login and attachments go; data stays)
        |                |
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
| [`store.attach`](#storeattach) | PostgreSQL | `{ ok, outcome }` | `admin` |
| [`store.detach`](#storedetach) | PostgreSQL | `{ ok }` | `admin` |
| [`store.move`](#storemove) | sqlite | `{ moved_bytes }` | `admin` |
| [`store.remove`](#storeremove) | sqlite | `{ ok }`, or `{ left }` | `admin` |
| [`store.repair_mount_holders`](#storerepair_mount_holders) | both | a page of holders or done rows (`items`, `next_cursor`), or one settled/released op | `admin` |
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
| `items[]` short | `store`, `engine`; when set: `state`, `machine`, `user`, `machines`, `projects`, `projects_count`, `backups`, `op`, `blocked_by`, `reason`, `fix`, `observed_at` |
| `items[]` long | `store, engine, state, machine, url, user, password_saved, password_changed_at, binding, reach, machines, projects, projects_count, backups, op, blocked_by, reason, fix, observed_at, profile, synchronous_commit, sqlite_version, created_at, last_transfer` |
| `items[].projects[]` short | each a `project` |
| `items[].machines[]` short | each a `machine` (the attached machines' names) |
| `items[].backups[]` short | `bucket` |
| `items[].backups[]` long | `bucket, file, sha256, heads_sha256, last, attempt` |
| `--with stats` | adds `size` |
| call | writes one call-log line; MCP `admin` |

Reads every `store/*/record.json`; a machine's state is derived once for
all its Stores. Each item is as `get` answers it, with `store` first,
except that a PostgreSQL Store's `machines` are the attached machines'
names, with no reach.
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
| short | `engine`, `state`; when set: `machine`, `url`, `user`, `machines`, `projects`, `projects_count`, `backups`, `op`, `blocked_by`, `reason`, `fix`, `observed_at` |
| long | `engine, state, machine, url, user, password_saved, password_changed_at, binding, reach, machines, projects, projects_count, backups, op, blocked_by, reason, fix, observed_at, store, profile, synchronous_commit, sqlite_version, created_at, last_transfer` |
| `projects[]` short | each a `project` |
| `reach` | `at, ok, latency_ms, server_version, error` |
| `machines[]` short | `machine`; when set: `reached` |
| `machines[]` long | `machine, reached, reach` |
| `backups[]` short | `bucket` |
| `backups[]` long | `bucket, file, sha256, heads_sha256, last, attempt` |
| `--with live` | adds `holders`, `phases`, `behind`, `server_version`, `latency_ms` |
| `--with stats` | adds `size`, `logical_bytes`, `canonical_bytes`, `stored_bytes`, `metadata_bytes`, `file_bytes`, `reusable_bytes`, `dedupe`, `compression`, `init` |
| call | writes one call-log line; MCP `admin` |

A Store's record and records-only state
([`get.rs`](../../../../control/crates/layerfs-store/src/get.rs)). A sqlite
Store answers `machine`, `profile` and `sqlite_version`; a PostgreSQL Store
`url` (its address exactly as connected, never a login), `user`,
`password_saved` (whether this home holds its password), `password_changed_at`
(when it was last saved), `binding` (long form: the first 8 bytes of the id
in its schema as 16 hex, the open-Store cache's key), `reach` (this
computer's last reach), `machines` and `synchronous_commit`; the other
engine's keys are null. `machines` lists each attached machine by name as
`{ machine, reached, reach }`: the last reach seen from it and its word
(`ok`, `failed`, null). Reaches live in the Store's observation file,
`store/<store>/observed.json` (`here` and `reach.<machine>`, each `{ at, ok,
latency_ms, server_version, error }`;
[`observed.rs`](../../../../control/crates/layerfs-store/src/observed.rs)):
`store connect` records its check as this computer's, `store attach` its
check from the machine, `store doctor` each reach (this
computer's measured with the same check), and a Sandbox create on a machine
`{ at, ok: true }` after its reply. A writer takes the Store's lock after its
Store I/O and writes nothing once the record is gone; a live read's `at`
merges without a lock, as a read takes none. None of it moves the home's
`generation`. A record without a `user`, or whose password this home does
not hold, has no state of its own: a call that dials it is refused
`InvalidInput` with `store connect <store> <url> --password-stdin` as its
fix ([`login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs)
`dial`). `projects` shows at
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

Connect a PostgreSQL database as a Store, or change a connected one's address, login or setting.

```text
CLI     layerfs store connect <store> <url> [--user U] [--password-stdin] [--synchronous-commit local] [--dry-run]
Node    await layerfs.store.connect(store, url, { user, password, synchronous_commit, dry_run })
Python  layerfs.store.connect(store, url, *, user=None, password=None, synchronous_commit=None, dry_run=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | the Store's name in this home |
| `url` | text | a `postgresql://` address: host, port, database and TLS settings, with no user or password |
| `user` option | text | the PostgreSQL role; required on a Store's first connect, else the saved one is kept |
| `password` option | content | its password; required on a Store's first connect, else the saved one is kept; the shell reads it from stdin with `--password-stdin` |
| `synchronous_commit` option | text | off, local, remote_write, on or remote_apply; a new Store takes `local`, a connected one keeps its own when omitted |
| `dry_run` option | bool | check from this computer and each attached machine, and save nothing |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `projects`, `projects_count`; a dry run's also `outcome`, `here`, `machines` |
| long | `ok, projects, projects_count, outcome, store, engine, url, user, synchronous_commit, machines, here` |
| call | writes one call-log line; MCP `admin` |

This computer is the one place a PostgreSQL Store's address and login are
saved (owner, 2026-11-24); every attached machine dials with them
([`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs)).
The record keeps the URL exactly as given, the `user` and
`synchronous_commit`, and the Store's `binding`. The password is kept in the
home's secret folder as secret kind `login` under the Store's name
(`secret/login/<store>.json`, mode `0600` in a `0700` folder, `{ password,
changed }`), never answered, logged or written into a record; user and
password are put into the URL only when the Store is dialled
([`login.rs`](../../../../control/crates/layerfs-store/src/postgresql/login.rs)
`dial`). A URL that may hold a login is `InvalidInput`: one holding an `@`,
one the PostgreSQL client reads a user or a password in, one it does not
read, or one whose percent-decoded text holds a login key (`user`,
`password`, `passfile`, `sslkey`, `sslcert`, `sslpassword`,
`oauth_client_secret`) and `=` anywhere, in any case
(`postgres://h/db&password=p`, `postgres://h/db?application_name=me?password=p`);
so is one holding a `#` (written `%40` or `%23` when part of the address).
A URL is read only as that client reads it and shown exactly as written;
none is ever rewritten. The client takes `sslmode`
`disable`, `prefer` or `require` and no certificate or key file:
`sslmode=require` verifies the server's certificate chain and host name, and
`sslmode=verify-full`, `verify-ca`, `sslrootcert`, `sslcert` and `sslkey` are
refused, the refusal naming the one it met. A private CA is trusted where
the Store is dialled: on Linux, where `native-tls` is OpenSSL, in the
system's trusted roots (`SSL_CERT_FILE` reaches only calls in the process
that reads it: a Sandbox's daemon starts with a cleared environment); on
macOS, where it is the Security framework, in the login or system
keychain.

On a Store's first connect `--user` and the password are required and are
checked from this process before anything is saved; `--synchronous-commit`
defaults to `local`. It creates the `layerfs` schema in an empty database or
reads the Store's id from it; an existing database's projects become this
home's project records. Short: `projects` (the first 20 names,
`projects_count` only past those), or `{"ok":true}` for an empty database.
The long `outcome` is `connected`.

On a connected Store `connect` changes its address, user, password or
`synchronous_commit`: an omitted user or password keeps the saved one, and
the server must be reached with the result and hold the same Store (its
`binding`) before anything is saved. The Store's open connections in this
process are closed; a running Sandbox keeps the connection it started with.
Short `{"ok":true}`, long `outcome` `updated`. A connect that is saved,
first or not, records its check as this computer's reach (`store get`'s
`reach`: `ok` and how long the check took), as `store doctor` does; a
refused connect records nothing. Attachments are kept: each
attached machine dials the new address and login from its next connection.
So a change of the address, user or password is checked from every
attached machine too, one after another, after this computer's check and
before anything is saved
([`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs)
`connect`): one that fails refuses the connect `Conflict`, naming the
machines, with `machines` (`[{ machine, ok: false, step, message }]`) beside
it and `layerfs store detach <store> --machine <first>` as its fix; nothing
is saved, so no attached machine is cut off unseen. A change of
`synchronous_commit` alone checks no machine. A sqlite Store of that name
is `Exists`.

Every refused check names the connection step it stopped at as `step`,
flat beside `error` and `message`: one of the doctor's
`store.connection.<step>` words, decided by the one classifier both use
([`layerfs-persistence` `ConnectionStep`](../../../../engine/crates/layerfs-persistence/src/diagnostic/step.rs)).
`configuration` is the address, its options or the saved login before any
dial; `transport` no server answered (the driver makes DNS, TCP and TLS as
one step, so a TLS failure is `transport` too); `authentication` a login
the server refused; `database` a database it does not have; `schema` no
Store schema, or a role that may not use it or lacks a privilege a
Sandbox needs; `identity` a schema holding another Store's id.

`--dry-run` makes the same checks and saves and records nothing (no
record, password or reach), and no check refuses it. It answers `{ ok,
outcome: "tested", store, url, here, machines }`: `here` this computer's
check, `{ ok: true, latency_ms, server_version }` or `{ ok: false, step,
code, message }` (`code` the refusal's public code); `machines` one such row per attached machine with its
`machine` (each checked for any change, the setting alone included), or,
when this computer's check failed, each with `ok: null` and no dial; `ok`
whether every check passed. A first connect's dry run checks from this
computer only and makes no schema: an empty database answers
`here.empty: true`. The password comes from stdin as usual; a connected
Store's saved one is used when none is given. Input refusals (`InvalidInput`,
`Exists`) refuse a dry run as they do a connect.

| Refusal | When |
| --- | --- |
| `InvalidInput` | a URL that is not PostgreSQL, names a user or holds a password, holds an `@` or `#`, or that the PostgreSQL client does not read; a bad `--synchronous-commit`; `--user` or the password missing on a first connect; a connected Store whose record names no user or whose password this home does not hold, when none is given |
| `Exists` | a sqlite Store of that name, or another Store of this home with that database's id |
| `Unavailable` | no server answered, the server refused the login or the database, or the address holds another Store; `step` says which |
| `Conflict` | an attached machine does not reach a new address or login (`machines`); the Store was disconnected or connected to another database meanwhile |

Run on macOS native against a local `postgres:17`, with a debug build:

```console
$ layerfs store connect shop-pg 'postgresql://postgres:pw@127.0.0.1:55231/shop'
{"error":"InvalidInput","message":"the URL names a user or holds a password (or a password file); give the address alone and the login with --user and --password-stdin; an `@` or `#` that is part of the address is written %40 or %23"}
$ printf '%s' "$PGPASSWORD" | layerfs store connect shop-pg postgresql://127.0.0.1:55231/shop --user layerfs --password-stdin
{"ok":true}
```

### store.disconnect

Forget a PostgreSQL Store, its saved login and its attachments; its data stays.

```text
CLI     layerfs store disconnect <store>
Node    await layerfs.store.disconnect(store)
Python  layerfs.store.disconnect(store)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a Store name |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, store` |
| call | writes one call-log line; MCP `admin` |

Forgets a PostgreSQL Store in this home with its saved password and its
attachments; the database keeps every byte and `store connect` can record it
again. No server is contacted and no outcome is uncertain. The long form is
`ok, outcome ("disconnected"), store`.

Refusals: `InvalidInput` (a sqlite Store: `store remove` deletes one);
`InUse` (a Sandbox or Workspace uses the Store, another process holds its
lease, or a call in this process is running on it;
[`open.rs`](../../../../control/crates/layerfs-store/src/open.rs)).

### store.attach

Let a machine run Sandboxes of a PostgreSQL Store, checked from that machine first.

```text
CLI     layerfs store attach <store> --machine M [--dry-run]
Node    await layerfs.store.attach(store, { machine, dry_run })
Python  layerfs.store.attach(store, *, machine, dry_run=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a PostgreSQL Store |
| `machine` option | name | the machine; required |
| `dry_run` option | bool | check from the machine as attach does, and record nothing |

| Answer | Keys |
| --- | --- |
| short | `ok`, `outcome`; when set: `latency_ms` |
| long | `ok, outcome, store, machine, latency_ms, server_version` |
| call | writes one call-log line; MCP `admin` |

A machine runs Sandboxes of a PostgreSQL Store only once it is attached
([`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs)
`attach`). The machine must exist and its platform run PostgreSQL Sandboxes
on this host; the Store is then checked from the machine with the URL and
login this computer saved, as its Sandboxes dial them
([`layerfs-platform/src/reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs)).
Only after the check succeeds is the machine added to the record's
`attached` list
([`postgresql/check.rs`](../../../../control/crates/layerfs-store/src/postgresql/check.rs)
`attach`) and its reach written to `store/<store>/observed.json`
(`reach.<machine>`). Attaching an attached machine checks it again: the
outcome is `checked` instead of `attached`, and a failed check is recorded
as its reach while the attachment stays. A machine that reaches the server
only through this computer's loopback is refused with advice to connect the
Store again with an address the machine reaches. Machine native is attached,
checked and detached like any other machine, its platform deciding whether
a loopback URL is allowed (it shares this computer's loopback on macOS and
Linux); its reach is its own, apart from this computer's. `store connect`
attaches nothing.

A refused check names the `machine` and the connection `step` it stopped at
(as `store connect`'s do), flat beside `error` and `message`; a loopback
address the machine does not share is refused `Unsupported` at
`configuration`, before any dial, and a machine whose worker was not reached
names no step. `--dry-run` makes the same checks (the Store, the machine's
platform, the check from the machine) and records nothing: neither
`attached` nor the reach. It answers `outcome: tested` or the refusal attach
would give, and takes no lock, so dry runs of several machines of one Store
may run at once (the UI's per-machine Test).

| Refusal | When |
| --- | --- |
| `InvalidInput` | a sqlite Store (`store move` moves one to a machine) |
| `NotFound` | no such Store or machine |
| `Unsupported` | the machine's platform runs no PostgreSQL Sandboxes on this host |
| `Unavailable` | the machine is stopped or does not answer; the server does not answer the machine, or holds another Store |
| `Denied` | the server refuses the login from the machine, or the role lacks a privilege the Store needs |
| `Conflict` | the Store was disconnected or connected again meanwhile |

### store.detach

Stop a machine running Sandboxes of a PostgreSQL Store; refused while one runs there.

```text
CLI     layerfs store detach <store> --machine M
Node    await layerfs.store.detach(store, { machine })
Python  layerfs.store.detach(store, *, machine)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a PostgreSQL Store |
| `machine` option | name | the machine; required |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, store, machine` |
| call | writes one call-log line; MCP `admin` |

Removes the machine from the record's `attached` list and forgets its reach;
no server or machine is contacted. Refused `InUse` while a Sandbox of the
Store's projects is on the machine (the message counts and names them, `next`
lists them, and the `fix` removes them before the detach;
[`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)
`store_idle_on`), `NotFound` when the machine is not attached, and
`InvalidInput` for a sqlite Store. The long form is `ok,
outcome ("detached"), store, machine`.

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
Nothing is automatic: list, preview and audit write nothing, the doctor only
reports the same rows without releasing them, and no action deletes a live
row.

```text
CLI     layerfs store repair-mount-holders <project> <branch> <action> [--op HEX] [--legacy] [--force] [--operator NAME] [--limit 100] [--cursor C]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | the Branch whose holders are repaired; `audit` reads the rows recorded under this name, so a removed Branch's rows stay readable |
| `action` | name | `list`, `preview`, `settle`, `release`, `settle_legacy` or `audit` |
| `op` option | text | the holder `settle` and `release` act on: 16 bytes of hex |
| `legacy` option | bool | `settle` only a row without owner or place evidence |
| `force` option | bool | `settle` a holder that still looks `live` |
| `operator` option | text | the label a release records: at most 128 characters, no control characters; empty is `local`; not an authenticated principal |
| `limit` option | int | rows of one page (`list`, `preview`, `audit`): 1 to 1000, default 100 |
| `cursor` option | text | the `next_cursor` of the page before; it carries the project, Branch and action, and the place the page ended (the last holder op, or for `audit` the last row's `updated_ms` and op) |

| Action | Effect |
| --- | --- |
| `list` | one page of holders by op, read-only |
| `preview` | the same page (what `settle` and `release` would act on), read-only |
| `settle` | marks exactly `--op` uncertain, answering `{ action, op, state: "uncertain" }`; a holder whose `liveness` is `live` is refused `InUse` unless `--force` (or `--legacy` for a row without evidence); a row released since it was listed is `NotFound`; a later call reports `Unresolved`, never `Busy` |
| `release` | needs the preceding settle; retains the row in state `done` with `action=release;operator=NAME;op=HEX` in `evidence` (replacing the `place:v1;...` place tokens a live row keeps there) and clears the owner columns, so the unique owner key cannot block a remount, answering `{ action, op, state: "done" }`. The row is stamped no earlier than one millisecond after the Branch's newest done row, so a host whose clock is behind still ranks it newest. A Branch keeps its newest 100 done rows: a release deletes the oldest beyond them, never the row it retains. On PostgreSQL the holder's own session keeps its advisory keys until the owner releases its op (that release unlocks them and leaves the done row) or the session closes |
| `settle_legacy` | marks only rows with missing owner or place evidence: `{ action, settled }` |
| `audit` | one page of the Branch's retained `done` rows, oldest first by `(updated_ms, op)`; the next page starts strictly after the last row, so a release between two pages repeats no row and skips none of the Branch's own (releases are serialized and each is stamped after its Branch's newest done row). The audit selects by project and Branch name, while the stamp follows only the current Branch's rows: after a Branch (or its project) is removed and created again under the same names, a release of the new Branch whose host clock is not past the earlier Branch's newest stamp sorts among the earlier rows, and a cursor already past them misses it; a read without `cursor` returns every retained row. Changes nothing |

A page answers `{ action, items, next_cursor }`. Each holder row carries the
typed view (`op`, `project`, `branch`, `workspace`, `sandbox`, `machine`,
`owner_runtime`, `owner_boot`, `machine_incarnation`, `sandbox_generation`,
`legacy`) and three state words: `state`, the row's own state (`live`, or
`uncertain` once settled); `liveness`, what admission would say of it now
from the owner check and this home's place view (`live`, `uncertain` or
`dead`; a shared PostgreSQL Store proves no death from a place, so there a
row is never `dead` from its place evidence, which gives `uncertain` at most,
and only this process's own owner check can call one `dead`; a holder on
another host stays `live`, so most PostgreSQL settles need `--force`); and
`safe`, true unless it still looks
`live`. A worker's call is sent the place view of the calling home. Every
call is in the home's `trace/calls.jsonl` call log. On a SQLite Store the call
runs on the machine that holds the file through its worker; a PostgreSQL Store
runs in the calling process.

| Error | When |
| --- | --- |
| `InvalidInput` | an unknown action; a missing or malformed `--op`; a bad operator label; a `limit` out of range; a cursor of another action, project or Branch, or one whose place cannot be read |
| `NoSuchStore` | no Store of that name in this home |
| `NoSuchProject` | no project of that name in the Store |
| `NoSuchBranch` | no Branch of that name (every action but `audit`, which reads the names the rows recorded) |
| `NotFound` | no mount holder with that op at the Branch (for a settle or release: already released, on either Store kind) |
| `InUse` | `settle` of a holder that still looks `live`; `fix: … settle --op HEX --force` |
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
| `machine` option | name | check a PostgreSQL Store from this computer and this attached machine only |
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
`store.record` (its record reads and names it), `store.operation` (an
operation whose owner is gone with its outcome unknown is an error whose
`fix` is the call that clears it; for a sqlite Store it also reads the
compaction record beside the file through its machine), `store.connection`
(a sqlite Store's file is where its record says, read through its machine; a
PostgreSQL Store's server answers this computer with the saved login),
`store.identity` (the Store's id is the recorded one), `store.schema` (its
schema is this build's) and, for a sqlite Store, `store.backups` (a notice
per bucket whose backup heads differ, `fix: layerfs backup create <store>
<bucket>`). No Store data is scanned: `project doctor --full` reads a
project's data. A PostgreSQL Store reached from this computer is then
checked from every attached machine, or from `--machine` alone, each as one
more `store.connection` row with `from`; when this computer cannot connect,
no machine is checked (each would dial the same URL with the same login). A
`--machine` that is not attached is refused `InvalidInput` with `store attach
<store> --machine <machine>` as its fix; a sqlite Store refuses `--machine`. A Store whose machine
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
