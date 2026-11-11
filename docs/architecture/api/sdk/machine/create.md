# machine.create

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › create

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker, Microsandbox and SSH machines were removed in 2.0; a record
an older home still holds for one is shown, refused and only forgotten.
See [removed platforms](../../../control/support-matrix/disabled-platforms.md).

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
await layerfs.machine.create(name, platform, { cpus, memory, disk, image, os_image, on_progress });
await client.machine.create(name, platform, { cpus, memory, disk, image, os_image, on_progress });
```

```python
# Python
layerfs.machine.create(name, platform, cpus=None, memory=None, disk=None,
                       image=None, os_image=None, on_progress=None)
```

```bash
# CLI
layerfs machine create <name> <platform> [--cpus N] [--memory SIZE] \
  [--disk SIZE] [--image TEXT] [--os-image TEXT] [--progress]
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
| `lima` | yes | no | no | cpus, memory, disk, image | [lima](../../../control/machines/platforms/lima.md) |

`native` and `docker` are built in. `firecracker`, `microsandbox` and `ssh`
were removed in 2.0: naming one is refused `Unsupported` with
``platform `<name>` was removed in LayerFS 2.0``.

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `cpus` | Int | 2 | the VM's CPUs | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `DEFAULT_CPUS` |
| `memory` | Size | 4 GiB | bytes, or `K`/`M`/`G` text (powers of 1024) | same, `DEFAULT_MEMORY` |
| `disk` | Size | 20 GiB | the data disk mounted at `/layerfs/data` | same, `DEFAULT_DISK` |
| `image` | Text | none | Sandbox image; see below | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `spec` |
| `os_image` | Text | installed prepared image, else stock | VM operating-system image | `bundle::prepared_os_image`, `spec.rs` |
| `on_progress` | Function | none | startup stage events | `progress.rs` |

`image` selects the Sandbox image archive or content identity. `os_image` selects
the machine's operating-system image: an installed OCI reference for Apple
Container, or an OS image location for Lima. An explicit selection is passed to
the provider without silently substituting its stock image. With no explicit
selection, a compatible locally prepared mapping is used if installed; otherwise
the stock provider image is used. Preparation is explicit through
`tools/machine_image.py`; it is separate from machine creation and startup.

`on_progress` reports stage boundaries and elapsed milliseconds. See
[machine.start](start.md#options). The selected OS image is stored and returned as
`os_image`; each instance still gets private writable state and worker credentials.

## Returns

The machine's summary, with `state: "running"` once its worker answered:

```text
 { name, platform, backend, supports: { sqlite, postgresql_sandbox, reason },
   state, cpus, memory, disk, stores: [], sandboxes: [],
   image?, error? }
```

The fields are the same as [machine.get](get.md#returns) and are described
on [machine.list](list.md#returns). The record also keeps `vm_name`
(`lfs-<home id prefix>-<name>`, so two homes never share a VM) and
`created`.

## Where it runs

```text
 caller process                                  provider CLI / VM
 --------------                                  -----------------
 Platform::parse; created?
 sandbox_allowed(kind, host, sqlite) --> Unsupported (table's reason)
 probe tools --> Unsupported, detail.missing
 bundle(): sealed Linux bundle --> Unsupported if absent
 home lock: new_name, write record with op "create"
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
| `InvalidInput` | bad name; a built-in name; unknown platform; a built-in platform; a folder-scoped client | no | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs) |
| `Exists` | a machine of that name exists | no | same, `new_name` |
| `Unsupported` | the platform was removed (`firecracker`, `microsandbox`, `ssh`); it is not supported on this host; its tool is missing (`detail.missing`); no sealed bundle; a build without the `machine` feature | no | same, [`bundle.rs`](../../../../../control/crates/layerfs-machine/src/bundle.rs) |
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
layerfs machine create vm1 lima --cpus 4 --memory 8G
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

## Provisioning progress and cached artifacts

The optional `on_progress` callback (CLI `--progress`) reports bundle validation
and provider/worker setup with elapsed milliseconds. The existing create event
naming the machine is retained. Bundle protocol and architecture metadata are
checked before claiming the machine or booting it. Guest installation preserves
unchanged artifacts, including those preinstalled in a Lima image selected by
`os_image`. See [machine.start artifact reuse](start.md#artifact-reuse).
