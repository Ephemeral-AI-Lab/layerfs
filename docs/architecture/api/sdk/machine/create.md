# machine.create

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › create

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker is **permanently disabled for new work**. Its code and records
remain for inspection and explicit cleanup. Microsandbox and SSH machines
were removed in 2.0; a record an older home still holds for one is shown,
refused and only forgotten. See the [platform restriction](../../../control/support-matrix/disabled-platforms.md) for
refusals, existing-resource handling and the source change required for
re-enabling. Descriptions of Firecracker's adapter mechanics below are
retained implementation details, not selectable features.

Makes a new VM machine on this host, copies the sealed Linux bundle in and
starts its worker, so Stores and Sandboxes can be placed on it.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/create.rs`](../../../../../api/crates/layerfs-api/src/machine/create.rs) |
| Implemented in | `layerfs_control::machine::create` ([`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs)), then `layerfs_platform::create` ([`setup/calls.rs`](../../../../../control/crates/layerfs-platform/src/setup/calls.rs)) |
| Runs in | the caller's process (host build with the `machine` feature); the provider's CLI makes the VM |

## Signature

```js
// JavaScript
await layerfs.machine.create(name, platform, { cpus, memory, disk, image, nested, host });
await client.machine.create(name, platform, { cpus, memory, disk, image, nested, host });
```

```python
# Python
layerfs.machine.create(name, platform, cpus=None, memory=None, disk=None,
                       image=None, nested=None, host=None)
```

```bash
# CLI
layerfs machine create <name> <platform> [--cpus N] [--memory SIZE] \
  [--disk SIZE] [--image TEXT] [--nested] [--host NAME]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | `MachineName::parse`; not `native` or `docker`; not taken | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs) `new_name` |
| `platform` | Name | an enabled created platform: `apple-container`, `lima` | [`platform.rs`](../../../../../control/crates/layerfs-platform/src/platform.rs) `created` |

Which platform this host may create comes from the support table
([`support.rs`](../../../../../control/crates/layerfs-platform/src/support.rs),
[`control/SUPPORT.json`](../../../../../control/SUPPORT.json)):

| Platform | macOS | Linux | Windows | Options | Page |
| --- | --- | --- | --- | --- | --- |
| `apple-container` | yes | no | no | cpus, memory, disk, image | [apple-container](../../../control/machines/platforms/apple-container.md) |
| `lima` | yes | no | no | cpus, memory, disk, image, nested | [lima](../../../control/machines/platforms/lima.md) |
| `firecracker` | disabled | disabled | disabled | none selectable | [firecracker](../../../control/machines/platforms/firecracker.md) |

`native` and `docker` are built in. `microsandbox` and `ssh` were removed
in 2.0: naming one is refused `Unsupported` with
``platform `<name>` was removed in LayerFS 2.0``.

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `cpus` | Int | 2 | the VM's CPUs | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `DEFAULT_CPUS` |
| `memory` | Size | 4 GiB | bytes, or `K`/`M`/`G` text (powers of 1024) | same, `DEFAULT_MEMORY` |
| `disk` | Size | 20 GiB | the data disk mounted at `/layerfs/data` | same, `DEFAULT_DISK` |
| `image` | Text | none | recorded as given; see below | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `spec` |
| `nested` | Bool | false | `lima` only: a VM that can host `firecracker` | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs) |
| `host` | Name | none | `firecracker` only, and required: a `lima` machine made with `nested` | same |

`image` means two things. The platform profiles declare it as the VM's own
system image ([`options.rs`](../../../../../control/crates/layerfs-platform/src/options.rs)
`IMAGE`; `apple-container` boots `Spec::os_image`, default
`debian:trixie-slim`). Control does not pass it that
way: `machine::spec` turns an `image` that starts with `/` into
`Spec::image`, the Sandbox image archive copied in at start, and fills
`Spec::os_image` from `image` only for the `docker` machine. So on a
created VM an absolute path selects the Sandbox image archive, and any
other value is recorded but does not reach the VM's system image (the
record's `os_image` field does, and no call writes it).

## Returns

The machine's summary, with `state: "running"` once its worker answered:

```text
 { name, platform, backend, supports: { sqlite, postgresql_sandbox, reason },
   state, cpus, memory, disk, stores: [], sandboxes: [],
   image?, host?, nested?, error? }
