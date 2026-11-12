# backup.restore

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [backup](README.md) › restore

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Downloads a backup as a sqlite Store on `native`: a new Store, or a
replacement for an idle Store that has not changed since its last backup
or restore through that bucket.

| | |
| --- | --- |
| Family | [backup](README.md) |
| Library entry | [`api/crates/layerfs-api/src/backup/restore.rs`](../../../../../api/crates/layerfs-api/src/backup/restore.rs) |
| Implemented in | `layerfs_control::backup::restore` → `restore` ([`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs)), which calls `layerfs_bucket::backup::load` |
| Runs in | the caller's process, on `native`; no machine is reached |

## Signature

```js
// JavaScript
await layerfs.backup.restore(store, bucket, file?, { on_progress });
await client.backup.restore(store, bucket, file?, { on_progress });
```

```python
# Python
layerfs.backup.restore(store, bucket, file=None, on_progress=None)
client.backup.restore(store, bucket, file=None, on_progress=None)
```

```bash
# CLI
layerfs backup restore <store> <bucket> [<file>] [--progress]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `store` | Name | the Store to make or replace (`StoreName::parse`) | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| `bucket` | Name | a connected bucket | [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs) |
| `file` | Name, optional | default: the file the Store's link to this bucket names, else the Store's name | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `on_progress` | Function | none | called with `{ phase, done, total, pieces }`: `check`, `download` (with bytes, and pieces when the file is stored as pieces), `verify`, `install`; see [Progress](README.md#progress) | [`backup/progress.rs`](../../../../../control/crates/layerfs-store/src/backup/progress.rs) `Shown` |

## Returns

```text
 { store, bucket, file, bytes, sha256, heads, projects: [ <name>, ... ] }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `store`, `bucket`, `file` | text | what was restored, from where | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| `bytes`, `sha256` | int, text | the downloaded copy | `backup::load` |
| `heads` | text or null | its heads digest | same |
| `projects` | list of text | the projects the restored file holds; each record is rebuilt as `project.init` writes it (its `manifest` and `installed`), and records of projects it lacks are removed | same, `projects_in`, `write_projects` |

## Where it runs

```text
 caller process (native)                         bucket
 -----------------------
 existing Store? PostgreSQL --> InvalidInput
   locks: replaceable? (on native, no op, idle, heads == link's heads,
                        no -wal/-shm)                --> else refuse
   hold its lease
 home lock: remove folders of ended restores,
   make store/.restore-<random>/, lock it
 download into it <------------------------------ pieces / manifest
   checked; binding must match an existing Store's
 read the file's projects and each one's main Branch (own connection, closed)
 home lock (+ Store lock when replacing):
   new:      hard-link into store/<name>/store.sqlite, write record
   replace:  re-check binding and replaceable, rename over the file
 write the link; rebuild project records
 staging folder removed in every case
```

The restore holds an `flock` on its folder's `lock` file until it ends. A
restore killed mid-download leaves its folder and partial file; the next
restore in the home takes that lock, so it knows the owner is gone, and
removes the folder. A folder whose lock is held, or that has no lock file,
is left alone.

| Case | What happens | Source |
| --- | --- | --- |
| no Store of that name | a new Store record (`StoreRecord::restored`) with a new cursor key | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| an idle sqlite Store on `native`, unchanged | its file is replaced; its cursor key is kept; the backup's binding must be its own | same |
| a Store on another machine | `Unsupported`: pull it first | same, `replaceable` |

Each rebuilt record is the one `project.init` writes: the project's `main`
Branch and its base root supply `manifest` and the `installed` manifest that
`sandbox.create` reads, with the restored Store's binding, cursor key, file
length and the SQLite version its push recorded (`layerfs-sqlite`, else the
one that opens the copy here), and `created` set to the restore's time when
the record has none. A record that already holds both manifests is kept
unchanged, so a restore over an existing Store leaves its records as they
were; one without them (written by an earlier restore of the same backup)
is completed. A project whose Store holds no `main` Branch has no manifest
to rebuild, and keeps the name and stack its listing gives.

See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad names; a PostgreSQL Store; a folder-scoped client | no | [`backup/restore.rs`](../../../../../control/crates/layerfs-store/src/backup/restore.rs) |
| `NotFound` | no such bucket; the Store was removed during the download | no | same |
| `Unsupported` | the Store to replace is not on `native` | no | same, `replaceable` |
| `Busy` / `Unresolved` | an operation runs on the Store / an earlier one is unresolved | no | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) `unblocked` |
| `InUse` | a process holds the Store, a Sandbox of it is recorded, or its `-wal`/`-shm` exists | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle`, `replaceable` |
| `Conflict` | the Store changed since its last backup or restore through this bucket, has no link, or was replaced during the download | no | same |
| `Exists` | a Store of that name was made during the download | no | same |
| `Failed` | the download or its checks failed (including an uncertain read); the backup's binding is another Store's; the restored Store's projects or their history cannot be read; I/O | no | same |

A restore writes only after a complete, checked download, and the install
is one link or rename under the home lock. No outcome is reported as
`Unknown`. A failure before the install leaves the old Store (or no Store)
as it was; a failure after it (writing the link or the project records)
leaves the restored file in place without its new link, so a second
restore over it is refused `Conflict`; read [store.get](../store/get.md)
before acting again.

## Examples

Not run; written from the call table.

```js
const r = await layerfs.backup.restore("shop-copy", "archive", "shop");
console.log(r.projects);
```

```python
layerfs.backup.restore("shop", "archive", on_progress=print)
```

```bash
layerfs backup restore shop-copy archive shop --progress
```

## Related

- Before: [backup.list](list.md); [store.pull](../store/pull.md) to bring a
  Store to `native` before replacing it. After: [project.list](../project/list.md).
- [Backup and restore flow](../../../flows/backup.md),
  [Control: backup and restore](../../../control/backups/backup-and-restore.md).

---

← [backup.create](create.md) · [Up: backup](README.md) · [API: Front ends](../../front-ends/README.md) →
