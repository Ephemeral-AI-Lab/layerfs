# backup.connect

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [backup](README.md) › connect

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Records an r2 or MinIO bucket under a name, after one listing proves the
bucket answers with the given key.

| | |
| --- | --- |
| Family | [backup](README.md) |
| Library entry | [`api/crates/layerfs-api/src/backup/connect.rs`](../../../../../api/crates/layerfs-api/src/backup/connect.rs) |
| Implemented in | `layerfs_control::backup::family::run` → `connect` ([`backup/family.rs`](../../../../../control/crates/layerfs-control/src/backup/family.rs)) |
| Runs in | the caller's process; it lists the bucket over the network |

## Signature

```js
// JavaScript
await layerfs.backup.connect(name, url, { key, region, part_size });
await client.backup.connect(name, url, { key, region, part_size });
```

```python
# Python
layerfs.backup.connect(name, url, key=None, region=None, part_size=None)
client.backup.connect(name, url, key=None, region=None, part_size=None)
```

```bash
# CLI
layerfs backup connect <name> <url> [--key REF] [--region R] [--part-size SIZE]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a bucket name (`BucketName::parse`), not yet connected | [`common/names.rs`](../../../../../control/crates/layerfs-control/src/common/names.rs) |
| `url` | Text | see the URL table below | [`backup/record.rs`](../../../../../control/crates/layerfs-control/src/backup/record.rs) `bucket_record` |

| URL form | Provider | Recorded fields |
| --- | --- | --- |
| `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu\|fedramp]` | `r2` | `account`, `jurisdiction`, `bucket`, `prefix`, `region: "auto"` |
| `http://<host:port>/<bucket>/<prefix>` or `https://…` | `minio` | `endpoint` (scheme and host), `bucket`, `prefix`, `region` |

The prefix may be empty; a URL with no bucket is `InvalidInput`.

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `key` | Text | `env:LAYERFS_BUCKET_<NAME>` (name upper-cased, `-` and `.` as `_`) | `env:LAYERFS_<UPPER_DIGITS_>` reads `<prefix>_ACCESS_KEY` and `<prefix>_SECRET_KEY`; `file:/absolute/path` must not be readable by group or others | [`credentials.rs`](../../../../../control/crates/layerfs-bucket/src/bucket/credentials.rs) `CredentialRef` |
| `region` | Text | `us-east-1` | MinIO only; r2 records `auto` | [`backup/record.rs`](../../../../../control/crates/layerfs-control/src/backup/record.rs) |
| `part_size` | Size | 64 MiB (`DEFAULT_PART_SIZE`) | bytes, or text with `K`, `M`, `G`, made whole bytes before dispatch | [`calls/check.rs`](../../../../../control/crates/layerfs-control/src/calls/check.rs), [`minio/provider.rs`](../../../../../control/crates/layerfs-bucket/src/bucket/minio/provider.rs) |

The key is a reference, never a secret: only the reference is written.

## Returns

```text
 r2:     { name, provider: "r2", account, jurisdiction, bucket, prefix,
           region: "auto", key, part_size }
 MinIO:  { name, provider: "minio", endpoint, bucket, prefix, region,
           key, part_size }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name`, `provider` | text | the bucket's name here, `r2` or `minio` | [`backup/record.rs`](../../../../../control/crates/layerfs-control/src/backup/record.rs) |
| `account`, `jurisdiction` | text | r2 account and jurisdiction (empty for none) | same |
| `endpoint` | text | MinIO `http(s)://host:port` | same |
| `bucket`, `prefix` | text | where objects go: `<prefix>/<file>/store.sqlite` | same |
| `key` | text | the credential reference | same |
| `part_size` | int | upload part size in bytes | same |

The answer is the record `backup/<name>/record.json` without its `v`.

## Where it runs

```text
 caller process                                  bucket
 --------------                                  ------
 BucketName::parse; parse url, key, part_size
 driver(): resolve the key now (env or file)
 list_stores ---------------------------------->  one listing
 home lock: name taken? --> Exists
 write backup/<name>/record.json
```

See the [routing table](../../../control/routing/routing-table.md). Machines never
hold bucket keys.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name or URL; a key reference that is not `env:LAYERFS_…` or an absolute `file:`; the variable is unset; a credential file readable by others; a folder-scoped client | no | [`backup/record.rs`](../../../../../control/crates/layerfs-control/src/backup/record.rs), [`credentials.rs`](../../../../../control/crates/layerfs-bucket/src/bucket/credentials.rs) |
| `Exists` | a bucket of that name is connected | no | [`backup/family.rs`](../../../../../control/crates/layerfs-control/src/backup/family.rs) |
| `Failed` | the listing failed (wrong key, no bucket, network) | no | [`backup/record.rs`](../../../../../control/crates/layerfs-control/src/backup/record.rs) `remote_error` |
| `Unknown` | the driver reports the listing's outcome uncertain | yes, but a listing changes nothing; call again | same |

## Examples

Not run; written from the call table.

```js
await layerfs.backup.connect("archive", "r2://acct/lfs/home1");
```

```python
layerfs.backup.connect("lab", "http://minio:9000/lfs", key="env:LAYERFS_MINIO", part_size="16M")
```

```bash
LAYERFS_BUCKET_ARCHIVE_ACCESS_KEY=… LAYERFS_BUCKET_ARCHIVE_SECRET_KEY=… \
  layerfs backup connect archive r2://acct/lfs/home1
```

## Related

- After: [backup.list](list.md), [backup.create](create.md),
  [backup.restore](restore.md). Undo: [backup.disconnect](disconnect.md).
- [Control: buckets](../../../control/backups/buckets.md),
  [Control: credentials](../../../control/network/credentials.md).

---

← [SDK: backup](README.md) · [Up: backup](README.md) · [backup.disconnect](disconnect.md) →
