# machine.status

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › status

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`machine.status` asks the machine's provider for its state now, its data
disk's free space, the provider tool's version and why it is down; it shows
an `op` in flight. It starts nothing.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/status.rs`](../../../../../api/crates/layerfs-api/src/machine/status.rs) |
| Implemented in | [`layerfs_control::machine::status`](../../../../../api/crates/layerfs-control/src/machine/status.rs) |
| Runs in | the caller's process (host build with the `machine` feature) |

## Signature

```js
// JavaScript
await layerfs.machine.status(name);
```

```python
# Python
layerfs.machine.status(name)
```

```bash
# CLI
layerfs machine status <name> [--text]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a machine name (see [`get`](get.md#arguments)) | [`record.rs`](../../../../../api/crates/layerfs-control/src/machine/record.rs) `of` |

## Options

None.

## Returns

```text
{ name, platform, state,
  free_disk,        bytes free at /layerfs/data, or null
  tool_version,     provider tool's version, or null
  down_reason,      why it is down, or null
  stores: [name],
  op?: { id, name, phase, started, uncertain } }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `state` | text | `running`, `stopped`, `failed`, `missing` from the provider; `creating`, `removing`, `unresolved` while an `op` holds the machine | [`setup/calls.rs`](../../../../../connect/crates/layerfs-machine/src/setup/calls.rs) `State`, [`record.rs`](../../../../../api/crates/layerfs-control/src/machine/record.rs) `state` |
| `free_disk` | int or null | free bytes of the data disk as the guest sees them | `setup/calls.rs` `Status` |
| `tool_version` | text or null | the provider tool's version | `Status` |
| `down_reason` | text or null | why the machine is down; on `native`, `missing: host:root, ...` when a Linux host lacks something | [`native/observe.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/native/observe.rs) |
| `stores` | list | sqlite Stores on the machine | [`inventory.rs`](../../../../../api/crates/layerfs-control/src/inventory.rs) `stores_on` |
| `op` | map | present while the record holds a create, connect, remove or disconnect: its id, call name, phase, start time and `uncertain` | [`common/op.rs`](../../../../../api/crates/layerfs-control/src/common/op.rs) `to_sdk` |

`native` always answers `running` (unlike `list` and `get`, which show
`ready`), with nulls for disk and version.

## Where it runs

```text
caller's process
  MachineRecord::of(home, name)
  op held (live or uncertain)?
     yes --> state from the record, no probe; op shown, never cleared
     no  --> spec(record) --> layerfs-machine status(spec)
             (provider CLI / ssh / docker; no 10 s list deadline,
              each command bounded by its own tool timeout)
```

All in the caller ([routing table](../../routing/routing-table.md)). A
stopped VM answers `stopped` without being started.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput`, `NotFound` | bad name; no such machine | no | [`record.rs`](../../../../../api/crates/layerfs-control/src/machine/record.rs) `of` |
| `Unavailable`, `Denied`, `Failed` | the provider or guest does not answer; ssh refuses the login or host key; the probe failed | no: a read changes nothing | [`spec.rs`](../../../../../api/crates/layerfs-control/src/machine/spec.rs) `refusal` |
| `Unknown` | a provider command did not end in time and was stopped | no: a read changes nothing | [`setup/tool.rs`](../../../../../connect/crates/layerfs-machine/src/setup/tool.rs) |
| `Busy`, `Unresolved` | a firecracker machine whose Lima host is held by an `op` | no | [`spec.rs`](../../../../../api/crates/layerfs-control/src/machine/spec.rs) `machine_of` |
| `Unsupported` | a build without the `machine` feature | no | [`layerfs-api/src/machine/status.rs`](../../../../../api/crates/layerfs-api/src/machine/status.rs) |

Unlike `list` and `get`, a failed probe is the call's error. An
`unresolved` state is cleared by [`machine.remove`](remove.md) (VM) or
[`machine.disconnect`](disconnect.md) (ssh); see
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Examples

Not run; written from the call table.

```js
const s = await layerfs.machine.status("vm1");
console.log(s.state, s.free_disk);
```

```python
s = layerfs.machine.status("vm1")
```

```console
$ layerfs machine status vm1
```

## Related

- Before: [`machine.get`](get.md). After: [`machine.start`](start.md) when
  `stopped`; [`machine.remove`](remove.md) or
  [`machine.disconnect`](disconnect.md) when `unresolved`.
- [Connect: Machines](../../../connect/machines/README.md),
  [Control: Families](../../control/families.md#machine).

---

← [machine.get](get.md) · [Up: machine](README.md) · [machine.start](start.md) →
