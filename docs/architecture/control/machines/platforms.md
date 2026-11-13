# Platforms

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The four machine platforms, the one support table that decides where each
may run, and how to add one, within [Machines](README.md).

| | |
| --- | --- |
| Source | [`platforms/`](../../../../control/crates/layerfs-platform/src/platforms), [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs), [`control/SUPPORT.json`](../../../../control/SUPPORT.json), [`platform_policy.rs`](../../../../control/crates/layerfs-sandbox-backend/src/platform_policy.rs) |

## Profiles (`platforms/<p>/profile.rs`)

| Platform | How | Tool | Hosts | Backend | Kernel | sqlite calls | Worker binds | Options |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `native` | built in | none | macOS, Linux, Windows | macOS `native` (`HostProcess`); Linux `namespaces`; Windows none | macOS `nfs`; Linux `fuse` | in process; Windows `worker` | (no worker) | none |
| `docker` | built in | `docker` | macOS, Linux | `namespaces` in the worker container | `fuse` | worker | `0.0.0.0:7070` (macOS); host network on Linux | none |
| `apple-container` | created | `container` | macOS | `namespaces` | `fuse` | worker | `0.0.0.0:7070` | `cpus`, `memory`, `disk`, `image`, `os_image` |
| `lima` | created | `limactl` | macOS | `namespaces` | `fuse` | worker | `127.0.0.1:7070` | `cpus`, `memory`, `disk`, `image`, `os_image` |

`image` is the Sandbox image; `os_image` the machine's own system. With no
`os_image`, `lima` and `apple-container` use a prepared image when
`<cache>/layerfs/machine-images/<platform>/linux-<arch>/<hex>/current.json`
maps the bundle's Sandbox image to one (an incompatible or unsanitized
mapping is `Unavailable`; a missing prepared Lima file `Unsupported`, both
naming `tools/machine_image.py`), else the provider's stock image
([`bundle.rs`](../../../../control/crates/layerfs-machine/src/bundle.rs)
`prepared_os_image`).

## native (`platforms/native/`)

The host itself: one per home, never created or removed. On macOS and
Linux, Store calls run in the caller's process and there is no worker.

| Call | Answer |
| --- | --- |
| `create`, `remove` | `InvalidInput`: "is built in and never created" / "… never removed" |
| `start` | `None` on macOS and Linux; on Windows `Unsupported` (its Stores would run in a WSL2 worker, which is not designed) |
| `stop` | nothing to stop |
| `status` | `running`; `down_reason` "missing: …" when the host lacks something |
| `guest`, `copy_in`, `copy_out` | `InvalidInput`: "runs in-process; its files are the host's" |
| `reach` | the URL as given (`Loopback::Shared`) |
| `forward` | `127.0.0.1:<guest port>`, no pid |

## docker (`platforms/docker/`)

Docker's kernel (Docker Desktop's VM or the Linux host). The machine is one
privileged worker container, `spec.vm_name`, that `start` runs; its worker
runs `namespaces` Sandboxes inside it, as a VM's worker does.

```text
 start(spec, bundle)                                   docker/power.rs
   image = spec.os_image (none -> InvalidInput)
   docker inspect <vm>: ours -> docker rm -f; not ours -> Exists
   docker run -d --pull never --name <vm> --init --privileged
     --cgroupns private --label layerfs.role=machine-worker
     --label layerfs.bundle=<hash>
     macOS: -p 127.0.0.1::7070            LISTEN 0.0.0.0:7070
     Linux: --network host --add-host host.docker.internal:host-gateway
                                          LISTEN 127.0.0.1:<free port>
     -v <vm>-data:/layerfs/data -v <bundle>:/layerfs/bundle:ro
     --tmpfs /layerfs/state:mode=0700 --entrypoint sh <image> -c ENTRY
   ENTRY: mkdir /layerfs/data/{store,sandbox} 0755; wait <= 60 s for
          the secret; exec /layerfs/bundle/layerfs worker
   macOS: docker port <vm> 7070/tcp -> host port
   docker exec: write /layerfs/state/secret (0600, stdin); ready <= 60 s
```

