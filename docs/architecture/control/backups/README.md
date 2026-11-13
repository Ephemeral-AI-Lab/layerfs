# Backups

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Whole sqlite Store backup to a bucket and restore from it, over bucket drivers
that know nothing about Stores.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-store` (the `backup` calls), `layerfs-bucket` |
| Source | [`layerfs-store/src/backup`](../../../../control/crates/layerfs-store/src/backup), [`layerfs-bucket/src/backup`](../../../../control/crates/layerfs-bucket/src/backup), [`layerfs-bucket/src/bucket`](../../../../control/crates/layerfs-bucket/src/bucket) |
| Uses | [Engine: Store](../../engine/store/README.md), [Engine: Store, SQLite](../../engine/store/sqlite.md), [Stores and projects](../stores-and-projects/README.md), [Home](../home/README.md), [Machines: workers and reach](../machines/workers-and-reach.md) |
| Used by | [SDK: backup](../../api/sdk/backup.md), [Doctor](../doctor/README.md) (bucket diagnostics) |
| Flows | [Flows](../../flows/README.md): backup and restore |

## Picture

```text
 caller process: holds the bucket key; every bucket request is sent from here
+----------------------------------------------------------------------------+
| layerfs-store backup/       layerfs-bucket backup/   layerfs-bucket bucket/ |
|  connect, disconnect        copies: check, list --->  Driver (sealed)       |
|  list (states) ---------->  heads digest              pieces: whole, or     |
|  create: stage on the Store's machine (store.file)    pieces + manifest     |
|    [worker: 8 MiB reads into this process] -------->  upload                |
|  restore: install, <------- load: download, verify,   download              |
|    records                  open Disposable, link        |                  |
+----------------------------------------------------------|-----------------+
                                                           v once per request
     local_folder (tests)  |  minio --+--> s3_api: SigV4 over HTTP/1.1
                              r2 -----+
     bucket: <prefix>/<file>/store.sqlite  [ .piece-000000 ... ]
