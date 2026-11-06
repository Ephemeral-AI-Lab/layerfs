# machine.update

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › update

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Changes a machine's recorded settings: a built-in machine's image, Docker
socket, WSL distribution and daemon, or any machine's image. Resizing a
VM is in the call table but not built yet.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`api/crates/layerfs-api/src/machine/update.rs`](../../../../../api/crates/layerfs-api/src/machine/update.rs) |
| Implemented in | `layerfs_control::machine::update` ([`machine/update.rs`](../../../../../control/crates/layerfs-control/src/machine/update.rs)) |
| Runs in | the caller's process; it writes the record and reaches nothing |

## Signature

```js
// JavaScript
await layerfs.machine.update(name, { cpus, memory, image, docker, wsl, daemon });
```

```python
# Python
layerfs.machine.update(name, cpus=None, memory=None, image=None,
                       docker=None, wsl=None, daemon=None)
```

```bash
# CLI
layerfs machine update <name> [--image TEXT] [--docker SOCKET] \
  [--wsl DISTRO] [--daemon PATH] [--cpus N] [--memory SIZE]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | any recorded machine not held by an operation | [`machine/spec.rs`](../../../../../control/crates/layerfs-control/src/machine/spec.rs) `machine_of` |

## Options

| Name | Kind | Applies to | Rule | Source |
| --- | --- | --- | --- | --- |
| `image` | Text | any machine | recorded as `image`; on `docker` it is the worker container's image; an absolute path is the Sandbox image archive | [`machine/spec.rs`](../../../../../control/crates/layerfs-control/src/machine/spec.rs) `spec` |
| `docker` | Text | built-in only | the Docker socket this home's tools use | [`machine/update.rs`](../../../../../control/crates/layerfs-control/src/machine/update.rs), `tools` in spec.rs |
| `wsl` | Text | built-in only, Windows host only | the WSL distribution | same |
| `daemon` | Text | built-in only | the absolute path of an existing file; the daemon its Sandboxes run, replacing what `import-env` recorded | same |
| `cpus`, `memory` | Int, Size | VM machines | refused on built-ins; on a VM, `Unsupported` ("not built yet"), nothing changed | same |

## Returns

The machine's summary with the new settings; see
[machine.get](get.md#returns).

## Where it runs

```text
 caller process
 --------------
 home lock
   machine_of(name)               --> NotFound / Busy / Unresolved
   built-in + cpus/memory?        --> InvalidInput
   VM + docker/wsl/daemon?        --> InvalidInput
   daemon not an absolute file?   --> InvalidInput
   wsl off Windows?               --> InvalidInput
   can_change_machine: Sandboxes running on it? --> InUse
   cpus/memory on a VM?           --> Unsupported (nothing changed)
   write image/docker/wsl/daemon into the record
```

A running Sandbox keeps the image and settings it started with, so the
call is refused while one is recorded. A new setting takes effect at the
next [machine.start](start.md) or [sandbox.create](../sandbox/create.md).
See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | the option combinations above; a folder-scoped client | no | [`machine/update.rs`](../../../../../control/crates/layerfs-control/src/machine/update.rs) |
| `NotFound` | no machine of that name | no | [`machine/record.rs`](../../../../../control/crates/layerfs-control/src/machine/record.rs) |
| `Busy` / `Unresolved` | another operation holds the machine / left it unresolved | no | [`machine/spec.rs`](../../../../../control/crates/layerfs-control/src/machine/spec.rs) `machine_of` |
| `InUse` | Sandboxes are recorded on it | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_change_machine` |
| `Unsupported` | resizing a VM; a build without the `machine` feature | no | [`machine/update.rs`](../../../../../control/crates/layerfs-control/src/machine/update.rs) |
| `Failed` | the record cannot be written | no | same |

## Examples

Not run; written from the call table.

```js
await layerfs.machine.update("docker", { docker: "/Users/me/.colima/default/docker.sock" });
```

```python
layerfs.machine.update("native", daemon="/opt/layerfs/bin/layerfs-daemon")
```

```bash
layerfs machine update docker --image layerfs/sandbox:2.0.0
```

## Related

- Before: [machine.get](get.md), [sandbox.close](../sandbox/close.md).
  After: [machine.start](start.md).
- [Control: docker](../../../control/machines/platforms/docker.md),
  [Control: native](../../../control/machines/platforms/native.md).

---

← [machine.disconnect](disconnect.md) · [Up: machine](README.md) · [SDK: store](../store/README.md) →
