# #506 One link per Store: implementation spec

> **Status:** Owner-approved direction (2026-10-30), not implemented. It
> replaces the per-remote `copies` model of [README §6](README.md#6-runtime-calls-contract-for-integrations-such-as-connect)
> for SQLite Stores. The Connect design that uses it is
> `layerfs-connect/docs/ui/storage-links.md` (private repository).

## 1. Owner direction (2026-10-30)

1. Adding a connection to MinIO or Cloudflare R2 binds nothing. The user
   picks which remote SQLite file to load.
2. A connection is bound to the S3 provider (endpoint, bucket, keys), never
   to a SQLite Store.
3. The user picks which Store to push or pull; that Store maps to one remote
   location.
4. Each local SQLite Store is linked to at most one remote file, on one
   connection. A home may have many connections (several MinIO, several R2).
5. **One to one.** No two local Stores link to the same remote file. Pushing
   a Store somewhere else makes a **new** remote file, which no Store links
   to until the user loads it (pull), which makes a new local Store.
6. Settling the data that exists today is a separate phase that runs after
   the implementation (§7).

## 2. Why the copies model goes

Today one Store keeps a list of copies, one per remote, each with its last
push and pull (`stores.json`), plus a `remote` field naming the last one used.
The native record `HOME/remote-state/<store>` keeps **one** last transfer.
The two disagree:

- **Wrong push refusals:** `team-examples` was pushed to `local-minio`, then
  to `r2`. Its native record now names `r2`. A push to `local-minio` checks
  the MinIO file against R2's sha256 and is refused as "the remote copy
  moved", though MinIO holds exactly what was pushed there. Observed
  2026-10-30 with bundle `build-linux-90168c86…` at `00665aa721`.
- **Clones that look broken:** a clone is checked at a path named after the
  clone (`<prefix>/<clone>/store.sqlite`), where nothing was ever pushed, so
  every clone reads "missing".
- **Remote file named after the Store:** the object path is derived from the
  Store's name, so a Store cannot track a file of another name.

One link per Store, with the remote file named in the link, matches the
native record and removes all three problems.

## 3. Model

```text
LayerFS home
├── remotes (connections): name → provider, endpoint/account, bucket, prefix,
│                          credential reference. No Store field.
├── stores.json v2
│   ├── SQLite Store  { name, volume, link | null, projects, … }
│   │     link = { remote, file, linked, last }
│   │            remote: a connection's name
│   │            file:   the remote file's name; object <prefix>/<file>/store.sqlite
│   │            linked: when the link was made (ms)
│   │            last:   { direction: push | pull, at (ms), bytes, sha256, heads } | null
│   └── PostgreSQL Store { name, connection, schema, … }   (unchanged, never linked)
└── remote-state/<store>   the native twin of `link`: remote, file, last transfer
```

- **One to one, per home.** A `(remote, file)` pair is linked by at most one
  Store of the home. Every call that makes a link checks it under one home
  lock (the registry write lock), so two concurrent loads or publishes of the
  same file cannot both link.
- **Other homes are not seen.** Another machine may link the same file. That
  is what the push and pull check is for (§4.3).
- **A remote file** is any `<prefix>/<file>/store.sqlite` object (whole or in
  pieces) in a connection's bucket. Linked or not is a fact of this home
  only, never written to the bucket.

## 4. Runtime calls

Every call is made once with no retry; destructive calls take `confirm`.
Jobs keep their current shape (`runtime.sync.get`, phases, progress).

### 4.1 Changed and new calls

| Call | Does |
| --- | --- |
| `runtime.store.list()` | as today, but a SQLite Store has `link: { remote, file, linked, last } \| null` instead of `remote` and `copies`. `bytes`, `open`, `heads`, `importedFrom` unchanged |
| `runtime.store.start_publish(store, { remote, file = store, credentials })` | job: uploads an **unlinked** Store to a **new** file and links it. Refuses `Linked` when the Store already has a link, `Exists` when `(remote, file)` is linked by another Store of the home or an object is already there (checked by HEAD before upload, no overwrite) |
| `runtime.store.start_push(store, { credentials, force })` | job: uploads to the Store's link. No `remote` argument. Refuses `NotLinked` without a link. `Conflict` when the file is not what `link.last` recorded (sha256, either direction), unless `force` |
| `runtime.store.start_pull(store, { credentials, confirm })` | job: replaces the Store from its link. No `remote` argument. Refuses `NotLinked`; `Busy` while the Store is open, as today |
| `runtime.store.start_send(store, { remote, file, credentials })` | job: uploads a copy of any SQLite Store (linked or not) to a **new** file. Links nothing and changes no record of the Store. Refuses `Exists` when an object is there or `(remote, file)` is linked in the home. Its result names `{ remote, file, bytes, sha256, heads }` |
| `runtime.store.start_load(remote, { file, store = file, credentials })` | job: a new local Store `store` from the file, linked to it. Replaces `start_clone`. Refuses `Linked` (naming the Store) when the file is already linked in the home, `Exists` for a taken Store name |
| `runtime.store.unlink(store, { confirm })` | drops the link and its `remote-state` record. The remote file stays. `confirm` is the Store's name |
| `runtime.store.check(store, { credentials })` | one HEAD of the Store's linked file: `{ store, remote, file, present, bytes, sha256, heads, modified, checkedAt }`. Refuses `NotLinked` |
| `runtime.remote.list_stores(remote, { credentials })` | as today, each item `{ file, bytes, sha256, heads, modified, linkedBy }`; `linkedBy` is the home's Store linked to it, or null |
| `runtime.remote.remove(name, { confirm })` | refuses `InUse`, naming the Stores, while any Store links to the connection |

Removed: `start_clone`, the `remote` argument of push and pull, `copies`, and
the Store-level `remote` field. Delete their tests rather than adapt them
(repository rule); cover the surviving behaviour on the new calls.

### 4.2 Native (layerfs-store-service, binding)

- `remote-state/<store>` gains a `file` line. Push, pull and check use the
  object `<prefix>/<file>/store.sqlite` from the link, never the Store name.
- Push's expected-copy check compares the file's sha256 with the record's,
  whatever its direction. The record names the same `(remote, file)` the
  push targets, so the mismatch of §2 cannot happen.
- `publish` and `send` HEAD the target first and refuse an existing object
  (`Exists`) before uploading. A race with another home between that HEAD
  and the upload is out of scope (no conditional PUT on every provider);
  say so in the docs.
- `load` is today's clone with the file named explicitly; it writes
  `remote-state/<new store>` for the link.
- Each record keeps times in **ms** (the native record's `at` is seconds
  today; the runtime converts it when reading old records).

### 4.3 What a sync state compares

One link means one comparison. The host computes it (Connect §4): local
`heads` now vs `link.last.heads`, and the file's `sha256` now (check) vs
`link.last.sha256`. A missing baseline (`last` null or `last.heads` null) is
**unknown**, never guessed. §7 removes every such baseline from today's data.

## 5. Code layout

Scope: `api/runtime/src/store/*.js` (JS runtime), the binding dispatch in
`engine/crates/layerfs-node`, and `engine/crates/layerfs-store-service/src/remote/`.
No new crate and no new dependency. Planned changes only; take production LOC
for every commit with `tools/production_loc.py` as the repository requires.

- `registry.js`: `withCopy` → `withLink`; v2 entries; the one-to-one check.
- `sync.js`: `start_publish`, `start_send`, `start_load`; push and pull read
  the link.
- `calls.js`: `unlink`, `check` by link, `list_stores` `linkedBy`,
  `remote.remove` `InUse`.
- `remote/state.rs`: the `file` line, ms times.
- `remote/push.rs`, `pull.rs`, `copies.rs`: object path from the link;
  publish/send existence check.
- README §6: replace the copy rows by §4.1, and update the diagram in §2.

## 6. Tests and proof

- Unit: `withLink`, the one-to-one check (two loads of one file, publish onto
  a linked file), `NotLinked` and `Linked` refusals, v1→v2 reading.
- Native: push conflict against the link's own record (the §2 case must now
  pass), publish/send refusing an existing object, load writing the record.
