# backup family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `backup` calls: connect r2 or S3-endpoint buckets, list the Store files on them, copy a sqlite Store up to a bucket or back down to `native`, and check a bucket, within the [SDK](README.md). `backup.doctor` is one of [the doctors](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/backup`](../../../../api/crates/layerfs-api/src/backup) (one file per call), [`layerfs-store/src/backup`](../../../../control/crates/layerfs-store/src/backup), [`layerfs-control/src/backup.rs`](../../../../control/crates/layerfs-control/src/backup.rs), [`layerfs-bucket`](../../../../control/crates/layerfs-bucket/src) |
| Runs by default | the caller's process (records, bucket I/O); `backup.create` stages its snapshot on the Store's machine (here for `native`, else in its worker) and the bytes go machine → this computer → bucket |
| Design | [Backups](../../control/backups/README.md), [buckets](../../control/backups/buckets.md) |

## Calls

```text
 connect <name> <url> --access-key K --key-stdin   (bucket name, url, key)
    v
 list [--bucket B]  <----------------------------------+
    |   behind | changed | missing | not_listed        |
 create <store> <bucket> [--file F]  (sqlite Store -> bucket) -- link saved
    v
 restore <bucket> <file> <store>     (bucket -> sqlite Store on native) -- link saved
    v
 doctor <bucket> [--full]            (record, key, reach, one listing; --full writes)
 disconnect <bucket>                 (forget bucket, key and links; files stay)
```

