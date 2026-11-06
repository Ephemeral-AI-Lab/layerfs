# Docker

[Architecture](../../../README.md) › [Control](../../README.md) › [Machines](../README.md) › Docker

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Docker platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](../README.md) |
| Source | [`control/crates/layerfs-platform/src/platforms/docker`](../../../../../control/crates/layerfs-platform/src/platforms/docker) |

## How it is made

`docker` is built in: Docker's kernel (Docker Desktop's VM, or the Linux
host). Nothing is created or removed. On macOS and Linux the machine is
one privileged worker container, `spec.vm_name`, that `start` runs
([`docker/power.rs`](../../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs)).
Its worker runs `namespaces` Sandboxes inside it, as a Linux VM's worker
does (D-8, [Sandboxes: Docker machine Sandboxes](../../sandboxes/docker.md)):

```text
 start(spec, bundle)
   image = spec.os_image                       none -> InvalidInput
   docker inspect <vm>   ours -> docker rm -f   not ours -> Exists
   docker run -d --pull never --name <vm> --init
     --privileged --cgroupns private
     --label layerfs.role=machine-worker --label layerfs.bundle=<hash>
     macOS: -p 127.0.0.1::7070           LISTEN=0.0.0.0:7070
     Linux: --network host               LISTEN=127.0.0.1:<free port>
            --add-host host.docker.internal:host-gateway
     -v <vm>-data:/layerfs/data    -v <bundle>:/layerfs/bundle:ro
     --tmpfs /layerfs/state:mode=0700
     --entrypoint sh <image> -c ENTRY
   ENTRY: mkdir /layerfs/data/{store,sandbox} (0755); wait up to 60 s
          for a secret; exec /layerfs/bundle/layerfs worker
   macOS: docker port <vm> 7070/tcp  -> the published host port
   docker exec: write /layerfs/state/secret (0600, from stdin)
   worker::ready (up to 60 s)  -> Running { endpoint, lasting }
```

| Call | Answer | Source |
| --- | --- | --- |
| `create`, `remove` | `InvalidInput`: "is built in and never created" / "… never removed" | [`docker/adapter.rs`](../../../../../control/crates/layerfs-platform/src/platforms/docker/adapter.rs) |
| `start` | a fresh worker container, as above; the Store volume stays | `docker/power.rs` |
| `stop` | `docker stop -t 5 <vm>` if it exists; Docker and the volume stay | `docker/power.rs` |
| `status` | `stopped` when the Engine does not answer; else `inspect` state (`running`, `dead` → failed, else stopped) through `with_worker` | [`docker/unix.rs`](../../../../../control/crates/layerfs-platform/src/platforms/docker/unix.rs) |
| `guest` | `docker -H unix://<socket> exec` in the worker container | `docker/unix.rs` |
| `load_image` | `docker load -i <archive>` (up to 300 s), answers `sha256:<64 hex>`: the worker container's image when the record names none (#535 P5) | `docker/power.rs` |
| `forward` | `Unsupported`: Docker publishes a container's ports only when it starts, so a Sandbox daemon's port in the worker container cannot be published later; its Workspace calls go through the worker | `docker/adapter.rs` |

The secret is written after the container starts, into a private tmpfs, so
it is in no command line and no container config. The container mounts
only the `<vm>-data` volume and the bundle (read-only): never the host home
and never the Docker Engine socket.

The container is privileged in a cgroup namespace of its own, so its worker
has what a namespaces Sandbox needs: root, `/dev/fuse`, a writable cgroup
v2 tree with `cgroup.kill`, and a runtime root on the data volume that can
hold an overlay's upper layer. Its entry script prepares the data volume
as a VM guest's `worker::prepare` does, but never writes
`user.max_user_namespaces`: that would change the whole Docker host or
Docker Desktop VM. Every Sandbox command runs under the seccomp filter that
refuses a user namespace instead
([Sandboxes: Native and namespaces](../../sandboxes/native-and-namespaces.md#no-user-namespaces)).

On a host that is neither macOS nor Linux,
[`docker/windows.rs`](../../../../../control/crates/layerfs-platform/src/platforms/docker/windows.rs)
answers every call `Unsupported`, `missing: host:wsl2-worker`; since the
crate is `#![cfg(unix)]`, that file builds only on other Unix hosts.

## Backend and kernel route

| Host | `backend` | `kernel` | `description` |
| --- | --- | --- | --- |
| macos | `namespaces` | `fuse` | Docker's kernel (Docker Desktop's VM, or the Linux host); Sandboxes run in Linux namespaces and a cgroup |
| linux | `namespaces` | `fuse` | same |
| windows | `null` | `null` | Docker's kernel (Docker Desktop's VM, or the Linux host) |

