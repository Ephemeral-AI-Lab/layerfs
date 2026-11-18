# store family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `store` calls: create, connect, update, inspect, move, remove and compact the databases that hold every project's history, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/store`](../../../../api/crates/layerfs-api/src/store), [`layerfs-store/src`](../../../../control/crates/layerfs-store/src) (`sqlite/`, `postgresql/`, `list.rs`, `get.rs`, `state.rs`, `live.rs`, `stats.rs`, `intent.rs`), [`layerfs-control/src/store.rs`](../../../../control/crates/layerfs-control/src/store.rs) |
| Runs by default | the caller's process (records, locks, leases, PostgreSQL connections); a sqlite Store's file steps run in its machine's worker (`store.file`) when it is not on a locally executing machine |
| Design | [Stores and projects](../../control/stores-and-projects/README.md) (records, placement, leases), [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md) (transfer phases, connect, update and the check), [Engine: Store](../../engine/store/README.md) |

## Calls

```text
  reads, any time:  list    get [--with live,stats]
                         |
     sqlite              |                 PostgreSQL
     ------              |                 ----------
     create [--machine]  |                 connect (url, password ref)
        |                |                    |  runs on every machine
        v                |                    v
     move <store> <machine>                update [url, password, sync]
     move <store> native (back)            update --machine <m> (its own
        |                                     v            connection)
        v                                  disconnect (data stays)
     remove (file + projects)
     compact (reclaim dead bytes)
```

