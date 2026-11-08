# machine.remove

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › remove

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Deletes a VM machine with its data disk, once no Store, Sandbox or old
Firecracker record it hosted is on it.
It also clears an unresolved create or remove. A machine whose record names
a platform removed in 2.0 (`firecracker`, `microsandbox`, `ssh`) is only
forgotten: its record goes and nothing outside the home is touched.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/remove.rs`](../../../../../api/crates/layerfs-api/src/machine/remove.rs) |
| Implemented in | `layerfs_control::machine::remove` → `take_down` ([`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs)), then `layerfs_platform::remove` |
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
| `name` | Name | a created VM machine, or a machine of a removed platform; built-ins are never removed | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) `removable` |

## Options

None.

## Returns

```text
 { name, removed: true }
 { name, removed: true, note }      a removed platform's machine
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name` | text | the machine | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) |
| `removed` | bool | always true on success; `machine/<name>/` is gone | same |
| `note` | text | only for a removed platform: ``forgot the record only: platform `ssh` was removed in LayerFS 2.0; nothing on its host was touched``; for `microsandbox` it names the VM and its `<vm>-data` volume, which were not touched, and the `msb remove --force <vm>` and `msb volume remove <vm>-data` commands that remove them; for `firecracker` it names the microVM's folder `/layerfs/data/firecracker/vms/<vm>` and data disk `/layerfs/data/firecracker/disks/<vm>.ext4`, which stay in its Lima host | same, `forget` |

## Where it runs

```text
 caller process                                   provider CLI
 --------------                                   ------------
 record; a live op on it --> Busy
 start lock (no start meanwhile)
 removed platform (firecracker, microsandbox, ssh) --> home lock:
            no Stores / Sandboxes / hosted records (else InUse); rm -r
            machine/<name>/; answer with `note` (no provider CLI)
 home lock: removable? (not built in,
            no Stores / Sandboxes / hosted records --> InUse)
            write op "remove", phase delete
 layerfs_platform::remove ------------------------>  delete VM + data disk
 home lock: rm -r machine/<name>/
```

| Adapter outcome | Record afterwards | Source |
| --- | --- | --- |
| removed | folder gone | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) `take_down` |
| refused or failed | put back as it was | same |
| `Unknown` | `op` kept, uncertain, with `error`; logged to `log/uncertain.jsonl` | same |

A gone process's unresolved intent does not refuse a remove: `machine
remove` is its clearing call. A running machine is removed as it is; it
need not be stopped first. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a built-in machine; a folder-scoped client | no | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) |
| `NotFound` | no machine of that name | no | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) |
| `Busy` | a live process's create or remove holds it | no | same, `not_running` |
| `InUse` | sqlite Stores, Sandboxes or old Firecracker records it hosted are on it (`detail.related`) | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_remove_machine` |
| `Unsupported` | a build without the `machine` feature | no | [`machine/create.rs` (api)](../../../../../api/crates/layerfs-api/src/machine/create.rs) |
| `Unavailable`, `Failed` | the provider refused or failed; the record is put back | no | [`error.rs`](../../../../../control/crates/layerfs-platform/src/error.rs) |
| `Unknown` | **the provider timed out or lost its answer** | **yes** | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) |

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
- [Control: machines](../../../control/machines/README.md).

---

← [machine.create](create.md) · [Up: machine](README.md) · [machine.update](update.md) →
