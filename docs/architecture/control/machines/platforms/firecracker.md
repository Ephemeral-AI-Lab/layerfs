# Firecracker

[Architecture](../../../README.md) › [Control](../../README.md) › [Machines](../README.md) › Firecracker

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker and SSH are **permanently disabled for new work**. Their code
and records remain for inspection and explicit cleanup. See the
[platform restriction](../../support-matrix/disabled-platforms.md) for refusals, existing-resource handling and
the source change required for re-enabling. Descriptions of their adapter
mechanics below are retained implementation details, not selectable features.

The Firecracker platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](../README.md) |
| Source | [`control/crates/layerfs-platform/src/platforms/firecracker`](../../../../../control/crates/layerfs-platform/src/platforms/firecracker) |

## How it is made

A created machine: one Firecracker microVM inside a [`lima`](lima.md)
machine made with `nested`. macOS has no KVM, so the microVM runs under the
Lima VM's nested KVM
([`firecracker/observe.rs`](../../../../../control/crates/layerfs-platform/src/platforms/firecracker/observe.rs)).

```text
 macOS host
   |  ssh -N -L 127.0.0.1:<port>:172.30.<n>.2:7070 lima-<host> (detached)
   v
 +-- nested lima machine <host> ---------------------------------------+
 | /layerfs/data/firecracker/                                          |
 |   bin/firecracker  vmlinux  rootfs.ext4  id_ed25519   (from bundle) |
 |   vms/<vm>/  rootfs.ext4 config.json vm.env firecracker.pid         |
 |   disks/<vm>.ext4                                                   |
 | tap lfsfc<n> 172.30.<n>.1/30    iptables MASQUERADE 172.30.0.0/16   |
 |   |                                                                 |
 |   +-- Firecracker microVM <vm> --------------------------------+    |
 |       eth0 172.30.<n>.2   /dev/vdb -> /layerfs/data (ext4)     |    |
 |       worker on 0.0.0.0:7070; Sandbox daemons on its loopback  |    |
 +---------------------------------------------------------------------+
```

```text
 create(spec, bundle)                          firecracker/create.rs
   `ssh` on the search path; the lima host must run  else Unavailable
   CHECK in host: /dev/kvm, the four files, mkfs.ext4 ip iptables
                  flock ssh truncate sysctl  -> Unsupported, each missing
   INSPECT <vm>                      present -> Exists
   DISK:   truncate + mkfs.ext4 disks/<vm>.ext4   exit 17 -> Exists
   CREATE: under flock, network n (recorded, else first free, <= 250),
           rootfs copy, config.json, vm.env       prints n
       fails, no VM folder: rm the disk (one cleanup), Failed
       fails, VM exists: Failed, vm_exists (kept)
   BOOT_VM: huge pages if free, tap, ip_forward, masquerade,
            firecracker --no-api --config-file, wait for sshd (60 s),
            mount /dev/vdb at /layerfs/data
   forward::open  ssh -L to 172.30.<n>.2:7070 through the lima host
   worker::bring_up via the host (ssh -i id_ed25519 root@172.30.<n>.2)
   Running { endpoint (forward pid), lasting { port, network: n } }
```

| Call | Does | Source |
| --- | --- | --- |
| `start` | `NotFound` if no VM folder; `BOOT_VM` unless running; a new forward; `bring_up` | [`firecracker/power.rs`](../../../../../control/crates/layerfs-platform/src/platforms/firecracker/power.rs) |
| `stop` | ends the forward; `STOP`: `sync; reboot -f`, waits 5 s, then `kill -9`; removes the tap | same, [`scripts.rs`](../../../../../control/crates/layerfs-platform/src/platforms/firecracker/scripts.rs) |
| `remove` | ends the forward; `STOP … delete`: also the VM folder and data disk | [`firecracker/create.rs`](../../../../../control/crates/layerfs-platform/src/platforms/firecracker/create.rs) |
| `status` | lima host down: `stopped`, "its Lima host is down: …"; no VM folder: `missing`; VMM alive and forward running: `running`; else `stopped` | `observe.rs` |
| `guest` | the lima host's guest, then `ssh` into the microVM as root | [`firecracker/host.rs`](../../../../../control/crates/layerfs-platform/src/platforms/firecracker/host.rs) `FIRECRACKER_GUEST` |
| `forward` | `Unsupported`: daemons listen on the microVM's loopback, which a forward through the lima host cannot reach | [`firecracker/adapter.rs`](../../../../../control/crates/layerfs-platform/src/platforms/firecracker/adapter.rs) |