| Call | Answer |
| --- | --- |
| `create`, `remove` | `InvalidInput` (built in) |
| `stop` | `docker stop -t 5 <vm>` if it exists; the volume stays |
| `status` | `stopped` when the Engine does not answer; else `inspect` state (`running`; `dead` is failed; else stopped), through `with_worker` |
| `guest` | `docker -H unix://<socket> exec` in the worker container |
| `load_image` | `docker load -i` (up to 300 s), `sha256:<64 hex>` |
| `reach` | Linux host: as given (host network); Docker Desktop: a loopback host becomes the address the worker resolves `host.docker.internal` to (`getent ahostsv4`) |
| `forward` | `Unsupported`: Docker publishes a container's ports only at start, so Workspace calls go through the worker |

The secret goes into a private tmpfs after start, so it is in no command
line or container config. The container is privileged in its own cgroup
namespace so its worker has root, `/dev/fuse` and a writable cgroup v2 tree
with `cgroup.kill`. It never writes `user.max_user_namespaces` (that would
change the whole Docker host); Sandbox commands rely on the seccomp
filter. `machine update docker --docker <socket>` points the platform's own
`docker` calls at another Engine; the socket is never mounted. A Sandbox
recorded under the retired docker backend is refused, asking to recreate
it ([Sandboxes: backends](../sandboxes/backends.md)).

## apple-container (`platforms/apple_container/`)

One container of Apple's `container` CLI (macOS, Apple silicon). Its main
process is a shell that sleeps and ends on SIGTERM; its data disk is a
named volume.

```text
 create(spec, bundle)                         apple_container/create.rs
   host: macOS, aarch64, `container` (else Unsupported + missing)
   container inspect <vm> / volume inspect <vm>-data: present -> Exists
   port = recorded port or a free host port
   container volume create -s <disk>M <vm>-data
   container run --detach --name <vm> --cpus N --memory <M>M
     --cap-add SYS_ADMIN --cap-add SYS_RESOURCE --cap-add NET_ADMIN
     --mount type=volume,source=<vm>-data,target=/layerfs/data
     --publish 127.0.0.1:<port>:7070 --label layerfs.role=machine
     <os_image | debian:trixie-slim> sh -c "trap 'exit 0' TERM; ..."
   bring_up over `container exec`: prepare, install, launch, ready
```

| Call | Answer |
| --- | --- |
| `start` | needs the recorded port (else `InvalidInput`) and the container (else `NotFound`); `container start` if stopped or failed; `bring_up`; records the network gateway |
| `stop` | `container stop` if running |
| `remove` | `container delete --force`, then `container volume delete` |
| `status` | `container inspect` `status.state` `running`/`stopped`, else failed |
| `reach` | a loopback host becomes the recorded gateway, else `status.networks[0].ipv4Gateway` read while it runs |
| `forward` | `Unsupported`: the CLI publishes a port only at create |

`/proc/sys` is read-only until remounted; preparation remounts it before
setting `user.max_user_namespaces=0`. A host PostgreSQL server is reached
through the gateway only when it listens on that bridge (`0.0.0.0`), not
on `127.0.0.1` alone.

## lima (`platforms/lima/`)

One Lima instance (`vmType: vz`, macOS), made from a template written to
`limactl create`'s stdin.

```text
 create(spec, bundle)                                   lima/create.rs
   limactl list <vm> / disk list --json lfs<8 hex>: present -> Exists
   limactl disk create lfs<8 hex> --size <disk>MiB --format raw
   limactl create --name <vm> --tty=false -  < template
   limactl start --tty=false <vm>
   /mnt/lima-lfs<8 hex> must be ext4; bind it at /layerfs/data
   bring_up over ssh -F <dir>/ssh.config lima-<vm> sudo -n
```

