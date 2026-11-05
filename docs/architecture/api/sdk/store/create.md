# store.create

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › create

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Makes a new, empty sqlite Store on `native`, ready for `project.init`.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`api/crates/layerfs-api/src/store/create.rs`](../../../../../api/crates/layerfs-api/src/store/create.rs) |
| Implemented in | `layerfs_control::store::family::run` → `create` ([`store/create.rs`](../../../../../api/crates/layerfs-control/src/store/create.rs)) |
| Runs in | the caller's process, always; the file goes to `store/<name>/store.sqlite` under the home |

## Signature

```js
// JavaScript
await layerfs.store.create(name);
await client.store.create(name);
```

```python
# Python
layerfs.store.create(name)
client.store.create(name)
```

```bash
# CLI
layerfs store create <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a new Store name (`StoreName::parse`); no Store record of that name may exist, of either kind | [`create.rs`](../../../../../api/crates/layerfs-control/src/store/create.rs), [`common/names.rs`](../../../../../api/crates/layerfs-control/src/common/names.rs) |

## Options

None. The Store kind is always sqlite and the profile always Disposable
(WAL, `synchronous=OFF`), the only permitted sqlite profile. A PostgreSQL
Store is attached with [store.connect](connect.md) instead.

## Returns

```text
 {
   name,
   kind: "sqlite",
   machine: "native",
   profile: "disposable",
   created: <ms since epoch>,
   sqlite: <SQLite version that made the file>,
   projects: []
 }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name`, `kind` | text | the new Store | [`identity.rs`](../../../../../api/crates/layerfs-control/src/store/identity.rs) `save_record` |
| `machine` | text | always `native` | [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) `created_on_native` |
| `profile` | text | always `disposable` | same |
| `created` | int | milliseconds since the Unix epoch | same |
| `sqlite` | text | the SQLite version of the sealed empty file | [`identity.rs`](../../../../../api/crates/layerfs-control/src/store/identity.rs) `make_empty` |
| `projects` | list | empty | [`family.rs`](../../../../../api/crates/layerfs-control/src/store/family.rs) `public` |

The record also holds a random binding and cursor key, which are never
shown. The answer is the same shape as [store.get](get.md).

## Where it runs

```text
 caller process (holds the home lock throughout)
 ------------------------------------------------
 StoreName::parse(name)
 home lock  ----------------------------------- waits up to 120 s
 store/<name>/record.json exists? --> Exists
 mkdir store/<name>/ (0700)
 identity: kind sqlite, random binding, random cursor key
 make_empty:  ProjectApi::create_store  -> store.sqlite.creating
              (Disposable profile, frozen default policy)
              chmod 0600; rename -> store/<name>/store.sqlite
   failure:   remove store/<name>/ (nothing was published)
 save_record + created_on_native; write record.json
```

| Store kind | Where | Source |
| --- | --- | --- |
| sqlite | this process, under the home, on `native` | [`create.rs`](../../../../../api/crates/layerfs-control/src/store/create.rs) |
| PostgreSQL | not made by this call: use [store.connect](connect.md) | [`create.rs`](../../../../../api/crates/layerfs-api/src/store/create.rs) |

No worker or machine is involved; to place the Store on another machine,
create it and then [push](push.md) it. See the
[routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad Store name; a folder-scoped client | no | [`common/names.rs`](../../../../../api/crates/layerfs-control/src/common/names.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| `Exists` | a Store record of that name exists, or a file is already at the target | no | [`create.rs`](../../../../../api/crates/layerfs-control/src/store/create.rs), [`identity.rs`](../../../../../api/crates/layerfs-control/src/store/identity.rs) `make_empty` |
| `Unavailable` | another process held the home lock or the Store's lock for 120 s | no | [`common/home/lock.rs`](../../../../../api/crates/layerfs-control/src/common/home/lock.rs) |
| `Unsupported` | the kernel guard refuses `store/<name>/` (not on a local lock-carrying filesystem, or an existing directory is not owned by this user or has group/other access instead of mode `0700`) | no | [`identity.rs`](../../../../../api/crates/layerfs-control/src/store/identity.rs), [`kernel_guard.rs`](../../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| `Failed` | the empty Store could not be made or written (`Store create: …`), or the record write failed | no | [`identity.rs`](../../../../../api/crates/layerfs-control/src/store/identity.rs) |

A failure before the record is written removes the call's own
`store/<name>/` folder.

## Examples

Not run; written from the call table.

```js
await layerfs.store.create("shop");
await layerfs.project.init("shop/api", "/src/api");
```

```python
layerfs.store.create("shop")
layerfs.project.init("shop/api", "/src/api")
```

```bash
layerfs store create shop
```

## Related

- After: [project.init](../project/init.md), then optionally
  [store.push](push.md) to another machine or
  [backup.create](../backup/create.md).
- [Engine: Store lifecycle](../../../engine/store/lifecycle.md),
  [Engine: SQLite](../../../engine/store/sqlite.md),
  [Control: Home and records](../../control/home-and-records.md).

---

← [store.status](status.md) · [Up: store](README.md) · [store.connect](connect.md) →
