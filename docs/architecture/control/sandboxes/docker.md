# Docker

[Architecture](../../README.md) › [Control](../README.md) › [Sandboxes](README.md) › Docker

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Docker backend and its Engine client.

| | |
| --- | --- |
| Component | [Sandboxes](README.md) |
| Source | [`control/crates/layerfs-sandbox-backend/src/docker_engine`](../../../../control/crates/layerfs-sandbox-backend/src/docker_engine), [`control/crates/layerfs-sandbox-backend/src/backend/docker`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker) |

One container per Sandbox. `DockerProvider::launch` creates it, uploads
the daemon and its setup, starts it and waits for its listener line;
`docker::Sandbox` owns it until one stop and one delete.

## Docker Engine client

```text
 Docker { socket, control_wait, deadline, container_name, kept, minor }
   |
   | first request: GET /version (unversioned, once) -> minor in 48..=54
   v
 one exchange = one connection attempt on the Engine's Unix socket
   Mode::Close    metadata (create, inspect, stop, delete, logs, archive)
   Mode::Keep     Exec create and inspect, on one kept idle connection
   Mode::Upgrade  Exec start: the hijacked duplex stream
        |
        v
 body.rs  Fixed(n) | Chunked | Close | Hijacked   exact framing
 mux.rs   8-byte header: [stream 0..3, 0, 0, 0, length u32 BE]
```

| Part | What | Source |
| --- | --- | --- |
| `Docker::new` | An absolute socket path and a non-zero control wait; nothing is dialed. | [`http.rs`](../../../../control/crates/layerfs-sandbox-backend/src/docker_engine/http.rs) |
| Version | API 1.48 (Engine 28.0) to 1.54. The daemon's version, capped at 1.54, read once and shared by clones; older is refused before the request; an unreadable answer uses 1.54. | [`version.rs`](../../../../control/crates/layerfs-sandbox-backend/src/docker_engine/version.rs) |
| Socket | One nonblocking connect; readiness waits use `poll`. Control waits never apply to command streams. | [`socket.rs`](../../../../control/crates/layerfs-sandbox-backend/src/docker_engine/socket.rs) |
| Kept connection | One idle keep-alive connection for Exec create and inspect, used only while idle (30 s, `KEPT_IDLE`) and quiet; returned only after its body was read to the framed end. A failure on it is that request's failure, never resent. | [`http.rs`](../../../../control/crates/layerfs-sandbox-backend/src/docker_engine/http.rs) |
| Failures | `WireFailure` keeps whether the request was attempted, header and body bytes sent, the HTTP status, the requested and observed container and exec ids, and a bounded error-body prefix. | [`types/failure.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/failure.rs) |

`WireFailure::container_stopped` and `container_gone` let the caller tell
"the exact container exists but is not running" from "the Engine says no
such container" without guessing.

## Containers

```text
 DockerProvider::launch(LaunchRequest)                   provider.rs
   Validate  options are Docker; image sha256:<64 hex>; port != 0;
             setup 1..=8192 bytes; startup wait > 0; open the executable
   Create    POST /containers/create[?name=<name>]       create.rs
   (upload)  PUT /containers/<id>/archive  tar:          upload.rs
               layerfs-local/ config/ overlay/  0700
               workspaces/                      0755
               layerfs-local/config/daemon.setup  0600
               usr/local/bin/layerfs-daemon       0755
   Start     POST /containers/<id>/start                 create.rs
   Listener  GET /containers/<id>/logs?follow=true  until
             "LAYERFS_DAEMON_LISTEN <addr>"              listener.rs
 any failure after Create: LaunchFailure { phase, instance: Some(owner) }
```

The container create body (`SandboxRequest::encode` in
[`create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/create.rs)):

| Field | Value |
| --- | --- |
| `Image` | the immutable `sha256:` digest |
| `User`, `Entrypoint`, `Cmd` | `0:0`, `/usr/local/bin/layerfs-daemon`, `--config /layerfs-local/config/daemon.setup` |
| `HostConfig.Init` | true: the Engine's init is PID 1 and reaps what commands leave |
| `RestartPolicy`, `LogConfig` | `no`; `json-file`, 10m × 2 |
| Privilege | not privileged; `CapAdd: CAP_SYS_ADMIN`; device `/dev/fuse`; `no-new-privileges=true`, `apparmor=unconfined` |
| `SecurityOpt` seccomp | allow all but user-namespace creation (`unshare`/`clone` with `CLONE_NEWUSER` give `EPERM`; `clone3` gives `ENOSYS`) ([`topology.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/topology.rs)) |
| `ExtraHosts` (Linux only) | `host.docker.internal:host-gateway`, for [Reach](../network/reach.md) |
| `Mounts` | the Store at `/layerfs-store`: a volume (`NoCopy`), a volume folder (`Subpath`) or a host folder (bind); none for a PostgreSQL Store |
| `PortBindings` | `<port>/tcp` on `127.0.0.1`, Engine-chosen host port |

The Store's path inside is `/layerfs-store/global/store.sqlite` for a whole
volume, else `/layerfs-store/store.sqlite` (`store_locator` in
[`sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/sandbox.rs)).
The Store volume is borrowed: `delete` sends `DELETE
/containers/<id>?force=false&v=false` and never removes it.

`endpoint` inspects the container and answers the published loopback
address ([`endpoint.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/endpoint.rs)).
That address is why `machine::forward` refuses on `docker`
([Forwards](../network/forwards.md#platforms-that-refuse)). `reopen` starts
nothing: it checks the container's user, entrypoint, command, `Tty`,
labels, privilege, security options, capabilities, devices, init, restart
and log settings, and its Store mount against the record
([`topology.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/topology.rs)).

## Labels and naming

| Item | Rule | Source |
| --- | --- | --- |
| Container name | `<prefix><id>`; prefix `deploy.containerPrefix`, else `layerfs-`. Engine rule: an ASCII letter or digit, then letters, digits, `_`, `.`, `-`, at most 128. A name another container holds fails the create; it is never replaced or adopted. | `sandbox_name` in [`serve.rs`](../../../../control/crates/layerfs-sandbox/src/serve.rs), `naming` in [`http.rs`](../../../../control/crates/layerfs-sandbox-backend/src/docker_engine/http.rs) |
| Labels set by the API | `layerfs.role=sandbox`, `layerfs.project=<project>`, `layerfs.sandbox=<id>`, and `layerfs.store=<store>` for a shared Store. | `labels` in [`serve.rs`](../../../../control/crates/layerfs-sandbox/src/serve.rs) |
| Label text | Each key once; key and value 1 to 128 of `[A-Za-z0-9_.-]`. | `SandboxRequest::check` in [`sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/sandbox.rs) |
| Reopen | The container must carry every recorded label with its value; the image may add others. | `config` in [`topology.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/topology.rs) |
| Volume name | 1 to 128, alphanumeric first, then `[A-Za-z0-9_.-]`. | `volume_name` in [`sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/docker/sandbox.rs) |

The machine's own worker container is labelled `layerfs.role=machine-worker`
([`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs)),
so the two kinds of container are told apart.

## Related

- [Commands and streams](commands-and-streams.md): Exec create, start and inspect on this client.
- [Backends](backends.md): where Docker sits among the backends.
- [Machines: Docker](../machines/platforms/docker.md): the docker machine and its worker container.
- [Network: Credentials](../network/credentials.md): the private setup file.

---

← [Sandboxes: Backends](backends.md) · [Up: Sandboxes](README.md) · [Sandboxes: Native and namespaces](native-and-namespaces.md) →
