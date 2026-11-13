# store family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `store` calls: create, connect, inspect, move and remove the databases that hold every project's history, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/store`](../../../../api/crates/layerfs-api/src/store), [`layerfs-store/src`](../../../../control/crates/layerfs-store/src) (`sqlite/`, `postgresql/`, `list.rs`, `status.rs`, `stats.rs`), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) |
| Runs by default | the caller's process (records, locks, leases, PostgreSQL connections); a sqlite Store's file steps run in its machine's worker (`store.file`) when it is not on a locally executing machine |
| Design | [Stores and projects](../../control/stores-and-projects/README.md) (records, placement, leases), [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md) (transfer phases, connect and attach), [Engine: Store](../../engine/store/README.md) |

## Calls

```text
  reads, any time:  list    get    status    stats
                         |
     sqlite              |                 PostgreSQL
     ------              |                 ----------
     create (on native)  |                 connect (url, password ref)
        |                |                    |  attached to native
        v                |                    v
     push --> on <machine> --> pull        attach <machine> [url, password]
        |     (back to native)             detach <machine>
        v                                     v
     remove (file + projects)              disconnect (data stays)
```

| Call | Kind | Returns |
| --- | --- | --- |
| [`store.list`](#storelist) | both | `{ items: [record + projects + state] }` |
| [`store.get`](#storeget) | both | the record with `projects` |
| [`store.status`](#storestatus) | both | `{ name, kind, state, holders, op, down_reason, machine/file \| server }` |
| [`store.stats`](#storestats) | both | publication and allocation counters |
| [`store.create`](#storecreate) | sqlite | the new record |
| [`store.connect`](#storeconnect) | PostgreSQL | `{ name, kind, schema, synchronous_commit, machines, projects }` |
| [`store.attach`](#storeattach) | PostgreSQL | the Store as `get` shows it |
| [`store.detach`](#storedetach) | PostgreSQL | the Store as `get` shows it |
| [`store.push`](#storepush) | sqlite | `{ store, from, to, bytes, heads }` |
| [`store.pull`](#storepull) | sqlite | `{ store, from, to, bytes, heads }` |
| [`store.remove`](#storeremove) | sqlite | `{ removed: name }` |
| [`store.disconnect`](#storedisconnect) | PostgreSQL | `{ disconnected: name }` |

A sqlite Store is made with the Disposable profile (WAL,
`synchronous=OFF`), the only permitted profile. Durable execution is
`NOT_RUN — disabled by owner until explicit reauthorization`. A `name` is a
Store name: 1–63 of `a-z 0-9 _ . -`, starting and ending with a letter or
digit, not a Windows device name (`StoreName`,
[`names.rs`](../../../../control/crates/layerfs-home/src/names.rs)). A
folder scope refuses every `store` call.

The **record** (`StoreRecord::to_sdk`,
[`record.rs`](../../../../control/crates/layerfs-store/src/record.rs)) never
shows `cursor`, `binding` or `v`:

| Field | Kind | Meaning |
| --- | --- | --- |
| `name`, `kind` | both | `sqlite` or `postgresql` (a record without `kind` is sqlite) |
| `machine`, `profile`, `created`, `sqlite` | sqlite | the machine that holds the file; `disposable`; ms since the epoch; the SQLite version that made the file (`create` only) |
| `last_transfer` | sqlite | the last push or pull `{ call, from, to, at, bytes, sha256, heads, retired }`; `retired: false` means the source copy is not yet deleted |
| `incoming_on` | sqlite | a machine a certain-failed push left a partial `.incoming` copy on; the next push or remove deletes it ([`sqlite/transfer/resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)) |
| `backups` | sqlite | one link per bucket: the file, its heads, the last `create` or `restore` ([`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs)) |
| `url`, `schema`, `synchronous_commit`, `password`, `machines` | PostgreSQL | the URL without a password, always `layerfs`, the setting, the password reference, attached machines `{ url?, password?, checked? }` |
| `op` | both | the recorded intent `{ id, name, phase, started, uncertain }` ([`op.rs`](../../../../control/crates/layerfs-home/src/op.rs)) |
| `projects` | both | the home's project records for this Store, sorted |

| `state` (list, status) | When, first match wins |
| --- | --- |
| `busy` | the record's `op` belongs to a live process |
| `unresolved` | the `op` is uncertain and its process is gone |
| `down` | a sqlite Store whose machine did not answer `stat` |
| `idle`, `in_use` | no other live process holds its lease, or one does |

### store.list

Every Store of the home, each with its record, projects and state.

```text
JS      await layerfs.store.list()
Python  layerfs.store.list()
CLI     layerfs store list
```

Reads every `store/*/record.json` and the leases without a lock; a sqlite
Store not on a local machine gets one `store.file stat` in its machine's
worker, and a machine that fails once is `down` for its other Stores too.
PostgreSQL servers are not dialed. It starts nothing and changes nothing
([`list.rs`](../../../../control/crates/layerfs-store/src/list.rs)). No paging.
Error: `Failed` (a record or lease folder cannot be read).

### store.get

One Store's record and its project names, from the home alone.

```text
JS      await layerfs.store.get(name)
Python  layerfs.store.get(name)
CLI     layerfs store get <name>
```

No machine, worker or server is contacted; no `state` (use `status`).
Errors: `NotFound`, `Failed` (the record cannot be read).

### store.status

One Store's live state: who holds it, the operation on it, and its file or
server. Changes nothing.

```text
JS      await layerfs.store.status(name)
Python  layerfs.store.status(name)
CLI     layerfs store status <name>
```

```text
{ name, kind, state, holders: [pid], op, down_reason, ... }
 sqlite:     machine, file: { exists, free, bytes, wal_bytes, incoming_bytes }
                          | { unavailable }
 PostgreSQL: server: { reachable: true, server_version, latency_ms,
                       latency_us, schemas: { layerfs: <bytes> | null } }
                   | { reachable: false, error: { code, message } }
                   | { unavailable }
```

`holders` are live processes other than this one holding its lease; stale
leases are ignored, not removed
([`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs)). The
password reference is resolved here and the server dialed once; a
PostgreSQL Store is never `down` (an unreachable server shows in `server`)
([`status.rs`](../../../../control/crates/layerfs-store/src/status.rs)). It is
the call an `Unresolved` refusal names. Errors: `NotFound`; `InvalidInput` (a
record of unknown kind); `Unavailable` (a 1.1.0 Store whose machine is not
known; `layerfs import-env` fixes it).

### store.stats

Store-wide publication and allocation counters.

```text
JS      await layerfs.store.stats(name)
Python  layerfs.store.stats(name)
CLI     layerfs store stats <name>
```

```text
{ name, kind, history_logical, canonical, stored, metadata, file, free,
  init: null | { entries, files, bytes, canonical, stored, metadata, millis } }
```

| Field | Meaning |
| --- | --- |
| `history_logical` | saved-tree bytes summed over Commit and Layer records; each publication counts its tree again, so it is not a deduplication numerator |
| `canonical`, `stored` | allocated immutable content; removing a project does not reclaim it |
| `metadata` | accounted metadata plus database allocation overhead |
| `file` | database pages (WAL pages included on SQLite; the schema's relations in 8 KiB pages on PostgreSQL), not `status`'s file length |
| `free` | SQLite free-page bytes; null for PostgreSQL |

It reads the existing statistics totals in constant work, with no history,
tree or object scan ([`stats.rs`](../../../../control/crates/layerfs-store/src/stats.rs)).
A sqlite Store on another machine is read through its worker; a stopped
worker is `Unavailable`.

### store.create

A new empty sqlite Store on `native`, ready for `project.init`.

```text
JS      await layerfs.store.create(name)
Python  layerfs.store.create(name)
CLI     layerfs store create <name>
```

Returns `{ name, kind: "sqlite", machine: "native", profile: "disposable",
created, sqlite, projects: [] }`. Under the home lock (waits up to 120 s)
the call makes `store/<name>/` (0700), builds the empty file as
`store.sqlite.creating` (0600) and renames it to `store.sqlite`; a failure
before the record is written removes the call's folder
([`sqlite/create.rs`](../../../../control/crates/layerfs-store/src/sqlite/create.rs),
[`identity.rs`](../../../../control/crates/layerfs-store/src/identity.rs)).
To place it elsewhere, create it and then push it. Errors: `Exists` (a
record of that name, of either kind, or a file at the target); `Unavailable`
(a lock held 120 s); `Failed`.

### store.connect

Attaches a PostgreSQL database as a Store: an empty database gets the
`layerfs` schema; an existing one keeps its data and its projects become this
home's project records.

```text
JS      await layerfs.store.connect(name, url, { password, synchronous_commit })
Python  layerfs.store.connect(name, url, *, password=None, synchronous_commit=None)
CLI     layerfs store connect <name> <url> [--password <password>] [--synchronous-commit <synchronous-commit>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `url` | Text | `postgresql://` or `postgres://`, holding no password (no `user:pass@`, no query parameter whose name contains `password`); names its user when `password` is given |
| `password` option | Text | a reference, `env:NAME` or `file:/absolute/path` (trailing newline dropped), never the password; it must resolve now and is resolved again in the caller on every later call ([`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs)) |
| `synchronous_commit` option | Text | `off` (default), `local`, `remote_write`, `on` or `remote_apply`; applied to the Store's write transactions |

```text
{ name, kind: "postgresql", schema: "layerfs", synchronous_commit,
  machines: { native: { checked? } }, projects: [name] }
```

`native` is attached with the Store's own connection; `checked` is present
when the `store.attach` check passed. The schema is made before any record is
written, so a refusal after it leaves an empty schema and no record; connecting
again keeps its binding
([`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs)).

| Error | When |
| --- | --- |
| `InvalidInput` | a URL that is not PostgreSQL, holds a password, or lacks a user with `password`; a bad `synchronous_commit`; a password that is not a reference |
| `Unavailable` | the reference does not resolve; no session (no database, wrong user or password); a lock held 120 s |
| `Exists` | the name is taken, or another PostgreSQL Store of this home connects the same database (by URL, or by binding once read) |
| `Unsupported` | a build without the `postgresql` feature (the default Node, Python and CLI builds) |

### store.attach

Lets a machine run Sandboxes of a PostgreSQL Store, after checking from that
machine that it reaches the database and its role may write there.

```text
JS      await layerfs.store.attach(name, machine, { url, password })
Python  layerfs.store.attach(name, machine, *, url=None, password=None)
CLI     layerfs store attach <name> <machine> [--url <url>] [--password <password>]
```

`machine` must be a platform that runs PostgreSQL Sandboxes on this host (the
support table, as `sandbox.create`). `url` and `password` replace the Store's
for that machine, under `store.connect`'s rules; either may be given alone.
On an attached machine it checks the new connection and replaces the old one;
with no options it checks the current one again. The check runs where the
machine's Sandbox calls run (here, or the worker as `store.reach`): server
version, role, the `layerfs` schema, `USAGE` on it, `TEMP` on the database,
`SELECT`/`INSERT`/`UPDATE`/`DELETE` on each table and `USAGE` on each
sequence ([`access.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/access.rs)).
A failed check writes nothing; LayerFS never creates or grants roles
([`store.rs`](../../../../control/crates/layerfs-control/src/store.rs),
[`postgresql/attach.rs`](../../../../control/crates/layerfs-store/src/postgresql/attach.rs)).

| Error | When |
| --- | --- |
| `InvalidInput` | a sqlite Store (names `store push`); a URL holding a password |
| `Unsupported` | the platform runs no PostgreSQL Sandboxes here; an ssh loopback URL; no `postgresql` engine; a worker bundle without `store.reach` |
| `Unavailable` | the reference does not resolve; the machine is stopped; the server is not reached from it, or its database holds another Store or none (names the machine and host, never the URL) |
| `Denied` | the server refused the login, or the role lacks a privilege (names the first and how many) |
| `Conflict` | the Store was disconnected or reconnected elsewhere during the check |

### store.detach

Stops a machine running Sandboxes of a PostgreSQL Store. The database and
the Store's other machines are untouched.

```text
JS      await layerfs.store.detach(name, machine)
Python  layerfs.store.detach(name, machine)
CLI     layerfs store detach <name> <machine>
```

Dials nothing. Detaching `native` is allowed: Store calls still run in the
caller with the Store's connection; only Sandboxes on `native` are refused
afterwards. Errors: `InvalidInput` (a sqlite Store, or not attached there);
`InUse` (a Sandbox of the Store is recorded on the machine, `detail.related`,
[`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle_on`).

### store.push

Moves a sqlite Store's file from `native` to another machine, checked byte
for byte.

```text
JS      await layerfs.store.push(name, machine, { on_progress })
Python  layerfs.store.push(name, machine, *, on_progress=None)
CLI     layerfs store push <name> <machine> [--progress]
```

`machine` is a recorded machine other than `native`. Returns
`{ store, from: "native", to, bytes, heads }`; `heads` is the hex digest of
the Store's history heads, checked on the target. The record then names
`machine: <to>` with a `last_transfer`. Progress is `{ phase, done, total }`
through the phases `check`, `snapshot`, `send` (8 MiB pieces), `verify`,
`install`, `record`, `retire`; `done` and `total` are bytes in `snapshot`,
`send` and `verify`, else null
([`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) `progress`).
Each step runs once and a lost worker answer is never resent; what each
phase leaves behind on failure is on
[SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md).
Every refusal's message starts `store push, <phase>:` and carries
`detail.phase` ([`sqlite/transfer/phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs)).

| Error | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store; the Store is not on `native`; `machine` is `native` |
| `NotFound` | no such Store or machine; no Store file at snapshot |
| `InUse` | a live process holds the Store, or a Sandbox of one of its projects is recorded (`detail.related`) |
| `Exists` | the target already holds a Store file of that name |
| `Capacity` | the target's free disk is below the bytes plus max(1 GiB, 10%) (`detail.needed`, `detail.free`) |
| `Unavailable` | the target is stopped or its worker gone; names `layerfs machine start` |
| `Unsupported` | the target's worker does not report free disk (an older bundle) |
| `Conflict` | the Store moved between the check and the intent |
| `Failed` | a `-wal` no open-and-close could checkpoint; a digest mismatch at `verify`; I/O |
| `Unknown` | a worker's answer was lost, or any failure of `record` (both copies exist). Both copies are kept and the `op` stays uncertain; read `store.status`, then push again: it first deletes the copy the record does not name |

### store.pull

Brings a sqlite Store's file back to `native`: `store.push` with the sides
swapped.

```text
JS      await layerfs.store.pull(name, { on_progress })
Python  layerfs.store.pull(name, *, on_progress=None)
CLI     layerfs store pull <name> [--progress]
```

There is no target: a pull always ends on `native` (to move between two other
machines, pull, then push). Returns `{ store, from, to: "native", bytes,
heads }`. Errors as for push, with `InvalidInput` when the Store is already on
`native`. A `Failed` snapshot on a `-wal` means a process ended with the Store
open on its machine: open it there once (any Store call) and pull again.

### store.remove

Deletes a sqlite Store, wherever its file is, with every project in it;
also the call that clears an unresolved operation on the Store.

```text
JS      await layerfs.store.remove(name)
Python  layerfs.store.remove(name)
CLI     layerfs store remove <name>
```

Returns `{ removed: name }`. What goes: the file with its `-wal` and `-shm`,
`store/<name>/` in the home (record and project entries), its folder under a
worker's root, and a partial `.incoming` copy a push left elsewhere. What
stays: its backups in their buckets
([`sqlite/remove.rs`](../../../../control/crates/layerfs-store/src/sqlite/remove.rs)).
A gone process's unresolved intent does not refuse it (except a push's or
pull's, whose refusal names that push or pull, `clearing_call`).

| Error | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store (use `store.disconnect`) |
| `Busy` | another live process's operation runs on the Store |
| `InUse` | a live process, or this one, holds it; a Sandbox of one of its projects is recorded |
| `Conflict` | the Store moved to another machine meanwhile; nothing removed |
| `Unknown` | the worker's answer to `remove` was lost; `op` stays uncertain at phase `remove`; read `store.status`, then remove again |

### store.disconnect

Forgets a PostgreSQL Store in this home; the database keeps every byte and
`store.connect` can record it again.

```text
JS      await layerfs.store.disconnect(name)
Python  layerfs.store.disconnect(name)
CLI     layerfs store disconnect <name>
```

Returns `{ disconnected: name }`. No server is contacted and no outcome is
uncertain. Errors: `InvalidInput` (a sqlite Store; use `store.remove`);
`InUse` (another process holds its lease, a Sandbox of one of its projects is
recorded, or a call in this process is running on it,
[`open.rs`](../../../../control/crates/layerfs-store/src/open.rs)); `Failed`.

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name or option; a folder scope | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NotFound` | no Store of that name | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) |
| `Busy`, `Unresolved` | another operation's `op`; an unresolved one is shown by `store status` and cleared by `store remove` (or the same push or pull) | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | a stopped machine; a server that does not answer | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs), [`postgresql/open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| `Unsupported` | a build without the `postgresql` feature; new work on a removed platform's machine | [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs), [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| `Unknown` | a step's answer was lost; an uncertain push, pull or remove appends to `log/uncertain.jsonl`, and nothing is cleaned up on a guess | [`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) |

## Related

- [Stores and projects](../../control/stores-and-projects/README.md), [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md).
- [Engine: Store](../../engine/store/README.md), [Store calls](../../engine/store-calls/README.md).
- [backup family](backup.md), [machine family](machine.md).
