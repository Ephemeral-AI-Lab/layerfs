# machine.disconnect

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › disconnect

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Forgets an ssh machine: stops its worker and forward and removes what
LayerFS installed on the host. The host's data root stays.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/disconnect.rs`](../../../../../api/crates/layerfs-api/src/machine/disconnect.rs) |
| Implemented in | `layerfs_control::machine::disconnect` → `take_down` ([`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs)), then `layerfs_platform::remove` |
| Runs in | the caller's process; `ssh` to the host |

## Signature

```js
// JavaScript
await layerfs.machine.disconnect(name);
await client.machine.disconnect(name);
```

```python
# Python
layerfs.machine.disconnect(name)
```

```bash
# CLI
layerfs machine disconnect <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | an `ssh` machine; a VM is removed with [machine.remove](remove.md) | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) `removable` |

## Options

None.

## Returns

```text
 { name, removed: true }
```

The same shape as [machine.remove](remove.md#returns).

## Where it runs

```text
 caller process                                   ssh host
 --------------                                   --------
 record; a live op --> Busy ; start lock
 home lock: removable? (ssh, no Stores / Sandboxes --> InUse)
            write op "disconnect", phase delete
 layerfs_platform::remove ------------------------>  stop worker + forward
                                                   rm /layerfs/bundle,
                                                   /layerfs/state
                                                   (data root kept)
 home lock: rm -r machine/<name>/
```

Failure handling is the same as [machine.remove](remove.md#where-it-runs):
a refused step puts the record back, `Unknown` keeps the `op` uncertain.
`machine disconnect` is the clearing call of an ssh machine's unresolved
intent. See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a built-in machine; a VM ("use machine remove"); a folder-scoped client | no | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) |
| `NotFound` | no machine of that name | no | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) |
| `Busy` | a live process's operation holds it | no | same |
| `InUse` | sqlite Stores or Sandboxes are on it (`detail.related`) | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_remove_machine` |
| `Denied`, `Unavailable`, `Failed` | the host refuses, does not answer or a step fails; record put back | no | [`error.rs`](../../../../../control/crates/layerfs-platform/src/error.rs) |
| `Unknown` | **a step timed out or lost its answer** | **yes** | [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) |

**After `Unknown`**, read [machine.status](status.md) and call
`machine disconnect` again.

## Examples

Not run; written from the call table.

```js
await layerfs.machine.disconnect("box");
```

```python
layerfs.machine.disconnect("box")
```

```bash
layerfs machine disconnect box
```

## Related

- Before: [store.pull](../store/pull.md) for Stores on it,
  [sandbox.close](../sandbox/close.md). Again later:
  [machine.connect](connect.md); the data root is still there.
- [Control: ssh](../../../control/machines/platforms/ssh.md).

---

← [machine.remove](remove.md) · [Up: machine](README.md) · [machine.update](update.md) →
