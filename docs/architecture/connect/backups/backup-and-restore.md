# Backup and restore

[Architecture](../../README.md) › [Connect](../README.md) › [Backups](README.md) › Backup and restore

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Whole-Store backup and restore.

| | |
| --- | --- |
| Component | [Backups](README.md) |
| Source | [`connect/crates/layerfs-store-backup/src/backup`](../../../../connect/crates/layerfs-store-backup/src/backup); the calls in [`api/crates/layerfs-control/src/backup`](../../../../api/crates/layerfs-control/src/backup) |

The unit is a whole sqlite Store with every project in it, held on the
bucket as `<prefix>/<file>/store.sqlite` (`store_object(file)`). LayerFS
never opens SQLite over the network
([`backup/mod.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/mod.rs)).

## Create

```text
 backup.create(store, bucket, file?)                 api/…/backup/create.rs
   caller: refuse PostgreSQL; hold the Store's lease
   check(bucket object == expected, exported-from == this home) --+
   Store's machine (in process on native, else its worker):       |
     store.file "stage": OpenedStore file().snapshot ->           |
       <store>.backup  + { bytes, sha256, heads, sqlite }         |
   worker's copy? -> read it to the caller in 8 MiB chunks,       |
                     compare its sha256 with the worker's         |
   check again --------------------------------------------------+
   pieces::upload(driver, object, staged, metadata)   caller only
   unstage; save the link in the Store record (backups.<bucket>)
```

Metadata a backup carries, besides `layerfs-sha256`, `layerfs-bytes` and
`layerfs-layout` from [pieces](pieces-and-manifest.md):

| Key | Value | Source |
| --- | --- | --- |
| `layerfs-sqlite` | SQLite version that wrote the snapshot (`SQLITE_VERSION`) | [`backup/create.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/create.rs) |
| `layerfs-binding` | the Store's binding key, hex; not a secret; the cursor key never leaves its holder (`BINDING`) | same |
| `layerfs-heads` | history heads digest of the snapshot (`HEADS`), when readable | [`backup/heads.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/heads.rs) |
| `layerfs-exported-from` | `<home id>/<machine>`, added by the API | [`api/…/backup/create.rs`](../../../../api/crates/layerfs-control/src/backup/create.rs) |

The crate also has `backup::push(opened, store, driver, name, binding,
expected)`, the same sequence in one process:

| Step | What | Failure |
| --- | --- | --- |
| 1 | `store_object(name)`; binding 1 to 128 bytes. | `PushFailure::Invalid` |
| 2 | Room beside the Store for main file + WAL + `STORE_HEADROOM_BYTES` (`statvfs`). | `Space { needed, available }` |
| 3 | `check_expected`: `Any`, `Absent`, or `Sha256(x)` against the remote's `layerfs-sha256`. | `RemoteMoved { expected, found }` |
| 4 | Snapshot to `<store>.pushing`, read heads; refuse a manifest over 16 MiB. | `Snapshot` / `Remote(TooLarge)` |
| 5 | `check_expected` again, then `pieces::upload`; uploaded size must equal the snapshot's. | `Remote(…)`, `Snapshot` |
| 6 | Remove `<store>.pushing` and its `-wal`/`-shm` whatever happened. | — |

The 2.0 `backup.create` composes the stage step and `pieces::upload`
itself and does not call `push`; within the source, `push` and `pull`
have callers only in tests
([`api/crates/layerfs-api/tests/remote_store.rs`](../../../../api/crates/layerfs-api/tests/remote_store.rs)).
The check is not compare-and-swap: a push by another machine between the
check and the upload is not detected.

## Copies

| Function | Requests | Answers | Source |
| --- | --- | --- | --- |
| `check(driver, file)` | one `stat` | `Absent`, or `Present(Copy { file, bytes, modified, sha256, heads })` | [`copies.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/copies.rs) |
| `list_stores(driver)` | one `list("")`, plus at most 64 stats (`MAX_LIST_STATS`) where the listing lacks metadata | every `<file>/store.sqlite`, in name order; other files skipped | same |

A copy stored as pieces whose metadata was not read is counted at the sum
of its pieces' sizes. `backup.list` compares each copy with the Store's
link to report its state ([`api/…/backup/list.rs`](../../../../api/crates/layerfs-control/src/backup/list.rs)).

## Heads

```text
 layerfs_persistence::read_heads(store file)   own connection, closed after
   -> lines, sorted
   -> "<line>\n<line>\n..."  -> sha256  = heads digest (32 bytes)
```

The digest changes exactly when the history a restore would bring changes,
and is equal on two machines with the same history
([`heads.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/heads.rs)).
A backup records it as `layerfs-heads`; a restore reads it from the
downloaded file. The API uses it to tell whether a Store changed since its
last transfer (`replaceable` in
[`api/…/backup/restore.rs`](../../../../api/crates/layerfs-control/src/backup/restore.rs)).

## Link

Two link formats exist:

| Format | Where | Who reads it | Source |
| --- | --- | --- | --- |
| 2.0 `StoreLink` | the Store record's `backups.<bucket>`: `file`, `heads`, `last` (`op`, `at`, `bytes`, `sha256`) | `backup.create`, `backup.restore`, `backup.list` | [`api/…/backup/record.rs`](../../../../api/crates/layerfs-control/src/backup/record.rs) |
| `remotes` file and `remote-state/<store>` (`Remotes`, `Provider`, `Transfer`) | the LayerFS home, text, 0600, staged and renamed | the home migration from 1.1.0 | [`backup/link.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/link.rs), [`migrate/v110.rs`](../../../../api/crates/layerfs-control/src/common/home/migrate/v110.rs) |

In both, the bucket side is named by its `file`; the name need not be the
Store's own (#509). Neither holds a secret: credentials are references.
`link::expected` gives what a push through a link must find: the recorded
sha256 when the record names that remote and file, else `Absent`, so a
push never overwrites a file this home did not move.

## Restore

```text
 backup.restore(store, bucket, file?)               api/…/backup/restore.rs
   existing Store? refuse PostgreSQL; replaceable? (lock; checked)
   backup::load(driver, file, store/.restore-<random>/store.sqlite, cursor)
   |  1 store_object(file); kernel_guard on the directory; installer_user
   |  2 destination absent (load); no -wal/-shm beside it
   |  3 stat: present; binding from layerfs-binding (else refused)
   |  4 download to <target>.pulling (0600, create_new): pieces::download
   |      manifest, each piece, whole file against sha256
   |  5 open as Disposable with the binding and cursor key; close
   |  6 heads digest; no -wal/-shm left after closing
   |  7 hard link into place; remove the temporary name
   v
   binding == existing Store's?  (else Failed)
   read its projects
   home lock: re-check replaceable; hard link (new) or rename (replace)
   write the link and project records
```

| # | Check | Refusal | Source |
| --- | --- | --- | --- |
| 1 | The destination's filesystem passes the Store kernel guard; the installer is the daemons' user. | `PullFailure` cause | [`restore.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/restore.rs) |
| 2 | Load: no Store at the target. Pull: no `-wal` or `-shm` beside it (no opener holds it). | "a Store exists"; "the local Store … is open" | same |
| 3 | The remote file exists and records `layerfs-binding`; a pull's expected binding matches. | "records no layerfs-binding; push it again"; "has another binding" | same |
| 4 | Manifest, every piece and the whole file match their sha256. | `Integrity` | [`pieces.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/pieces.rs) |
| 5 | The bytes open as a Disposable Store with that binding and cursor key. | "the pulled Store does not open" | [`restore.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/restore.rs) |
| 6 | Closing it left no `-wal` or `-shm`. | "kept its -wal after closing" | same |
| 7 | Load places it with a hard link, so a Store that appeared meanwhile is never replaced; pull renames over the target after checking again that no opener holds it. | link or rename error | same |

One attempt: a failure keeps the temporary file it left and names it in
`PullFailure::left`. The API runs the download and its checks before the
home lock; the lock covers only the re-checks, the install and the record
writes. Its staging folder `store/.restore-<random>/` is removed when the
call ends.

## Related

- [Pieces and manifest](pieces-and-manifest.md): the byte-level checks.
- [Buckets](buckets.md): drivers, limits and credentials.
- [Engine: Store, SQLite](../../engine/store/sqlite.md): snapshot, binding and the Disposable profile a restore opens with.
- [Backup and restore flow](../../flows/backup.md): the calls across layers.

---

← [Backups: Pieces and manifest](pieces-and-manifest.md) · [Up: Backups](README.md) · [API](../../api/README.md) →
