# Docker machine Sandboxes

[Architecture](../../README.md) › [Control](../README.md) › [Sandboxes](README.md) › Docker machine Sandboxes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How Sandboxes run on the `docker` machine, and what became of the docker
backend.

| | |
| --- | --- |
| Component | [Sandboxes](README.md) |
| Source | [`control/crates/layerfs-platform/src/platforms/docker`](../../../../control/crates/layerfs-platform/src/platforms/docker), [`control/crates/layerfs-sandbox-backend/src/backend/namespaces`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces), [`control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs) |

There is no docker backend. Owner decision D-8 (layerfs-project#111)
retired it: the `docker` machine runs `namespaces` Sandboxes inside its
worker container, as a Linux VM's worker does. The backend's one
container per Sandbox, its Docker Engine HTTP client (`docker_engine/`)
and its per-Sandbox published daemon ports are deleted.

## The picture

```text
 caller --sandbox.host--> docker machine's worker container
                          (privileged, --cgroupns private, --init)
                            |
                            | namespaces backend, as in a VM
                            v
                          Sandbox: layerfs-daemon in its own mount, PID
                          and IPC namespaces and cgroup; root unpacked
                          from /layerfs/bundle/sandbox-image.tar
                            |
                            v
                          commands: helper exec, under the filter that
                          refuses a user namespace
```

| Fact | Value | Source |
| --- | --- | --- |
| Backend | `namespaces`. A docker-machine Sandbox shows `backend: "namespaces"` and `runtime: "namespaces"`. | [`docker/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/profile.rs) |
| Platform | `docker`, stored, shown and accepted as before. The deployment names the platform its Sandboxes keep. | [`deploy.rs`](../../../../control/crates/layerfs-sandbox/src/deploy.rs) `choose`, [`runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs) `platform` |
| Where it runs | Inside the worker container, which is root, has `/dev/fuse`, a writable cgroup v2 tree with `cgroup.kill`, and its runtime root on the `<vm>-data` volume. | [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |
| Sandbox root | The bundle's `sandbox-image.tar` in the worker (`/layerfs/bundle/sandbox-image.tar`). A `sandbox.create --image sha256:...` names a digest that archive must hold, as on a VM. | `deploy.rs` `choose` |
| Machine `image` | Only the worker container's Docker image (the record's `image`, else the bundle image loaded into Docker once, `loaded_image`, #535 P5); never a Sandbox's root. | `docker/power.rs` `start`, `load_image` |
| Route | Every Sandbox and Workspace call goes through the worker (`sandbox.host`), as on `apple-container`. There is no direct route to a docker Sandbox's daemon. | [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs), [Forwards](../network/forwards.md#platforms-that-refuse) |
| Forward | `Unsupported`: Docker publishes a container's ports only when it starts, so a daemon's port in the worker container cannot be published later. | [`docker/adapter.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/adapter.rs) |
| Loopback PostgreSQL | On Docker Desktop `reach` rewrites a loopback host to the address the running worker resolves `host.docker.internal` to (the Sandbox image's root cannot resolve the name); on a Linux Engine the URL is dialled as given, the network being the host's. | `docker/adapter.rs`, [Reach](../network/reach.md) |

## User namespaces

A namespaces Sandbox needs that no command can create a user namespace: one
could hold its own copy of a Workspace mount, outside the daemon's unmount.
A VM guest blocks them machine-wide with `user.max_user_namespaces=0`. The
docker worker never writes that sysctl, since it would change the whole
Docker host or Docker Desktop VM. A filter on each command does it instead.

| Rule | Source |
| --- | --- |
| `layerfs-sandbox-helper exec` installs a seccomp filter just before exec, after setting no-new-privs. | [`seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs), [`bin/…/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/exec.rs) |
| `unshare` and `clone` with `CLONE_NEWUSER` answer `EPERM`; `clone3` answers `ENOSYS` (the C library falls back to `clone`); every other call is allowed. | same |
| A foreign ABI (i386 on x86_64, arm on aarch64) is filtered by its own numbers, x32 by the native ones; an unknown ABI is killed. | same |
| If the filter cannot be installed, the command fails. No command runs without it, and no weaker Sandbox is given. | same |
| The filter applies to every namespaces Sandbox command, in a VM too. | same |

The namespaces host check accepts either `user.max_user_namespaces=0` or a
kernel whose `/proc/sys/kernel/seccomp/actions_avail` lists `allow`,
`errno` and `kill_process`
([`namespaces/host.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs)).

## Sandboxes recorded under the docker backend

A record whose `backend` or `runtime` is `"docker"`, or a docker-machine
record with no `runtime`, was written by the retired backend.

| Call | Answer |
| --- | --- |
| `sandbox.close` | Forgets it: removes the record and answers the container's name, for the user to remove by hand (`docker rm <name>`). |
| Any other call | `Unsupported`, with a message to recreate the Sandbox. |
| Home migration of a layout-2 docker Sandbox | Reported as unable to be checked: there is no Engine client to probe its container. |

## Related

- [Machines: Docker](../machines/platforms/docker.md): the worker container.
- [Native and namespaces](native-and-namespaces.md): the namespaces lifecycle the worker runs.
- [Backends](backends.md): the two backends left.
- [Network: Forwards](../network/forwards.md) and [Network: Reach](../network/reach.md).

---

← [Sandboxes: Backends](backends.md) · [Up: Sandboxes](README.md) · [Sandboxes: Native and namespaces](native-and-namespaces.md) →
