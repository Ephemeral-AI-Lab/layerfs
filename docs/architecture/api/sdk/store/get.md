# store.get

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › get

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Returns one Store's record and the names of its projects, read from the
home alone.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`store/get.rs`](../../../../../api/crates/layerfs-api/src/store/get.rs) |
| Implemented in | `layerfs_store::sqlite::family::get` → `public(record_of(..), View::Full)` ([`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs)) |
| Runs in | the caller's process; nothing else is contacted |

## Signature

```js
// JavaScript
await layerfs.store.get(name);
await client.store.get(name);
```

```python
# Python
layerfs.store.get(name)
client.store.get(name)
```

```bash
# CLI
layerfs store get <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a Store name: 1–63 lowercase letters, digits, `_`, `.`, `-`, starting and ending with a letter or digit, not a Windows device name | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) `StoreName`, [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) `StoreRecord::of` |

## Options

None.

## Returns

```text
 sqlite Store                          PostgreSQL Store
 {                                     {
   name,                                 name,
   kind: "sqlite",                       kind: "postgresql",
   machine: "native" | <machine>,        url,            (no password)
   profile: "disposable",                schema: "layerfs",
   created, sqlite,                      synchronous_commit,
   last_transfer?: { call, from, to,     created,
     at, bytes, sha256, heads,           password?,      (env:/file: ref)
     retired },                          op?,
   incoming_on?: <machine>,              projects: [name]
   backups?: { <bucket>: { file,         }
     heads, last: { op, at, bytes,
     sha256 } } },
   op?: { id, name, phase, started,
          uncertain },
   projects: [name]
 }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name` | text | the Store's name, always first | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) `to_sdk` |
| `kind` | text | `sqlite` or `postgresql`; a record without `kind` is sqlite | `kind_of` |
| `machine` | text | sqlite: the machine that holds the file | `created_on_native`, [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `profile` | text | `disposable`, the only sqlite profile LayerFS makes | `created_on_native`, `restored` |
| `created` | int | milliseconds since the Unix epoch | same |
| `sqlite` | text | the SQLite version that made the file (`store.create` only; a restored Store has none) | [`sqlite/create.rs`](../../../../../control/crates/layerfs-store/src/sqlite/create.rs) |
| `last_transfer` | map | the last push or pull; `retired: false` means the source copy is not yet deleted | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `incoming_on` | text | a machine a `store.push` that failed for certain at `send`, `verify` or `install` left a partial copy on, kept while the intent that named it is gone; the doctor reports that copy, and the next `store push` or `store remove` deletes it (until the record names the target the Store is on `native`, where `store pull` is refused) | [`sqlite/transfer/resolve.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs) `note_incoming`, `partial_on` |
| `backups` | map | one link per bucket: the file, its heads and the last `create` or `restore` | [`backup/record.rs`](../../../../../control/crates/layerfs-store/src/backup/record.rs) `StoreLink` |
| `url`, `schema`, `synchronous_commit`, `password` | text | PostgreSQL: the URL without a password, the fixed schema, the setting, and the password reference | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) `connected` |
| `op` | map | the recorded intent, when there is one | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) `to_sdk` |
| `projects` | list of text | the home's project records for this Store, sorted | [`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs) `public` |

`cursor`, `binding` and `v` are never shown. `get` shows no `state`; use
[store.status](status.md).

## Where it runs

```text
 caller process
 --------------
 StoreName::parse(name)
 read store/<name>/record.json      (no lock, no lease)
 list store/<name>/project/          --> projects
 to_sdk(View::Full)
```

It reads the home only, for both Store kinds: no machine, worker or server
is contacted. See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad Store name; the client has a folder scope | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NotFound` | no Store record of that name | no | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) `StoreRecord::of` |
| `Failed` | the record cannot be read, is not JSON, or is not a map | no | [`home/records.rs`](../../../../../control/crates/layerfs-home/src/home/records.rs) |

## Examples

Not run; written from the call table.

```js
const shop = await layerfs.store.get("shop");
console.log(shop.kind, shop.machine, shop.projects);
```

```python
shop = layerfs.store.get("shop")
print(shop["kind"], shop.get("machine"), shop["projects"])
```

```bash
layerfs store get shop
```

## Related

- Before: [store.list](list.md), [store.create](create.md) or
  [store.connect](connect.md). After: [store.status](status.md).
- [Control: Home and records](../../../control/management/home-and-records.md),
  [backup family](../backup/README.md) (the `backups` links).

---

← [store.list](list.md) · [Up: store](README.md) · [store.status](status.md) →
