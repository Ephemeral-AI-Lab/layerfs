# backup family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › backup

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `backup` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/backup`](../../../../../api/crates/layerfs-api/src/backup), [`backup`](../../../../../control/crates/layerfs-store/src/backup) |

## Calls

```text
 connect (bucket name, url, key ref)
    |
    v
 list  <---------------------------------------------+
    |   current | store_changed | bucket_changed ...  |
    v                                                 |
 create (sqlite Store -> bucket) --- link saved ------+
    |
    v
 restore (bucket -> sqlite Store on native) --- link saved
    |
    v
 disconnect (forget bucket + links; objects stay)
```

Five calls, in help order
([`backup/mod.rs`](../../../../../api/crates/layerfs-api/src/backup/mod.rs)).
Each runs `layerfs_control::backup::family::run`
([`backup/family.rs`](../../../../../control/crates/layerfs-store/src/backup/family.rs)).
A bucket is an r2 or MinIO bucket and prefix, recorded by name; each Store
keeps one link per bucket in its record.

| Call | Arguments | Options | Returns |
| --- | --- | --- | --- |
| [`connect`](connect.md) | `name` (Name), `url` (Text) | `key`, `region` (Text), `part_size` (Size) | the bucket record (all fields but `v`) |
| [`disconnect`](disconnect.md) | `name` | — | `{ disconnected, unlinked: [store] }`; objects stay |
| [`list`](list.md) | `bucket?` (Name) | — | `{ items: [{ bucket, file, bytes, heads, store, state, reason? }] }` |
| [`create`](create.md) | `store`, `bucket` (Name), `file?` (Name) | `on_progress` (Function) | `{ store, bucket, file, object, bytes, pieces, sha256, heads }` |
| [`restore`](restore.md) | `store`, `bucket`, `file?` | `on_progress` | `{ store, bucket, file, bytes, sha256, heads, projects }` |

Sources: [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs),
[`backup/list.rs`](../../../../../control/crates/layerfs-store/src/backup/list.rs),
[`backup/create.rs`](../../../../../control/crates/layerfs-store/src/backup/create.rs),
[`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs).
LayerFS never deletes a bucket object.

## Arguments and options

| Name | Kind | Rule |
| --- | --- | --- |
| `url` | Text | `http(s)://host:port/<bucket>/<prefix>` for MinIO; `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu\|fedramp]` for r2 |
| `key` | Text | a credential reference, `env:LAYERFS_…` or `file:/absolute/path`, never a secret; default `env:LAYERFS_BUCKET_<NAME>`; resolved when a driver is made ([`credentials.rs`](../../../../../control/crates/layerfs-bucket/src/bucket/credentials.rs)) |
| `region` | Text | MinIO region, default `us-east-1` |
| `part_size` | Size | upload part size; `K`, `M`, `G` text is turned into bytes before dispatch |
| `file` | Name | the object name; `create` defaults to the Store's name, `restore` to the Store's link |
| `on_progress` | Function | `PROGRESS`; events `{ progress: { phase, done, total, pieces } }` (see [Progress](#progress)) |

## Progress

`create` and `restore` send `{ phase, done, total, pieces }`
([`backup/progress.rs`](../../../../../control/crates/layerfs-store/src/backup/progress.rs)):

| Field | Value |
| --- | --- |
| `phase` | the step now running (below) |
| `done`, `total` | bytes while the phase moves bytes (`total` null when unknown); null in a phase that moves none |
| `pieces` | `{ done, of }` while a file goes up or down as pieces, else null |

The transfer runs on a thread of its own while the calling thread sends the
bucket library's counts: at most one byte update per 250 ms, each phase's
exact last count before the next phase starts, and nothing repeated. Byte
counts are the bytes written to or read from the connection; the S3 part
hash computed before each multipart part is not counted. A phase that
reaches its total says only that its bytes moved: `restore` still verifies
and installs, and either call has succeeded only when it returns its
result. A failed transfer's last update is what it moved; nothing is
retried. The CLI prints each update as one JSON line on stderr with
`--progress`, and nothing without it.

```text
 backup.create                          backup.restore
 check     the bucket holds what        check     (before the download)
           this home expects            download  bytes, pieces
 snapshot  store.file stage on the      verify    the bytes open as the Store
           Store's machine              install   a sqlite Store on native
 send      (worker only) bytes
           -> this process
 check     again, just before upload
 hash      bytes of the snapshot
 upload    bytes, pieces -> bucket
           unstage on the machine
```

`connect` lists the bucket once with the key before it records anything.
An object is `<prefix>/<file>/store.sqlite` (whole or a manifest) plus its
pieces.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name, URL or key reference | [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs) |
| `NotFound` | no bucket or Store of that name | [`backup/family.rs`](../../../../../control/crates/layerfs-store/src/backup/family.rs), [`backup/create.rs`](../../../../../control/crates/layerfs-store/src/backup/create.rs) |
| `Exists` | a bucket name taken; an object written by another home; a Store made during a restore | [`backup/create.rs`](../../../../../control/crates/layerfs-store/src/backup/create.rs), [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| `Conflict` | the bucket's copy is not the one this home last moved; the Store changed since its last backup or restore | [`backup/create.rs`](../../../../../control/crates/layerfs-store/src/backup/create.rs), [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| `InUse` | `restore` over a Store a process holds or a Sandbox uses | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| `Unsupported` | `restore` over a Store that is not on `native` | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| `Unavailable` | the Store's machine's worker is not reached | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Failed` | a bucket or step failure that ran once | [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs) |
| `Unknown` | **uncertain:** the bucket driver reports `RemoteError::Uncertain`, or a worker's answer was lost | [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs) |

After an uncertain upload, `backup list` shows the bucket's state of each
file. See [error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

| Step | Where | Source |
| --- | --- | --- |
| records, links, bucket driver and key | caller's process | [`backup/family.rs`](../../../../../control/crates/layerfs-store/src/backup/family.rs) |
| `stage`, `unstage` and piece reads of a snapshot | the Store's machine: here for `Executor::Local`, else `store.file` in its worker | [`backup/create.rs`](../../../../../control/crates/layerfs-store/src/backup/create.rs) |
| upload and download | caller's process; machines never hold bucket keys | [`backup/create.rs`](../../../../../control/crates/layerfs-store/src/backup/create.rs) |
| install of a restored Store | caller's process, on `native` | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |

The route of the snapshot steps is `place::plan` on the Store's record
([`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs)).
See the [routing table](../../../control/routing/routing-table.md).

## Example

Not run; written from the call table.

```js
await layerfs.backup.connect("archive", "r2://acct/lfs/home1", { key: "env:LAYERFS_BUCKET_ARCHIVE" });
await layerfs.backup.create("shop", "archive", { on_progress: console.log });
```

```python
layerfs.backup.connect("archive", "r2://acct/lfs/home1", key="env:LAYERFS_BUCKET_ARCHIVE")
layerfs.backup.create("shop", "archive", on_progress=print)
```

```console
$ layerfs backup connect archive r2://acct/lfs/home1 --key env:LAYERFS_BUCKET_ARCHIVE
$ layerfs backup create shop archive --progress
```

## Related

- [store family](../store/README.md).
- [Control: Families](../../../control/management/families.md) and [Routing table](../../../control/routing/routing-table.md).
- [Control: Backups](../../../control/backups/README.md), [buckets](../../../control/backups/buckets.md).
- [Backup and restore flow](../../../flows/backup.md).

---

← [store.disconnect](../store/disconnect.md) · [Up: SDK](../README.md) · [backup.connect](connect.md) →
