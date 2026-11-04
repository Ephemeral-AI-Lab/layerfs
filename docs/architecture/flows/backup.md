# Backup and restore

[Architecture](../README.md) › [Flows](README.md) › Backup and restore

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`backup.create` and `backup.restore`: from a Store to a bucket and back.

The unit is a whole sqlite Store with every project in it, held in a bucket
as one object `<file>/store.sqlite` (whole, or as pieces and a manifest). A
PostgreSQL Store is backed up with PostgreSQL's own tools. Bucket I/O always
runs in the caller's process: a worker never sees the bucket's credentials.
Terms are in the [glossary](../glossary.md).

## The path

```text
 caller process          machine worker         Store file      bucket
                         (Store on another                      (S3 API)
                          kernel only)
 ----------------------  ---------------------  --------------  ---------
 backup.create(store, bucket, file?)
 lease::hold
 check: stat object --------------------------------------------> HEAD
 store.file "stage" ---> with_service:
 ==|== (worker route) =|   VACUUM INTO ----------> read tx
                         <store>.backup + {bytes, sha256, heads}
 <-- read x N (8 MiB) --  (worker route only)
 sha256 == worker's?
 check again ---------------------------------------------------> HEAD
 pieces::upload: pieces ... then manifest ---------------------> PUT x N
 store.file "unstage" -> remove <store>.backup
 save link in the Store record

 backup.restore(store, bucket, file?)          (Store on native only)
 [home + Store lock] replaceable? (idle, unchanged since link)
 backup::load into store/.restore-<random>/:
   stat; download manifest, pieces --------------------------> GET x N
   check sha256s; open as Disposable --> store.sqlite (staged)
   heads; no -wal/-shm; hard link into place
 binding == existing Store's?
 read its projects
 [home lock] re-check; hard link (new) | rename (replace)
 write the link and project records
```

| Step | Component |
| --- | --- |
| 1 | [API: Control](../api/control/README.md) |
| 2 | [Connect: Backups](../connect/backups/README.md) |
| 3 | [Engine: Store](../engine/store/README.md) |

## Steps

