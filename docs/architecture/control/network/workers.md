# Workers

[Architecture](../../README.md) › [Control](../README.md) › [Network](README.md) › Workers

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Machine workers and how calls reach them.

| | |
| --- | --- |
| Component | [Network](README.md) |
| Source | [`control/crates/layerfs-platform/src/setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`control/crates/layerfs-platform/src/setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs), [`control/crates/layerfs-platform/src/setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs), [`control/crates/layerfs-platform/src/setup/vm.rs`](../../../../control/crates/layerfs-platform/src/setup/vm.rs); server and client in [`worker`](../../../../control/crates/layerfs-machine/src/worker); the binary in [`api/crates/layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |

A worker is one `layerfs worker` process per machine. It runs the calls
that must run on the machine's kernel: sqlite Store calls (`store.call`,
`store.file`), Sandbox host operations (`sandbox.host`) and project Init
(`project.*`). Which calls go to it is [routing](../routing/README.md).

## Worker setup

```text
 machine::create / start                           (setup/calls.rs)
   |
   | 1  adapter makes or boots the kernel and data disk  (platforms/<p>/)
   v
 worker::prepare   mkdir /layerfs/data/{store,sandbox}, /layerfs/state 0700
   |               echo 0 > /proc/sys/user/max_user_namespaces, read back
   v 2
 worker::install   copy the bundle to /layerfs/bundle once per hash
   |               (.sealed marker); copy the Sandbox image if absent
   v 3
 worker::launch    new secret -> /layerfs/state/secret (0600, via stdin)
   |               LAUNCH: end an earlier worker (pid + exact cmdline),
   |               start `layerfs worker` detached as root
   v 4
 worker::ready     until the guest listens on the port AND the host's
   |               127.0.0.1:<port> accepts; at most 60 s; every 200 ms
   v
 Running { endpoint: { address, secret }, lasting }
   |
   | 5  the caller writes worker.json and record.json
   v
```

