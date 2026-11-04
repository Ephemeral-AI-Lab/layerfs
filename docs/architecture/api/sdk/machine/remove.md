# machine.remove

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › remove

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Deletes a VM machine with its data disk, once no Store, Sandbox or hosted
machine is on it. It also clears an unresolved create or remove.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/remove.rs`](../../../../../api/crates/layerfs-api/src/machine/remove.rs) |
| Implemented in | `layerfs_control::machine::remove` → `take_down` ([`machine/remove.rs`](../../../../../api/crates/layerfs-control/src/machine/remove.rs)), then `layerfs_machine::remove` |
| Runs in | the caller's process; the provider's CLI deletes the VM |

## Signature

```js
// JavaScript
await layerfs.machine.remove(name);
await client.machine.remove(name);
```

```python
# Python
layerfs.machine.remove(name)
```

```bash
# CLI
layerfs machine remove <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a created VM machine; built-ins are never removed; an `ssh` machine is disconnected instead | [`machine/remove.rs`](../../../../../api/crates/layerfs-control/src/machine/remove.rs) `removable` |

## Options

None.

## Returns

```text
 { name, removed: true }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name` | text | the machine | [`machine/remove.rs`](../../../../../api/crates/layerfs-control/src/machine/remove.rs) |
| `removed` | bool | always true on success; `machine/<name>/` is gone | same |

## Where it runs

```text
 caller process                                   provider CLI
 --------------                                   ------------
 record; a live op on it --> Busy
 start lock (no start meanwhile)
 home lock: removable? (not built in, not ssh,
            no Stores / Sandboxes / hosted machines --> InUse)
            write op "remove", phase delete
 layerfs_machine::remove ------------------------>  delete VM + data disk
 home lock: rm -r machine/<name>/
```

| Adapter outcome | Record afterwards | Source |
| --- | --- | --- |
| removed | folder gone | [`machine/remove.rs`](../../../../../api/crates/layerfs-control/src/machine/remove.rs) `take_down` |
| refused or failed | put back as it was | same |
| `Unknown` | `op` kept, uncertain, with `error`; logged to `log/uncertain.jsonl` | same |

A gone process's unresolved intent does not refuse a remove: `machine
remove` is its clearing call. A running machine is removed as it is; it
need not be stopped first. See the
[routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a built-in machine; an `ssh` machine ("use machine disconnect"); a folder-scoped client | no | [`machine/remove.rs`](../../../../../api/crates/layerfs-control/src/machine/remove.rs) |
| `NotFound` | no machine of that name | no | [`machine/record.rs`](../../../../../api/crates/layerfs-control/src/machine/record.rs) |
| `Busy` | a live process's create, connect or remove holds it | no | same, `not_running` |
| `InUse` | sqlite Stores, Sandboxes or firecracker machines it hosts are on it (`detail.related`) | no | [`inventory.rs`](../../../../../api/crates/layerfs-control/src/inventory.rs) `can_remove_machine` |
| `Unsupported` | a build without the `machine` feature | no | [`machine/create.rs` (api)](../../../../../api/crates/layerfs-api/src/machine/create.rs) |
| `Unavailable`, `Failed` | the provider refused or failed; the record is put back | no | [`error.rs`](../../../../../connect/crates/layerfs-machine/src/error.rs) |
| `Unknown` | **the provider timed out or lost its answer** | **yes** | [`machine/remove.rs`](../../../../../api/crates/layerfs-control/src/machine/remove.rs) |

**After `Unknown`**, read [machine.status](status.md), then call
`machine remove` again.

## Examples

Not run; written from the call table.

```js
await layerfs.machine.remove("vm1");
```

```python
layerfs.machine.remove("vm1")
```

```bash
layerfs machine remove vm1
```

## Related

- Before: [store.pull](../store/pull.md) or [store.remove](../store/remove.md)
  for its Stores, [sandbox.close](../sandbox/close.md) for its Sandboxes.
- [machine.disconnect](disconnect.md) for an ssh machine.
- [Connect: machines](../../../connect/machines/README.md).

---

← [machine.connect](connect.md) · [Up: machine](README.md) · [machine.disconnect](disconnect.md) →