- End to end on the issue's MinIO and R2 example infrastructure (Connect's
  `storage_real.mjs` pattern): publish, push, pull, send, load the sent file,
  unlink, publish again elsewhere, remote remove `InUse`.
- Build the worker bundle with
  `--features layerfs-node/postgresql,layerfs-daemon/postgresql,layerfs-node/tls`.
- Every test command ≤120 s, unfiltered output; Disposable profile only.

## 7. Phase 2: settle today's data (after §4–§6 land)

Not part of the migration code. Run it on the owner's Connect home
(`~/.layerfs-connect/home`) once the new calls work, and report the end
state. Nothing is deleted; every Store keeps its data.

1. **Read the evidence:** for each SQLite Store, its `stores.json` copies,
   its `remote-state` record, and a HEAD of every file it ever moved
   (sha256 equality identifies which file a Store came from).
2. **Choose each link by these rules:**
   - A Store's link is the file its native record names, when that file's
     sha256 still equals the record's.
   - A clone links to the file it was loaded from only when no other Store
     claims that file under the rule above; otherwise it becomes local only.
   - Every other file a Store pushed stays in its bucket as a not-loaded file.
3. **Write v2 entries and native records** for those links, with `file`.
4. **Give every link a baseline:** push each linked Store with the conflict
   check (refused if the file moved), so `last.heads` is recorded. Any
   refusal is reported, not forced.
5. **Fix the host fingerprint gap** found on 2026-10-30: inside the running
   Connect host `store.list` answers `heads: null` for every Store, while a
   fresh process with the same settings gets all 13. Find the cause and fix
   it where it lives.
6. **Done when:** every SQLite Store is either linked and **in sync**, or
   local only; no link reads unknown or missing; no file is linked twice;
   every remote file in each connection reads linked or not loaded; the
   Connect Storage page shows exactly that. Record the before/after table
   in the issue.

Expected outcome from today's evidence (confirm in step 1, do not assume):

| Store | Today | After |
| --- | --- | --- |
| `team-examples` | copies on `local-minio` and `r2`; record `r2` | linked `r2/team-examples`; `local-minio/team-examples` not loaded |
| `services` | copies on `r2` and `local-minio` | linked to the record's file; the other not loaded |
| `r2-examples` | copy on `r2` | linked `r2/r2-examples` |
| `team-examples-copy` | clone, reads missing | links the file it came from if unclaimed, else local only |
| `team-examples-r2` | clone, reads missing | same rule |
| `r2-examples-clone` | clone of `r2/r2-examples`, reads missing | local only (`r2-examples` claims it) |
| `dsa`, `heavy-check`, `playground`, `connect-test-*` | local | local only (the four test Stores await the owner's decision) |
| `team-examples-pg`, `services-pg` | PostgreSQL | unchanged |