| Call | Short answer | MCP |
| --- | --- | --- |
| [`backup.connect`](#backupconnect) | `{ ok }` | `admin` |
| [`backup.disconnect`](#backupdisconnect) | `{ ok }` + `unlinked` | `admin` |
| [`backup.list`](#backuplist) | `{ items: [{ bucket, file }] }` | `admin` |
| [`backup.create`](#backupcreate) | `{ file, size }` | `admin` |
| [`backup.restore`](#backuprestore) | `{ ok }` + `projects` | `admin` |
| [`backup.doctor`](#backupdoctor) | the doctor report | `admin` |

A bucket is an r2 or S3-endpoint bucket and prefix, recorded by name in
`backup/<name>/record.json`; each Store keeps one link per bucket in its
record (its file, heads, last transfer and last attempt, as
[`store get`](store.md#storeget) shows them). An object is
`<prefix>/<file>/store.sqlite` (whole, or a manifest) plus its pieces.
LayerFS never deletes a bucket object. A folder scope refuses every
`backup` call.

### backup.connect

Connect a bucket (r2, or an S3 endpoint) for Store backups.

```text
CLI     layerfs backup connect <name> <url> --access-key K --key-stdin
Node    await layerfs.backup.connect(name, url, { access_key, secret_key })
Python  layerfs.backup.connect(name, url, *, access_key, secret_key)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | name | the bucket's name in this home |
| `url` | text | the bucket's address: `r2://...`, or an S3 endpoint `http(s)://<host:port>/<bucket>/<prefix>` |
| `access_key` option | text | the key's id; required; required |
| `secret_key` option | content | the key's secret; required; the shell reads it from stdin with --key-stdin; required; shell `--key-stdin` |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, bucket, provider, url, access_key` |
| call | writes one call-log line; MCP `admin` |

Records a bucket after listing it once with the key
([`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs)).
`url` names the provider, the bucket and the prefix:
`r2://<account>/<bucket>/<prefix>` (with `?jurisdiction=eu|fedramp`) or an
S3 endpoint `http(s)://<host:port>/<bucket>/<prefix>`; the prefix may be
empty, the bucket may not. An r2 bucket's region is `auto` and an S3
endpoint's `us-east-1`. `--access-key` is the key's id; its secret comes on
stdin with `--key-stdin` and no other way (Node and Python pass
`secret_key`). Both are required: a command line without one is exit 2, and
a Node, Python or MCP call without one is refused `InvalidInput` before
anything runs. The secret is kept in the home's secret folder
(`secret/bucket/<bucket>`, `0600`), never answered, and the call log records
its byte count only, as it does for the URL. A bucket connected before 2.0
keeps the key reference its record names. The long form is `ok, outcome,
bucket, provider, url, access_key`.

| Refusal | When |
| --- | --- |
| `InvalidInput` | a bad URL or name; an empty key or secret |
| `Exists` | the name is taken, or the same bucket and prefix is connected under another name; to change a key, disconnect and connect |
| `Unavailable` | the bucket cannot be reached or refuses the key (the one listing failed) |

```bash
printf '%s' "$SECRET" | layerfs backup connect r2-main r2://<account>/backups/shop --access-key "$KEY_ID" --key-stdin    # (not run)
```

### backup.disconnect

Forget a bucket; its files stay.

```text
CLI     layerfs backup disconnect <bucket>
Node    await layerfs.backup.disconnect(bucket)
Python  layerfs.backup.disconnect(bucket)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `bucket` | name | a connected bucket's name |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `unlinked`, `unlinked_count` |
| long | `ok, unlinked, unlinked_count, outcome, bucket` |
| `unlinked[]` short | each a `store` |
| call | writes one call-log line; MCP `admin` |

Forgets a bucket, its key and every Store's link to it. Nothing in the
bucket is deleted and the bucket is not contacted. `unlinked` lists the
Stores whose link was removed (the first 20, `unlinked_count` only past
those). A failure part way leaves some links removed and the record in
place; calling again finishes it. No outcome is uncertain
([`backup/family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs)).

### backup.list

The Store files in the connected buckets, with the Store each is linked to.

```text
CLI     layerfs backup list [--bucket B]
Node    await layerfs.backup.list({ bucket })
Python  layerfs.backup.list(*, bucket=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `bucket` option | name | list only this bucket |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `not_listed` |
| long | `items, not_listed` |
| `items[]` short | `bucket`, `file`; when set: `size`, `store`, `behind`, `changed`, `missing` |
| `items[]` long | `bucket, file, size, store, behind, changed, missing, message, modified_at, sha256, heads_sha256` |
| `not_listed[]` short | `bucket`; when set: `blocked_by`, `reason`, `message` |
| `not_listed[]` long | `bucket, blocked_by, reason, message` |
| call | writes one call-log line; MCP `admin` |

One row per Store file in each connected bucket (or the one `--bucket`
names), and one per link whose file the bucket does not hold
(`missing: true`). `store` is the Store of this home linked to the file; an
unlinked file has none. `behind: true` says the Store changed since its last
backup (its heads read where it lives: in this process on this computer,
through the machine's worker otherwise); `changed: true` says the file in
the bucket is not the one the link last moved; each is null with a `message`
when it could not be read. A bucket that cannot be listed does not fail the
call: it is named in `not_listed` with `reason` `lost` or `timeout` (the
message says which), and the others are still listed. Not paged: a row per
Store file under each bucket's prefix; `sha256` is read for at most 64 files
of one listing and is null past that
([`backup/list.rs`](../../../../control/crates/layerfs-store/src/backup/list.rs)).

```console
$ layerfs backup list
{"items":[]}
```

### backup.create

Back up a sqlite Store to a bucket.

```text
CLI     layerfs backup create <store> <bucket> [--file F] [--progress]
Node    await layerfs.backup.create(store, bucket, { file, on_progress })
Python  layerfs.backup.create(store, bucket, *, file=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` | name | a sqlite Store |
| `bucket` | name | a connected bucket's name |
| `file` option | name | the file's name in the bucket; the Store's name when left out |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `file`, `size` |
| long | `file, size, outcome, store, bucket, object, sha256, heads_sha256` |
| call | sends progress; writes one call-log line; MCP `admin` |

Uploads a consistent snapshot of a sqlite Store and links the two, refusing
to overwrite a copy this home did not last write there
([`backup/create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs)).
`--file` defaults to the Store's name: `[A-Za-z0-9._-]`, at most 128, no
leading dot ([`link.rs`](../../../../control/crates/layerfs-bucket/src/backup/link.rs)).
It is recorded as the Store's `op` (`backup`), so every client reads the
Store `busy`; it runs beside the Store's readers. Progress is `{phase, done,
total}`: `check` (the bucket holds what this home expects), `snapshot`
(staged on the Store's machine), `send` (from a machine to this computer,
bytes), `hash`, `upload` (bytes); at most one byte update per 250 ms, each
phase's exact last count before the next starts
([`backup/progress.rs`](../../../../control/crates/layerfs-store/src/backup/progress.rs)).
The stage is removed afterwards; one a killed backup left is a `home
leftovers` entry of kind `backup_staging`. A failure leaves the link as it
was and writes what was tried to its `attempt`. The long form is `file,
size, outcome, store, bucket, object, sha256, heads_sha256`.

| Refusal | When |
| --- | --- |
| `InvalidInput` | a PostgreSQL Store (back it up with PostgreSQL's tools); a bad `--file` |
| `Busy` | a move or remove runs on the Store, or another backup of it runs in this home |
| `Exists` | another home wrote the object |
| `Conflict` | the object is not the copy this home last moved, or exists when none was expected; nothing is uploaded |
| `Failed` | the snapshot, a piece read or the upload failed; the fetched copy's sha256 is not the snapshot's |
| `Unknown` | the reply to the upload was lost: the file may or may not be replaced, and nothing is resent. The Store is `broken` `unresolved` until `layerfs store get <store> --with live` settles it |

### backup.restore

Restore a Store file from a bucket as a sqlite Store on this computer.

```text
CLI     layerfs backup restore <bucket> <file> <store> [--progress]
Node    await layerfs.backup.restore(bucket, file, store, { on_progress })
Python  layerfs.backup.restore(bucket, file, store, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `bucket` | name | a connected bucket's name |
| `file` | name | a Store file in the bucket, as `backup list` names it |
| `store` | name | the sqlite Store it becomes on this computer |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `projects`, `projects_count` |
| long | `ok, projects, projects_count, outcome, store, bucket, file, size, sha256, heads_sha256` |
| `projects[]` short | each a `project` |
| call | sends progress; writes one call-log line; MCP `admin` |

Reads `file` from the bucket into the sqlite Store `store` on this computer:
a new Store (`outcome` `created`), or a replacement for an idle Store on
`native` unchanged since its last backup or restore through this bucket
(`outcome` `replaced`), held as the Store's `op` (`restore`). If
`store/<name>/` already exists, the kernel guard runs before anything is
staged and again under the home lock before the install. Progress phases:
`check`, `download` (bytes), `verify` (the bytes open as a Store),
`install`. `projects` lists the projects the restored file holds (the first
20); each project's record is rebuilt as `project init` writes it, and
records of projects it lacks are removed
([`backup/restored.rs`](../../../../control/crates/layerfs-store/src/backup/restored.rs)).
It writes only after a complete, checked download, and the install is one
rename under the home lock. A restore whose reply is lost while it replaces
a Store leaves it `broken` `unresolved` until `layerfs store get <store>
--with live` settles it; a killed restore's staging folder is removed by the
next restore
([`backup/restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs)).

| Refusal | When |
| --- | --- |
| `Unsupported` | the Store to replace is not on `native` (move it there first); `store/<name>/` exists and is not a private `0700` folder of this user |
| `InUse` | a process holds the Store, a Sandbox of it is recorded (the next field `sandboxes` names them; when no process holds it the `fix` removes them, as `store.move`'s), or its `-wal`/`-shm` exists |
| `Conflict` | the Store changed since its last transfer through this bucket, has no link, or was replaced during the download |
| `Exists` | a Store of that name was made during the download |
| `Failed` | the download or its checks failed; the backup's binding is another Store's; the restored Store's projects cannot be read |

### backup.doctor

Check a bucket; --full also tests a write.

```text
CLI     layerfs backup doctor <bucket> [--full]
Node    await layerfs.backup.doctor(bucket, { full })
Python  layerfs.backup.doctor(bucket, *, full=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `bucket` | name | a connected bucket's name |
| `full` option | bool | also write one small object, read it back and delete it |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `findings`, `not_checked` |
| long | `ok, findings, not_checked, status, counts, observed_at, tree` |
| `findings[]` short | `<thing>`, `check`, `severity`, `message`; when set: `fix`, `from` |
| `findings[]` long | `<thing>, check, severity, message, fix, from, evidence` |
| `not_checked[]` short | `<thing>`; when set: `check`, `blocked_by`, `reason` |
| `not_checked[]` long | `<thing>, check, blocked_by, reason` |
| call | writes one call-log line; MCP `admin` |

One bucket's checks, in order: `bucket.configuration` (its record),
`bucket.credentials` (its key can be read from the home),
`bucket.transport` (its address answers), `bucket.list` (one listing page
under its prefix). `--full` also writes one small object, reads it back and
deletes it: `bucket.write_capability`, `bucket.put`, `bucket.get`,
`bucket.delete`, and `bucket.reconcile` after a put whose reply was lost;
the node's coverage is then `full`. Without `--full` those checks are left
out of the report. A backup that is behind is not a bucket check: `store
doctor` reports it (`store.backups`). It sends no progress, changes nothing
but the `--full` test object, and exits 0 once its report is made. It
replaces the shell's `layerfs doctor backup`, removed with no alias.

```console
$ layerfs backup doctor nope
{"error":"NotFound","message":"no bucket named nope"}
```

## Family errors

`InvalidInput` (a bad name, URL or `file`; a folder scope), `NotFound` (no
bucket or Store of that name), `Busy` or `Unresolved` (an operation on the
Store), `Unavailable` (a bucket that does not answer or refuses the key; the
Store's machine is stopped or its worker gone,
[`place.rs`](../../../../control/crates/layerfs-store/src/place.rs)),
`Failed` (a bucket or step failure that ran once), `Unknown` (an uncertain
upload, or a lost worker answer). New backups of a Store on a removed
platform's machine are `Unsupported`
([`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs)).

## Related

- [Backups](../../control/backups/README.md), [buckets](../../control/backups/buckets.md).
- [store family](store.md), [Doctors](../../control/doctor/README.md).
- [Errors and events](../calls/errors-and-events.md).
