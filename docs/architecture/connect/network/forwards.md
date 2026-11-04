# Forwards

[Architecture](../../README.md) › [Connect](../README.md) › [Network](README.md) › Forwards

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Port forwards that let Workspace calls skip the worker.

| | |
| --- | --- |
| Component | [Network](README.md) |
| Source | [`connect/crates/layerfs-machine/src/setup/forward.rs`](../../../../connect/crates/layerfs-machine/src/setup/forward.rs), `forward` and `forward_close` in [`setup/calls.rs`](../../../../connect/crates/layerfs-machine/src/setup/calls.rs), `Forward` in [`spec.rs`](../../../../connect/crates/layerfs-machine/src/spec.rs) |

A Sandbox daemon listens on `127.0.0.1:<guest_port>` inside its machine.
`machine::forward(spec, guest_port)` answers the host loopback address at
which this process reaches it (#535 G4). Workspace calls then dial the
daemon directly instead of going through the worker.

## forward

```text
 host                                       machine guest
+------------------------------+          +---------------------------+
| Workspace call               |          |                           |
|   dials 127.0.0.1:<free>     |          |  Sandbox daemon           |
|            |                 |          |   127.0.0.1:<guest_port>  |
|            v                 |  ssh     |            ^              |
| ssh -N (detached, pid kept)  |=========>|------------+              |
+------------------------------+          +---------------------------+

 ssh -N -o ExitOnForwardFailure=yes <options>
     -L 127.0.0.1:<free>:127.0.0.1:<guest_port> <dest>
```

| Platform | `forward` answers | `pid` | Source |
| --- | --- | --- | --- |
| `native` | `127.0.0.1:<guest_port>`: the daemon is on this host's loopback | `None` | `shared` in [`forward.rs`](../../../../connect/crates/layerfs-machine/src/setup/forward.rs) |
| `ssh` | a free host port through `ssh <options> <dest>` | the `ssh -L` | `daemon_forward` in [`ssh/observe.rs`](../../../../connect/crates/layerfs-machine/src/platforms/ssh/observe.rs) |
| `lima` | a free host port through Lima's own `ssh.config` (its template forwards no other guest port) | the `ssh -L` | `daemon_forward` in [`lima/observe.rs`](../../../../connect/crates/layerfs-machine/src/platforms/lima/observe.rs) |

Steps of `forward::daemon` and `forward::open`:

| Step | What | Bound |
| --- | --- | --- |
| 1 | `port 0` is refused `InvalidInput` before any tool runs. | — |
| 2 | `tool::free_port`: a host loopback port that was free a moment ago. | — |
| 3 | `tool::detach` starts `ssh -N -o ExitOnForwardFailure=yes … -L 127.0.0.1:<port>:<target> <dest>`, stderr to `<work_dir>/forward.log`. | 10 s to start |
| 4 | Wait `SETTLE`, then check the pid still runs with the marker `127.0.0.1:<port>:` on its command line. | 600 ms |
| 5 | Dead: `Failed`, "ssh -L 127.0.0.1:<port> ended at once", with the log's last 3 lines. Nothing is retried. | — |

`ExitOnForwardFailure` makes a busy port fail at once rather than leave a
forward that carries nothing. The forward is detached so it outlives one
CLI call; the caller records its address and pid
([`spec.rs`](../../../../connect/crates/layerfs-machine/src/spec.rs)).

The same `forward::open` also carries a worker's port on `ssh` and
`firecracker` ([Workers](workers.md#loopback-and-secret)); that pid is the
endpoint's `forward_pid`, ended by `forward::close` at machine stop.

## forward_close

```text
 forward_close(spec, &Forward { address, pid })
   pid None? -------------------------------------> nothing to end
   tool::alive(pid, "127.0.0.1:<port>:")? --no----> nothing (pid reused)
   SIGTERM --> wait up to 3 s (50 ms looks) --> still there? SIGKILL
```

| Rule | Source |
| --- | --- |
| A reused pid is never ended: the marker must be on its command line (`ps -o command= -p <pid>`). | `tool::alive` in [`tool.rs`](../../../../connect/crates/layerfs-machine/src/setup/tool.rs) |
| Bounded: SIGTERM, up to 3 s, then SIGKILL. | `tool::end` in [`tool.rs`](../../../../connect/crates/layerfs-machine/src/setup/tool.rs) |
| `forward_close` reports nothing; it is a best-effort end of a helper the caller recorded. | [`calls.rs`](../../../../connect/crates/layerfs-machine/src/setup/calls.rs) |
| `forward::running` tells whether the worker's recorded forward still runs. | [`forward.rs`](../../../../connect/crates/layerfs-machine/src/setup/forward.rs) |

## Platforms that refuse

On these platforms Workspace calls stay on the worker. Each refusal is
`forward::refuse`: "the <platform> machine <name> forwards no Sandbox
daemon port: <why>".

| Platform | Error | Why | Source |
| --- | --- | --- | --- |
| `docker` | `InvalidInput` | Each docker Sandbox publishes its daemon's port on the host's loopback when it is created (`PortBindings`, host IP `127.0.0.1`, Engine-chosen port); that published address is its endpoint. | [`docker/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/docker/adapter.rs), [`backend/docker/create.rs`](../../../../connect/crates/layerfs-sandbox/src/backend/docker/create.rs) |
| `apple-container` | `Unsupported` | The `container` CLI publishes a port only when the machine's container is created. | [`apple_container/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/apple_container/adapter.rs) |
| `microsandbox` | `Unsupported` | `msb` publishes a port only when the machine's sandbox is created. | [`microsandbox/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/microsandbox/adapter.rs) |
| `firecracker` | `Unsupported` | Its daemons listen on the microVM's loopback, which a forward through the Lima host cannot reach (it reaches only the microVM's tap address). | [`firecracker/adapter.rs`](../../../../connect/crates/layerfs-machine/src/platforms/firecracker/adapter.rs) |

`docker` is `InvalidInput`, not `Unsupported`: a direct address exists,
it just does not come from `forward`.

## Related

- [Workers](workers.md): the route Workspace calls take when there is no forward.
- [Sandboxes: Docker](../sandboxes/docker.md): the loopback-published daemon port.
- [Machines: Adapter](../machines/adapter.md): the `Adapter::forward` method.
- [API: Routing and workers](../../api/routing/README.md) and [the daemon channel](../../api/routing/daemon-channel.md): who dials the forwarded address.

---

← [Network: Reach](reach.md) · [Up: Network](README.md) · [Network: Credentials](credentials.md) →
