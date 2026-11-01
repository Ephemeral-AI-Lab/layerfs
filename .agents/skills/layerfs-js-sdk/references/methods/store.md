# store calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. `create`, `list`, `get`, `status` and a `remove`
> refusal were run on macOS in a private home on 2026-11-02. `connect`,
> `move`, `disconnect` and a successful `remove` were not run; those rows come
> from the table, S §3.8 and source. An agent working on a project rarely needs
> this family.

```js
store.list()
store.get(name)
store.status(name)
store.create(name)
store.connect(name, url, { password, synchronous_commit })
store.move(name, machine, { on_progress })
store.remove(name)
store.disconnect(name)
```

A Store holds projects and all their history. The verb says the kind:
`create` makes a sqlite Store (one file, on one machine at a time; it starts on
`native`); `connect` attaches a PostgreSQL database. Every sqlite Store is
Disposable (WAL, `synchronous=OFF`), the only allowed profile.

## Reads

`store.list()` reads the home's records, with no network call:
`{ items: [record, …] }`. `store.get(name)` is one record:

```json
{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable","created":1793574061147,"sqlite":"3.53.2","projects":["api"]}
```

`store.status(name)` is the live state, read from the Store's machine:

```json
{"name":"shop","kind":"sqlite","state":"idle","holders":[],"in_progress":null,"machine":"native",
 "file":{"exists":true,"bytes":212992,"wal_bytes":799312}}
```

`holders` are processes holding the Store open; `in_progress` names a move
under way (`{ from, to, pid, started, … }`). For a PostgreSQL Store a `server`
field carries the server's status instead of `file` (not run here).

## Changes

| Call | What it does | Notes |
| --- | --- | --- |
| `create(name)` | An empty sqlite Store on `native` | Live result: the record above, with `projects: []`. `Exists` for a taken name |
| `connect(name, url, …)` | Attaches a PostgreSQL database | `url` is `postgresql://<user>@<host>:<port>/<database>[?sslmode=…]` without a password; `password` is a reference, `env:LAYERFS_<NAME>` or `file:/path`. `synchronous_commit` defaults to `off` |
| `move(name, machine, …)` | Moves a sqlite Store's file to another machine | Progress `{ phase, done, total }` on `on_progress` / `--progress`. `InUse` while the Store is held (Sandboxes running, processes with it open) |
| `remove(name)` | Deletes a sqlite Store with every project in it | Returns `{ removed }`. `NotFound` for an unknown name (live). Backups stay |
| `disconnect(name)` | Forgets a PostgreSQL Store; its data stays | |

Moving a Store moves where its Sandboxes can run: a Sandbox of a sqlite Store
runs only on the Store's machine.
