# #506 Store backends and remote storage: the Store-first model

> **Status:** Proposal; target LayerFS 2.0; not a released contract.

Issue: [#506](https://github.com/Ephemeral-AI-Lab/layerfs/issues/506). Board:
[LayerFS 2.0](https://github.com/orgs/Ephemeral-AI-Lab/projects/1/views/1),
areas A (Store backends) and R (remote storage). Base: `origin/main`
`35d36575c1`, branch `issue506-store-backends-remote`.

## 1. Owner direction (2026-10-29)

- PostgreSQL is a second Store backend next to SQLite. Each Workspace overlay
  stays its own local SQLite file.
- A PostgreSQL Store's `synchronous_commit` is a setting: `off` (default),
  `local`, `remote_write`, `on` or `remote_apply`, applied with `SET LOCAL` in
  each write transaction. The SQLite Store stays Disposable only.
- A remote holds whole SQLite Store files. Push uploads one; pull downloads one
  and replaces the local Store. Compare-and-swap on the remote is not required.
  MinIO is the first provider.
- **One Store holds many projects, and a home holds many Stores**: many local
  SQLite Stores, many PostgreSQL connections, many remote sources.

## 2. Model

A **Store** is the unit of storage, sharing, sync, writing and access. A
**project** is a name inside one Store.

- **Already in the format:** every Branch, Layer and Commit row carries a
  `layer_stack_id`, and `layer_stack.name` names it
  ([history.sql](../../../../engine/crates/layerfs-persistence/sql/sqlite/history.sql)).
  Packs and objects are Store-wide. The method surface already resolves the
  `project` argument to a stack by name
  ([args.rs](../../../../engine/crates/layerfs-api/src/base/args.rs)).
- **What makes it new:** today each Init creates a new Store file holding one
  stack, so a project and its Store look like the same thing.

```text
LayerFS home
│
├── stores.json ── list of Stores, one entry each: kind, location, remote binding
│
├── Local SQLite Stores (one Docker volume each)
│   ├── Store "work"    volume <prefix>store-work  → store.sqlite
│   │     ├── project shop   (layer_stack: Branches, Layers, Commits)
│   │     ├── project api
│   │     └── packs/objects  shared by shop + api (stored once)
│   └── Store "scratch" → project playground
│
├── Pulled SQLite Stores (local file + the remote it came from)
│   └── Store "team"  ⇄ remote minio-a  ── projects web, docs
│
├── PostgreSQL connections (named, many; the secret stays with the host)
│   ├── pg-local  ── schema lfc_main = Store "main" (shop2, ml)
│   │             ── schema lfc_lab  = Store "lab"  (exp1)
│   └── pg-cloud  ── schema lfc_prod = Store "prod" (app)
│
└── Remotes (named, many; hold Store files)
    ├── minio-a  http://localhost:9000  bucket layerfs  prefix lfc
    │     └── lfc/team/store.sqlite   (+ sha256, size)
    └── minio-b  https://minio.lab:9000 bucket backups  prefix home2
```

What follows from the Store being the unit:

1. **Sync works per Store.** A push uploads every project in the Store; a pull
   replaces all of them. Two machines working on different projects in one
   Store refuse each other's pushes. Group projects by who works on them
   together, or later push changed rows only (#495's bundles).
2. **Writers share a lane per Store.** On SQLite, publishes to different
   projects in one Store take turns: one writer per file, short holds. On
   PostgreSQL the lane locks the head row being published, so projects do not
   queue behind each other.
3. **Storage is shared within a Store.** Forks and shared dependencies are
   stored once per Store. Nothing is shared across Stores.
4. **Project lifecycle:**
   - Adding a project Inits a new stack into a live Store through the writer
     lane.
   - Removing a project drops its stack, then reclaims unreachable packs
     (A8, #411).
   - Moving a project to another Store needs a cross-Store import, because pack
     ids and inode-value ordinals are Store-local and ordinals sit inside
     pooled pack bodies.
5. **Addressing is Store plus project everywhere:** the SDK open call, the
   runtime list, Workspace mounts and daemons.
6. **A Store is an access boundary.** Whoever can open it can read all of its
   projects.

## 3. Workflows

```text
 Init new Store               Add project to a Store        Remove project
 ──────────────               ──────────────────────        ──────────────
 folder ──Init──► new         folder ──Init──► live Store    drop layer_stack
 store.sqlite (sealed)        (through the writer lane,     ──► reclaim
 ──► install in a new         new layer_stack)              unreachable packs
 volume                                                     (A8 / #411)

 Push (whole Store file)                        Pull (replaces one Store)
 ───────────────────────                        ─────────────────────────
 live Store                                     remote copy (sha, size)
   │ VACUUM INTO snapshot (consistent,            │ download → temp file
   │ writers keep going)                          │ beside the Store
   ▼                                              ▼
 temp copy beside Store ── free-space check    verify sha256
   │                                              │ refuse if any daemon,
   │ ≤ provider max? single PUT / multipart       │ Workspace or worker
   │ > max file?     split into pieces            │ holds the Store open
   ▼                                              ▼
 <prefix>/<file>/store.sqlite (the link's file) one rename: the Store is replaced
 record push (remote, file, at ms, sha, bytes) (all of its projects)
 in the home, not in the Store                 record pull in the home

 Load (pull into a NEW Store)                  Import SQLite Store → PostgreSQL
 ────────────────────────────                  ────────────────────────────────
 remote file ──download──► new volume          sealed SQLite Store ──import──►
 ──► register a Store linked to that file      one schema (all stacks, same ids)

 One link per SQLite Store (#509, links.md)
 ──────────────────────────────────────────
 connection (provider, endpoint, bucket, prefix, keys ref)   binds no Store
 local Store ──link──► one remote file      a file is linked by ≤ 1 Store
 Publish: unlinked Store → new file, linked    Send: a copy → new file, unlinked
 Load: file → new Store, linked                Unlink: the file stays, loadable

 Two machines, one remote (whole file)
 ─────────────────────────────────────
  A: pull team ─ work on web  ─ push ✔
  B: pull team ─ work on docs ─ push ✘ the remote moved since B's pull
```

- **Link and push/pull state:** each SQLite Store's link (remote, file) and
  its last push or pull live in a home file, keyed by Store
  (`remote-state/<store>`, twin of the runtime's `stores.json` link),
  because a pull replaces the Store file.
- **Pull refuses while the Store is open:** any daemon, mounted Workspace, or
  SDK or worker handle on it. On macOS the worker keeps a Store open for a
  quiet minute after use.
- **Push checks free space first** for one Store-sized copy, inside the Docker
  volume on macOS.

## 4. Live path (macOS)

```text
LayerFS Connect (Electron)
  Storage page: Stores list → Store detail (projects, link, Push/Pull/Import)
  host/ (Node): storage.js keeps remote keys and PostgreSQL URLs, owner-only
        │ @layerfs/runtime: store.* · project.* · remote.* · sync jobs
        │ @layerfs/agent:   fs, branch, workspace on (store, project)
        ▼ loopback + secret (Linux: in-process)
lfc-worker (Linux container)
  layerfs-node → layerfs-api → layerfs-store-service
    store/open → layerfs-persistence
                   backend/sqlite/      (rusqlite, flock lane)
                   backend/postgresql/  (postgres crate, row-lock lane)
    remote/    → layerfs-remote-storage: s3_api/, minio/, cloudflare_r2/, local_folder/
        │ volumes                 │ TCP                  │ HTTP(S)
        ▼                         ▼                      ▼
  Store volumes              PostgreSQL            MinIO / R2 sources
                         (host.docker.internal or a shared Docker network)
Sandboxes: each daemon opens its Store and mounts Workspace(store, project, branch)
```

- **Credentials:** the host owns them. LayerFS stores a credential reference,
  never the secret, and gets keys per call or as `LAYERFS_*` settings when the
  worker starts.
- **No HTTP Store server.** D12 and D14 stay open.

## 5. Code layout (planned)

Persistence engines, with the same role folders on both sides:

```text
engine/crates/layerfs-persistence/src/backend/
  sqlite/      session/ lane/ objects/ metadata/ history/ storage/ acquisition/ lifecycle/ snapshot.rs
  postgresql/  session/ lane/ objects/ metadata/ history/ storage/ cache/ import.rs schema.rs
engine/crates/layerfs-persistence/sql/{sqlite,postgresql}/
```

Remote storage, one folder per provider:

```text
engine/crates/layerfs-remote-storage/src/
  driver.rs limits.rs pieces.rs error.rs
  s3_api/ local_folder/ minio/          (first)
  cloudflare_r2/                        (HTTPS, `tls` feature)
  aws_s3/ google_drive/ …               (later)
engine/crates/layerfs-store-service/src/remote/   settings, push, pull, load, link state
```

Store-first API layer:

```text
engine/crates/layerfs-project/          add a project to an existing Store; list and remove stacks
engine/crates/layerfs-api/              store.*, remote.*, sync jobs, import to PostgreSQL
engine/crates/layerfs-node/             binding for the above
api/runtime/src/store/                  stores.json, store volumes, Store-level jobs
```

## 6. Runtime calls (contract for integrations such as Connect)

Built by session c (`@layerfs/runtime`). All calls are made once, with no
retry; destructive calls take `confirm` set to the name they destroy. A
job call answers `{ id, … }` at once; `runtime.sync.get(id)` (or
`runtime.project.get_init(id)` for an Init) reads it until `ended`.

| Call | Does |
| --- | --- |
| `runtime.store.list()` | Stores in the home: `{ name, kind: 'sqlite' \| 'postgresql', volume?, connection?, schema?, synchronousCommit?, link, legacy, created, projects: [names], bytes, open, heads, importedFrom }`. A SQLite Store's `link` (#509, [links.md](links.md)) is `{ remote, file, linked, last } \| null`: the connection, the remote file (object `<prefix>/<file>/store.sqlite`), when it was linked (ms) and its last transfer `{ direction: 'push' \| 'pull', at (ms), bytes, sha256, heads } \| null`; a PostgreSQL Store's is null. The binding keeps the link's twin in `HOME/remote-state/<store>`. `stores.json` is version 2; a version 1 file (per-remote `copies`) is read with no links and kept as `stores.v1.json` at the first write. For a local SQLite Store, `bytes: { file, wal }` and `open` (its `-shm` exists) come from stat calls, and `heads` is its history heads digest (below), all from one `runtime.store.stat` binding call in the process that sees the volume (the worker on macOS and Windows); null when that call fails, and always null for a PostgreSQL Store. `importedFrom: { store, at, bytes }` names the SQLite Store a PostgreSQL Store was imported from (null otherwise) |
| `runtime.store.create(name)` | a new empty local SQLite Store in its own volume `<prefix>store-<name>` |
| `runtime.store.remove(name, { confirm, stopSandboxes })` | deletes a local Store's volume (every project in it; Busy while a Sandbox runs on it, unless `stopSandboxes`), or forgets a PostgreSQL Store (its schema stays) |
| `runtime.project.start_init(name, folder, { store })` | Inits a project into the live Store `store` through its writer lane and admission (Exists for a taken name, Busy while another add runs, Unsupported for a Store without acquisition tables or a PostgreSQL Store); without `store`, into a new Store of the same name, as before |
| `runtime.project.remove(name, { store, confirm, stopSandboxes, url })` | in a Store that holds others: removes the project's Sandboxes (Busy while one runs, unless `stopSandboxes`) and drops its stack (a PostgreSQL Store's `url` is passed for the call), answering `{ kind: 'Removed', project, store, stack, packsKept: true }`; packs stay until A8 (#411). A project alone in its own Store is removed whole, with its volume, as before |
| `runtime.project.list()`, `get_state(name, since, { store, url })`, `runtime.sandbox.reclaim(name, { store, url })` | `list` items carry `store` and `shared`; `{ store }` names the Store when a project name is in several (InvalidInput otherwise); a project of a PostgreSQL Store needs its connection's `url` for that call (InvalidInput without it), and its client is opened for the call and closed after it |
| `runtime.remote.list() / add(r) / remove(name, { confirm })` | remotes in the home; `add({ name, provider: 'minio', endpoint, bucket, prefix?, region?, credentials?, partSize? })` or `add({ name, provider: 'r2', account, jurisdiction?, bucket, prefix?, credentials?, partSize? })` takes a credential reference (`env:LAYERFS_…` or `file:/path`, default `env:LAYERFS_REMOTE`), never keys. R2 is reached at `https://<account>[.<jurisdiction>].r2.cloudflarestorage.com`, region `auto`, and needs a build with the `tls` feature. A connection binds no Store; `remove` is refused `InUse`, naming them, while a Store links to it (#509) |
| `runtime.store.start_publish(store, { remote, file = store, credentials })` | job: uploads an **unlinked** SQLite Store to a **new** remote file and links it. `Linked` when the Store has a link; `Exists` when a Store of the home links to `(remote, file)`, a running job of this process is making it, or an object is there (one HEAD before the snapshot and again before the upload; never overwritten). A race with another home between that HEAD and the upload is not detected (no conditional PUT on every provider) |
| `runtime.store.start_push(store, { credentials, force })` | job: whole-Store upload to the Store's link; no `remote` argument (`InvalidInput`). `NotLinked` without a link; `Conflict` when the file is not what the link's record names (sha256, whatever the record's direction; no file when there is no record), unless `force` |
| `runtime.store.start_send(store, { remote, file, credentials })` | job: uploads a copy of any SQLite Store to a **new** file; links nothing and changes no record of the Store. `Exists` as for publish. `result` names `{ remote, file, bytes, sha256, heads }` |
| `runtime.store.start_pull(store, { credentials, confirm })` | job: replaces the Store (all of its projects) from its link; no `remote` argument. `NotLinked` without a link; closes this process's clients first, refuses Busy while anything else holds the Store |
| `runtime.store.start_load(remote, { file, store = file, credentials })` | job: a new local Store `store` from the remote file, linked to it (replaces `start_clone`). `Linked`, naming the Store, when a Store of the home links to the file; `Exists` for a taken Store name or while another job of this process loads or links the file |
| `runtime.store.unlink(store, { confirm })` | drops the Store's link and its `remote-state` record; the remote file stays and can be loaded. `confirm` is the Store's name; `NotLinked` without a link |
| `runtime.store.attach_postgresql({ name, connection, url, schema, synchronousCommit })` | registers the PostgreSQL Store `name` after checking it opens: a schema this home imported keeps its recorded identity; any other LayerFS schema is read for its binding and gets a fresh cursor key |
| `runtime.store.start_import_postgresql(store, { name, connection, url, schema, synchronousCommit })` | copies a SQLite Store (every project, the same ids) into a new schema and registers it as the PostgreSQL Store `name` (default `<store>-pg`) |
| `runtime.sync.get(id)` | a publish, push, pull, send, load or import job: `{ id, kind, store, remote, file, step: 'running' \| 'done' \| 'failed', phase, bytes, total, pieces, error: { code, message }, started, ended, result }`; `error.message` is readable as is (for example a push's free-space refusal, code Capacity, or a pull refused while the Store is open). While it runs, `phase` and the phase's `bytes` of `total` (and `pieces: { done, of }` for a file stored as pieces) are the native call's progress cell, which it updates at most every 250 ms for bytes (a new phase at once): publish, push and send `check`, `snapshot` (bytes copied of the copy's size), `hash`, `upload`, `record` (send without `record`); pull `check`, `download`, `verify`, `install`, `record`; load the same without `check`; import `copy` (no byte count) and `record`. Once done, `phase` and `pieces` are null and `bytes` is what moved; `result.transfer` carries `heads` |
| `runtime.store.check(store, { credentials })` | one HEAD of the Store's linked file `<prefix>/<file>/store.sqlite`: `{ store, remote, file, present, bytes, sha256, heads, modified, checkedAt }` (times in ms since the epoch; `bytes` is the whole file's, also for a file stored as pieces); `NotLinked` without a link; failures keep their code (`Unavailable`, `Denied` for 401/403, `Unknown` when the outcome is unknown) |
| `runtime.remote.list_stores(remote, { credentials })` | every Store file in the remote: `[{ file, bytes, sha256, heads, modified, linkedBy }]` (`linkedBy`: the home's Store linked to it, or null; a fact of this home only, never written to the bucket), from one paged ListObjectsV2 pass plus a HEAD per file where the listing lacks its metadata (S3 listings never carry it), at most 64; beyond that `sha256` and `heads` are null |
| `runtime.remote.open_view(remote, { file, credentials })` | #510, [live-view.md](live-view.md): a live, read-only view of the Store file `file` (object `<prefix>/<file>/store.sqlite`), read in place and never downloaded. One HEAD pins the file's version (ETag; a pieced file's manifest is read too), then the reads that open its Store and list its projects: `{ view, storePath, remote, file, bytes, etag, sha256, heads, modified, projects: [names] }`. `storePath` is `remote:<remote>/<file>#<view>`. A file pushed before #510 A opens too, with more requests. Refused `NoSuchFile`, `InvalidInput` (no `layerfs-binding`, or not a Store), `Unavailable`, `Denied` or `Unknown`. The keys stay in the view's process while it is open |
| reads with `{ view }` | `runtime.project.list({ view })` lists the view's projects (`kind: 'view'`, `store: 'view:<id>'`, `readOnly: true`); `runtime.project.get_state(name, since, { view })` answers the history alone (no Sandboxes). An `@layerfs/agent` client opened with `LAYERFS_STORE_PATH` set to the view's `storePath` answers `project.list_branches`, `branch.list_commits`, `branch.diff_commits`, `branch.diff_branches`, `fs.read`, `fs.list`, `fs.stat`, `fs.grep`, `fs.glob`, `fs.diff_files`, `fs.blame` and the six `backend.store.*` reads; every other call on it is refused `ReadOnly` before it runs. A read whose remote request failed is refused with that request's kind: `Changed` (the file was replaced since the view opened: open a new view), `Unavailable`, `Denied`, `Unknown` or `Failed`; nothing is retried. A view is never in `stores.json`, never linked, never a Store of the home |
| `runtime.remote.close_view(view)` | `{ closed }` (false when it was already closed); its clients close and its cache goes. A view also closes after a quiet minute, in the runtime and in the process that holds it (the worker on macOS and Windows); a later call on it is refused `NoSuchView` |
| `runtime.remote.view_stats(view)` | `{ requests, bytes, fileBytes, hits, misses, fetches, fetched, resident, capacity }`: requests sent and bytes received (the open's HEAD and manifest included), the block cache's hits and misses (64 KiB blocks), its ranged requests and their bytes, and the bytes it holds against its bound (`LAYERFS_REMOTE_VIEW_CACHE`, bytes, default 64 MiB, at least 1 MiB) |
| `runtime.remote.providers()` | the providers this build can add: `[{ id: 'minio' \| 'r2' \| 'local_folder', label, available, reason, fields: [{ name, label, kind, required }] }]`; R2 is available only in a build with `tls` |
| `runtime.postgresql.status({ url, schemas })` | one connection, made once: `{ reachable: true, serverVersion, latencyMs, latencyUs, schemas: { [schema]: bytes \| null } }` (the `SHOW server_version` round trip; the sum of `pg_total_relation_size` over the schema's tables and materialized views; null for a schema the server lacks), or `{ reachable: false, error: { code: 'Unavailable' \| 'Denied', message } }`; a failed query after connecting is refused `Failed` |

- **Credentials:** `credentials` is `{ accessKey, secretKey }` for that call
  only; without it the remote's reference is read.
- **History heads digest (#506 E2).** The lowercase hex sha256 of the
  sorted lines `stack <name> <head_layer_id>` (from `layer_stack`) and
  `branch <project> <name> <base_layer_id> <head_commit_id or ->` (from
  `branch`), each followed by a newline, ids in lowercase hex
  (`layerfs_persistence::read_heads`). It changes exactly when history a
  pull would bring changes (a project added or removed, a Branch made,
  moved, merged or deleted, every Commit) and is equal on two machines with
  the same history. `store.list` reads it only for a Store whose file or
  WAL changed (size or mtime) since that process last read it, on a
  connection of its own that is closed before the call returns: never
  through the clients the runtime or worker keeps open, so it never leaves
  the Store open for a pull to find. SQLite cannot open a WAL Store
  read-only while no process holds it, so that connection opens read-write
  as `query_only` and writes nothing; the last opener's close removes the
  sidecars it made. Reads are serialized with that process's pull and
  load of the Store: while either runs the remembered value is answered.
  Push reads it from its snapshot, records it and sends it as the copy's
  `layerfs-heads` metadata; pull and load read it from the downloaded file
  before installing it. Every upload also records the whole file's size as
  `layerfs-bytes`. A PostgreSQL `url` is
  passed per call and is never written to the home or kept in a job.
- **PostgreSQL Stores** are not pushed or pulled. `@layerfs/agent` opens one
  with `LAYERFS_STORE` and `LAYERFS_STORE_POSTGRESQL` (`{ "url": … }`);
  `LAYERFS_VOLUME` is not needed.
- **Sandboxes on a PostgreSQL Store** (#506 d). `sandbox.create` sends the
  client's URL once, into the daemon's private config file
  (`/layerfs-local/config/daemon.setup`, 0600, daemon uid, non-dumpable
  daemon); it is never in the container's env, argv or labels, the Sandbox
  record or a log, and commands (uid 501, no capabilities) cannot read it.
  The daemon opens the schema with the Store record's binding, cursor key
  and `synchronous_commit`, starts ready with no install, and mounts no
  Store volume. Each Workspace overlay stays its own local SQLite file.
  Mount, Exec, Commit and unmount work as on SQLite; a Commit publishes in
  the schema through the per-Store `FOR UPDATE` lane. The daemon dials the
  URL from the container's network: `host.docker.internal` reaches the
  host's loopback on Docker Desktop; on a Linux Docker Engine the URL must
  name an address containers reach. Demand-loaded reads and Commit lookups
  go to the server, behind the daemon's 64 MiB shared object cache, with no
  host SQLite cache yet (#420). Each server statement is a round trip, so
  the engine keeps them few: immutable reads (locations, inode values,
  packs) run outside a transaction, a pack's descriptor comes with the
  first 64 KiB of its body, and binding hints and file digests go as array
  statements (up to 1024 rows each). A server in a container on the same
  Docker engine is best reached at its bridge address (Connect does this);
  through `host.docker.internal` a round trip on Docker Desktop costs about
  0.75 ms instead of about 0.13 ms. Merges work as on SQLite: a merge's working
  records live in temporary tables of the writer's connection (outside the
  checked Store schema, gone with the connection, written without the
  publication lane), so a Commit whose Branch moved since mount merges, and
  so do `branch.merge` calls. Build the
  bundle with `--features layerfs-node/postgresql,layerfs-daemon/postgresql`;
  a daemon built without it refuses the Store (`BackendUnavailable`). The
  open-time sweep skips PostgreSQL projects (it holds no URL); reclaim them
  with `runtime.sandbox.reclaim(name, { store, url })`.
- **Opening a project of a shared Store:** `LayerFS.open(project)` with
  `LAYERFS_STORE` (or the `store` option) and that Store's path.
- **Home layout.** `<home>/stores.json` lists the Stores the runtime made or
  registered (`{ version: 1, stores: [...] }`, no secret). A Store made by
  its project's own Init (before #506, or with no `store`) is listed from
  that project's record as `legacy`, and gets an entry once another project
  joins it or its first project is removed. Native records:
  `<home>/<project>/project.record` for a project alone in its own Store;
  `<home>/.stores/<store>/store.record` (kind, binding, cursor key; schema
  and `synchronous_commit` for PostgreSQL) and
  `<home>/.stores/<store>/projects/<project>/project.record` for a shared
  Store. Sandboxes of a shared Store's project are named
  `<prefix><store>.<project>-<n>` and labelled `layerfs.store`.
- **Linux, macOS, Windows.** On Linux every call runs in the caller's
  process. On macOS and Windows the calls go to the worker, which reaches
  MinIO and PostgreSQL by the endpoint and URL it is given (use
  `host.docker.internal` for a service on the host's loopback). The worker's
  sealed Linux build takes the PostgreSQL engine with
  `python3 tools/worktree.py build --resources <resources.json> --platform linux --profile debug|release --jobs N --features layerfs-node/postgresql,layerfs-daemon/postgresql`;
  MinIO is on by default in `layerfs-remote-storage`. HTTPS remotes
  (Cloudflare R2, MinIO behind TLS) need `--features layerfs-node/tls`, which
  uses the `native-tls` already pinned for PostgreSQL.

## 7. Work split

| Part | Owner | Code |
| --- | --- | --- |
| a. Persistence: SQLite/PostgreSQL seam, snapshot, PostgreSQL engine, import | session a | `layerfs-persistence`, `store-service/src/store/` |
| b. Remote storage: drivers, push, pull | session b | `layerfs-remote-storage`, `store-service/src/remote/` |
| c. Store-first API layer: projects in a live Store, `store.*`, runtime, worker | session c | `layerfs-project`, `layerfs-api`, `layerfs-node`, `api/runtime` |
| Connect wiring | coordinating session | `Ephemeral-AI-Lab/layerfs-connect` |

**Done when:**

- One Store holds two projects; a second Store and a PostgreSQL Store exist
  side by side.
- A SQLite Store pushes to MinIO and is cloned into a fresh Store with
  identical history.
- A SQLite Store imports into PostgreSQL with identical object, root and
  Commit ids.
- Connect's Storage page lists all of them and drives push, pull, clone and
  import.

## 8. Open items

- Init into a live Store: built through the writer lane and admission, one
  add at a time per Store (Busy otherwise); how it uses the parallel
  construction profile that Namespace Init keeps is open.
- Reclaiming a removed project's packs (A8, #411).
- Incremental push with #495's bundles.
- The host SQLite cache for PostgreSQL Stores (#420); until then a PostgreSQL
  Sandbox pays the server round trips of cold reads and Commits.
- A PostgreSQL Sandbox on a Linux Docker Engine reaching a database on the
  host's loopback (no `host-gateway` mapping is added).
- Moving a project between Stores (cross-Store import).
- More providers (R2, AWS S3, Google Drive) and TLS for the S3 client.
