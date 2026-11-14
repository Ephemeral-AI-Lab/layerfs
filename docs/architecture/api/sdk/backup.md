# backup family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `backup` calls: connect r2 or MinIO buckets, list the Store files on them, and copy a sqlite Store up to a bucket or back down to `native`, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/backup`](../../../../api/crates/layerfs-api/src/backup), [`layerfs-store/src/backup`](../../../../control/crates/layerfs-store/src/backup), [`layerfs-control/src/backup.rs`](../../../../control/crates/layerfs-control/src/backup.rs), [`layerfs-bucket`](../../../../control/crates/layerfs-bucket/src) |
| Runs by default | the caller's process (records, bucket I/O); `backup.create` stages its snapshot on the Store's machine (here for `native`, else `store.file` in its worker) |
| Design | [Backups](../../control/backups/README.md), [buckets](../../control/backups/buckets.md) |

## Calls

```text
 connect (bucket name, url, key ref)
    v
 list  <---------------------------------------------+
    |   current | store_changed | bucket_changed ...  |
 create (sqlite Store -> bucket) --- link saved ------+
    v
 restore (bucket -> sqlite Store on native) --- link saved
    v
 disconnect (forget bucket + links; objects stay)
```

| Call | Returns |
| --- | --- |
| [`backup.connect`](#backupconnect) | the bucket record (all fields but `v`) |
| [`backup.disconnect`](#backupdisconnect) | `{ disconnected, unlinked: [store] }` |
| [`backup.list`](#backuplist) | `{ items: [{ bucket, file, bytes, modified, heads, store, state, reason? }] }` |
| [`backup.create`](#backupcreate) | `{ store, bucket, file, object, bytes, pieces, sha256, heads }` |
| [`backup.restore`](#backuprestore) | `{ store, bucket, file, bytes, sha256, heads, projects }` |

A bucket is an r2 or MinIO bucket and prefix, recorded by name in
`backup/<name>/record.json`; each Store keeps one link per bucket in its
record, `backups.<bucket> = { file, heads, last: { op, at, bytes, sha256 } }`.
An object is `<prefix>/<file>/store.sqlite` (whole, or a manifest) plus its
pieces. LayerFS never deletes a bucket object. A folder scope refuses every
`backup` call.

### backup.connect

Records a bucket, after listing it once with the key.

```text
JS      await layerfs.backup.connect(name, url, { key, region, part_size })
Python  layerfs.backup.connect(name, url, *, key=None, region=None, part_size=None)
CLI     layerfs backup connect <name> <url> [--key <key>] [--region <region>] [--part-size <part-size>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | Name | a bucket name not yet connected |
| `url` | Text | `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu\|fedramp]` (r2), or `http(s)://<host:port>/<bucket>/<prefix>` (MinIO); the prefix may be empty, the bucket may not |
| `key` option | Text | a credential reference, never a secret: `env:LAYERFS_<UPPER_DIGITS_>` (reads `<prefix>_ACCESS_KEY` and `<prefix>_SECRET_KEY`) or `file:/absolute/path` not readable by group or others; default `env:LAYERFS_BUCKET_<NAME>` ([`credentials.rs`](../../../../control/crates/layerfs-bucket/src/bucket/credentials.rs)) |
| `region` option | Text | MinIO only, default `us-east-1`; r2 records `auto` |
| `part_size` option | Size | upload part size, default 64 MiB (`DEFAULT_PART_SIZE`) |

```text
r2:    { name, provider: "r2", account, jurisdiction, bucket, prefix,
         region: "auto", key, part_size }
MinIO: { name, provider: "minio", endpoint, bucket, prefix, region,
         key, part_size }
```

Errors: `InvalidInput` (a bad URL or key reference, an unset variable, a
credential file readable by others); `Exists`; `Failed` (the listing failed:
wrong key, no bucket, network); `Unknown` (the listing's outcome is uncertain;
a listing changes nothing)
([`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs)).

### backup.disconnect

Forgets a bucket and every Store's link to it. Nothing in the bucket is
deleted and the bucket is not contacted.

```text
JS      await layerfs.backup.disconnect(name)
Python  layerfs.backup.disconnect(name)
CLI     layerfs backup disconnect <name>
```

Returns `{ disconnected, unlinked }`, the Stores whose link was removed. A
failure part way leaves some links removed and the record in place; calling
again finishes it. No outcome is uncertain
([`backup/family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs)).

### backup.list

Every Store file on the connected buckets, or one bucket, with whether it
and its linked Store changed since their last transfer.

```text
JS      await layerfs.backup.list(bucket?)
Python  layerfs.backup.list(bucket=None)
CLI     layerfs backup list [<bucket>]
```

| `state` | Meaning |
| --- | --- |
| `current` | neither changed |
| `store_changed` | the Store's heads now differ from the link's |
| `bucket_changed` | the object's sha256 is not the one the link's last transfer moved |
| `both_changed` | both |
| `unknown` | cannot tell (`reason`): the Store is on another machine or is PostgreSQL, the link has no heads or sha256, or the listing read no sha256 (at most 64 stats per listing, `MAX_LIST_STATS`) |
| `unlinked` | no Store of this home links the file (`store: null`) |
| `missing` | the link names a file the bucket no longer has |

One item per linked Store, so a file linked by two Stores shows twice; not
paged ([`backup/list.rs`](../../../../control/crates/layerfs-store/src/backup/list.rs)).

### backup.create

Uploads a consistent snapshot of a sqlite Store, refusing to overwrite a copy
this home did not last write there.

```text
JS      await layerfs.backup.create(store, bucket, file?, { on_progress })
Python  layerfs.backup.create(store, bucket, file=None, *, on_progress=None)
CLI     layerfs backup create <store> <bucket> [<file>] [--progress]
```

`file` defaults to the Store's name: `[A-Za-z0-9._-]`, at most 128, no
leading dot ([`link.rs`](../../../../control/crates/layerfs-bucket/src/backup/link.rs)).
Phases: `check` (the bucket holds what this home expects), `snapshot`
(staged on the Store's machine), `send` (worker machines only, bytes to this
process), `check` again, `hash`, `upload` (bytes and pieces); the stage is
removed afterwards ([`backup/create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs)).

| Error | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store ("back it up with PostgreSQL's tools") |
| `Busy` | a push, pull or remove runs on the Store; or another `backup.create` of it runs in this home (both would stage at one path) |
| `Exists` | another home wrote the object |
| `Conflict` | the object is not the copy this home last moved, or exists when none was expected |
| `Failed` | snapshot, piece read or upload failed; the fetched copy's sha256 is not the snapshot's |
| `Unknown` | the driver reports the upload uncertain, or a worker's answer was lost. The link is not updated, so a later create may be `Conflict` if it landed; `backup.list` shows `bucket_changed` then |

### backup.restore

Downloads a backup as a sqlite Store on `native`: a new Store, or a
replacement for an idle Store unchanged since its last backup or restore
through that bucket.

```text
JS      await layerfs.backup.restore(store, bucket, file?, { on_progress })
Python  layerfs.backup.restore(store, bucket, file=None, *, on_progress=None)
CLI     layerfs backup restore <store> <bucket> [<file>] [--progress]
```

`file` defaults to the file the Store's link names, else the Store's name.
Phases: `check`, `download` (bytes, pieces), `verify` (the bytes open as the
Store), `install`. `projects` lists the projects the restored file holds;
each project's record is rebuilt as `project.init` writes it (carrying its
`manifest` and `installed` manifest when the project has a `main` Branch;
existing complete records are kept and incomplete ones completed), and
records of projects it lacks are removed
([`backup/restored.rs`](../../../../control/crates/layerfs-store/src/backup/restored.rs)).
Records are planned and their manifests encoded under the home lock before
the install, so a manifest that does not encode refuses the restore with
nothing installed; planning and writing project records hold only the home
lock, while replacing an existing Store and writing the link each take the
Store lock separately. It writes only after a complete, checked download,
and the install is one link or rename under the home lock; no outcome is
reported `Unknown`. A failure after the install writing the link leaves the
file without its new link, so a second restore is `Conflict`; a failure
writing the project records leaves the file and its link in place, so a
second restore of the same backup completes them: read `store.get` first
([`backup.rs`](../../../../control/crates/layerfs-control/src/backup.rs),
[`backup/restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs)).

| Error | When |
| --- | --- |
| `Unsupported` | the Store to replace is not on `native` |
| `InUse` | a process holds the Store, a Sandbox of it is recorded, or its `-wal`/`-shm` exists |
| `Conflict` | the Store changed since its last transfer through this bucket, has no link, or was replaced during the download |
| `Exists` | a Store of that name was made during the download |
| `Failed` | the download or its checks failed (an uncertain read included); the backup's binding is another Store's; the restored Store's projects or their history cannot be read; a rebuilt manifest does not encode |

## Progress

`create` and `restore` send `{ phase, done, total, pieces }`
([`backup/progress.rs`](../../../../control/crates/layerfs-store/src/backup/progress.rs)):
`done` and `total` are bytes while a phase moves bytes (`total` null when
unknown), null otherwise; `pieces` is `{ done, of }` while a file moves as
pieces. The transfer runs on its own thread while the calling thread sends
the bucket library's counts: at most one byte update per 250 ms, each phase's
exact last count before the next starts, nothing repeated. Counts are bytes
on the connection (the S3 part hash is not counted). A phase at its total
says only that its bytes moved; the call has succeeded only when it returns.

## Family errors

`InvalidInput` (a bad name, URL, key reference or `file`; a folder scope),
`NotFound` (no bucket or Store of that name), `Busy` or `Unresolved` (an
operation on the Store), `Unavailable` (the Store's machine is stopped or its
worker gone, [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs)),
`Failed` (a bucket or step failure that ran once), `Unknown`
(`RemoteError::Uncertain` from the driver, or a lost worker answer). New
backups of a Store on a removed platform's machine are `Unsupported`
([`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs)).

## Related

- [Backups](../../control/backups/README.md), [buckets](../../control/backups/buckets.md).
- [store family](store.md).
- [Errors and events](../calls/errors-and-events.md).
