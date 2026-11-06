# machine.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`machine.list` answers every machine in the home with its live state, its
Stores and Sandboxes, and what it supports. It starts nothing.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/list.rs`](../../../../../api/crates/layerfs-api/src/machine/list.rs) |
| Implemented in | [`layerfs_control::machine::list`](../../../../../control/crates/layerfs-machine/src/list.rs) |
| Runs in | the caller's process (host build with the `machine` feature) |

## Signature

```js
// JavaScript
await layerfs.machine.list();
await layerfs.client({ home }).machine.list();
```

```python
# Python
layerfs.machine.list()
layerfs.client(home=home).machine.list()
```

```bash
# CLI
layerfs machine list [--text]
```

## Arguments

None.

## Options

None. The machine calls take no paging: every machine is in one answer.

## Returns

```text
{ machines: [                      one summary per machine/<name>/ folder
    { name, platform, backend,
      supports: { sqlite, postgresql_sandbox, reason },
      state, cpus, memory, disk,
      stores: [name], sandboxes: [name],
      image?, host?, nested?, docker?, wsl?, daemon?, error?, ssh? }
] }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `name` | text | the machine's name (its folder) |
| `platform` | text | `native`, `docker`, `apple-container`, `microsandbox`, `lima`, `firecracker` or `ssh` |
| `backend` | text | the Sandbox backend on this host: `native`, `namespaces` (also on `docker`), or `-` for none ([`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `backend_name`) |
| `supports` | map | the platform's support row on this host: `sqlite` (bool), `postgresql_sandbox` (bool), `reason` (text or null) ([`platforms.rs`](../../../../../control/crates/layerfs-machine/src/platforms.rs) `supports`) |
| `state` | text | see the state table below |
| `cpus`, `memory`, `disk` | int or null | the recorded size; bytes for `memory` and `disk`; null on a built-in or ssh machine |
| `stores` | list | the sqlite Stores on this machine ([`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `stores_on`) |
| `sandboxes` | list | the Sandboxes recorded on it (`sandboxes_on`) |
| `image`, `host`, `nested`, `docker`, `wsl`, `daemon` | various | present only when the record has them |
| `error` | text | the probe's reason when the state is `unknown`, else a failed create's error |
| `ssh` | map | an ssh machine's `{ host, user, port, identity, known_hosts, root, host_id }` |

| `state` | When | Source |
| --- | --- | --- |
| `creating`, `removing` | a live process holds the machine's `op` (create or connect; remove or disconnect) | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) `state` |
| `unresolved` | a gone process left an uncertain `op` | `record.rs` |
| `running`, `stopped`, `failed`, `missing` | the provider's answer to the live probe | [`setup/calls.rs`](../../../../../control/crates/layerfs-platform/src/setup/calls.rs) `State` |
| `unknown` | the probe failed or did not answer within 10 s (`error` says why) | [`list.rs`](../../../../../control/crates/layerfs-machine/src/list.rs) `PROBE` |
| `ready` | `native`, which is never probed | `record.rs` |

## Where it runs

```text
caller's process
  read every machine/<name>/record.json   (no home lock)
  for each machine except native and one held by an op:
    thread --> layerfs-platform status_within(spec, deadline)
               (provider CLI, ssh or docker; starts nothing)
  wait for all probes together, at most 10 s
  late or failed probe --> state "unknown", its command killed
  nothing is written back
```

All of it runs in the caller; no worker is asked
([routing table](../../../control/routing/routing-table.md)). The built-in `docker`
machine is probed like a VM, because its worker is started and stopped by
hand (#535 D11).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `Unsupported` | a build without the `machine` feature (the guest worker's) | no | [`layerfs-api/src/machine/list.rs`](../../../../../api/crates/layerfs-api/src/machine/list.rs) |
| `Failed` | a record names no platform LayerFS knows, or the home cannot be read | no | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) `platform` |

A probe that fails is not an error of the call: that machine shows
`unknown` with its reason.

## Examples

Not run; written from the call table.

```js
const { machines } = await layerfs.machine.list();
```

```python
machines = layerfs.machine.list()["machines"]
```

```console
$ layerfs machine list --text
```

## Related

- Next: [`machine.get`](get.md) for one machine, [`machine.status`](status.md)
  for its free disk and tool version, [`machine.platforms`](platforms.md)
  for what could be created here.
- [Control: Families](../../../control/management/families.md#machine),
  [Control: Machines](../../../control/machines/README.md),
  [Control: Support matrix](../../../control/support-matrix/README.md).

---

← [SDK: machine](README.md) · [Up: machine](README.md) · [machine.platforms](platforms.md) →