```

`layerfs-bucket/src/bucket` holds files by name and nothing else; its drivers,
pieces and manifest are on [Buckets](buckets.md). `layerfs-bucket/src/backup`
turns a Store file into one bucket file and back; `layerfs-store/src/backup`
is the `backup` call family, its records and its locks. Where each call runs
is in the [control layer table](../README.md).

## What it does

The unit is a whole sqlite Store with every project in it, held as the object
`<prefix>/<file>/store.sqlite` (`store_object(file)`). `file` defaults to the
Store's name and need not equal it. LayerFS never opens SQLite over the network.
A PostgreSQL Store is not backed up here.

| Behaviour | Detail | Source |
| --- | --- | --- |
| `backup.connect(name, url, { key, region, part_size })` | URL `http(s)://host:port/<bucket>/<prefix>` (MinIO) or `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu\|fedramp]` (R2). Defaults: key `env:LAYERFS_BUCKET_<NAME>` (upper case, `-` and `.` become `_`), region `us-east-1` (R2 always `auto`), part size 64 MiB. One listing proves the bucket answers with that key, then `backup/<name>/record.json` is written under the home lock. | [`family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs), [`record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| `backup.disconnect(name)` | Removes the bucket record and every Store's link to it; answers `{ disconnected, unlinked }`. The objects stay. | [`family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs) |
| `backup.list(bucket?)` | One listing per bucket. Each copy gets one item per linked Store: `current`, `store_changed` (Store heads differ from the link's), `bucket_changed` (copy sha256 differs from the link's last), `both_changed`, `unknown` with `reason` (Store not on `native`, PostgreSQL, or a link or listing without heads or sha256), `unlinked`, or `missing` (linked, gone from the bucket). | [`list.rs`](../../../../control/crates/layerfs-store/src/backup/list.rs) |
| `backup.create(store, bucket, file?)` | Steps below. Answers `{ store, bucket, file, object, bytes, pieces, sha256, heads }`. | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| `backup.restore(store, bucket, file?)` | Steps below. `file` is the argument, else the Store's link, else the Store's name. Answers `{ store, bucket, file, bytes, sha256, heads, projects }`. | [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| Store link | The Store record's `backups.<bucket>`: `file`, `heads`, `last { op, at, bytes, sha256 }`, written after each create (`op: create`) and restore (`op: restore`). Holds no secret. | [`record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| Heads digest | sha256 of the sorted `layerfs_persistence::read_heads` lines, each followed by `\n`, read on a connection of its own. It changes exactly when the history a restore would bring changes. | [`heads.rs`](../../../../control/crates/layerfs-bucket/src/backup/heads.rs) |
| Copies | `check(driver, file)`: one stat, `Absent` or `Present(Copy { file, bytes, modified, sha256, heads })`. `list_stores(driver)`: one listing, every `<file>/store.sqlite` in name order, plus at most 64 stats where the listing lacks metadata; a pieced copy with unread metadata counts its pieces' sizes. | [`copies.rs`](../../../../control/crates/layerfs-bucket/src/backup/copies.rs) |
| Progress | `{ phase, done, total, pieces }`. Create: `check`, `snapshot`, `send` (worker only), `check`, `hash`, `upload`. Restore: `check`, `download`, `verify`, `install`. Byte counts at most every 250 ms plus each phase's exact last count; the transfer runs on a thread of its own. | [`progress.rs`](../../../../control/crates/layerfs-store/src/backup/progress.rs) |
| Library `push` and `pull` | The same sequences in one process (`push` adds a `statvfs` room check and a 16 MiB manifest bound). Only tests call them; the calls above use the stage step, `pieces::upload` and `load`. | [`create.rs`](../../../../control/crates/layerfs-bucket/src/backup/create.rs), [`restore.rs`](../../../../control/crates/layerfs-bucket/src/backup/restore.rs) |
| Legacy link files | `remotes` and `remote-state/<store>` (`Remotes`, `Transfer`, `link::expected`), 0600 text in the home, read only by the home migration from 1.1.0. `Provider` (MinIO, R2, local folder) builds every driver. | [`link.rs`](../../../../control/crates/layerfs-bucket/src/backup/link.rs), [`v110.rs`](../../../../control/crates/layerfs-control/src/migrate/v110.rs) |

### `backup.create`

| # | Step | Source |
| --- | --- | --- |
| 1 | Read the bucket and Store records; refuse PostgreSQL and a record with an unfinished operation. `expected`: the link's last sha256 when the link names this `file`, else the object must be absent. Hold the Store's lease and `store/<s>/backup.lock` (`flock`, no wait). | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| 2 | `check`: one stat. The object must match `expected`, and its `layerfs-exported-from` must name this home. | same, `check` |
| 3 | Stage (`snapshot`) on the Store's machine (in process on `native`, else the worker's `store.file` step): remove any old `<store>.backup`, copy the live Store with `VACUUM INTO` in one read transaction, answer `{ bytes, sha256, heads, sqlite }`. | [`steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| 4 | Worker only (`send`): read the copy in 8 MiB steps into `store/<s>/backup.staging` and compare its sha256 with the worker's. | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs), `fetch_staged` |
| 5 | Metadata: `layerfs-sqlite`, `layerfs-binding` (hex; not a secret; the cursor key never leaves its holder), `layerfs-exported-from` (`<home id>/<machine>`), `layerfs-heads` when readable. | same; [`create.rs` (bucket)](../../../../control/crates/layerfs-bucket/src/backup/create.rs) |
| 6 | `check` again, then `pieces::upload` (`hash`, `upload`). | [`pieces.rs`](../../../../control/crates/layerfs-bucket/src/bucket/pieces.rs) |
| 7 | Remove the staged copies (`unstage`) whatever happened; on success save the link. | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |

### `backup.restore`

| # | Step | Source |
| --- | --- | --- |
| 1 | Existing `store/<name>` folder (if present): `store.create`'s kernel guard requires a real `0700` directory of this user (`Unsupported`, `chmod 700` before any staging). Existing Store: refuse PostgreSQL; under the home lock and the Store's lock check `replaceable`: on `native`, no unfinished operation, this process's open copy closed, no other lease holder or running Sandbox (`store_idle`), heads equal to the link's, no `-wal` or `-shm`. Then hold its lease. A new Store gets a fresh cursor key. | [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |
| 2 | Make `store/.restore-<random>/` with a held `lock` file; first remove older restore folders whose lock can be taken (killed restores). | same, `Staging`, `sweep` |
| 3 | `load` into it: `installer_user` (daemons' user), make the destination directory `0700` when absent (`install::make_private_directory`), `kernel_guard` on the Store file (a guard refusal removes only the still-empty directory this call's `mkdir` made; installer-user or `mkdir` failure removes nothing), no Store at the target, no sidecars; stat the object and take `layerfs-binding`; download to `<target>.pulling` (0600, create-new) through `pieces::download`. | [`restore.rs` (bucket)](../../../../control/crates/layerfs-bucket/src/backup/restore.rs) |
| 4 | `verify`: open the bytes as a Disposable Store with that binding and cursor key, close, read heads, require no `-wal`/`-shm` left; `install`: hard link into place. | same |
| 5 | The binding must be the existing Store's. Read the Store's projects on a connection of the call's own. | [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs), `projects_in` |
| 6 | Home lock: re-check the target's folder with the kernel guard (`backup::restore::install`) so a folder that appeared or lost its privacy during the download is refused `Unsupported` with nothing installed; a new Store is hard-linked (never over one that appeared), an existing one is re-checked and renamed over. Write the link and project records; remove records of projects the Store no longer holds. The staging folder goes when the call ends. | same, `write_projects` |

The download and its checks run before the home lock; the lock covers only
the re-checks, the install and the record writes. A restore opens the bytes
with the Disposable profile (WAL, `synchronous=OFF`), the only permitted Store
profile; Durable execution is `NOT_RUN — disabled by owner until explicit
reauthorization`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Every bucket request is sent from the caller's process; a worker stages and serves bytes but never sees the key. | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| Records hold credential references, never secrets; the key is resolved on each call, so a changed key needs no reconnect. | [`record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs), [`credentials.rs`](../../../../control/crates/layerfs-bucket/src/bucket/credentials.rs) |
| Each request is attempted once; an unknown outcome is reported, never resent. | [`error.rs`](../../../../control/crates/layerfs-bucket/src/bucket/error.rs) |
| A create overwrites only the copy this home last moved through the link, or nothing; a copy another home wrote is never overwritten. Not compare-and-swap: a write between the check and the upload is not detected. | `check` in [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| One create of a Store at a time per home (`backup.lock`). | `Creating` in [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| Restored bytes pass every sha256 check and open as a Disposable Store with the right binding before they are installed. | `fetch` in [`restore.rs` (bucket)](../../../../control/crates/layerfs-bucket/src/backup/restore.rs) |
| A restore replaces only an idle Store on `native` unchanged since its last transfer through that bucket; a new Store never replaces one that appeared meanwhile. | `replaceable` in [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| Disconnect deletes no object; the only deletes are stale pieces beyond a new upload's count. | [`family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs), [`pieces.rs`](../../../../control/crates/layerfs-bucket/src/bucket/pieces.rs) |

## Limits and failures

Bucket errors map to call refusals: `Uncertain` is `Unknown`, `Invalid` is
`InvalidInput`, every other driver error is `Failed` with its text
(`remote_error` in [`record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs)).

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Bucket file name | 1 to 128 of `[A-Za-z0-9._-]`, not starting with `.`; else `InvalidInput` | `store_object` in [`link.rs`](../../../../control/crates/layerfs-bucket/src/backup/link.rs) |
| No such bucket or Store | `NotFound` | [`record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| Connect a name already connected | `Exists` | [`family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs) |
| PostgreSQL Store | `InvalidInput` ("back it up with PostgreSQL's tools") | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs), [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| Store record holds an unfinished operation | `Busy` or `Unresolved` | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| Create: another create of the Store runs | `Busy`; nothing changed | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| Create: object written by another home | `Exists`; back up to another file | same, `check` |
| Create: object is not the copy this home last moved | `Conflict`; nothing uploaded | same |
| Create: snapshot fails | `Failed` "snapshot: …" | [`steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| Create: fetched worker copy differs | `Failed` "the staged copy is not the snapshot the worker made" | [`create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| Create: a request's answer is lost | `Unknown`; the object may or may not be replaced; not resent | [`record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| Restore over a Store not on `native`, or into a non-private `store/<name>` folder | `Unsupported`; move to `native` or `chmod 700` the Store's folder (checked before staging and again under the home lock before install) | `replaceable`, `install` in [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| Restore over a changed Store | `Conflict`; back it up first or restore under a new name | same |
| Restore over an open Store (lease holder, Sandbox, `-wal`/`-shm`) | `InUse` | same; [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |
| Restore: any load check (missing object, no binding, sha256, open) | `Failed` naming the check, e.g. "no Store `<file>` on the remote: remote file missing"; the staging folder is removed | [`restore.rs` (bucket)](../../../../control/crates/layerfs-bucket/src/backup/restore.rs) |
| Restore: another binding | `Failed` "… has another binding" | [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| Restore: a Store appeared, vanished or was replaced during the download | `Exists`, `NotFound`, `Conflict` | same |
| Stats per listing | 64 (`MAX_LIST_STATS`); further copies show no sha256 or heads (`unknown`) | [`copies.rs`](../../../../control/crates/layerfs-bucket/src/backup/copies.rs) |
| Local space | Create: the snapshot beside the Store, plus a second copy in the caller for a worker's Store. Restore: one copy in the staging folder | [`steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs), [`restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| Left by a killed call | `<store>.backup`: removed by the next create's stage. `store/.restore-*/`: removed by the next restore. Incomplete multipart uploads: removed with the provider's tools | same |
| Object, request, part and manifest limits | per provider | [Buckets](buckets.md) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`folder_pieces.rs`](../../../../control/crates/layerfs-bucket/tests/folder_pieces.rs) | `cargo +1.85.1 test --locked -p layerfs-bucket --test folder_pieces` | any | Folder driver (whole, ranged, stat, delete); a file over `max_file` goes up as pieces with a manifest and is verified; manifest refusals; reserved metadata keys refused. |
| [`copies.rs`](../../../../control/crates/layerfs-bucket/tests/copies.rs) | `cargo +1.85.1 test --locked -p layerfs-bucket --test copies` | any | One stat tells a copy's size, sha256 and heads; one listing finds every Store copy, pieced ones at whole size. |
| [`outcomes.rs`](../../../../control/crates/layerfs-bucket/tests/outcomes.rs), [`keep_alive.rs`](../../../../control/crates/layerfs-bucket/tests/keep_alive.rs) | `cargo +1.85.1 test --locked -p layerfs-bucket --test outcomes --test keep_alive` | any (local fake server) | A lost completion stays `Uncertain`; a lost part with a lost abort is `PartsLeft`; a 403 sent before the body is read is definite; a closed kept connection is not reused and nothing is resent. |
| [`minio.rs`](../../../../control/crates/layerfs-bucket/tests/minio.rs) | `control/crates/layerfs-bucket/tests/minio_service.sh start NAME`, eval its exports, then `cargo +1.85.1 test --locked -p layerfs-bucket --test minio -- --ignored` | Docker host | The S3 driver against MinIO: PUT, ranged GET, HEAD, DELETE, multipart, aborted failures. `part_sizes_and_config_refusals` runs without MinIO. |
| [`backup_refusals.rs`](../../../../api/crates/layerfs-api/tests/backup_refusals.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test backup_refusals` | Linux | Restore over a Store with a running Sandbox is `InUse` before any request; a second create is `Busy`; the next restore removes a killed restore's folder. |
| [`backup_family.rs`](../../../../api/crates/layerfs-api/tests/backup_family.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test backup_family` with the `minio_service.sh` exports | Linux, MinIO | connect, create, list states, restore `Conflict`, restore as a new Store, a Store behind a worker, concurrent restores, foreign copies, disconnect keeping objects. Prints NOT_RUN and passes without the exports. |
| [`backup_progress.rs`](../../../../api/crates/layerfs-api/tests/backup_progress.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test backup_progress` with the exports | Linux, MinIO | Create and restore send their phases in order, ending on the bytes moved. |

## Pages

| Page | What it covers |
| --- | --- |
| [Buckets](buckets.md) | The driver contract, pieces and manifest, progress, credentials, the local folder, MinIO, R2 and S3 API drivers, and the tested providers. |