| Step | What | Source |
| --- | --- | --- |
| 1 | The platform adapter makes the VM or container and mounts the data disk at `/layerfs/data`. | [Machines: Adapter](../machines/adapter.md) |
| 2 | `prepare`: guest folders; user namespaces blocked machine-wide (`user.max_user_namespaces=0`), else `Unsupported`. `install`: bundle copied as one `tar` stream when its hash changed. | [`worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| 3 | `launch`: the worker gets `LAYERFS_WORKER_LISTEN=<bind>:7070` and `LAYERFS_WORKER_SECRET=/layerfs/state/secret`; output goes to `/layerfs/state/worker.log`. | [`worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |
| 4 | `ready`: reads `/proc/net/tcp` in the guest and connects from the host. A worker that ends meanwhile is `Failed` with its log's last 8 lines. | [`worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| 5 | Records are written by the API layer, not here. | [`calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |

The `docker` machine does steps 2 and 3 in its own container start
([Machines: Docker](../machines/platforms/docker.md)): the bundle is mounted
read-only at `/layerfs/bundle`, and the container's entry script makes
`/layerfs/data/{store,sandbox}` (0755) as `prepare` does. It never writes
`user.max_user_namespaces`, which would change the whole Docker host; the
namespaces Sandboxes it runs rely on the command filter instead
([Sandboxes: Native and namespaces](../sandboxes/native-and-namespaces.md#no-user-namespaces)).
Step 4 (`ready`) is shared.

`check_bundle` refuses a bundle without `layerfs`, `layerfs-daemon` and
`layerfs-sandbox-helper`, or whose hash is not hex. The worker serves until
`machine.stop`; it has no idle stop (#535 D11).

## Loopback and secret

How each platform carries the worker's guest port to the host's loopback:

| Platform | Worker binds in the guest | Carried to host `127.0.0.1:<port>` by | Source |
| --- | --- | --- | --- |
| `lima` | `127.0.0.1:7070` | Lima template port forward, guest 7070 to host `<port>` | [`lima/template.rs`](../../../../control/crates/layerfs-platform/src/platforms/lima/template.rs) |
| `apple-container` | `0.0.0.0:7070` | `--publish 127.0.0.1:<port>:7070` at create | [`apple_container/create.rs`](../../../../control/crates/layerfs-platform/src/platforms/apple_container/create.rs) |
| `docker` (macOS) | `0.0.0.0:7070` in the worker container | `-p 127.0.0.1::7070`, port read back with `docker port` | [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |
| `docker` (Linux) | `127.0.0.1:<free port>`, host network | nothing: the container shares the host's network | [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |
| `native` | no worker on macOS or Linux: calls run in the caller's process | — | [`native/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/profile.rs) |

The bind address is the profile's `worker_bind`
([`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs)).
On two platforms (`apple-container`, `docker`) it is
`0.0.0.0` inside the guest's own network; every
host-side address is loopback.

The secret is 32 bytes from `/dev/urandom` as 64 hex digits
(`tool::secret`). It is written through stdin into a 0600 file, so it is
never on a command line (`Guest::write_file`). The server reads it from
that file and compares it in constant time (`same` in
[`server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs)).

## One request per connection

```text
 caller (client.rs)                          worker (server.rs)
   | connect 127.0.0.1:<port>   (<= 10 s)        |
   |--- frame {secret} --------------------------->| reads <= 4 KiB, <= 10 s
   |<-- frame {ready:true, protocol, bundle} -------| or {refused: Denied}
   |    other build? send nothing, close           |
   |--- frame {op, args, home_id?, trace_step?} --->| <= 30 s between bytes
   |         from here nothing is resent           | runs op once, root =
   |<-- frame {event} ... (progress, output) -------|  /layerfs/data
   |<-- frame {value} | {refused} (+ steps?) ------|
   x  connection closes                            x
```

A frame is a 4-byte big-endian length then JSON; a frame holding byte
values sets the length's top bit and carries them raw after its JSON
([`worker/frame.rs`](../../../../control/crates/layerfs-machine/src/worker/frame.rs);
layout in [Routing: Workers](../routing/workers.md#worker-frames)).

| Outcome | Error | Request sent? | Source |
| --- | --- | --- | --- |
| Connect, secret or hello fails | `Unavailable` (`Unreached`) | No | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| Wrong secret | `Denied` (`Unreached`) | No | [`server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| Worker of another build | `Hello::mismatch` refusal (`Unreached`) | No | [`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs) |
| Write of the request fails part way | `Unknown`, "it was not resent" | Maybe | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| Answer lost | `Unknown`, "it may have run, and was not resent" | Yes | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

Only `Unreached` may lead the caller to start a machine before it sends the
request once (#535 R1). A ping waits at most 10 s for its answer; other
operations wait as long as they run.

## Guest tools and VMs

Setup reaches each guest's shell as root through one `Guest` variant:

| `Guest` | Host command |
| --- | --- |
| `Lima` | `ssh -F <dir>/ssh.config lima-<vm> -- sudo -n -- …` |
| `AppleContainer` | `container exec [-i] <vm> …` |
| `Docker` | `docker -H unix://<socket> exec [-i] <container> …` |

Source: [`guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs).

| Helper | Behaviour | Bound | Source |
| --- | --- | --- | --- |
| `Guest::exec`, `script` | One command, output captured. | 60 s (`EXEC`) | [`guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| `Guest::copy_in` / `copy_out` | One `tar` stream in / one `cat` out, never buffered whole; `copy_out` makes a new 0600 file. | 600 s (`COPY`) | [`guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| `tool::run` | Runs once; a timeout is `Unknown`. Keeps 1 MiB of each stream. | per call | [`tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| `tool::within` | Every command a probe starts ends by its deadline, killed with its process group. | caller's | [`tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| `tool::detach` | A helper reparented to init, its stderr in a log, its pid returned. | 10 s to start | [`tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| `vm::call`, `checked` | One provider CLI call. | 30 s / 120 s create / 300 s boot | [`vm.rs`](../../../../control/crates/layerfs-platform/src/setup/vm.rs) |
| `vm::new_port`, `recorded_port` | The worker's host port: the recorded one, else a free one. | — | [`vm.rs`](../../../../control/crates/layerfs-platform/src/setup/vm.rs) |

`machine.status` reads a running kernel whose worker does not answer the
hello as `Stopped` ("the machine runs but its worker does not answer";
`with_worker` in [`calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs)).
The hello is checked, not a bare connect, because an `ssh -L` or Lima
forward accepts on the host while no worker listens in the guest.

## Related

- [Forwards](forwards.md): the same `ssh -L` helper, for Sandbox daemons.
- [Credentials](credentials.md): what a `sandbox.host` request carries.
- [Machines](../machines/README.md) and [Machines: Adapter](../machines/adapter.md): step 1 per platform.
- [Control: Routing and workers](../routing/README.md) and [its workers page](../routing/workers.md): which calls go to a worker.

---

← [Control: Network](README.md) · [Up: Network](README.md) · [Network: Reach](reach.md) →