```

The fields are the same as [machine.get](get.md#returns) and are described
on [machine.list](list.md#returns). The record also keeps `vm_name`
(`lfs-<home id prefix>-<name>`, so two homes never share a VM) and
`created`.

## Where it runs

```text
 caller process                                  provider CLI / VM
 --------------                                  -----------------
 Platform::parse; created? nested/host rules
 sandbox_allowed(kind, host, sqlite) --> Unsupported (table's reason)
 probe tools --> Unsupported, detail.missing
 bundle(): sealed Linux bundle --> Unsupported if absent
 home lock: new_name, host check, write record with op "create"
 progress { phase: "create", machine }
 start lock (a machine start waits)
 layerfs_platform::create ----------------------->  VM + data disk
                                                   copy bundle, start
                                                   worker, forward,
                                                   wait for its secret
 home lock: worker.json, state running, drop op   (finish)
```

| Outcome of the adapter | Record afterwards | Source |
| --- | --- | --- |
| running | `state: running`, `op` dropped, `worker.json` written | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs) `finish` |
| failed before the VM existed | the machine folder is removed | same |
| failed after (`detail.vm_exists: true`) | `state: failed` with `error`; VM kept for inspection | same |
| `Unknown` | everything kept; `op` uncertain with `error`; logged to `log/uncertain.jsonl` | same |

The home lock covers the claim and the record writes only, never the boot.
See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad name; a built-in name; unknown platform; a built-in platform; `nested` off lima; firecracker without `host` or with a host that is not a nested lima; a folder-scoped client | no | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs) |
| `Exists` | a machine of that name exists | no | same, `new_name` |
| `NotFound` | `host` names no machine | no | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) |
| `Busy` / `Unresolved` | `host` is held by another operation | no | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `machine_of` |
| `Unsupported` | the platform is disabled (`firecracker`) or was removed (`microsandbox`, `ssh`); it is not supported on this host; its tool is missing (`detail.missing`); no sealed bundle; a build without the `machine` feature | no | same, [`bundle.rs`](../../../../../control/crates/layerfs-machine/src/bundle.rs) |
| `Unavailable`, `Denied`, `Failed` | the provider refused or failed; `detail.vm_exists` says whether the VM is kept | no | [`error.rs`](../../../../../control/crates/layerfs-platform/src/error.rs) |
| `Unknown` | **the provider timed out or its answer was lost** | **yes** | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs) `finish` |

**After `Unknown`**, read [machine.status](status.md) (its `op` and
`error`) and the provider's own listing; clear the machine with
[machine.remove](remove.md), which deletes the VM if it exists.

## Examples

Not run; written from the call table.

```js
await layerfs.machine.create("vm1", "lima", { cpus: 4, memory: "8G", disk: "64G" });
```

```python
layerfs.machine.create("box", "apple-container", memory="2G")
```

```bash
# Permanently refused: layerfs machine create fc1 firecracker --host vm1
```

## Related

- Before: [machine.platforms](platforms.md) (what this host can create and
  what is missing). After: [store.push](../store/push.md),
  [sandbox.create](../sandbox/create.md), [machine.stop](stop.md),
  [machine.start](start.md), [machine.update](update.md).
- [Control: machines](../../../control/machines/README.md),
  [adapter](../../../control/machines/adapter.md),
  [support matrix](../../../control/support-matrix/README.md).

---

← [machine.stop](stop.md) · [Up: machine](README.md) · [machine.remove](remove.md) →
