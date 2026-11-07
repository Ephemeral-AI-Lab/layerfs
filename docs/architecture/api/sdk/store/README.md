# store family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › store

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `store` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/store`](../../../../../api/crates/layerfs-api/src/store), [`control/crates/layerfs-control/src/store`](../../../../../control/crates/layerfs-store/src) |

## Calls

```text
  reads, any time:  list    get    status
                         |
     sqlite              |                 PostgreSQL
     ------              |                 ----------
     create (on native)  |                 connect (url, password ref)
        |                |                    |
        v                |                    |
     push --> on <machine> --> pull           |  project.init, branch.*,
        |     (back to native)                |  file.*, Sandboxes ...
        v                                     v
     remove (file + projects)              disconnect (data stays)
```

Nine calls, in help order
([`store/mod.rs`](../../../../../api/crates/layerfs-api/src/store/mod.rs)).
`connect` and `disconnect` run `layerfs_store::postgresql::connect`
(PostgreSQL); the others run `layerfs_store::sqlite::family` (sqlite, and
the reads of both kinds); `remove`, `push`, `pull` and `disconnect` go
through `layerfs_control::store` for their in-use check.
A Store is the one database file or schema that holds every project's
history ([glossary](../../../glossary.md)).

| Call | Arguments | Options | Returns |
| --- | --- | --- | --- |
| [`list`](list.md) | — | — | `{ items: [record + state] }` |
| [`get`](get.md) | `name` (Name) | — | the Store's record with `projects`; never the cursor key |
| [`status`](status.md) | `name` | — | `{ name, kind, state, holders, op, down_reason }` plus `machine` and `file` (sqlite) or `server` (PostgreSQL) |
| [`create`](create.md) | `name` | — | the new record: `machine: "native"`, `profile: "disposable"`, `created`, `sqlite`, `projects` |
| [`connect`](connect.md) | `name`, `url` (Text) | `password`, `synchronous_commit` (Text) | `{ name, kind: "postgresql", schema, synchronous_commit, projects }` |
| [`push`](push.md) | `name`, `machine` (Name) | `on_progress` (Function) | `{ store, from, to, bytes, heads }` |
| [`pull`](pull.md) | `name` | `on_progress` | `{ store, from, to, bytes, heads }` |
| [`remove`](remove.md) | `name` | — | `{ removed: name }` |
| [`disconnect`](disconnect.md) | `name` | — | `{ disconnected: name }`; the database keeps its data |

Sources: [`list.rs`](../../../../../control/crates/layerfs-store/src/list.rs),
[`status.rs`](../../../../../control/crates/layerfs-store/src/status.rs),
[`sqlite/create.rs`](../../../../../control/crates/layerfs-store/src/sqlite/create.rs),
[`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs),
[`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs),
[`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs),
[`sqlite/remove.rs`](../../../../../control/crates/layerfs-store/src/sqlite/remove.rs).
`create` makes the Disposable profile, the only permitted sqlite profile.

## Arguments and options

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a Store name (`StoreName::parse`) | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |
| `url` | Text | a PostgreSQL URL with no password in it | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| `password` | Text | a reference, `env:NAME` or `file:/path`, resolved on each call and never written down | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| `synchronous_commit` | Text | `off` (default), `local`, `remote_write`, `on` or `remote_apply` | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| `machine` | Name | `push` target; `pull` always brings the Store to `native` | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `on_progress` | Function | `PROGRESS`; events `{ progress: { phase, done, total } }` | [`calls/table.rs`](../../../../../api/crates/layerfs-api/src/calls/table.rs) |

```text
 store.push / store.pull: one pass of phases, each written to `op` first
 check -> snapshot -> send -> verify -> install -> record -> retire
  |                    (bytes)          '-- written uncertain before start
  '-- before `op`: sides, both machines, target file, free disk (Capacity)
```

A push or pull writes its `op` under the home lock and the Store's lock,
then works past the locks; a failure names its phase in
`detail.phase` and keeps whatever exists
([`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs)).

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name, URL with a password, bad `synchronous_commit`; `remove` of a PostgreSQL Store | [`postgresql/connect.rs`](../../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`sqlite/remove.rs`](../../../../../control/crates/layerfs-store/src/sqlite/remove.rs) |
| `NotFound` | no Store of that name | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) |
| `Exists` | the name is taken, the database is connected already, or the target machine has the file | [`sqlite/create.rs`](../../../../../control/crates/layerfs-store/src/sqlite/create.rs), [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `InUse` | a process holds the Store open or a Sandbox of it runs | [`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs), [`open.rs`](../../../../../control/crates/layerfs-store/src/open.rs) |
| `Busy`, `Unresolved` | another operation's `op`; shown by `store status`, cleared by `store remove` | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Capacity` | the target machine lacks room (`detail.needed`, `detail.free`) | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `Conflict` | the Store moved meanwhile | [`sqlite/remove.rs`](../../../../../control/crates/layerfs-store/src/sqlite/remove.rs), [`sqlite/transfer/resolve.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs) |
| `Unavailable` | a stopped machine, or a server that does not answer | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs), [`store/postgres.rs`](../../../../../control/crates/layerfs-store/src/postgresql/open.rs) |
| `Unsupported` | a build without the `postgresql` feature | [`service.rs`](../../../../../control/crates/layerfs-store/src/service.rs) |
| `Unknown` | **uncertain:** a step's answer was lost, or any failure of the `record` phase after both copies exist | [`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs) |

An uncertain push, pull or remove keeps both copies, leaves `op` marked
uncertain and appends to `log/uncertain.jsonl`; nothing is cleaned up on a
guess. See [error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

| Call | Caller's process | Machine's worker |
| --- | --- | --- |
| `list`, `status` | records, leases | `store.file` `stat` of a sqlite file on a worker machine; no machine is started |
| `get`, `create` | records; `create` makes the file on `native` | — |
| `connect`, `disconnect` | reads the schema over the network, writes records | — |
| `push`, `pull` | records, `op`, locks | `store.file` steps on each side that is not local |
| `remove` | records, `op`, locks | `store.file` steps when the file is on a worker machine |

`family::step` runs a step with `transfer::steps::run` in this process for
`Executor::Local`, else sends it as `store.file`
([`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs),
[`worker_server.rs`](../../../../../control/crates/layerfs-control/src/worker_server.rs)).
A PostgreSQL Store has no machine. The Store calls on a Store's projects
are the `branch`, `commit` and `file` families. See the
[routing table](../../../control/routing/routing-table.md).

## Example

Not run; written from the call table.

```js
await layerfs.store.connect("pg", "postgresql://app@db:5432/app", { password: "env:LAYERFS_PG_PASSWORD" });
await layerfs.store.push("shop", "vm1", { on_progress: console.log });
```

```python
layerfs.store.connect("pg", "postgresql://app@db:5432/app", password="env:LAYERFS_PG_PASSWORD")
layerfs.store.push("shop", "vm1", on_progress=print)
```

```console
$ layerfs store connect pg postgresql://app@db:5432/app --password env:LAYERFS_PG_PASSWORD
$ layerfs store push shop vm1 --progress
```

## Related

- [Control: Families](../../../control/management/families.md) and [Control: Home and records](../../../control/management/home-and-records.md).
- [Routing table](../../../control/routing/routing-table.md).
- [Engine: Store](../../../engine/store/README.md), [PostgreSQL](../../../engine/store/postgresql.md), [SQLite](../../../engine/store/sqlite.md).
- [backup family](../backup/README.md).

---

← [machine.update](../machine/update.md) · [Up: SDK](../README.md) · [store.list](list.md) →
