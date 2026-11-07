# machine.get

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › get

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`machine.get` answers one machine's summary: its record, live state, Stores
and Sandboxes. It starts nothing.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`machine/get.rs`](../../../../../api/crates/layerfs-api/src/machine/get.rs) |
| Implemented in | [`layerfs_control::machine::get`](../../../../../control/crates/layerfs-machine/src/get.rs) |
| Runs in | the caller's process (host build with the `machine` feature) |

## Signature

```js
// JavaScript
await layerfs.machine.get(name);
```

```python
# Python
layerfs.machine.get(name)
```

```bash
# CLI
layerfs machine get <name> [--text]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a machine name: 1–63 lowercase letters, digits, `_ . -`, starting and ending with a letter or digit | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) `MachineName` |

## Options

None.

## Returns

```text
{ name, platform, backend, supports: { sqlite, postgresql_sandbox, reason },
  state, cpus, memory, disk, stores: [name], sandboxes: [name],
  image?, host?, nested?, docker?, wsl?, daemon?, error?, ssh? }
```

The same summary as one item of [`machine.list`](list.md#returns); the
field and `state` tables are there. `start`, `stop`, `create`, `connect`
and `update` answer the same shape
([`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) `to_sdk`).

## Where it runs

```text
caller's process
  MachineRecord::of(home, name)        InvalidInput | NotFound
  op held by a live or gone process? --> no probe; state from the record
  native?                            --> no probe; state "ready"
  otherwise: one live probe, at most 10 s (late --> "unknown")
  summary with stores_on / sandboxes_on
```

All in the caller ([routing table](../../../control/routing/routing-table.md)).
`get` is a read: it answers while an `op` holds the machine, showing
`creating`, `removing` or `unresolved`, and it never clears an `op`.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | `name` is not a machine name | no | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) `of` |
| `NotFound` | no machine of that name | no | `record.rs` `of` |
| `Failed` | the record names no known platform | no | `record.rs` `platform` |
| `Unsupported` | a build without the `machine` feature | no | [`machine/get.rs`](../../../../../api/crates/layerfs-api/src/machine/get.rs) |

A failed probe is not an error: `state` is `unknown` and `error` says why.

## Examples

Not run; written from the call table.

```js
const vm = await layerfs.machine.get("vm1");
if (vm.state !== "running") await layerfs.machine.start("vm1");
```

```python
vm = layerfs.machine.get("vm1")
```

```console
$ layerfs machine get vm1
```

## Related

- Before: [`machine.list`](list.md). After: [`machine.status`](status.md)
  (free disk, tool version, `op`), [`machine.start`](start.md).
- [Control: Families](../../../control/management/families.md#machine),
  [Control: Machines](../../../control/machines/README.md).

---

← [machine.platforms](platforms.md) · [Up: machine](README.md) · [machine.status](status.md) →
