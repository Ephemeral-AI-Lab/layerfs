# Lima

[Architecture](../../../README.md) › [Connect](../../README.md) › [Machines](../README.md) › Lima

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Lima platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](../README.md) |
| Source | [`connect/crates/layerfs-machine/src/platforms/lima`](../../../../../connect/crates/layerfs-machine/src/platforms/lima) |

## How it is made

A created machine: one Lima instance (`vmType: vz`, macOS only), made from
a template written to `limactl create`'s stdin
([`lima/observe.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/lima/observe.rs)).

```text
 create(spec, bundle)                              lima/create.rs
   host_check: macOS; nested -> host:macos15, host:apple-m3; `limactl`
   limactl list <vm>                       present -> Exists
   limactl disk list --json: lfs<8 hex>    present -> Exists
   port = spec.state.port or a free host port
   limactl disk create lfs<8 hex> --size <disk>MiB --format raw
   limactl create --name <vm> --tty=false -   < template
       fails, nothing made: limactl disk delete (one cleanup), Failed
       fails, instance exists: Failed, vm_exists (kept)
   limactl start --tty=false <vm>
   ATTACH: /mnt/lima-lfs<8 hex> must be ext4; bind it at /layerfs/data
   nested: bundle's firecracker/ -> /layerfs/data/firecracker (once)
   worker::bring_up over ssh -F <dir>/ssh.config lima-<vm> sudo -n:
     launch on 127.0.0.1:7070 -> ready at host 127.0.0.1:<port>
```

| Template key ([`lima/template.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/lima/template.rs)) | Value |
| --- | --- |
| `vmType` | `vz`; `nestedVirtualization: true` when `nested` |
| `cpus`, `memory` | from the spec (`memory` in MiB) |
| `images` | `os_image` when it is an `https://` URL or an absolute path, else Ubuntu 24.04's release cloud image for `aarch64` and `x86_64` |
| `containerd` | system and user off |
| `mounts` | `[]`: the guest never sees the host home |
| `additionalDisks` | the data disk, `format: true`, `fsType: ext4` |
| `portForwards` | guest `127.0.0.1:7070` to host `127.0.0.1:<port>`; every other guest port ignored |

The disk is named `lfs` and 8 hex digits of an FNV-1a hash of the VM name,
so Lima's ext4 label `lima-<disk>` fits its 16 bytes and the formatted disk
is found again at every boot.

| Call | Does | Source |
| --- | --- | --- |
| `start` | needs the recorded port; `NotFound` if missing; `Failed` if broken; boots if stopped; then attach, nested files, `bring_up` | [`lima/power.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/lima/power.rs) |
| `stop` | `limactl stop` if running (up to 300 s); the instance and disk stay | same |
| `remove` | `limactl delete --force`, then `limactl disk delete` | [`lima/create.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/lima/create.rs) |
| `status` | `limactl list`: `Running`, `Broken` (failed), else stopped; through `with_worker` | `observe.rs` |
| `forward` | a detached `ssh -N -L 127.0.0.1:<free>:127.0.0.1:<guest port>` over Lima's `ssh.config` with `ControlMaster=no`; answers its pid | `observe.rs` `daemon_forward` |

## Backend and kernel route

| Host | `backend` | `kernel` | `description` |
| --- | --- | --- | --- |
| macos | `namespaces` | `fuse` | a Lima VM (vz); Sandboxes run in Linux namespaces and a cgroup |
| linux | `null` | `null` | a Lima VM (vz) |
| windows | `null` | `null` | a Lima VM (vz) |

## Where Store calls run

| Host | `sqlite` | `postgresql.store_calls` |
| --- | --- | --- |
| macos | `worker` | `in_process` |
| linux | `null` | `in_process` |
| windows | `null` | `in_process` |

The worker binds the guest's loopback (`worker_bind` `127.0.0.1`), which
Lima's port forward reaches. Workspace calls skip the worker through
`forward`.

## PostgreSQL reach

| Host | `postgresql.sandbox` | `postgresql.reason` |
| --- | --- | --- |
| macos | `true` | `null` |
| linux | `false` | "lima runs on macOS hosts only" |
| windows | `false` | "lima runs on macOS hosts only" |

`reach` rewrites a loopback host to `192.168.5.2`, the gateway of Lima's
user-mode network that the guest's `/etc/hosts` names
`host.lima.internal`. The address is used, not the name, because a Sandbox
daemon runs in the Sandbox image's root, which has no such hosts entry.

## Options

| Option | Kind | `Spec` field |
| --- | --- | --- |
| `cpus`, `memory`, `disk` | int, size, size | `cpus`, `memory`, `disk` (the data disk only; Lima's root disk keeps Lima's size) |
| `image` | text | see [Options](../platforms-and-profiles.md#options) |
| `nested` | bool | `nested`: a host for `firecracker` machines |

## Live receipts

| Receipt | What ran |
| --- | --- |
| [g7-reach](../../../../../core/docs/issues/535/receipts/g7-reach/RECEIPT.md) | host reach probed by hand with the template's flags |
| [g7-live](../../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md) | PostgreSQL Sandboxes reaching a server in Docker Desktop (Lima 2.2.1) |
| [K3](../../../../../core/docs/issues/535/receipts/fe6f85013b/K3/RESULTS.md) | first terminal pass |
| [L3](../../../../../core/docs/issues/535/receipts/35ed671b6c/L3/RESULTS.md) | final pass: sqlite, PostgreSQL loopback and remote cells, power |
| [TERMINAL-PASS](../../../../../core/docs/issues/535/TERMINAL-PASS.md) | the merged verdict |

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: host:macos` / `tool:limactl` | not a Mac, or no `limactl` | `observe.rs` `host_check` |
| `missing: host:macos15` / `host:apple-m3` | `nested` on an older macOS or chip | [`probe.rs`](../../../../../connect/crates/layerfs-machine/src/probe.rs) `nested_missing` |
| `Exists`: "the Lima disk … for … already exists; it is left alone" | a disk from an earlier machine | `create.rs` |
| `Failed`: "the Lima instance … is broken" | `limactl list` says `Broken` | `power.rs` |
| "/mnt/lima-… is not an ext4 mount" | the data disk did not attach as ext4 | `power.rs` `ATTACH` |
| `Unavailable`: "the lima machine … is stopped" | `guest` or `forward` on a stopped instance | `observe.rs` `guest` |
| `Failed`: "ssh -L 127.0.0.1:<port> ended at once" | the forward died within 600 ms; `forward.log` tail | [`setup/forward.rs`](../../../../../connect/crates/layerfs-machine/src/setup/forward.rs) |

## Related

- [Firecracker](firecracker.md): runs inside a `nested` lima machine.
- [Sandboxes: Native and namespaces](../../sandboxes/native-and-namespaces.md).
- [Network: Workers](../../network/workers.md), [Network: Reach](../../network/reach.md), [Network: Forwards](../../network/forwards.md).
- [Adapter](../adapter.md), [Support matrix: Table](../../support-matrix/table.md).

---

← [Machines: microsandbox](microsandbox.md) · [Up: Machines](../README.md) · [Machines: Firecracker](firecracker.md) →
