# backup.create

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [backup](README.md) › create

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Uploads a consistent snapshot of a sqlite Store to a bucket, refusing to
overwrite a copy this home did not last write there.

| | |
| --- | --- |
| Family | [backup](README.md) |
| Library entry | [`api/crates/layerfs-api/src/backup/create.rs`](../../../../../api/crates/layerfs-api/src/backup/create.rs) |
| Implemented in | `layerfs_control::backup::family::run` → `create` ([`backup/create.rs`](../../../../../control/crates/layerfs-control/src/backup/create.rs)) |
| Runs in | the snapshot on the Store's machine (here for `native`, else `store.file` in its worker); the upload in the caller's process |

## Signature

```js
// JavaScript
await layerfs.backup.create(store, bucket, file?, { on_progress });
await client.backup.create(store, bucket, file?, { on_progress });
```

```python
# Python
layerfs.backup.create(store, bucket, file=None, on_progress=None)
client.backup.create(store, bucket, file=None, on_progress=None)
```

```bash
# CLI
layerfs backup create <store> <bucket> [<file>] [--progress]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `store` | Name | a sqlite Store; PostgreSQL is refused | [`backup/create.rs`](../../../../../control/crates/layerfs-control/src/backup/create.rs) |
| `bucket` | Name | a connected bucket | [`backup/record.rs`](../../../../../control/crates/layerfs-control/src/backup/record.rs) |
| `file` | Name, optional | default the Store's name; `[A-Za-z0-9._-]`, at most 128, no leading dot; the object is `<file>/store.sqlite` under the prefix | [`link.rs`](../../../../../control/crates/layerfs-bucket/src/backup/link.rs) `store_object` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `on_progress` | Function | none | called with `{ phase, done, total, pieces }`: `check`, `snapshot`, `send` (worker machines only, with bytes), `check`, `hash` and `upload` (with bytes; `upload` with pieces when the file goes up as pieces); see [Progress](README.md#progress) | [`backup/progress.rs`](../../../../../control/crates/layerfs-control/src/backup/progress.rs) `Shown` |

## Returns

```text
 { store, bucket, file, object, bytes, pieces, sha256, heads }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `store`, `bucket`, `file` | text | what was backed up, and where | [`backup/create.rs`](../../../../../control/crates/layerfs-control/src/backup/create.rs) |
| `object` | text | `<file>/store.sqlite` | same |
| `bytes`, `pieces` | int | uploaded size and piece count | `pieces::upload` |
| `sha256` | text | hex sha256 of the snapshot | same |
| `heads` | text or null | hex heads digest of the snapshot | same |

The Store's record keeps a link `backups.<bucket>` =
`{ file, heads, last: { op: "create", at, bytes, sha256 } }`.

## Where it runs

```text
 caller process                  Store's machine            bucket
 --------------                  ---------------            ------
 records; unblocked; driver
 lease on the Store
 check: stat object --------------------------------------> metadata
   written by another home? Exists; not the expected copy? Conflict
 snapshot: stage ------------->  <store>.backup (a
                                 consistent snapshot)
 send (worker only): read 8 MiB
   pieces <--------------------  staged copy; sha256 checked here
 check again, then upload --------------------------------> pieces +
   (sqlite version, binding, heads, exported-from)          manifest
 unstage --------------------->  delete <store>.backup
 save the link in the Store record
```

| Expected in the bucket | When | Source |
| --- | --- | --- |
| the copy with the link's `last.sha256` | the Store links this bucket with this `file` and a sha256 | [`backup/create.rs`](../../../../../control/crates/layerfs-control/src/backup/create.rs) `expected` |
| nothing | no link, another file, or a link with no sha256 | same |

The object's metadata `layerfs-exported-from` is `<home id>/<machine>`.
The snapshot runs while the Store is live; no Store `op` is written, so a
backup does not refuse other calls. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad names or `file`; a PostgreSQL Store ("back it up with PostgreSQL's tools"); a folder-scoped client | no | [`backup/create.rs`](../../../../../control/crates/layerfs-control/src/backup/create.rs) |
| `NotFound` | no such Store or bucket | no | same |
| `Busy` / `Unresolved` | a push, pull or remove runs on the Store / an earlier one's outcome is unknown | no | [`store/record.rs`](../../../../../control/crates/layerfs-control/src/store/record.rs) `unblocked` |
| `Busy` | another `backup.create` of the Store runs in this home (both would stage at one path); nothing was changed | no | [`backup/create.rs`](../../../../../control/crates/layerfs-control/src/backup/create.rs) `Creating` |
| `Exists` | another home wrote the object | no | same, `check` |
| `Conflict` | the object is not the copy this home last moved (or exists when none was expected) | no | same |
| `Unavailable` | the Store's machine is stopped or its worker gone | no | [`common/place.rs`](../../../../../control/crates/layerfs-control/src/common/place.rs) |
| `Failed` | snapshot, piece read or upload failed; the fetched copy's sha256 is not the snapshot's | no | [`backup/create.rs`](../../../../../control/crates/layerfs-control/src/backup/create.rs) |
| `Unknown` | **the driver reports the upload uncertain, or a worker's answer was lost** | **yes** | [`backup/record.rs`](../../../../../control/crates/layerfs-control/src/backup/record.rs) `remote_error` |

**After `Unknown`** the link is not updated, so the next create may refuse
`Conflict` if the upload did land. Read [backup.list](list.md): a
`bucket_changed` state for this Store says the object is not the one the
link records. LayerFS never deletes a bucket object; back up to another
`file`, or clear the object with the bucket's own tools.

## Examples

Not run; written from the call table.

```js
const b = await layerfs.backup.create("shop", "archive", { on_progress: (p) => console.log(p.phase) });
```

```python
layerfs.backup.create("shop", "archive", "shop-weekly", on_progress=print)
```

```bash
layerfs backup create shop archive --progress
```

## Related

- Before: [backup.connect](connect.md), [backup.list](list.md).
  After: [backup.restore](restore.md).
- [Backup and restore flow](../../../flows/backup.md),
  [Control: backup and restore](../../../control/backups/backup-and-restore.md),
  [Control: pieces and manifest](../../../control/backups/pieces-and-manifest.md).

---

← [backup.list](list.md) · [Up: backup](README.md) · [backup.restore](restore.md) →
