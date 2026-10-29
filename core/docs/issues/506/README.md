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
   │ backup API snapshot (consistent,             │ download → temp file
   │ writers keep going)                          │ beside the Store
   ▼                                              ▼
 temp copy beside Store ── free-space check    verify sha256
   │                                              │ refuse if any daemon,
   │ ≤ provider max? single PUT / multipart       │ Workspace or worker
   │ > max file?     split into pieces            │ holds the Store open
   ▼                                              ▼
 <prefix>/<store>/store.sqlite                 one rename: the Store is replaced
 record push (store, remote, at, sha, bytes)   (all of its projects)
 in the home, not in the Store                 record pull in the home

 Clone (pull into a NEW Store)                 Import SQLite Store → PostgreSQL
 ─────────────────────────────                 ────────────────────────────────
 remote file ──download──► new volume          sealed SQLite Store ──import──►
 ──► register as a Store bound to the remote   one schema (all stacks, same ids)

 Two machines, one remote (whole file)
 ─────────────────────────────────────
  A: pull team ─ work on web  ─ push ✔
  B: pull team ─ work on docs ─ push ✘ the remote moved since B's pull
```

- **Push and pull state:** the record of each push and pull lives in a home
  file, keyed by Store, because a pull replaces the Store file.
- **Pull refuses while the Store is open:** any daemon, mounted Workspace, or
  SDK or worker handle on it. On macOS the worker keeps a Store open for a
  quiet minute after use.
- **Push checks free space first** for one Store-sized copy, inside the Docker
  volume on macOS.

## 4. Live path (macOS)

```text
LayerFS Connect (Electron)
  Storage page: Stores list → Store detail (projects, copies, Push/Pull/Import)
  host/ (Node): storage.js keeps remote keys and PostgreSQL URLs, owner-only
        │ @layerfs/runtime: store.* · project.* · remote.* · sync jobs
        │ @layerfs/agent:   fs, branch, workspace on (store, project)
        ▼ loopback + secret (Linux: in-process)
lfc-worker (Linux container)
  layerfs-node → layerfs-api → layerfs-store-service
    store/open → layerfs-persistence
                   backend/sqlite/      (rusqlite, flock lane)
                   backend/postgresql/  (postgres crate, row-lock lane)
    remote/    → layerfs-remote-storage: s3_api/, minio/, local_folder/
        │ volumes                 │ TCP                  │ HTTP(S)
        ▼                         ▼                      ▼
  Store volumes              PostgreSQL            MinIO sources
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
  aws_s3/ cloudflare_r2/ google_drive/ … (later)
engine/crates/layerfs-store-service/src/remote/   settings, push, pull, clone
```

Store-first API layer:

```text
engine/crates/layerfs-project/          add a project to an existing Store; list and remove stacks
engine/crates/layerfs-api/              store.*, remote.*, sync jobs, import to PostgreSQL
engine/crates/layerfs-node/             binding for the above
api/runtime/src/store/                  stores.json, store volumes, Store-level jobs
```

## 6. Runtime calls (contract for integrations such as Connect)

All calls are made once; destructive calls take `confirm`.

| Call | Does |
| --- | --- |
| `runtime.store.list()` | Stores in the home: `{ name, kind: 'sqlite' \| 'postgresql', volume?, connection?, schema?, remote?, projects: [...], copies: [...] }` |
| `runtime.store.create(name)` | a new empty local SQLite Store in its own volume |
| `runtime.store.remove(name, { confirm })` | deletes a local Store's volume, or forgets a PostgreSQL Store |
| `runtime.project.start_init(name, folder, { store })` | Inits a project into `store`; without `store`, into a new Store of the same name, as today |
| `runtime.project.remove(name, { store, confirm })` | drops the project's stack |
| `runtime.remote.list() / add(r) / remove(name, { confirm })` | remotes in the home, with no secret |
| `runtime.store.start_push(store, { remote, credentials })` | whole-Store upload job |
| `runtime.store.start_pull(store, { remote, credentials, confirm })` | replace-the-Store job |
| `runtime.store.start_clone(remote, { store, credentials })` | a new local Store from a remote copy |
| `runtime.store.attach_postgresql({ name, connection, schema, synchronousCommit })` | registers a PostgreSQL Store; the URL with its secret is passed per call |
| `runtime.store.start_import_postgresql(store, { url, schema, synchronousCommit })` | copies a SQLite Store into a PostgreSQL schema |
| `runtime.sync.get(id)` | a push, pull, clone or import job: `{ id, step, bytes, total, error, started, ended }` |

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

- Init into a live Store: admission, and how it uses the parallel construction
  profile that Namespace Init keeps.
- Reclaiming a removed project's packs (A8, #411).
- Incremental push with #495's bundles.
- The host SQLite cache for PostgreSQL Stores (#420).
- Moving a project between Stores (cross-Store import).
- More providers (R2, AWS S3, Google Drive) and TLS for the S3 client.
