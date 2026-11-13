# store.detach

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › detach

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Stops a machine running Sandboxes of a PostgreSQL Store: its attachment is
forgotten. The database, its data and the Store's other machines are not
touched. Detaching `native` is allowed: Store calls still run in the
caller's process with the Store's connection; only Sandboxes on `native`
are refused afterwards.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`store/detach.rs`](../../../../../api/crates/layerfs-api/src/store/detach.rs) |
| Implemented in | `layerfs_control::store::detach` ([`store.rs`](../../../../../control/crates/layerfs-control/src/store.rs)) over `layerfs_store::postgresql::attach::detach` |
| Runs in | the caller's process; dials nothing |

## Signature

```js
await layerfs.store.detach(name, machine);
```

```python
layerfs.store.detach(name, machine)
```

```bash
layerfs store detach <name> <machine>
```

## Returns

The Store as [store.get](get.md) shows it, without the machine in
`machines`.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `NotFound` | no Store of that name | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) |
| `InvalidInput` | a sqlite Store; the Store is not attached to the machine | [`postgresql/attach.rs`](../../../../../control/crates/layerfs-store/src/postgresql/attach.rs) `detach` |
| `InUse` | a Sandbox of the Store is recorded on the machine; `detail.related` names each | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle_on` |

LayerFS never drops a role: revoking a machine's own role is the
operator's step on the server.

---

← [store.attach](attach.md) · [Up: store](README.md) · [store.push](push.md) →