| Template key | Value |
| --- | --- |
| `vmType`, `cpus`, `memory` | `vz`, from the spec |
| `images` | `os_image` (an `https://` URL or absolute path), else Ubuntu 24.04's cloud image for `aarch64` and `x86_64` |
| `mounts` | `[]`: the guest never sees the host home |
| `additionalDisks` | the data disk, `format: true`, `fsType: ext4` (its name `lfs` + 8 hex of an FNV-1a hash fits Lima's 16-byte ext4 label) |
| `portForwards` | guest `127.0.0.1:7070` to host `127.0.0.1:<port>`; every other guest port ignored |

| Call | Answer |
| --- | --- |
| `start` | needs the recorded port (else `InvalidInput`) and the instance (else `NotFound`); `Failed` if `Broken`; boots if stopped; attach; `bring_up` |
| `stop` | `limactl stop` (up to 300 s); instance and disk stay |
| `remove` | `limactl delete --force`, then `limactl disk delete` |
| `reach` | a loopback host becomes `192.168.5.2` (the address of `host.lima.internal`; the Sandbox root has no such hosts entry) |
| `forward` | a detached `ssh -N -L` over Lima's `ssh.config`, answering its pid ([Workers and reach](workers-and-reach.md#forwards)) |

`disk` sizes the data disk only; Lima's root disk keeps Lima's own size.

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: host:root`, `host:/dev/fuse`, `host:cgroup2` | `native` on Linux without root, FUSE or cgroup v2 | [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs) `native_missing` |
| `missing: host:macos-or-linux` | `native` on Windows or another host | same |
| `missing: tool:docker`, `host:<socket>`, `host:wsl2` | no `docker` CLI; the Engine does not answer; a Windows host | [`docker/adapter.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/adapter.rs) |
| docker `start` `Exists`: "… is not a LayerFS worker; it is left alone" | a container of that name without the worker label | [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |
| docker `start` `Failed` | image not in the Engine (`--pull never`), or no published port (macOS) | same |
| `missing: host:macos`, `host:apple-silicon`, `tool:container` | not a Mac, not `aarch64`, no `container` CLI | [`apple_container/adapter.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/adapter.rs) |
| apple-container `Failed`: "… inspect answer is not the shape this adapter knows" | another CLI version's JSON | [`apple_container/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/observe.rs) |
| apple-container `reach` `Unavailable`: no gateway | the machine is stopped | same |
| `missing: host:macos`, `tool:limactl` | not a Mac, no `limactl` | [`lima/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/observe.rs) |
| lima "… is not an ext4 mount" | the data disk did not attach as ext4 | [`lima/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/power.rs) |
| `Exists`: a disk or volume "already exists; it is left alone" | left by an earlier machine of that name | `create.rs` of the platform |
| `Unsupported`: "the guest of … still allows user namespaces" | the guest refused the sysctl | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |

## Support table (`support.rs`)

```text
 sandbox_allowed(platform, host, store)
   host == Other ............................ Err(OTHER_HOST)
   host not in profile.hosts ................ Err(per-platform text)
   platform.backend(host) is None ........... Err(NO_BACKEND)
   Sqlite ................................... Ok
   Postgresql -> postgresql_refusal ......... Err(text) | Ok
```

| Cell: backend · kernel · sqlite · PostgreSQL Sandbox | macOS | Linux | Windows |
| --- | --- | --- | --- |
| `native` (built_in) | native · nfs · in_process · yes | namespaces · fuse · in_process · yes | — · — · worker · no |
| `docker` (built_in) | namespaces · fuse · worker · yes | namespaces · fuse · worker · yes | — · — · — · no |
| `apple-container` (created) | namespaces · fuse · worker · yes | — · — · — · no | — · — · — · no |
| `lima` (created) | namespaces · fuse · worker · yes | — · — · — · no | — · — · — · no |

PostgreSQL Store calls run in the caller's process on every row. The
Windows column is table data, not a built host (`#![cfg(unix)]`).

| Refusal text (word for word) | For |
| --- | --- |
| `LayerFS names no host other than macOS, Linux and Windows` | any platform on another host |
| `no Sandbox backend on Windows (O5)` | `native` on Windows |
| `docker through WSL2 is not designed yet` | `docker` on Windows |
| `apple-container runs on macOS hosts only`, `lima runs on macOS hosts only` | those off macOS |

`machine create` and `sandbox create` refuse `Unsupported` with the row's
reason; `sandbox create` wraps it as "a Sandbox of <kind> Store <s> cannot
run on machine <m> (<platform>): <reason>". `machine.list` and `get` give
each machine its row's `supports: {sqlite, postgresql_sandbox, reason}`.

### SUPPORT.json

Generated by `support_json()`; never edited by hand. One block per host
(`macos`, `linux`, `windows`), one row per platform: the rows
`machine.platforms` answers without its live `ready`, `missing`, `version`.

| Key | Value |
| --- | --- |
| `platform`, `how` | name; `built_in` or `created` |
| `backend` | stored name (`native`, `namespaces`) or `null` off-host |
| `kernel` | `fuse`, `nfs` or `null` |
| `sqlite` | `in_process`, `worker` or `null` off-host |
| `postgresql` | `{store_calls: "in_process", sandbox, reason}` |
| `options` | `[{name, kind}]`, kind `int`, `size` or `text` |
| `description` | profile description, then `; ` and the backend's description |

After changing the table, write the file and review its diff:
`LAYERFS_BLESS_SUPPORT=1 cargo +1.85.1 test --locked -p layerfs-platform --test support`.

## Removed platforms (`platform_policy.rs`)

Firecracker, Microsandbox and SSH were removed in 2.0: no variant, adapter,
profile or row, and no setting brings one back. The refusal is
``platform `<name>` was removed in LayerFS 2.0``.

| Call on a removed platform's record | Behaviour |
| --- | --- |
| `machine list`, `get` | shown, never probed: `backend: "-"`, `supports` false with the reason, `state: "unsupported"` |
| `machine remove` | forgets the record only (same `InUse` check); its note names what is left on its host (the `msb` VM and volume, or the microVM folder and disk on its Lima host) |
| every other call, `machine create <name> firecracker\|microsandbox\|ssh` | `Unsupported`, before any effect |

A Lima machine that an old Firecracker record names as its host is refused
`machine remove` (`InUse`) until that record is forgotten
([`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs),
[`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs)).

## Adding a platform

```text
 platform.rs       + Platform variant, in ALL, profile() arm
 adapter.rs        + adapter() arm
 support.rs        + host_refusal() arm (and postgresql_refusal if needed)
 platforms/<p>/    mod.rs (declarations only), profile.rs PROFILE,
                   adapter.rs ADAPTER (every method), observe.rs,
                   create.rs, power.rs
 setup/guest.rs    + Guest variant only for a new shell transport
 tests/conformance.rs  + loopback() and daemon_port() arms
 control/SUPPORT.json  blessed, never edited
```

A created platform's `create` follows the shared shape: host check, refuse
`Exists`, make the disk then the VM (a failure with nothing under the name
removes the disk; one with the VM kept is `vm_exists`), boot, attach
`/layerfs/data`, `worker::bring_up`. A new option also needs its `Spec`
mapping in [`layerfs-machine/src/spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs).
The conformance kit covers a new variant once it compiles; a new Store
kind or kernel route is an engine change, not a platform.

## Related

- [Machines](README.md): the profile and adapter contracts.
- [Workers and reach](workers-and-reach.md): workers, `reach` and forwards per platform.
- [Sandboxes: backends](../sandboxes/backends.md): `native` and `namespaces`.
- [Kernel routes](../../engine/kernel-routes/README.md): `fuse` and `nfs`.
- User guide: [machines and Stores](../../../versioned/2.0.0/machines-and-stores.md).
