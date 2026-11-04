# backup.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [backup](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists every Store file on the connected buckets (or one bucket), each with
whether it and the Store linked to it changed since their last transfer.

| | |
| --- | --- |
| Family | [backup](README.md) |
| Library entry | [`api/crates/layerfs-api/src/backup/list.rs`](../../../../../api/crates/layerfs-api/src/backup/list.rs) |
| Implemented in | `layerfs_control::backup::family::run` → `list` ([`backup/list.rs`](../../../../../api/crates/layerfs-control/src/backup/list.rs)) |
| Runs in | the caller's process; one listing per bucket; heads read from Store files on `native` |

## Signature

```js
// JavaScript
await layerfs.backup.list(bucket?);
await client.backup.list(bucket?);
```

```python
# Python
layerfs.backup.list(bucket=None)
client.backup.list(bucket=None)
```

```bash
# CLI
layerfs backup list [<bucket>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `bucket` | Name, optional | one connected bucket; left off, every bucket under `backup/` | [`backup/list.rs`](../../../../../api/crates/layerfs-control/src/backup/list.rs) |

## Options

None. The answer is not paged.

## Returns

```text
 { items: [
     { bucket, file, bytes, modified, heads, store | null, state, reason? },
     { bucket, file, store, state: "missing" },        <- linked, gone
     ...
 ] }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `bucket`, `file` | text | the bucket and the file (`<prefix>/<file>/store.sqlite`) | [`backup/list.rs`](../../../../../api/crates/layerfs-control/src/backup/list.rs) |
| `bytes` | int | the object's size | same |
| `modified` | int or null | last-modified time from the listing | same |
| `heads` | text or null | the heads digest the object's metadata records | same |
| `store` | text or null | the Store of this home linked to it; null for `unlinked` | same |
| `state` | text | see below | same, `state` |
| `reason` | text | why the state is `unknown` | same |

| `state` | Store changed? | Copy changed? |
| --- | --- | --- |
| `current` | no | no |
| `store_changed` | yes | no |
| `bucket_changed` | no | yes (another Store or home wrote it) |
| `both_changed` | yes | yes |
| `unknown` | cannot tell: the Store is on another machine or is PostgreSQL, the link has no heads or sha256, or the listing read no sha256 (at most 64 stats per listing, `MAX_LIST_STATS`) | |
| `unlinked` | no Store of this home links the file | |
| `missing` | the link names a file the bucket no longer has | |

"Store changed" compares the Store file's heads now with the link's heads;
"copy changed" compares the object's sha256 with the one the link's last
transfer moved. One item appears per linked Store, so a file linked by two
Stores shows twice.

## Where it runs

```text
 caller process                                  each bucket
 --------------                                  -----------
 read every Store record's links
 for each bucket: BucketRecord::of, driver
   list_stores --------------------------------->  list + up to 64 stats
   per copy: heads of the linked Store file (native only), compare
```

See the [routing table](../../routing/routing-table.md). No machine is
reached; a Store on another machine shows `unknown`.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad bucket name; a bad key reference; a folder-scoped client | no | [`backup/record.rs`](../../../../../api/crates/layerfs-control/src/backup/record.rs) |
| `NotFound` | the named bucket is not connected | no | same |
| `Failed` | a listing failed; a record cannot be read; a record names no known provider | no | same, `remote_error` |
| `Unknown` | the driver reports a listing uncertain | read-only; call again | same |

## Examples

Not run; written from the call table.

```js
const { items } = await layerfs.backup.list();
items.filter((i) => i.state !== "current").forEach((i) => console.log(i.file, i.state));
```

```python
for item in layerfs.backup.list("archive")["items"]:
    print(item["file"], item["state"], item.get("reason", ""))
```

```bash
layerfs backup list archive
```

## Related

- Before: [backup.connect](connect.md). After: [backup.create](create.md)
  for `store_changed`, [backup.restore](restore.md) for a copy to bring
  back.
- [Connect: pieces and manifest](../../../connect/backups/pieces-and-manifest.md).

---

← [backup.disconnect](disconnect.md) · [Up: backup](README.md) · [backup.create](create.md) →
