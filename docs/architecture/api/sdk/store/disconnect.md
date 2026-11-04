# store.disconnect

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › disconnect

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Forgets a PostgreSQL Store in this home. The database and its `layerfs`
schema keep every byte; [store.connect](connect.md) can record it again.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`api/crates/layerfs-api/src/store/disconnect.rs`](../../../../../api/crates/layerfs-api/src/store/disconnect.rs) |
| Implemented in | `layerfs_control::store::connect::run` → `disconnect` ([`store/connect.rs`](../../../../../api/crates/layerfs-control/src/store/connect.rs)) |
| Runs in | the caller's process; no server is contacted |

## Signature

```js
// JavaScript
await layerfs.store.disconnect(name);
await client.store.disconnect(name);
```

```python
# Python
layerfs.store.disconnect(name)
client.store.disconnect(name)
```

```bash
# CLI
layerfs store disconnect <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a PostgreSQL Store (`StoreName::parse`); a sqlite Store is refused (use [store.remove](remove.md)) | [`store/connect.rs`](../../../../../api/crates/layerfs-control/src/store/connect.rs) |

## Options

None.

## Returns

```text
 { disconnected: <name> }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `disconnected` | text | the Store whose record folder is gone | [`store/connect.rs`](../../../../../api/crates/layerfs-control/src/store/connect.rs) `disconnect` |

## Where it runs

```text
 caller process
 --------------
 home lock + Store lock
   read record --> missing: NotFound; sqlite: InvalidInput
   store_idle: no other lease, no recorded Sandbox  --> else InUse
   close this process's open connection (postgresql_key)
   rm -r <home>/store/<name>/   (record, project entries)
 PostgreSQL server: untouched
```

| Store kind | Result | Source |
| --- | --- | --- |
| PostgreSQL | the record goes; the schema stays | [`store/connect.rs`](../../../../../api/crates/layerfs-control/src/store/connect.rs) |
| sqlite | `InvalidInput`; `store remove` deletes it | same |

A Sandbox's daemon dials the server from its machine and holds no lease
in this home, so recorded Sandboxes refuse the call too. See the
[routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a sqlite Store; a folder-scoped client | no | [`store/connect.rs`](../../../../../api/crates/layerfs-control/src/store/connect.rs) |
| `NotFound` | no Store of that name | no | same |
| `InUse` | another live process holds its lease, a Sandbox of one of its projects is recorded, or a call in this process is running on it | no | [`inventory.rs`](../../../../../api/crates/layerfs-control/src/inventory.rs) `store_idle`, [`store/open.rs`](../../../../../api/crates/layerfs-control/src/store/open.rs) |
| `Failed` | the folder cannot be removed | no | [`store/connect.rs`](../../../../../api/crates/layerfs-control/src/store/connect.rs) |

No outcome of this call is uncertain: it writes only local records.

## Examples

Not run; written from the call table.

```js
await layerfs.store.disconnect("pg");
```

```python
layerfs.store.disconnect("pg")
```

```bash
layerfs store disconnect pg
```

## Related

- Before: [store.status](status.md), [sandbox.close](../sandbox/close.md)
  for its Sandboxes. Again later: [store.connect](connect.md).
- [Engine: PostgreSQL](../../../engine/store/postgresql.md),
  [Control: Home and records](../../control/home-and-records.md).

---

← [store.remove](remove.md) · [Up: store](README.md) · [SDK: backup](../backup/README.md) →