### What a backup copies, and from where

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 1 | `backup.create(store, bucket, file?)`: the bucket and Store records are read; a PostgreSQL Store is refused. `file` defaults to the Store's name; the object is `store_object(file)`. What the bucket must hold is the copy this home last moved through the Store's link to that file (its sha256), else nothing. The caller holds its lease on the Store. | [Control: families](../api/control/families.md), [SDK: backup](../api/sdk/backup/README.md) | [`backup/create.rs` (api)](../../../api/crates/layerfs-control/src/backup/create.rs) |
| 2 | `check`: one `stat` of the object. It must be what this home expects, and its `layerfs-exported-from` must not name another home. | [Backups: backup and restore](../connect/backups/backup-and-restore.md#create) | same, `check` |
| 3 | Stage (progress `snapshot`): on the Store's machine (this process for `native`, else the worker's `store.file` op) the Store is opened and copied with `VACUUM INTO` on its own read-only connection, so writers in every process continue. The copy, `<store>.backup`, is the Store as of one read transaction; the step answers its bytes, sha256, heads digest and SQLite version. | [Store: SQLite: heads and snapshots](../engine/store/sqlite.md#heads-and-snapshots) | [`transfer/steps.rs`](../../../api/crates/layerfs-control/src/store/transfer/steps.rs), [`sqlite/snapshot.rs`](../../../engine/crates/layerfs-persistence/src/backend/sqlite/snapshot.rs) |
| 4 | Worker route only (progress `send`): the caller reads the staged copy in 8 MiB `read` requests into `store/<name>/backup.staging` and compares its sha256 with the worker's. | [Network: workers](../connect/network/workers.md#one-request-per-connection) | [`backup/create.rs` (api)](../../../api/crates/layerfs-control/src/backup/create.rs) `fetch_staged` |

### Pieces and the manifest

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 5 | Metadata: `layerfs-sqlite`, `layerfs-binding` (not a secret), `layerfs-exported-from` (`<home id>/<machine>`) and `layerfs-heads`. | [Backups: backup and restore](../connect/backups/backup-and-restore.md#create) | [`backup/create.rs` (api)](../../../api/crates/layerfs-control/src/backup/create.rs) |
| 6 | Progress `upload`: `check` again, then `pieces::upload`. The whole file is hashed once. A file within the driver's `max_file` goes up whole; a larger one goes up as `<name>.piece-NNNNNN` objects, each with its sha256, then the manifest last as `<name>`. Pieces left by an earlier, larger upload are removed. | [Backups: pieces and manifest](../connect/backups/pieces-and-manifest.md#pieces), [Backups: buckets](../connect/backups/buckets.md) | [`bucket/pieces.rs`](../../../connect/crates/layerfs-store-backup/src/bucket/pieces.rs) |
| 7 | The staged copies are removed (`unstage`), and the Store record's `backups.<bucket>` link saves `file`, bytes, sha256 and heads. The answer is `{ store, bucket, file, object, bytes, pieces, sha256, heads }`. | [Backups: backup and restore](../connect/backups/backup-and-restore.md#link) | [`backup/record.rs`](../../../api/crates/layerfs-control/src/backup/record.rs) |

Nothing is atomic across bucket requests. Because the manifest goes last, a
manifest names only pieces already sent, and a download racing an upload
fails a sha256 check instead of returning mixed bytes. The `check` is not a
compare-and-swap: another home's upload between the check and the upload is
not detected.

### Restore and its checks

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 1 | `backup.restore(store, bucket, file?)` restores to a sqlite Store on `native`. `file` is the argument, else the Store's link, else the Store's name. | [SDK: backup](../api/sdk/backup/README.md) | [`backup/restore.rs` (api)](../../../api/crates/layerfs-control/src/backup/restore.rs) |
| 2 | An existing Store must be replaceable, checked under the home lock and the Store's lock: on `native`, no intent, this process's open copy closed, no Sandbox or other holder, its heads equal to the link's heads (unchanged since its last backup or restore), and no `-wal` or `-shm` beside it. | [Backups: backup and restore](../connect/backups/backup-and-restore.md#restore) | same, `replaceable` |
| 3 | Progress `download`: `backup::load` into a private `store/.restore-<random>/` folder. Kernel guard on the folder; no Store at the target; the object exists and records `layerfs-binding`; manifest, every piece and the whole file match their sha256; the bytes open as a Disposable Store with that binding and the cursor key (the existing Store's, or a new one); closing leaves no sidecar; a hard link places it. | [Backups: backup and restore](../connect/backups/backup-and-restore.md#restore), [Backups: pieces and manifest](../connect/backups/pieces-and-manifest.md#sha256-manifest), [Store: the Disposable profile](../engine/store/lifecycle.md#the-disposable-profile) | [`backup/restore.rs` (connect)](../../../connect/crates/layerfs-store-backup/src/backup/restore.rs) |
| 4 | The downloaded binding must be the existing Store's. Its projects are read on a connection of this call's own. | [Backups: backup and restore](../connect/backups/backup-and-restore.md#heads) | [`backup/restore.rs` (api)](../../../api/crates/layerfs-control/src/backup/restore.rs) `projects_in` |
| 5 | Progress `install`, under the home lock: a new Store is hard-linked into place (never over a Store that appeared meanwhile); a replaced Store is checked again and renamed over. | — | same |
| 6 | The link (`op: restore`) and the project records are written; records of projects the Store no longer holds are removed. The staging folder is removed when the call ends. | [Control: home and records](../api/control/home-and-records.md#records) | same, `write_projects` |

The download and its checks run before the home lock; the lock covers only
the re-checks, the install and the record writes.

## Failures along the way

| Step | What can fail | What the caller sees |
| --- | --- | --- |
| create 1 | No such bucket or Store | `NotFound` |
| create 1 | A PostgreSQL Store | `InvalidInput` "back it up with PostgreSQL's tools" |
| create 1 | The Store record holds an intent | `Busy` or `Unresolved` |
| create 2, 6 | The object was written by another home | `Exists`; back up to another file |
| create 2, 6 | The object is not the copy this home last moved | `Conflict`; nothing uploaded |
| create 3 | Snapshot fails | `Failed` "snapshot: …" |
| create 3–4 | Worker not reached | `Unavailable`; nothing sent |
| create 4 | The fetched copy's sha256 differs | `Failed` "the staged copy is not the snapshot the worker made" |
| create 6 | A bucket request's answer is lost | `Unknown`; the call is not resent |
| create 6 | Invalid name or metadata | `InvalidInput` |
| create 6 | Any other bucket failure | `Failed` |
| restore 1–2 | Existing Store is PostgreSQL | `InvalidInput` |
| restore 2 | Existing Store is not on `native` | `Unsupported`; move it there first |
| restore 2 | Store changed since its last transfer | `Conflict`; back it up first or restore under a new name |
| restore 2 | Store open (a call running here, a holder, or `-wal`/`-shm`) | `InUse` |
| restore 3 | Any load check fails (missing object, no binding, sha256, open) | `Failed`, naming the check and any temporary file left |
| restore 4 | Another binding | `Failed` "… has another binding" |
| restore 5 | A Store of that name appeared, or was removed or replaced, during the download | `Exists`, `NotFound`, `Conflict` |

## Demonstration

These commands were written from source and were not run while writing this
page.

| Test | What it sets up | What it checks | Command | Needs |
| --- | --- | --- | --- | --- |
| [`minio.rs`](../../../connect/crates/layerfs-store-backup/tests/minio.rs) | The S3 driver against a running MinIO | Single PUT, ranged GET, HEAD, DELETE; multipart above the part size; pieces; a failed multipart aborted with no parts left; a lost part reply is uncertain, not resent | `cargo +1.85.1 test --locked -p layerfs-store-backup --test minio -- --ignored` | The service tests are `#[ignore]`: start MinIO with `connect/crates/layerfs-store-backup/tests/minio_service.sh start NAME` and eval its exports; `--features tls` for an `https://` MinIO (fault-injecting tests then report skipped). `part_sizes_and_config_refusals` runs without MinIO |
| [`backup_family.rs`](../../../api/crates/layerfs-api/tests/backup_family.rs) | A home, Stores on `native` and behind an in-process worker, and a bucket record for MinIO | `backup.connect`, `create`, the list states, the restore `Conflict`, a restore as a new Store, a backup of a Store behind a worker, concurrent restores under a new name, foreign copies, and `disconnect` keeping the objects | `cargo +1.85.1 test --locked -p layerfs-api --test backup_family` | `LAYERFS_REMOTE_ENDPOINT`, `LAYERFS_REMOTE_BUCKET` and the other variables `minio_service.sh` prints; without them the live tests print NOT_RUN and pass |

---

← [Flow: Fork and merge](merge.md) · [Up: Flows](README.md)
