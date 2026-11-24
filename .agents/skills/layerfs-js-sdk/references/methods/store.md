# store calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (macOS, a sqlite Store
> on `native`); PostgreSQL and `move` rows come from the table, the spec rows
> and the manual. Removed with no alias: `store.status` (now `get` with
> `with: ["live"]`), `store.stats` (now `get` with `with: ["stats"]`),
> `store.push` and `store.pull` (now `store.move`). An agent working on a
> project rarely needs this family.

```js
store.list({ machine, with: ["stats"] })
store.get(store, { with: ["live", "stats"] })
store.create(name, { machine })
store.connect(store, url, { user, password, synchronous_commit })
store.disconnect(store)
store.attach(store, { machine })
store.detach(store, { machine })
store.move(store, machine, { on_progress })
store.remove(store)
store.compact(store, { on_progress })
store.doctor(store, { machine, on_progress })
```

A Store holds projects and all their history. The verb says the kind:
`create` makes a sqlite Store (one file, on one machine at a time; on
`native` unless `machine` names another); `connect` connects a PostgreSQL
database with its login, saved on this computer only, and `attach` lets a
machine other than `native` run its Sandboxes.

## `store.list` and `store.get`

From the home's records, with no probe. A Store's `state` is `ready`, `busy`
(`op`: a move, remove, create, compaction, backup or restore), `blocked`
(its machine is not ready) or `broken` (`reason`, with a `fix` naming the
call that clears it). `get` short: `{ engine, state }` with `machine`,
`url`, `user`, `machines`, `projects`, `projects_count`, `backups` and the
companion when they say something; long adds `password_saved`,
`password_changed_at`, `binding` (16-hex prefix of the Store's database id),
`reach` (this computer's last reach of a PostgreSQL Store), `op`, `blocked_by`, `reason`, `fix`, `observed_at`,
`store`, `profile`, `synchronous_commit`, `sqlite_version`, `created_at`,
`last_transfer`. A PostgreSQL Store's `machines` are its attached machines,
each `{ machine, reached, reach }`. `list` rows are the same with `store`
first and `machines` as names; `machine` keeps the sqlite Stores on one
machine.

```bash
$ layerfs store list                                                           # seen
{"items":[{"store":"shop","engine":"sqlite","machine":"native","projects":["api"]}]}
$ layerfs store get shop                                                       # seen
{"engine":"sqlite","state":"ready","machine":"native","projects":["api"]}
```

Sections of `get`:

- `with: ["live"]` reads the Store where it lives, starting nothing: state
  and `observed_at` from that read, and `live: { holders, phases, behind,
  server_version, latency_ms }` (`holders`: pids of other live processes
  holding the Store; `phases`: a running compaction's; `behind`: the
  buckets whose backup is behind). For PostgreSQL, every connect-and-login
  handshake is bounded to 5 s (`POSTGRESQL_HANDSHAKE` or the URL's
  `connect_timeout`), one attempt, no retry: a silent server yields
  `state: "broken"`, `reason: "unreachable"` (`"the server did not answer
  within 5 s (connect and login)"`).
- `with: ["stats"]` opens the Store for its counters: `stats: { size,
  logical_bytes, canonical_bytes, stored_bytes, metadata_bytes, file_bytes,
  reusable_bytes, dedupe, compression, init }` (ratios as text, six
  decimals). `list`'s `with: ["stats"]` adds only each row's `size`.

```bash
$ layerfs store get shop --with live,stats                                     # seen
{"engine":"sqlite","state":"ready","machine":"native","projects":["api"],"observed_at":"2026-11-18T22:05:32Z","stats":{"size":307200,"logical_bytes":199,"canonical_bytes":6815,"stored_bytes":3354,"metadata_bytes":278242,"file_bytes":307200,"reusable_bytes":28672,"dedupe":"0.029200","compression":"2.031902"}}
```

## Changes

| Call | What it does | Notes |
| --- | --- | --- |
| `create(name)` | An empty sqlite Store | `{"ok":true}` (seen). A taken name is `Exists` (seen: `a Store named shop exists`). `machine` makes it on that running machine through its worker |
| `connect(store, url, …)` | Connects a PostgreSQL database as a Store (`user` and `password` required the first time, checked before anything is saved; `synchronous_commit` defaults to `local`); on a connected Store, changes its address, user, password or `synchronous_commit` (an omitted user or password keeps the saved one; the same Store must be reached) | The URL holds host, port, database and TLS settings only: one naming a user, password or `passfile` is `InvalidInput` with the `fix` `layerfs store connect <store> <url> --user <user> --password-stdin`. Shell: `--user U --password-stdin`; the password is never answered, logged or traced. Short `{ projects, projects_count? }`, or `{"ok":true}`; long `outcome` `connected` or `updated` |
| `disconnect(store)` | Forgets a PostgreSQL Store with its saved password and attachments, its data staying | `InUse` while a Sandbox or Workspace uses it |
| `attach(store, { machine })` | Lets a machine run Sandboxes of a PostgreSQL Store, checked from that machine with the saved URL and login first | Short `{ ok, outcome, latency_ms? }`; `outcome` `attached`, or `checked` when already attached. A sqlite Store or `native` is `InvalidInput`. `sandbox.create` on a machine not attached is refused with `layerfs store attach <store> --machine <machine>` as its `fix` |
| `detach(store, { machine })` | Stops a machine running Sandboxes of a PostgreSQL Store | `InUse` while a Sandbox of the Store's projects is on it; `NotFound` when not attached |
| `move(store, machine)` | Moves a sqlite Store between this computer and a machine | To or from `native` only (seen when already there: `store move, check: Store shop is already on native`, `InvalidInput`; moving off a machine whose record lacks `vm_name` is `Unavailable` with `blocked_by`). Progress `check`, `snapshot`, `send`, `verify`, `install`, `record`, `retire`. Short `{ moved_bytes }`. A sqlite Store's Sandboxes run only on its machine |
| `remove(store)` | Deletes a sqlite Store with every project in it | `{"ok":true}`. `NotFound` for an unknown name (seen). A PostgreSQL Store is disconnected instead. A record this build does not read, or a Store on a machine whose record has no 2.0 name, goes by its records alone, with `left` saying what may remain |
| `compact(store)` | Reclaims the dead bytes of one Store | The only byte-reclaiming call. Short `{ freed_bytes }` (seen: `{"freed_bytes":0}`); long `outcome` (`compacted` or `unchanged`), `dead_bytes`, `kept_waste`, `peak_temp_bytes`, `zero_reasons`, …. Progress `mark`, `build`, `verify`, `install` (sqlite; an `unchanged` run sends `mark` only). A sqlite Store on another machine is compacted by its worker. `InUse` while Workspaces are mounted; `Busy` (with `op`); `Unavailable`, `Unknown` and `Unresolved` (each with a `fix`) |
| `doctor(store)` | Checks the Store, then each of its projects | See [doctors](doctor.md); seen: `{"ok":true}` |
