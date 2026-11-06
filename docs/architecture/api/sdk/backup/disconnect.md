# backup.disconnect

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [backup](README.md) › disconnect

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Forgets a bucket and every Store's link to it in this home. Nothing in the
bucket is deleted.

| | |
| --- | --- |
| Family | [backup](README.md) |
| Library entry | [`api/crates/layerfs-api/src/backup/disconnect.rs`](../../../../../api/crates/layerfs-api/src/backup/disconnect.rs) |
| Implemented in | `layerfs_control::backup::family::run` → `disconnect` ([`backup/family.rs`](../../../../../control/crates/layerfs-store/src/backup/family.rs)) |
| Runs in | the caller's process; the bucket is not contacted |

## Signature

```js
// JavaScript
await layerfs.backup.disconnect(name);
await client.backup.disconnect(name);
```

```python
# Python
layerfs.backup.disconnect(name)
client.backup.disconnect(name)
```

```bash
# CLI
layerfs backup disconnect <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a connected bucket (`BucketName::parse`) | [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs) `BucketRecord::of` |

## Options

None.

## Returns

```text
 { disconnected: <name>, unlinked: [ <store>, ... ] }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `disconnected` | text | the bucket whose record folder is gone | [`backup/family.rs`](../../../../../control/crates/layerfs-store/src/backup/family.rs) `disconnect` |
| `unlinked` | list of text | the Stores whose `backups.<name>` link was removed | same |

## Where it runs

```text
 caller process
 --------------
 BucketRecord::of(name)            --> NotFound / InvalidInput
 home lock
   for each Store under store/:
     Store lock; record.without_link(name)? --> write, add to unlinked
   rm -r backup/<name>/
 bucket: untouched
```

The record does not need a known provider to be disconnected. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a folder-scoped client | no | [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs) |
| `NotFound` | no bucket of that name | no | same |
| `Failed` | a record cannot be read, written or removed | no | [`backup/family.rs`](../../../../../control/crates/layerfs-store/src/backup/family.rs) |

A failure part way leaves some links removed and the bucket record in
place; calling again finishes it. No outcome is uncertain: only local
records change.

## Examples

Not run; written from the call table.

```js
const { unlinked } = await layerfs.backup.disconnect("archive");
```

```python
layerfs.backup.disconnect("archive")
```

```bash
layerfs backup disconnect archive
```

## Related

- Before: [backup.list](list.md) to see what the bucket holds.
  Again later: [backup.connect](connect.md); a link is made anew by the
  next [backup.create](create.md) or [backup.restore](restore.md).
- [Control: backups](../../../control/backups/README.md).

---

← [backup.connect](connect.md) · [Up: backup](README.md) · [backup.list](list.md) →
