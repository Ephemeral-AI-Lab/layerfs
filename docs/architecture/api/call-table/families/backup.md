# backup family

[Architecture](../../../README.md) › [API](../../README.md) › [Call table](../README.md) › backup family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `backup` calls.

| | |
| --- | --- |
| Component | [Call table](../README.md) |
| Source | [`api/crates/layerfs-api/src/backup`](../../../../../api/crates/layerfs-api/src/backup), [`api/crates/layerfs-control/src/backup`](../../../../../api/crates/layerfs-control/src/backup) |

## Calls

Five calls, in help order
([`backup/mod.rs`](../../../../../api/crates/layerfs-api/src/backup/mod.rs)).
Each runs `layerfs_control::backup::family::run`
([`family.rs`](../../../../../api/crates/layerfs-control/src/backup/family.rs)).
A bucket is an r2 or MinIO bucket and prefix, recorded by name; each Store
keeps one link per bucket in its record.

| Call | Arguments | Options | Returns |
| --- | --- | --- | --- |
| [`connect`](../../../../../api/crates/layerfs-api/src/backup/connect.rs) | `name` (Name), `url` (Text) | `key`, `region` (Text), `part_size` (Size) | the bucket record (all fields but `v`) |
| [`disconnect`](../../../../../api/crates/layerfs-api/src/backup/disconnect.rs) | `name` | — | `{ disconnected, unlinked: [store] }`; objects stay |
| [`list`](../../../../../api/crates/layerfs-api/src/backup/list.rs) | `bucket?` (Name) | — | `{ items: [{ bucket, file, bytes, heads, store, state, reason? }] }` |
| [`create`](../../../../../api/crates/layerfs-api/src/backup/create.rs) | `store`, `bucket` (Name), `file?` (Name) | `on_progress` (Function) | `{ store, bucket, file, object, bytes, pieces, sha256, heads }` |
| [`restore`](../../../../../api/crates/layerfs-api/src/backup/restore.rs) | `store`, `bucket`, `file?` | `on_progress` | `{ store, bucket, file, bytes, sha256, heads, projects }` |

Sources: [`record.rs`](../../../../../api/crates/layerfs-control/src/backup/record.rs),
[`list.rs`](../../../../../api/crates/layerfs-control/src/backup/list.rs),
[`create.rs`](../../../../../api/crates/layerfs-control/src/backup/create.rs),
[`restore.rs`](../../../../../api/crates/layerfs-control/src/backup/restore.rs).
LayerFS never deletes a bucket object.

## Arguments and options

| Name | Kind | Rule |
| --- | --- | --- |
| `url` | Text | `http(s)://host:port/<bucket>/<prefix>` for MinIO; `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu\|fedramp]` for r2 |
| `key` | Text | a credential reference, `env:LAYERFS_…` or `file:/absolute/path`, never a secret; default `env:LAYERFS_BUCKET_<NAME>`; resolved when a driver is made ([`credentials.rs`](../../../../../connect/crates/layerfs-store-backup/src/bucket/credentials.rs)) |
| `region` | Text | MinIO region, default `us-east-1` |
| `part_size` | Size | upload part size; `K`, `M`, `G` text is turned into bytes before dispatch |
| `file` | Name | the object name; `create` defaults to the Store's name, `restore` to the Store's link |
| `on_progress` | Function | `PROGRESS`; events `{ progress: { phase } }` |

```text
 backup.create                          backup.restore
 snapshot  store.file stage on the      download  objects -> this process
           Store's machine              install   a sqlite Store on native
 send      (worker only) pieces
           -> this process
 upload    this process -> bucket
           unstage on the machine
```

`connect` lists the bucket once with the key before it records anything.
An object is `<prefix>/<file>/store.sqlite` (whole or a manifest) plus its
pieces.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name, URL or key reference | [`record.rs`](../../../../../api/crates/layerfs-control/src/backup/record.rs) |
| `NotFound` | no bucket or Store of that name | [`family.rs`](../../../../../api/crates/layerfs-control/src/backup/family.rs), [`create.rs`](../../../../../api/crates/layerfs-control/src/backup/create.rs) |
| `Exists` | a bucket name taken; an object written by another home; a Store made during a restore | [`create.rs`](../../../../../api/crates/layerfs-control/src/backup/create.rs), [`restore.rs`](../../../../../api/crates/layerfs-control/src/backup/restore.rs) |
| `Conflict` | the bucket's copy is not the one this home last moved; the Store changed since its last backup or restore | [`create.rs`](../../../../../api/crates/layerfs-control/src/backup/create.rs), [`restore.rs`](../../../../../api/crates/layerfs-control/src/backup/restore.rs) |
| `InUse` | `restore` over a Store a process holds or a Sandbox uses | [`restore.rs`](../../../../../api/crates/layerfs-control/src/backup/restore.rs) |
| `Unsupported` | `restore` over a Store that is not on `native` | [`restore.rs`](../../../../../api/crates/layerfs-control/src/backup/restore.rs) |
| `Unavailable` | the Store's machine's worker is not reached | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| `Failed` | a bucket or step failure that ran once | [`record.rs`](../../../../../api/crates/layerfs-control/src/backup/record.rs) |
| `Unknown` | **uncertain:** the bucket driver reports `RemoteError::Uncertain`, or a worker's answer was lost | [`record.rs`](../../../../../api/crates/layerfs-control/src/backup/record.rs) |

After an uncertain upload, `backup list` shows the bucket's state of each
file. See [error kinds](../errors.md#error-kinds) and
[uncertain outcomes](../errors.md#uncertain-outcomes).

## Where it runs

| Step | Where | Source |
| --- | --- | --- |
| records, links, bucket driver and key | caller's process | [`family.rs`](../../../../../api/crates/layerfs-control/src/backup/family.rs) |
| `stage`, `unstage` and piece reads of a snapshot | the Store's machine: here for `Executor::Local`, else `store.file` in its worker | [`create.rs`](../../../../../api/crates/layerfs-control/src/backup/create.rs) |
| upload and download | caller's process; machines never hold bucket keys | [`create.rs`](../../../../../api/crates/layerfs-control/src/backup/create.rs) |
| install of a restored Store | caller's process, on `native` | [`restore.rs`](../../../../../api/crates/layerfs-control/src/backup/restore.rs) |

The route of the snapshot steps is `place::plan` on the Store's record
([`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs)).
See the [routing table](../../routing/routing-table.md).

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

- [store family](store.md).
- [Control: Families](../../control/families.md) and [Routing table](../../routing/routing-table.md).
- [Connect: Backups](../../../connect/backups/README.md), [buckets](../../../connect/backups/buckets.md).
- [Backup and restore flow](../../../flows/backup.md).

---

← [Call table: store family](store.md) · [Up: Call table](../README.md) · [Call table: Errors](../errors.md) →