`docker` runs the `namespaces` backend in its worker container (owner
decision D-8, layerfs-project#111, which replaced #535 D8's own docker
backend). Its Sandboxes keep `platform: "docker"` and show `backend:
"namespaces"`. A Sandbox's root is the bundle's `sandbox-image.tar`
(`/layerfs/bundle/sandbox-image.tar` in the worker); the machine's
`image` is only the worker container's Docker image.

## Where Store calls run

| Host | `sqlite` | `postgresql.store_calls` |
| --- | --- | --- |
| macos | `worker` | `in_process` |
| linux | `worker` | `in_process` |
| windows | `null` | `in_process` |

A sqlite Store on `docker` lives in the volume `<vm>-data` and its calls go
to the worker container on every host (#535 G8).

## PostgreSQL reach

| Host | `postgresql.sandbox` | `postgresql.reason` |
| --- | --- | --- |
| macos | `true` | `null` |
| linux | `true` | `null` |
| windows | `false` | "docker through WSL2 is not designed yet" |

`reach` rewrites a loopback host to `host.docker.internal` (`DESKTOP_HOST`
in [`docker/adapter.rs`](../../../../../control/crates/layerfs-platform/src/platforms/docker/adapter.rs)).
On Linux the worker resolves that name through `--add-host … host-gateway`. Other hosts are dialled as given;
see [Network: Reach](../../network/reach.md) for the refusals.

## Options

None: `PROFILE.options` is empty. The worker's image is `Spec::os_image`,
and the Engine socket is `Tools::docker_socket`
(default `/var/run/docker.sock`); `layerfs-control` fills both from the
machine's record. `machine update docker --docker <socket>` points the
platform's own `docker` CLI calls (`run`, `inspect`, `exec`, `load`) at
that Engine; the socket is never mounted into the worker.

## Live receipts

These receipts predate D-8: they ran the retired docker backend. The
docker machine running `namespaces` has not been verified live yet.

| Receipt | Host | What ran |
| --- | --- | --- |
| [K1](../../../../../core/docs/issues/535/receipts/fe6f85013b/K1/RESULTS.md) | macOS, Docker Desktop | first terminal pass |
| [L1](../../../../../core/docs/issues/535/receipts/35ed671b6c/L1/RESULTS.md) | macOS, Docker Desktop | final pass: sqlite, PostgreSQL loopback and remote cells |
| [K3](../../../../../core/docs/issues/535/receipts/fe6f85013b/K3/RESULTS.md) | Linux (Lima VM) | first pass; Linux docker failed (D-1, fixed by `--network host`) |
| [L3](../../../../../core/docs/issues/535/receipts/35ed671b6c/L3/RESULTS.md) | Linux (Lima VM) | final pass; Linux `machine start docker` failed (D-3) |
| [R3](../../../../../core/docs/issues/535/receipts/3e257b91c3/R3/RESULTS.md) | Linux (Lima VM) | rerun after the D-3 fix: Linux docker cells |
| [TERMINAL-PASS](../../../../../core/docs/issues/535/TERMINAL-PASS.md) | both | the merged verdict |

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: tool:docker` | no `docker` CLI on the search path | `docker/adapter.rs` probe |
| `missing: host:<socket>` | the Engine does not answer on `Tools::docker_socket` | same |
| `missing: host:wsl2` | a Windows host | same |
| `status` `stopped`, "the Docker Engine at … does not answer" | Engine down | `docker/unix.rs` |
| `start` `Exists`: "the container … is not a LayerFS worker; it is left alone" | a container of that name without `layerfs.role=machine-worker` | `docker/power.rs` |
| `start` `InvalidInput`: "the docker machine's worker container needs an image" | no `os_image` | same |
| `start` `Failed`: "starting the worker container failed …" | e.g. the image is not in the Engine (`--pull never`) | same |
| `start` `Failed`: "the worker … publishes no port" | `docker port` gave no port (macOS) | same |
| `Failed`: "docker load named no image id" | `docker load` output had no `sha256:` id | same |
| `sandbox.create` `Unsupported`: "user namespaces are not blocked: …" | Docker's kernel neither has `user.max_user_namespaces=0` nor offers the seccomp filter's actions | [`namespaces/host.rs`](../../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs) |
| A call on an existing Sandbox is `Unsupported`, asking to recreate it | it was recorded under the retired docker backend; `sandbox.close` forgets it and names its container for `docker rm <name>` | [Sandboxes: Docker machine Sandboxes](../../sandboxes/docker.md#sandboxes-recorded-under-the-docker-backend) |

## Related

- [Sandboxes: Docker machine Sandboxes](../../sandboxes/docker.md): the namespaces Sandboxes in the worker container, and old docker-backend records.
- [Network: Workers](../../network/workers.md), [Network: Reach](../../network/reach.md), [Network: Forwards](../../network/forwards.md).
- [Kernel routes: FUSE](../../../engine/kernel-routes/fuse.md).
- [Support matrix: Table](../../support-matrix/table.md).

---

← [Machines: Native](native.md) · [Up: Machines](../README.md) · [Machines: Apple container](apple-container.md) →
