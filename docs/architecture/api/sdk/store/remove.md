# store.remove

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › remove

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Deletes a sqlite Store, wherever its file is, with every project in it.
It is also the call that clears an unresolved operation on the Store.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`api/crates/layerfs-api/src/store/remove.rs`](../../../../../api/crates/layerfs-api/src/store/remove.rs) |
| Implemented in | `layerfs_control::store::family::run` → `remove` ([`store/remove.rs`](../../../../../api/crates/layerfs-control/src/store/remove.rs)) |
| Runs in | the caller's process; the step `remove` in the machine's worker when the file is not under the home |

## Signature

```js
// JavaScript
await layerfs.store.remove(name);
await client.store.remove(name);
```

```python
# Python
layerfs.store.remove(name)
client.store.remove(name)
```

```bash
# CLI
layerfs store remove <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a sqlite Store; a PostgreSQL Store is refused (use [store.disconnect](disconnect.md)) | [`store/remove.rs`](../../../../../api/crates/layerfs-control/src/store/remove.rs) |

## Options

None.

## Returns

```text
 { removed: <name> }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `removed` | text | the Store whose folder under the home is gone | [`store/remove.rs`](../../../../../api/crates/layerfs-control/src/store/remove.rs) |

What goes: the Store's file, its `-wal`/`-shm`, its folder under the home
(`store/<name>/`, with the record and project entries) and, on a worker
machine, its folder under the worker's root. What stays: its backups in
their buckets.

## Where it runs

```text
 caller process                                   machine worker
 --------------                                   --------------
 record_of(name); PostgreSQL --> InvalidInput
 place::plan; Worker --> endpoint recorded?   (a stopped machine is
                                               Unavailable; none started)
 begin: home lock + Store lock
        same machine? running op --> Busy
        keep an unresolved op to put back on failure
        idle? write op "store.remove"
 settle: close own copy, idle again
 op marked uncertain, phase "remove"
   Local:  nothing yet (the file is in the folder)
   Worker: step "remove" ----------------------->  rm -r store/<name>
 home lock + Store lock: rm -r store/<name> under the home
```

| Machine of the Store | What deletes the file | Source |
| --- | --- | --- |
| a machine whose Stores open in this process (`native`) | removing `store/<name>/` under the home | [`store/remove.rs`](../../../../../api/crates/layerfs-control/src/store/remove.rs) |
| a worker machine | the worker step `remove`, then the home folder | [`transfer/steps.rs`](../../../../../api/crates/layerfs-control/src/store/transfer/steps.rs) `remove` |

The doc comment on `remove` says a stopped machine is started first; the
code does not. `Worker::ready` only reads the recorded endpoint and a
stopped machine is `Unavailable` (#535 D11,
[`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs)).
See the [routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a PostgreSQL Store; a folder-scoped client | no | [`store/remove.rs`](../../../../../api/crates/layerfs-control/src/store/remove.rs) |
| `NotFound` | no Store of that name | no | [`store/record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) |
| `Unavailable` | its machine is stopped or has no recorded worker | no | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| `Conflict` | the Store moved to another machine meanwhile; nothing removed | no | [`store/remove.rs`](../../../../../api/crates/layerfs-control/src/store/remove.rs) |
| `Busy` | another live process's operation runs on the Store | no | [`store/record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) `unblocked` |
| `InUse` | a live process holds it, this process holds it, or a Sandbox of one of its projects is recorded | no | [`inventory.rs`](../../../../../api/crates/layerfs-control/src/inventory.rs) `store_idle`, [`family.rs`](../../../../../api/crates/layerfs-control/src/store/family.rs) `settle` |
| `Failed` | a record or folder cannot be written or removed | no | [`store/remove.rs`](../../../../../api/crates/layerfs-control/src/store/remove.rs) |
| `Unknown` | **the worker's answer to `remove` was lost** | **yes** | same, `ended` |

An `Unresolved` intent of a process that has ended does not refuse a
remove: remove is its clearing call (except a push's or pull's, whose
refusal names the same push or pull,
[`store/record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) `clearing_call`).
On a certain failure the earlier unresolved intent is put back.
**After `Unknown`**, `op` stays marked uncertain at phase `remove`; read
[store.status](status.md) and run `store remove` again.

## Examples

Not run; written from the call table.

```js
await layerfs.store.remove("scratch");
```

```python
layerfs.store.remove("scratch")
```

```bash
layerfs store remove scratch
```

## Related

- Before: [store.status](status.md); close Sandboxes of its projects with
  [sandbox.close](../sandbox/close.md); keep a copy with
  [backup.create](../backup/create.md).
- [project.remove](../project/remove.md) deletes one project instead.
- [Control: Home and records](../../control/home-and-records.md),
  [Errors: uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

---

← [store.pull](pull.md) · [Up: store](README.md) · [store.disconnect](disconnect.md) →
