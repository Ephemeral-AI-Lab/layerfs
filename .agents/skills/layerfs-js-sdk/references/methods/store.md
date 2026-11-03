# store calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. `create`, `list`, `get`, `status` and a `remove`
> refusal were run on macOS in a private home on 2026-11-02. `connect`,
> `push`, `pull`, `disconnect` and a successful `remove` were not run; those
> rows come from the table, S §3.8, #536 §7.3 and source. An agent working on a project rarely needs
> this family.

```js
store.list()
store.get(name)
store.status(name)
store.create(name)
store.connect(name, url, { password, synchronous_commit })
store.push(name, machine, { on_progress })
store.pull(name, { on_progress })
store.remove(name)
store.disconnect(name)
```

A Store holds projects and all their history. The verb says the kind:
`create` makes a sqlite Store (one file, on one machine at a time; it starts on
`native`); `connect` attaches a PostgreSQL database. Every sqlite Store is
Disposable (WAL, `synchronous=OFF`), the only allowed profile.

## Reads

`store.list()` reads the home's records: `{ items: [record, …] }`, each
item the record `store.get` shows plus a last `state`, derived as
`store.status` derives it (from source and tests, #536 §10b; not run here).
It starts no machine but asks a sqlite Store's machine's recorded worker,
once per machine, whether the file is there; a machine that does not answer
makes its Stores `down`. `store.get(name)` is one record (no `state`):

```json
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable","created":1793574061147,"sqlite":"3.53.2","projects":["api"]}
```

`store.status(name)` is the live state, read from the Store's machine:

```json
{"name":"shop","kind":"sqlite","state":"idle","holders":[],"op":null,"machine":"native",
 "file":{"exists":true,"bytes":212992,"wal_bytes":799312}}
```

`holders` are processes holding the Store open; `op` names a push, pull or remove
under way (`{ id, name, phase, started, uncertain }`; `get` and `list` items
show it too). For a PostgreSQL Store a `server`
field carries the server's status instead of `file` (not run here).
`store.status` starts no machine (from source, `75f83ec9b6`): a Store whose
machine is stopped, or whose worker does not answer, is `state: "down"` with
`down_reason` and `file.unavailable`. `state` is `idle`, `in_use`, `busy`
(a live `op`), `down` or `unresolved` (a gone process's uncertain `op`).

## Changes

| Call | What it does | Notes |
| --- | --- | --- |
| `create(name)` | An empty sqlite Store on `native` | Live result: the record above, with `projects: []`. `Exists` for a taken name |
| `connect(name, url, …)` | Attaches a PostgreSQL database | `url` is `postgresql://<user>@<host>:<port>/<database>[?sslmode=…]` without a password (one in the user part or in any query parameter whose name contains `password` is `InvalidInput`); `password` is a reference, `env:LAYERFS_<NAME>` or `file:/path`. `synchronous_commit` defaults to `off`. The same database under a second name is `Exists` |
| `push(name, machine, …)` | Sends a sqlite Store's file from `native` to `machine` | `InvalidInput` unless the Store is on `native` (pull it first). Progress `{ phase, done, total }` on `on_progress` / `--progress`. `InUse` while the Store is held (Sandbox records, processes with it open); `Unavailable` naming `layerfs machine start <m>` for a stopped machine; while it runs, Store calls and operations on it are `Busy`, from the caller's process too. An uncertain push whose process is gone leaves every later call `Unresolved`, naming `store status` and `store remove` |
| `pull(name, …)` | Brings a sqlite Store's file from its machine to `native` | `InvalidInput` when it is already on `native`; otherwise as `push`. Between two machines: `pull`, then `push` |
| `remove(name)` | Deletes a sqlite Store with every project in it | Returns `{ removed }`. `NotFound` for an unknown name (live); `Unavailable` for a Store whose machine is unknown; `Busy` while a live process pushes, pulls or removes it. It clears an `Unresolved` Store. Backups stay |
| `disconnect(name)` | Forgets a PostgreSQL Store; its data stays | `InUse` while another process holds the Store (its calls hold it while it stays open) or a Sandbox of it runs |

Pushing or pulling a Store moves where its Sandboxes can run: a Sandbox of a sqlite Store
runs only on the Store's machine.