| Call | Kind | Returns (long form; the dispatcher makes the short form) |
| --- | --- | --- |
| [`store.list`](#storelist) | both | `{ items }`, each item as `get` answers it with `store` first |
| [`store.get`](#storeget) | both | the record and its records-only state; `live` and `stats` sections on request |
| [`store.create`](#storecreate) | sqlite | `{ ok, outcome: "created", store, engine, machine, profile, sqlite_version, created_at }` |
| [`store.connect`](#storeconnect) | PostgreSQL | `{ name, kind, schema, synchronous_commit, projects }` (not migrated) |
| [`store.update`](#storeupdate) | PostgreSQL | the Store's record (not migrated) |
| [`store.move`](#storemove) | sqlite | `{ moved_bytes, outcome: "moved", store, from, to, heads_sha256 }` |
| [`store.remove`](#storeremove) | sqlite | `{ ok, outcome: "removed", store }` |
| [`store.compact`](#storecompact) | both | `{ freed_bytes, outcome, store, ... }`, the flattened §11 report |
| [`store.disconnect`](#storedisconnect) | PostgreSQL | `{ disconnected: name }` (not migrated) |

`store.status` and `store.stats` were merged into `store.get` (`--with
live`, `--with stats`), and `store.push` and `store.pull` into
`store.move`, with no alias (call surface 2.0). A record a build before 2.0
wrote with a `store.push` or `store.pull` intent is read as the move it
was ([`intent.rs`](../../../../control/crates/layerfs-store/src/intent.rs) `is_move`).

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
| `last_transfer` | sqlite | the last move `{ call, from, to, at, bytes, sha256, heads, retired }`; `retired: false` means the source copy is not yet deleted |
| `incoming_on` | sqlite | a machine a certain-failed move to it left a partial `.incoming` copy on; the next move or remove deletes it ([`sqlite/transfer/resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)) |
| `backups` | sqlite | one link per bucket: the file, its heads, the last `create` or `restore`, and `attempt` `{ call, at, error }`, the last transfer that did not work ([`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs), [`get.rs`](../../../../control/crates/layerfs-store/src/get.rs) `ATTEMPT`) |
| `broken` | both | the reason word (`missing`, `unreachable`, `unresolved`) the last live read found wrong; absent when it found the Store well ([`state.rs`](../../../../control/crates/layerfs-store/src/state.rs) `SEEN`) |
| `url`, `schema`, `synchronous_commit`, `password` | PostgreSQL | the URL without a password, always `layerfs`, the setting, the password reference |
| `machines` | PostgreSQL | only the machines with a connection of their own, `{ <machine>: { url?, password? } }`; absent when none has one. Every other machine dials the Store's |
| `op` | both | the recorded intent `{ id, name, phase, started, uncertain }` of a move, remove, create on a machine, compaction, backup or restore ([`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`intent.rs`](../../../../control/crates/layerfs-store/src/intent.rs)); answers show it as `{ name, phase, started_at }` |
| `projects` | both | the home's project records for this Store, sorted |

| `state` | When, first match wins ([`state.rs`](../../../../control/crates/layerfs-store/src/state.rs) `recorded`) |
| --- | --- |
| `busy` | the record's `op` belongs to a live process (`op` companion) |
| `broken` `unresolved` | the `op`'s process is gone and its outcome unknown; `fix` names the clearing call (`intent::clearing`) |
| `blocked` | a sqlite Store whose machine is not ready (`blocked_by: { machine, reason }`, from the machine family's one derivation, `inventory::machine_state`, which the calls hand the Store code as `state::Machines`) |
| `broken` `unsupported` / `missing` / `unreachable` | a record kind this build does not read; or the reason the last live read wrote to `broken` |
| `ready` | otherwise |

This is the one records-only derivation every reader uses: `store.list`,
`store.get` and the Store nodes of `home.get --with tree`
([`home_tree.rs`](../../../../control/crates/layerfs-control/src/home_tree.rs)).
It contacts no machine, worker or server.

### store.list

Every Store of the home, from its records.

```text
JS      await layerfs.store.list({ machine, with: "stats" })
Python  layerfs.store.list(*, machine=None, with_="stats")
CLI     layerfs store list [--machine <machine>] [--with stats]
```

Reads every `store/*/record.json`; a machine's state is derived once for all
its Stores. `--machine` keeps the sqlite Stores on that machine. `--with
stats` adds each item's `size` (file plus write-ahead log) from one `stat`
on its machine, starting nothing; null for a PostgreSQL Store and for a
machine that does not answer. It changes nothing
([`list.rs`](../../../../control/crates/layerfs-store/src/list.rs)). No paging.

### store.get

One Store's record and records-only state.

```text
JS      await layerfs.store.get(store, { with: "live,stats" })
Python  layerfs.store.get(store, *, with_="live,stats")
CLI     layerfs store get <store> [--with live,stats]
```

The answer ([`get.rs`](../../../../control/crates/layerfs-store/src/get.rs)
`summary`) is `engine, state` and its companion (`op`, `blocked_by`,
`reason`, `fix`), `observed_at` (when it was last read live, from
`store/<store>/observed.json`, `layerfs_home::home::observed`), `machine`, `auth` (null until saved logins
land), `connections` (PostgreSQL: the Store's own address, then each
machine's own, never a password), `projects` (at most 20) and
`projects_count`, `backups` rows `{ bucket, file, sha256, heads_sha256,
last, attempt }`, `profile`, `synchronous_commit`, `sqlite_version`,
`created_at` and `last_transfer`; the other engine's keys are null.

**`--with live`** ([`live.rs`](../../../../control/crates/layerfs-store/src/live.rs))
reads the Store where it lives, starting nothing: a sqlite Store's file by
`stat`, its compaction record and its heads, in this process on `native`
or through its worker (`StoreStep::Stat`, `CompactOp`, `Heads`); a
PostgreSQL Store's server from this process. A blocked Store is not read.
The read sets `state`: `broken` `missing` (no file), `broken` `unreachable`
(no server), `busy` or `broken` `unresolved` from a compaction's own record,
`unknown` with the last known `observed_at` when the machine does not
answer. The record is written, under the Store's lock, only when the
reason it found changed (moving the home's generation); `observed_at` is
written to the observation file, which does not. A backup or restore whose
owner is gone with its outcome unknown is settled: the `op` is dropped and
what was tried is written to the bucket link's `attempt`. The `live`
section is `{ holders, phases, behind, server_version, latency_ms }`:
`holders` the live processes other than this one holding its lease (stale
leases are ignored, not removed, [`lease.rs`](../../../../control/crates/layerfs-store/src/lease.rs)),
`phases` a running compaction's phases, `behind` the buckets whose link
heads differ from the Store's (null for PostgreSQL or unread heads).

**`--with stats`** ([`stats.rs`](../../../../control/crates/layerfs-store/src/stats.rs))
opens the Store (refused while a move or remove holds it) and reads its
statistics totals in constant work: `size`, `logical_bytes`,
`canonical_bytes`, `stored_bytes`, `metadata_bytes`, `file_bytes`,
`reusable_bytes` (null for PostgreSQL), `dedupe` and `compression` (six
decimal text) and `init`. `logical_bytes` sums saved trees over Commit and
Layer records, so it is not a deduplication numerator.

Errors: `NotFound`, `Failed` (the record cannot be read).

### store.create

A new empty sqlite Store, ready for `project.init`: on `native`, or with
`machine` made on that machine directly through its worker
(`StoreStep::Create`, recording a `store.create` intent while it runs).

```text
JS      await layerfs.store.create(name, { machine })
Python  layerfs.store.create(name, *, machine=None)
CLI     layerfs store create <name> [--machine <machine>]
```

On `native`, under the home lock (waits up to 120 s)
the call makes `store/` (`0700`), creates `store/<name>/` (`0700`) when
absent and checks `store/<name>/store.sqlite` with `kernel_guard::check`
before building the empty file as `store.sqlite.creating` (`0600`) and
renaming it to `store.sqlite`. A failure of `make_empty` removes
`store/<name>/` only when this call's own `mkdir` made it (a pre-existing
folder or one made by another caller is never removed); once `make_empty`
succeeds, a `save_record` failure keeps the Store file and folder
([`sqlite/create.rs`](../../../../control/crates/layerfs-store/src/sqlite/create.rs),
[`identity.rs`](../../../../control/crates/layerfs-store/src/identity.rs)).
 Errors: `Exists` (a
record of that name, of either kind, or a file at the target);
`Unsupported` (the kernel guard refuses `store/<name>/`: not on a local
lock-carrying filesystem, or an existing directory is not owned by this
user, is a symlink, or has group/other access instead of `0700`);
`Unavailable` (a lock held 120 s); `Failed`.

### store.connect

Connects a PostgreSQL database as a Store: an empty database gets the
`layerfs` schema; an existing one keeps its data and its projects become this
home's project records. Its Sandboxes then run on every machine whose
platform runs PostgreSQL Sandboxes, with no further step.

```text
JS      await layerfs.store.connect(name, url, { password, synchronous_commit })
Python  layerfs.store.connect(name, url, *, password=None, synchronous_commit=None)
CLI     layerfs store connect <name> <url> [--password <password>] [--synchronous-commit <synchronous-commit>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `url` | Text | `postgresql://` or `postgres://`, holding no password (no `user:pass@`, no query parameter whose name contains `password`); names its user when `password` is given |
| `password` option | Text | a reference, `env:NAME` or `file:/absolute/path` (trailing newline dropped), never the password; it must resolve now and is resolved again in the caller on every later call ([`secrets.rs`](../../../../control/crates/layerfs-home/src/secrets.rs)) |
| `synchronous_commit` option | Text | `off`, `local` (default), `remote_write`, `on` or `remote_apply`; applied to the Store's write transactions |

```text
{ name, kind: "postgresql", schema: "layerfs", synchronous_commit,
  projects: [name] }
```

No machine is recorded: each machine dials the Store's connection (a
loopback host rewritten for it, [Reach](../../control/machines/workers-and-reach.md#reach-reachrs)),
and `sandbox.create` checks from the machine that it reaches this Store.
The schema is made before any record is written, so a refusal after it
leaves an empty schema and no record; connecting again keeps its binding
([`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs)).

| Error | When |
| --- | --- |
| `InvalidInput` | a URL that is not PostgreSQL, holds a password, or lacks a user with `password`; a bad `synchronous_commit`; a password that is not a reference |
| `Unavailable` | the reference does not resolve; no session (no database, wrong user or password); a lock held 120 s |
| `Exists` | the name is taken, or another PostgreSQL Store of this home connects the same database (by URL, or by binding once read) |

### store.update

Changes a PostgreSQL Store's connection, or gives one machine a connection
of its own.

```text
JS      await layerfs.store.update(name, { machine, url, password, synchronous_commit, clear })
Python  layerfs.store.update(name, *, machine=None, url=None, password=None, synchronous_commit=None, clear=None)
CLI     layerfs store update <name> [--machine <machine>] [--url <url>] [--password <password>] [--synchronous-commit <synchronous-commit>] [--clear]
```

| Form | Does |
| --- | --- |
| no `machine` | sets the Store's `url`, `password` reference and `synchronous_commit` (any of them). A new URL or password is dialled once from this process and must reach this Store: the same binding. `synchronous_commit` alone dials nothing |
| `machine` with `url` and/or `password` | sets the connection that machine alone dials, in place of the Store's; a field not given keeps the machine's own, else the Store's. Checked from the machine first ([`postgresql/check.rs`](../../../../control/crates/layerfs-store/src/postgresql/check.rs)) |
| `machine` with `clear` | removes the machine's own connection; it dials the Store's again. Dials nothing |

`url` and `password` follow `store.connect`'s rules. Every refusal of the
input comes before any connection, and a failed check saves nothing. The
record is written under the home lock and the Store's; this process's open
sessions on the Store are closed. A running Sandbox keeps the connection it
started with. LayerFS never creates or grants roles. Removing a machine
forgets its own connection in every Store
([`postgresql/update.rs`](../../../../control/crates/layerfs-store/src/postgresql/update.rs),
[`postgresql/check.rs`](../../../../control/crates/layerfs-store/src/postgresql/check.rs),
[`store.rs`](../../../../control/crates/layerfs-control/src/store.rs)).

| Error | When |
| --- | --- |
| `InvalidInput` | a sqlite Store; nothing to update; a URL holding a password; a bad `synchronous_commit`; `synchronous_commit` with `machine`; `clear` without `machine`, with `url` or `password`, or for a machine with no connection of its own |
| `NotFound` | no such Store or machine |
| `Unsupported` | the machine's platform runs no PostgreSQL Sandboxes here; a URL the platform cannot rewrite for the machine; a worker bundle without `store.reach` |
| `Unavailable` | the reference does not resolve; the machine is stopped; the server is not reached, or its database holds another Store or none (names the machine and host, never the URL) |
| `Denied` | for a machine: the server refused the login, or the role lacks a privilege (names the first and how many) |
| `Exists` | the new URL names a database another Store of this home connects |
| `InUse` | a call of this process is running on the Store |
| `Conflict` | the Store was disconnected or reconnected elsewhere during the check |

### store.move

Moves a sqlite Store's file between `native` and a machine, checked byte for
byte. One side is always `native`: a Store on a machine moves to another by
`store move <store> native` first (refused `InvalidInput` naming that call,
[`resolve.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs) `can_move`).

```text
JS      await layerfs.store.move(store, machine, { on_progress })
Python  layerfs.store.move(store, machine, *, on_progress=None)
CLI     layerfs store move <store> <machine> [--progress]
```

Returns `{ moved_bytes, outcome: "moved", store, from, to, heads_sha256 }`;
`heads_sha256` is the hex digest of the Store's history heads, checked on
the target. The record then names `machine: <to>` with a `last_transfer`.
Progress is `{ phase, done, total }` through `check`, `snapshot`, `send`
(8 MiB pieces), `verify`, `install`, `record`, `retire`; `done` and `total`
are bytes in `snapshot`, `send` and `verify`, else null. Each step runs once
and a lost worker answer is never resent; what each phase leaves behind on
failure is on [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md).
Every refusal's message starts `store move, <phase>:` and carries
`detail.phase` ([`phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs)).

| Error | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store; the Store is already on `machine`; neither side is `native` |
| `NotFound` | no such Store or machine; no Store file at snapshot |
| `InUse` | a live process holds the Store, or a Sandbox of one of its projects is recorded (`detail.related`) |
| `Exists` | the target already holds a Store file of that name |
| `Capacity` | the target's free disk is below the bytes plus max(1 GiB, 10%) |
| `Unavailable` | the machine is stopped or its worker gone; names `layerfs machine start` |
| `Conflict` | the Store moved between the check and the intent |
| `Failed` | a `-wal` no open-and-close could checkpoint (open the Store on its machine once and move again); a digest mismatch at `verify`; I/O |
| `Unknown` | a worker's answer was lost, or any failure of `record` (both copies exist). Both copies are kept and the `op` stays uncertain; read `store get --with live`, then move again: it first deletes the copy the record does not name |

### store.remove

Deletes a sqlite Store, wherever its file is, with every project in it;
also the call that clears an unresolved operation on the Store.

```text
JS      await layerfs.store.remove(name)
Python  layerfs.store.remove(name)
CLI     layerfs store remove <name>
```

Returns `{ ok, outcome: "removed", store }`. What goes: the file with its `-wal` and `-shm`,
`store/<name>/` in the home (record and project entries), its folder under a
worker's root, and a partial `.incoming` copy a move left elsewhere. What
stays: its backups in their buckets
([`sqlite/remove.rs`](../../../../control/crates/layerfs-store/src/sqlite/remove.rs)).
A gone process's unresolved intent does not refuse it (except a move's,
whose refusal names that move, `intent::clearing`).

| Error | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store (use `store.disconnect`) |
| `Busy` | another live process's operation runs on the Store |
| `InUse` | a live process, or this one, holds it; a Sandbox of one of its projects is recorded |
| `Conflict` | the Store moved to another machine meanwhile; nothing removed |
| `Unknown` | the worker's answer to `remove` was lost; `op` stays uncertain at phase `remove`; read `store get --with live`, then remove again |

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

### store.compact

Reclaims one Store's dead bytes by rebuilding it from its live history
(spec §3, §7 on SQLite, §8 on PostgreSQL): the
only byte-reclaiming call, with no confirm, force or dry
run and no automatic collection. It writes its operation intent before the
first checkpoint, verifies the rebuilt Store, and installs it by rename; a
definite failure before the rename keeps the old Store and the generation `G`,
while an installed-but-uncertain reply stays at `G + 1` and is reported, never
deleted. A Store with less than 10% dead bytes takes the `none` path:
it marks only and writes nothing. On PostgreSQL the dead bytes are those of
wholly dead packs and a retained pack is copied whole. On SQLite they also
include the dead records the mark found inside retained packs, and the rebuild
writes each such pack again without them: the pack keeps its id and gets a new
digest and length, and `kept_waste.dead_in_retained_packs_bytes` reports only
what the rebuild still kept (accepted by the owner,
[`gc-42-decisions.md`](../../../../core/docs/issues/project-41/gc-42-decisions.md)
D6). A SQLite rebuild also copies only the inode value blocks a kept inode
leaf reads, and `kept_waste.inode_blocks_bytes` then reports the blocks the
rebuilt Store holds; PostgreSQL and the `none` path report every block (accepted by
the owner, D7). `dead_bytes_refined` equals
`dead_bytes_bound` on every path, not only on `none`: the mark resolves the
whole retained set and no second payload sweep runs (documented amendment
#42 A10).

A `<store>.compact-op` record belongs to an in-flight run only until that run
replies: the run marks the record returned before it returns. The next call
reads the recorded pid, kernel start time, boot and PID namespace and refuses
`Busy` (as `BusyOp`, naming the record's entity, phase and operation) only
while that process is provably live and the run has not returned, so a run that
released the writer lane just before its `cleanup` settle stays protected. A
returned run's record, whether the run left a residue or an unknown outcome, is
a leftover the next call adopts and settles even though the owner process is
alive. The settle and a pre-install abort remove the record only while it still
names the operation being settled.

```text
JS      await layerfs.store.compact(name, { on_progress })
Python  layerfs.store.compact(name, on_progress=None)
CLI     layerfs store compact <name> [--progress]
```

`on_progress` receives `{ phase, done, total }` as each phase starts, with
null counts: `mark`, `build`, `verify`, `install` on SQLite and `mark`,
`build`, `swap`, `cleanup` on PostgreSQL; a run that takes the `none` path
sends `mark` alone. The events are for the caller only and are held in memory:
the run's op record (and the `store.compact` intent it records in the
Store's record while it runs) is what [`store.get`](#storeget) shows to another
process and what a later call judges after a crash (#42 A13).

Returns the flattened report ([`report.rs`](../../../../control/crates/layerfs-store/src/report.rs)
`compact_report`): `freed_bytes`, `outcome` (`compacted`, or `unchanged`
on the `none` path), `store`, `generation_before`, `generation_after`,
`dead_bytes`, `discarded_bytes`, `reusable_bytes`, `kept_waste
{dead_in_retained_packs_bytes, delta_bases_bytes, inode_blocks_bytes}`,
`delta_base_references` (at most 20) with `delta_base_references_count`,
`peak_temp_bytes`, `residue_bytes`, `residue_artifacts`, `residue_reason`,
`estimate` and `zero_reasons`.
`mark_bytes` is not a result field: PostgreSQL records the §11 `markBytes`
transfer metric as `compact.mark_bytes` (a #52 measurement: the distinct
live-pack body extents its Phase M walk reads, each pack counted once), and
SQLite, whose payload walk runs inside the mark, reports no value.

| Error | When | Source |
| --- | --- | --- |
| `NoSuchStore`, `Unsupported` | a missing Store; a non-private Store directory; or an unsupported filesystem. A sqlite Store on another machine is compacted by that machine's worker (spec S6, #42 D5) | [`compact.rs`](../../../../control/crates/layerfs-store/src/compact.rs) |
| `Unavailable`, `Unknown` | a sqlite Store on another machine whose worker is not running (nothing is started; the message names `layerfs machine start`), or whose answer was lost after the request was sent: the run may have finished, and its op record lets the next `store.compact` judge it | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `MountsActive`, `Busy`, `Unresolved` | a mount, long-lived client or operation holds the Store, or a dead owner left an uncertain one | spec §5 |
| `Busy` | a call still running on the Store in this process, so the Store cannot be closed for the rebuild: the open cache's refusal is reported as `Busy`, naming the Store as `entity` with `phase` and `op` null | spec §10 |
| `StaleStore`, `Conflict` | `StaleStore` for a connection that bypassed admission and touches the Store after the install; `Conflict` when the sealed state changed at install (sidecar, `(dev, ino)` identity, or the sealed size/mtime/change counter) | spec §7, §10 |
| `Capacity` | a real allocation refusal during the rebuild; the old Store is kept | spec §7 |
| `Unknown` | the reply was lost around install; read `store get --with live` and the op before any second call | spec §9 |

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name or option; a folder scope | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NotFound` | no Store of that name | [`record.rs`](../../../../control/crates/layerfs-store/src/record.rs) |
| `Busy`, `Unresolved` | another operation's `op`; an unresolved one is shown by `store get` and cleared by the call its `fix` names (`store remove`, the same move, or the next `store compact`) | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | a stopped machine; a server that does not answer | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs), [`postgresql/open.rs`](../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| `Unsupported` | new work on a removed platform's machine | [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs), [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| `Unknown` | a step's answer was lost; an uncertain move or remove appends to `log/uncertain.jsonl`, and nothing is cleaned up on a guess | [`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) |

## Related

- [Stores and projects](../../control/stores-and-projects/README.md), [SQLite and PostgreSQL](../../control/stores-and-projects/sqlite-and-postgresql.md).
- [Engine: Store](../../engine/store/README.md), [Store calls](../../engine/store-calls/README.md).
- [backup family](backup.md), [machine family](machine.md).
