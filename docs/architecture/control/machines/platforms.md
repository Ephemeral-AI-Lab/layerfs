# Platforms

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The four machine platforms, the one support table that decides where each
may run, and how to add one, within [Machines](README.md).

| | |
| --- | --- |
| Source | [`platforms/`](../../../../control/crates/layerfs-platform/src/platforms), [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs), [`control/SUPPORT.json`](../../../../control/SUPPORT.json), [`removed_platforms.rs`](../../../../control/crates/layerfs-sandbox-backend/src/support/removed_platforms.rs), [`docker_socket.rs`](../../../../control/crates/layerfs-platform/src/docker_socket.rs) |

## Profiles (`platforms/<p>/profile.rs`)

| Platform | How | Tool | Hosts | Backend | Kernel | sqlite calls | Worker binds | Options |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `native` | built in | none | macOS, Linux, Windows | macOS `native` (`Native`); Linux `namespaces`; Windows none | macOS `nfs`; Linux `fuse` | in process; Windows `worker` | (no worker) | none |
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
`prepared_os_image`). `os_image: "stock"` takes the stock image even when a
prepared one exists and records none
([`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) `STOCK`).
`home.get --with platforms` gives each size option its `default` (`DEFAULT_CPUS`,
`DEFAULT_MEMORY`, `DEFAULT_DISK` in `layerfs-machine` `spec.rs`).

## Tools and the docker socket

| What | Rule | Source |
| --- | --- | --- |
| Tool lookup | `Tools.path` is this process's `PATH`, then the fallback folders not already on it: on macOS `/opt/homebrew/bin`, `/usr/local/bin`, `/Applications/Docker.app/Contents/Resources/bin` (an app started from the Dock has a short `PATH`), none elsewhere; `LAYERFS_TOOL_FALLBACKS` (a `PATH`-style list, empty for none) replaces them, which tests use. Every provider command runs with `Tools.path` as its `PATH`. The Sandbox code's own Docker reads find `docker` the same way (`find_tool`) | [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs) `Tools::from_env`, `tool_fallbacks`; [`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) `command`, `find_tool` |
| Socket candidates | read from files and variables only: `DOCKER_HOST`, the current context (`DOCKER_CONTEXT`, else `currentContext` in `config.json` under `DOCKER_CONFIG` or `~/.docker`) and the other contexts' `contexts/meta/*/meta.json`, Docker Desktop's `~/.docker/run/docker.sock`, colima's `~/.colima/default/docker.sock` and OrbStack's `~/.orbstack/run/docker.sock` when they exist, and `/var/run/docker.sock`; deduplicated by path; a non-`unix://` endpoint is listed `usable: false`, never followed | [`docker_socket.rs`](../../../../control/crates/layerfs-platform/src/docker_socket.rs) `docker_candidates` |
| Automatic choice | `DOCKER_HOST` when `unix://`, else the current context's endpoint when `unix://`, else the first of Docker Desktop's, colima's and OrbStack's sockets that exists, else `/var/run/docker.sock` | same, `docker_automatic` |
| The docker machine's socket | its record's `docker` (`machine update docker --docker`, or written by its first start: under the home lock once the socket answers a ping, before an image is loaded or anything is made in that Engine, and again once the provider started, so it never moves to another Engine, where its data volume and loaded image are not; a start whose Engine does not answer pins nothing); else the automatic choice (`env:DOCKER_HOST`, `context:<name>`, `desktop`, `colima`, `orbstack` or `default`); no older record is given a fixed socket. `--docker ""` writes `docker: null`: automatic again until the next start records one. Changing it is refused `InUse` while the machine's worker is recorded running. The machine's adapter and the Sandbox code's Docker reads (`machine.images`, image copies) use this one choice | [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) `docker_socket`, [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) `record_socket`, [`docker.rs`](../../../../control/crates/layerfs-sandbox/src/docker.rs) `Docker::of` |
| Socket state | one connect (non-blocking on Linux, where a full backlog would hold a blocking one) and `GET /_ping` (2 s per read and write): `answering`, `absent` (no such file), `denied` (`EACCES`) or `silent`. The docker probe runs `docker version` only for an answering socket; `home.get --with platforms` pings again for `socket.state` only when the probe found no version. The automatic choice is named as the candidates name its socket | `docker_socket.rs` `socket_state`, [`docker/adapter.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/adapter.rs) |

## native (`platforms/native/`)

Sandboxes on the host's own OS, with no VM: one per home, never created or
removed. `native` is a machine, not the host: this computer is the control
process (the home, records, secrets and the near end of a transfer), and
nothing about it is written as `native`. What is special about `native`
comes from its platform on the host, never from its name: its executor is
`Local` on macOS and Linux (Store calls in the caller's process, no worker)
and `Worker` on Windows; its loopback is shared (`Loopback::Shared`); no
provider holds it (`Platform::provided` is false: no tool, no `vm_name`,
never stopped or missing, always `ready`). Behaviour asks those facts
(`layerfs_machine::runs_here`, `in_process`, `provided`); the name maps to
the platform only in [`local.rs`](../../../../control/crates/layerfs-machine/src/local.rs)
and the built-in names. On a host where no machine's calls run in this
process (Windows), the rules that need one (a sqlite Store's default
machine, `store move`, `backup restore`) refuse `Unsupported` and choose no
other machine.

| Call | Answer |
| --- | --- |
| `create`, `remove` | `InvalidInput`: "is built in and never created" / "… never removed" |
| `start` | `None` on macOS and Linux; on Windows `Unsupported` (its Stores would run in a WSL2 worker, which is not designed) |
| `stop` | nothing to stop |
| `status` | `running`; `Unsupported` with `missing` (`host:root`, `host:/dev/fuse`, `host:cgroup2`, `host:macos-or-linux`) when the host lacks what its Sandboxes need, as `provider_within` answers, so it never reads `ready` |
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
     --log-driver local --log-opt max-size=16m --log-opt max-file=2
     macOS: -p 127.0.0.1::7070            LISTEN 0.0.0.0:7070
     Linux: --network host --add-host host.docker.internal:host-gateway
                                          LISTEN 127.0.0.1:<free port>
     -v <vm>-data:/layerfs/data -v <bundle>:/layerfs/bundle:ro
     --tmpfs /layerfs/state:mode=0700
     -e LAYERFS_WORKER_LOG=/layerfs/data/log/worker.log
     --entrypoint sh <image> -c ENTRY
   ENTRY: mkdir /layerfs/data/{store,sandbox} 0755; macOS only: echo 1 >
          /proc/sys/net/ipv4/ip_forward (the container's own namespace; a
          Linux Engine's host network is never written); the log's folder and
          its one rotation; CGROUP_LEAF (every process of the namespace's
          cgroup root into /sys/fs/cgroup/layerfs-worker, then
          "+memory +pids" to the root's cgroup.subtree_control);
          echo $$ > /layerfs/state/worker.pid; wait
          <= 60 s for the secret;
          exec /layerfs/bundle/layerfs worker >> "$LAYERFS_WORKER_LOG" 2>&1
   macOS: docker port <vm> 7070/tcp -> host port
   docker exec: write /layerfs/state/secret (0600, stdin); ready <= 60 s
   a worker that ended (container exited): docker cp its worker.log out
```

The worker's output goes to `/layerfs/data/log/worker.log` on the data
volume, not to `docker logs`
([the worker's log](workers-and-reach.md#the-workers-log)).

| Call | Answer |
| --- | --- |
| `create`, `remove` | `InvalidInput` (built in) |
| `stop` | `docker stop -t 5 <vm>` (forced: `-t 0`) while it runs, restarts or is paused; the volume stays |
| `status` | `stopped` when the Engine does not answer; else `inspect` state (`running`; `dead` is failed; else stopped), through `with_worker` |
| `provider_within` | `inspect` state alone: `running`; `created`, `exited` stopped; `dead` failed; paused, restarting, removing or unknown `Unavailable` |
| a container named `<vm>` without the `layerfs.role=machine-worker` label | `Exists` from `start`, `stop` and `provider_within` alike ("… is not a LayerFS worker; it is left alone"); it is never touched |
| `guest` | `docker -H unix://<socket> exec` in the worker container |
| `load_image` | `docker load -i` (up to 300 s), `sha256:<64 hex>` |
| `reach` | Linux host: `Shared` (host network), every URL as given; Docker Desktop: `Apart`, a loopback host or Unix socket refused naming `host.docker.internal`, which the worker and its Sandboxes resolve (Docker Desktop's DNS); nothing is rewritten and no `docker` command runs ([Reach](workers-and-reach.md#reach-reachrs)) |
| `forward` | `Unsupported`: Docker publishes a container's ports only at start, so Workspace calls go through the worker |

The secret goes into a private tmpfs after start, so it is in no command
line or container config. The container is privileged in its own cgroup
namespace so its worker has root, `/dev/fuse` and a writable cgroup v2 tree
with `cgroup.kill`. Its cgroup root holds processes, and a cgroup that
holds processes cannot give its children controllers (only the real root is
exempt), so the entry moves them to a leaf first and the root then gives
Sandbox groups memory and pids ([`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs)
`CGROUP_LEAF`, layerfs-project#182 L4); `docker exec` then joins the leaf.
The VM launcher runs the same lines, which act only at a namespace root (an
Apple Container guest), never at a VM's real root (Lima). It never writes `user.max_user_namespaces` (that would
change the whole Docker host); Sandbox commands rely on the seccomp
filter. `machine update docker --docker <socket>` points the platform's own
`docker` calls at another Engine; the socket is never mounted (how the
socket is chosen: [Tools and the docker socket](#tools-and-the-docker-socket)). A Sandbox
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
| `start` | needs the recorded port (else `InvalidInput`) and the container (else `NotFound`); `container start` if stopped, and once if failed (the CLI's answer decides; `failed`'s fix is `machine start`); `bring_up`; records the network gateway |
| `stop` | `container stop` (5 s grace) if running; forced: `container stop --time 0`, also when failed or between states |
| `remove` | `container delete --force`, then `container volume delete` |
| `status` | `container inspect` `status.state`: `running`, `stopped`, `failed`; any other (`stopping`, `unknown`) is `Unavailable`, as lima's and docker's passing states, never recorded `failed` |
| `reach` | `Apart`: a loopback host or Unix socket refused naming the gateway the last start recorded (else vmnet's default `192.168.64.1`), on which the server must listen; the guest has no name for this host; no `container` command runs |
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
   limactl create --name <vm> --tty=false -  < template   (up to 120 s)
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
| `start` | needs the recorded port (else `InvalidInput`) and the instance (else `NotFound`); `Failed` if `Broken`, before any call: Lima says `Broken` when it cannot read the instance's own files, and `limactl start` refuses such an instance itself ("errors inspecting instance"), so unlike apple-container's `failed` it is not tried; boots if stopped; attach; `bring_up` |
| `stop` | `limactl stop` (up to 300 s) if running; forced: `limactl stop --force`, also when `Broken`; instance and disk stay |
| `status` | `limactl list` `Running`, `Stopped`, `Broken` (failed); any other is `Unavailable` |
| `remove` | `limactl delete --force`, then `limactl disk delete` |
| `reach` | `Apart`: a loopback host or Unix socket refused naming `host.lima.internal` (the guest's hosts entry for `192.168.5.2`, which its Sandboxes read through the bound resolver files) |
| `forward` | a detached `ssh -N -L` over Lima's `ssh.config`, answering its pid ([Workers and reach](workers-and-reach.md#forwards)) |

`disk` sizes the data disk only; Lima's root disk keeps Lima's own size.

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: host:root`, `host:/dev/fuse`, `host:cgroup2` | `native` on Linux without root, FUSE or cgroup v2 | [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs) `native_missing` |
| `missing: host:macos-or-linux` | `native` on Windows or another host | same |
| `Unsupported` from a `native` machine's `status`, with `missing` | the host lacks what its Sandboxes need (same facts as above) | [`native/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/observe.rs) |
| `missing: tool:docker`, `host:<socket>`, `host:wsl2` | no `docker` CLI on `PATH` or in the fallback folders; the socket does not answer a ping (its `state` says why); a Windows host | [`docker/adapter.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/adapter.rs) |
| `missing: bundle:<dir>` | a platform that runs a worker, and this build's sealed Linux bundle is absent there or of another worker protocol | [`platforms.rs`](../../../../control/crates/layerfs-machine/src/platforms.rs) |
| docker `start`, `stop` or provider check `Exists`: "… is not a LayerFS worker; it is left alone" | a container of that name without the worker label | [`docker/unix.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/unix.rs) `not_ours` |
| docker `start` `Failed` | image not in the Engine (`--pull never`), or no published port (macOS) | same |
| `missing: host:macos`, `host:apple-silicon`, `tool:container` | not a Mac, not `aarch64`, no `container` CLI | [`apple_container/adapter.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/adapter.rs) |
| apple-container `Failed`: "… inspect answer is not the shape this adapter knows" | another CLI version's JSON | [`apple_container/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/observe.rs) |
| `reach` `Unsupported`: "… its URL names this host's loopback (…), which the machine does not share; connect the Store again with an address the machine reaches (…): `layerfs store connect <store> <url>`" (or "… names no host (a Unix socket of the caller) …") | a loopback or Unix-socket PostgreSQL URL on docker (Docker Desktop), lima or apple-container | [`reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs) |
| apple-container `Unavailable`: "… a transitional or unrecognized state" | the container is `stopping` (or `unknown`); ask again once it settles, or force the stop | same; [`apple_container/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/power.rs) |
| `missing: host:macos`, `tool:limactl` | not a Mac, no `limactl` | [`lima/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/observe.rs) |
| lima "… is not an ext4 mount" | the data disk did not attach as ext4 | [`lima/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/power.rs) |
| lima `start` `Failed`: "… is broken: Lima cannot read its files" | Lima reports the instance `Broken`; `limactl list <vm>` says why; a start cannot repair it | same |
| `Exists`: a disk or volume "already exists; it is left alone" | left by an earlier machine of that name | `create.rs` of the platform |
| `Unsupported`: "the guest of … still allows user namespaces" | the guest refused the sysctl | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| `Unknown`: "… did not end within N s and was stopped", or "… ended, but its output was still held open when its time ran out" | a provider command passed its bound (30 s call, 120 s create, 300 s boot), or left a process holding its output past it; its whole process group was killed, the outcome is not known and nothing is retried | [`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) `run` |

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
| `no Sandbox backend on Windows` | `native` on Windows |
| `docker through WSL2 is not designed yet` | `docker` on Windows |
| `apple-container runs on macOS hosts only`, `lima runs on macOS hosts only` | those off macOS |

`machine create` and `sandbox create` refuse `Unsupported` with the row's
reason; `sandbox create` wraps it as "a Sandbox of <kind> Store <s> cannot
run on machine <m> (<platform>): <reason>". `machine.list` and `get` give
each machine its row's `supports: {sqlite, postgresql_sandbox, reason, log}`;
`log` is true where the row's sqlite call site is a worker and the table
refuses the platform nothing on the host (a worker this build starts, so a
worker log exists), false on every `native`.

### SUPPORT.json

Generated by `support_json()`; never edited by hand. One block per host
(`macos`, `linux`, `windows`), one row per platform: the rows
`home.get --with platforms` answers without its live `ready`, `missing`, `version`.

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

## Removed platforms (`removed_platforms.rs`)

Firecracker, Microsandbox and SSH were removed in 2.0: no variant, adapter,
profile or row, and no setting brings one back. The refusal is
``platform `<name>` was removed in LayerFS 2.0``.

| Call on a removed platform's record | Behaviour |
| --- | --- |
| `machine list`, `get` | shown, never probed: `backend: "-"`, `supports` false with the reason, `state: "unsupported"` |
| `machine remove` | removes the record only (same `InUse` check), answering `{ok: true}`; the long form's `message` says "only its record was removed (<reason>); nothing on its host was touched" |
| every other call, `machine create <name> firecracker\|microsandbox\|ssh` | `Unsupported`, before any effect |

A Lima machine that an old Firecracker record names as its host is refused
`machine remove` (`InUse`) until that record is removed
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
