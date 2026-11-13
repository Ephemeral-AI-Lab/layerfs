# Workers and reach

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a machine's worker is set up and spoken to, and how the caller's
PostgreSQL server and Sandbox daemons are reached across the machine
boundary, within [Machines](README.md).

| | |
| --- | --- |
| Source | [`layerfs-machine/src/worker/`](../../../../control/crates/layerfs-machine/src/worker), [`layerfs-control/src/worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs), [`layerfs-platform/src/setup/`](../../../../control/crates/layerfs-platform/src/setup), [`reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs), [`layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |

A worker is one `layerfs worker` process per machine whose kernel the
caller cannot open a sqlite Store on. It runs the library's host
operations for the requests a caller sends, behind a secret, on a port the
caller reaches on its own loopback. Guests never read the home: each
request carries what its operation needs. Which calls take a worker is the
"Where each call runs" table in [Control](../README.md).

## Setup (`setup/worker.rs`, `guest.rs`, `tool.rs`, `vm.rs`, `artifacts.rs`)

```text
 1 adapter makes or boots the kernel; data disk at /layerfs/data
 2 prepare  mkdir /layerfs/data/{store,sandbox}, /layerfs/state 0700;
            echo 0 > /proc/sys/user/max_user_namespaces, read back
   install  bundle files (layerfs, layerfs-daemon, layerfs-sandbox-helper,
            sandbox-image.tar) copied per file when their hash changed;
            bundle.json published last
 3 launch   new secret (32 random bytes, 64 hex) -> /layerfs/state/secret
            0600 via stdin; end an earlier worker; start detached as root:
            LAYERFS_WORKER_LISTEN=<bind>:7070
            LAYERFS_WORKER_SECRET=/layerfs/state/secret
            >> /layerfs/state/worker.log
 4 ready    guest /proc/net/tcp listens on the port AND host 127.0.0.1:<port>
            accepts; every 200 ms, at most 60 s
 5 the caller writes worker.json {address, secret, started} and record.json
```

| Step | Failure | Source |
| --- | --- | --- |
| 2 | the guest keeps user namespaces: `Unsupported` | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| 2 | an unchanged Sandbox image is never recopied; an interrupted copy is never sealed | [`setup/artifacts.rs`](../../../../control/crates/layerfs-platform/src/setup/artifacts.rs) |
| 4 | a worker that ends meanwhile: `Failed` with the last 8 lines of `worker.log` | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |

The `docker` machine does steps 2 and 3 in its own container start (bundle
mounted read-only, entry script makes the folders, no sysctl write);
step 4 is shared ([Platforms](platforms.md#docker-platformsdocker)).

| `Guest` | Host command | Source |
| --- | --- | --- |
| `Lima` | `ssh -F <dir>/ssh.config lima-<vm> -- sudo -n -- …` | [`setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| `AppleContainer` | `container exec [-i] <vm> …` | same |
| `Docker` | `docker -H unix://<socket> exec [-i] <container> …` | same |

| Helper | Bound | Source |
| --- | --- | --- |
| `Guest::exec`, `script` | 60 s | [`setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| `Guest::copy_in` / `copy_out` (one `tar` stream / one `cat`, never buffered whole) | 600 s | same |
| `tool::run`: runs once, keeps 1 MiB per stream, a timeout is `Unknown` | per call | [`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| `tool::within`: every command a probe starts ends by its deadline, killed with its process group | caller's | same |
| `tool::detach`: a helper reparented to init, stderr to a log, pid returned | 10 s to start | same |
| `vm::call`, `checked`: one provider CLI call | 30 s; 120 s create; 300 s boot | [`setup/vm.rs`](../../../../control/crates/layerfs-platform/src/setup/vm.rs) |
| `vm::recorded_port`, `new_port`: the worker's host port | — | same |

### Where the worker listens

| Platform | Binds in the guest | Carried to host `127.0.0.1:<port>` by |
| --- | --- | --- |
| `lima` | `127.0.0.1:7070` | the template's port forward |
| `apple-container` | `0.0.0.0:7070` | `--publish 127.0.0.1:<port>:7070` at create |
| `docker` (macOS) | `0.0.0.0:7070` | `-p 127.0.0.1::7070`, read back with `docker port` |
| `docker` (Linux) | `127.0.0.1:<free port>` | nothing: host network |
| `native` (macOS, Linux) | no worker | calls run in the caller's process |

Every host-side address is loopback; the secret guards the worker on every
bind address. `layerfs worker` reads its two settings, makes its root
(`/layerfs/data`, or `LAYERFS_WORKER_ROOT` in tests) `0701` and serves
until `machine.stop`: it never stops by itself
([`layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs)).

## Protocol (`layerfs-machine/src/worker/`)

```text
 caller (client.rs)                          worker (worker_server.rs)
   | connect 127.0.0.1:<port>  (<= 10 s)         |
   |--- {secret} -------------------------------->| reads <= 4 KiB, <= 10 s
   |<-- {ready:true, protocol, bundle} ----------| or {refused: Denied}
   |    other protocol or bundle: send nothing   |
   |--- {op, args, home_id?, trace_step?} ------->| <= 30 s between bytes
   |         from here nothing is resent         | runs op once,
   |<-- {event} ... (progress, output) ----------|   home = its root
   |<-- {value} | {refused} (+ steps?) ----------|
   x  connection closes: one request each        x
```

```text
 plain frame     | len: u32 BE, top bit 0 | JSON |
 attached frame  | 1<<31 | len | json len: u32 | JSON | count: u32 |
                 |   count x (len: u64 BE, raw bytes) |
   each byte value is {"attachment": i}; each attachment named once
   first frame <= 4 KiB (MAX_HELLO); any frame <= 512 MiB (MAX_FRAME)
```

| Item | Detail | Source |
| --- | --- | --- |
| Hello | `{ready: true, protocol: 3, bundle}`; `bundle` is the hash in `bundle.json` beside the executable, `null` outside a sealed bundle | [`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs) |
| Build check | same protocol and same bundle hash as the bundle this build deploys; a worker outside a sealed bundle is checked by protocol alone | same, `Hello::expected` |
| Byte values | travel raw in attached frames, never base64; a plain frame's `{"base64": ..}` still reads as bytes | [`worker/frame.rs`](../../../../control/crates/layerfs-machine/src/worker/frame.rs) |
| Oversize frame | `TooLarge` from its length, before its body is read; a malformed attached frame `Failed` | same |
| Concurrency | one thread per connection; one request per connection | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| Diagnostic connection | an allowlisted hello and ping (`doctor.sandbox`, `store.diagnose`) under one absolute deadline; never starts a worker | [`worker/diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs) |

| Outcome | Error | Request sent? |
| --- | --- | --- |
| connect, secret or hello fails | `Unavailable` (`Unreached`) | no |
| wrong secret | `Denied` (`Unreached`) | no |
| worker of another build | `Unavailable`, `detail {worker, expected}`, naming `layerfs machine stop` then `start` | no |
| request write fails part way | `Unknown`, "it was not resent" | maybe |
| answer lost | `Unknown`, "it may have run, and was not resent" | yes |

Only `Unreached` lets the caller start a machine before it sends. A ping
waits at most 10 s; other operations wait as long as they run
([`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs)).

## Server (`worker_server.rs`)

| Operation | Runs on the machine |
| --- | --- |
| `worker.ping` | `{kind: "worker", version, pid}`; `args.capacity` adds `free_disk` of the root's filesystem |
| `store.call` | one Store method on `store/<name>/store.sqlite` under the root, holding the Store's lease there |
| `store.file` | one step of a Store push, pull, remove, stat or backup, with `progress` events for hashing steps |
| `store.reach` | `store.attach`'s check of this machine's connection to a PostgreSQL Store |
| `store.diagnose`, `doctor.sandbox` | doctor's read-only observations |
| `sandbox.host` | a Sandbox or Workspace host operation on the machine's own `native`, decoded into its typed request first |
| `project.stage`, `project.init`, `project.export`, `project.remove` | write one part of a folder copy; add it as a project; stream a pinned snapshot back; drop a project |
| anything else | `InvalidInput`, "the worker has no operation …" |

The worker uses its own root as the home and the Store file under it as
the Store's path, whatever the caller sent. The root is not a home:
`home_id` lends it the caller's id
([Home](../home/README.md)).

### Lifecycle

| Event | Effect | Source |
| --- | --- | --- |
| `machine.start` | a recorded worker that answers its secret counts as running, whatever its build; else start under `start.lock`, check the new hello (a stale bundle is `Unavailable`, naming `python3 tools/linux_bundle.py`) | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) |
| any other call | dials `worker.json` once; none recorded or no answer is `Unavailable` naming `layerfs machine start`; nothing is started; later steps of one operation reuse the endpoint | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `machine.stop` | refused `InUse` while Sandboxes are recorded; stops worker and VM, removes `worker.json` | [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs) |
| idle | nothing: a worker has no idle stop | [`worker/mod.rs`](../../../../control/crates/layerfs-machine/src/worker/mod.rs) |
| `machine.status` | a running kernel whose worker does not answer the hello (secret, `{"ready":true}` within 2 s) is `stopped`; a bare connect is not enough, since a forward accepts with no worker | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |

## Reach (`reach.rs`)

A PostgreSQL Store's URL is written for the caller's host.
`reach(spec, url)` answers the URL that machine's daemon dials; it starts
nothing, and no message holds any part of the URL.

```text
 dial(spec, url, loopback)
   Shared? ------------------------------------------> url as given
   one postgres[ql]://[user@]host[:port]/... ?  no --> InvalidInput
   ?host= or ?hostaddr= ------------------------ yes --> Unsupported
   no host (a Unix socket) --------------------- yes --> Unsupported
   host not localhost / 127.0.0.1 / ::1 -------- yes --> url as given
   sslmode=verify-full ------------------------- yes --> Unsupported
   Alias(a) | Lookup(read) --> with_host(a), every other byte kept
```

| Platform | `Loopback` | A loopback host becomes |
| --- | --- | --- |
| `native` | `Shared` | unchanged; the URL is not parsed |
| `docker` | `Shared` on a Linux host; `Lookup` on Docker Desktop | the address the running worker resolves `host.docker.internal` to |
| `lima` | `Alias` | `192.168.5.2` |
| `apple-container` | `Alias` (recorded gateway), else `Lookup` | the gateway the last start recorded, else `container inspect`'s while running; a stale recorded gateway is dialled as recorded |

`verify-full` is refused because the rewritten host is not in the server's
certificate: give the Store a URL whose host every machine can verify.
`PostgresUrl` keeps userinfo byte for byte, brackets an IPv6 alias, and has
no `Debug`
([`postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/postgres_url.rs)).

## Forwards

A Sandbox daemon listens on `127.0.0.1:<guest_port>` in its machine.
`forward(spec, guest_port)` answers the host loopback address at which this
process reaches it, so Workspace calls skip the worker
([`setup/forward.rs`](../../../../control/crates/layerfs-platform/src/setup/forward.rs)).

```text
 host                                       machine guest
 Workspace call -> 127.0.0.1:<free>         Sandbox daemon 127.0.0.1:<gp>
                     | ssh -N (detached, pid kept)          ^
                     +======================================+
 ssh -N -o ExitOnForwardFailure=yes <opts> -L 127.0.0.1:<free>:127.0.0.1:<gp>
```

| Platform | Answer |
| --- | --- |
| `native` | `127.0.0.1:<guest_port>`, no pid |
| `lima` | a free host port through Lima's `ssh.config` with `ControlMaster=no`; the `ssh` pid |
| `docker`, `apple-container` | `Unsupported`: "the <platform> machine <name> forwards no Sandbox daemon port: …"; Workspace calls stay on the worker |

| Step | Bound |
| --- | --- |
| port 0 refused `InvalidInput`; pick a free host port; `tool::detach` the `ssh`, stderr to `<work_dir>/forward.log` | 10 s to start |
| wait, then check the pid runs with `127.0.0.1:<port>:` on its command line; dead is `Failed` with the log's last 3 lines, not retried | 600 ms |
| `forward_close`: only if the marker is still on the pid's command line; SIGTERM, then SIGKILL | 3 s |

## Related

- [Machines](README.md): the adapter that runs step 1.
- [Sandboxes: daemon channel and exec](../sandboxes/daemon-channel-and-exec.md): who dials a forwarded daemon.
- [Stores and projects](../stores-and-projects/README.md): `store.call`, `store.file` and `project.*` routes.
- [Stores and projects: SQLite and PostgreSQL](../stores-and-projects/sqlite-and-postgresql.md): the password inside the URL.
- [Bridge](../../engine/bridge/README.md): the daemon wire contract.