Firecracker's files come from the bundle's `firecracker/` folder, copied
straight to the nested host's data disk once per bundle hash when the lima
host starts. No guest's bundle copy carries that folder. The microVM's ssh
host key is not checked: its address is a point-to-point tap inside the
host, reused by the next VM of that number.

## Backend and kernel route

| Host | `backend` | `kernel` | `description` |
| --- | --- | --- | --- |
| macos | `namespaces` | `fuse` | a Firecracker microVM in a nested lima machine; Sandboxes run in Linux namespaces and a cgroup |
| linux | `null` | `null` | a Firecracker microVM in a nested lima machine |
| windows | `null` | `null` | a Firecracker microVM in a nested lima machine |

## Where Store calls run

| Host | `sqlite` | `postgresql.store_calls` |
| --- | --- | --- |
| macos | `worker` | `in_process` |
| linux | `null` | `in_process` |
| windows | `null` | `in_process` |

The worker binds every interface (`worker_bind` `0.0.0.0`) because the
forward arrives at the tap address. Workspace calls also go through the
worker.

## PostgreSQL reach

| Host | `postgresql.sandbox` | `postgresql.reason` |
| --- | --- | --- |
| macos | `true` | `null` |
| linux | `false` | "firecracker runs on macOS hosts only, in a nested lima machine" |
| windows | `false` | "firecracker runs on macOS hosts only, in a nested lima machine" |

`reach` rewrites a loopback host to `192.168.5.2`, the lima host's own
host address. The lima host masquerades the microVMs' traffic leaving
172.30.0.0/16 (`BOOT_VM`), and Lima maps that address to the Mac's
loopback.

## Options

| Option | Kind | Used as |
| --- | --- | --- |
| `cpus`, `memory`, `disk` | int, size, size | `Spec::cpus` as `vcpu_count`, `Spec::memory` as `mem_size_mib`, `Spec::disk` as the data disk's size |
| `image` | text | see [Options](../platforms-and-profiles.md#options); the microVM's root filesystem is always the kit's `rootfs.ext4` |
| `host` | name | `Spec::host`: the `Spec` of a `lima` machine with `nested`, else `InvalidInput` |

`Spec::validate` refuses a firecracker spec without a host, a host that is
not a nested lima machine, and a network number outside 1 to 250.

## Live receipts

| Receipt | What ran |
| --- | --- |
| [g7-reach](../../../../../core/docs/issues/535/receipts/g7-reach/RECEIPT.md) | host reach, firecracker simulated by hand |
| [g7-live](../../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md) | PostgreSQL Sandboxes reaching a server in Docker Desktop (Firecracker v1.17.0) |
| [K3](../../../../../core/docs/issues/535/receipts/fe6f85013b/K3/RESULTS.md) | first terminal pass |
| [L3](../../../../../core/docs/issues/535/receipts/35ed671b6c/L3/RESULTS.md) | final pass: sqlite, PostgreSQL loopback and remote cells, power |
| [TERMINAL-PASS](../../../../../core/docs/issues/535/TERMINAL-PASS.md) | the merged verdict |

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: host:<host>:/dev/kvm` | the lima host has no nested KVM | `scripts.rs` `CHECK` |
| `missing: host:<host>:/layerfs/data/firecracker/<file>` | the bundle had no `firecracker/` kit | same |
| `missing: host:<host>:tool:<t>` | a tool missing in the lima host | same |
| `missing: host:macos15` / `host:apple-m3` | probe on an older Mac | [`probe.rs`](../../../../../control/crates/layerfs-platform/src/probe.rs) |
| `Unavailable`: "the lima machine … is stopped" | the lima host does not run | [`lima/observe.rs`](../../../../../control/crates/layerfs-platform/src/platforms/lima/observe.rs) |
| "every network number is taken" / "network n is taken" | 250 microVMs on one host, or a recorded `n` in use | `scripts.rs` `CREATE` |
| "the microVM did not answer ssh within 60 s" / "firecracker ended: …" | boot failed; see `vms/<vm>/firecracker.log` | `scripts.rs` `BOOT_VM` |

## Related

- [Lima](lima.md): the nested host.
- [Sandboxes: Native and namespaces](../../sandboxes/native-and-namespaces.md).
- [Network: Workers](../../network/workers.md), [Network: Reach](../../network/reach.md), [Network: Forwards](../../network/forwards.md).
- [Adapter](../adapter.md), [Support matrix: Table](../../support-matrix/table.md).

---

← [Machines: Lima](lima.md) · [Up: Machines](../README.md) · [Machines: SSH](ssh.md) →
