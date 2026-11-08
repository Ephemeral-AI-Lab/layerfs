# Workers

[Architecture](../../README.md) › [Control](../README.md) › [Routing and workers](README.md) › Workers

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `layerfs worker` process and its protocol.

| | |
| --- | --- |
| Component | [Routing and workers](README.md) |
| Source | [`api/crates/layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs), [`worker`](../../../../control/crates/layerfs-machine/src/worker), [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs), [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs), [`bundle.rs`](../../../../control/crates/layerfs-machine/src/bundle.rs) |

A worker is the one LayerFS process on a machine whose kernel the caller
cannot open a SQLite Store on. It runs the library's host operations for the
calls a caller sends it, behind a secret, on a TCP port the caller reaches on
its own loopback ([`worker/mod.rs`](../../../../control/crates/layerfs-machine/src/worker/mod.rs)).
Where the worker sits on each platform and how its port is carried is in
[Control: Network, Workers](../network/workers.md).

## Worker frames

```text
 plain frame (no byte values: the secret, the hello, most others)
 +----------------+-------------------------------------------+
 | length: u32 BE | JSON text, `length` bytes                 |
 |   top bit 0    |                                           |
 +----------------+-------------------------------------------+

 attached frame (a frame holding byte values, protocol 3)
 +----------------+--------------+------+-------+--------------------------+
 | 1<<31 | length | json: u32 BE | JSON | count | count x (len: u64 BE,    |
 |   (u32 BE)     |              |      | u32 BE|          len raw bytes)  |
 +----------------+--------------+------+-------+--------------------------+
   each byte value is {"attachment": i} in the JSON, i its attachment;
   every attachment is named exactly once
   first frame of each side <= 4 KiB (MAX_HELLO)
   any frame body          <= 512 MiB (MAX_FRAME), attachments included
```

Byte values (a `project.stage` part, a Store file piece) travel raw, with no
base64 in either direction. Only this build's protocol sends or reads an
attached frame: the caller sends nothing to a worker whose hello names
another protocol, and the secret and hello frames hold no byte values, so
they stay plain. A protocol 2 peer sent bytes as `{"base64": "..."}` in a
plain frame; a plain frame with such a field still reads back as bytes.

| Frame | Sender | Shape | Source |
| --- | --- | --- | --- |
| secret | caller | `{secret}` | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) `greet` |
| hello | worker | `{ready: true, protocol, bundle}`, or `{refused}` for a wrong secret | [`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs) `to_frame`, [`server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) `answer` |
| request | caller | `{op, args, home_id?, trace_step?}`; `trace_step: true` only while the caller times its steps (`LAYERFS_TRACE_STEP`) | `client.rs` `request` |
| event | worker | `{event}`, zero or more: progress, command output, and `{exec_started: {stop}}` once an exec whose record the caller keeps runs (the caller's exec takes it; it is not passed on) | `server.rs` `answer` |
| answer | worker | `{value}` or `{refused: {code, message, detail}}`, with `steps: [{step, us}]` beside it when the request had `trace_step` | `server.rs` `answer`, `refusal_frame` |

```text
 caller (host)                                worker (machine)
   |  TCP connect (<= 10 s)                       |
   |--------------------------------------------->|
   |  {secret}                                    |  reads <= 4 KiB,
   |--------------------------------------------->|  waits <= 10 s
   |                {ready, protocol, bundle}     |  (wrong: {refused
   |<---------------------------------------------|   Denied}, close)
   |  compare protocol and bundle; another build: |
   |  close, send nothing (Unavailable)           |
   |  {op, args, home_id}   (sent once)           |  <= 30 s between
   |--------------------------------------------->|  bytes
   |                {event} ... {event}           |  runs op with
   |<---------------------------------------------|  home = its root
   |                {value} | {refused}           |
   |<---------------------------------------------|
   v  connection closes: one request each         v
```

A frame larger than allowed is refused `TooLarge` from its length, before any
of its body is read; an attached frame's JSON length, count and each
attachment length are checked against what is left of its body before
anything of that size is allocated or read, and a malformed one is refused
`Failed` ([`worker/frame.rs`](../../../../control/crates/layerfs-machine/src/worker/frame.rs)).
The worker serves each connection on its own thread, so several callers can
be served at once; each connection still carries one request.

| Operation | Runs on the machine | Source |
| --- | --- | --- |
| `worker.ping` | answers `{kind: "worker", version, pid}`; diagnostic `args.capacity=true` adds nullable `free_disk`, observed on the worker's own data-root filesystem without guest commands | `server.rs` `run` |
| `store.call` | one Store method on `store/<name>/store.sqlite` under the worker's root, holding the Store's lease there | `server.rs`, [`open.rs`](../../../../control/crates/layerfs-store/src/open.rs) |
| `store.file` | one step of a Store push, pull, remove, stat or backup | [`sqlite/transfer/steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `sandbox.host` | a Sandbox or Workspace host operation on the machine's own `native`: one of the nine in the host operation table (`OPS`, `workspace.exec_cancel` among them), decoded into the typed request by `decode`, else refused before any effect | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `in_worker`, [`host/call.rs`](../../../../control/crates/layerfs-sandbox/src/host/call.rs) `OPS`, [`host/frame.rs`](../../../../control/crates/layerfs-sandbox/src/host/frame.rs) `decode` |
| `project.stage` | writes or discards one part of a caller's folder copy (`StageFrame`); a caller sends up to four parts at once, whose data records apply in any order | [`project/staged.rs`](../../../../control/crates/layerfs-store/src/project/staged.rs) |
| `project.init` | adds the staged copy as a new project (`InitFrame`) | [`project/init.rs`](../../../../control/crates/layerfs-store/src/project/init.rs) |
| `project.export` | pins one Branch snapshot and streams bounded tree records to the caller (`ExportFrame`) | [`project/export/`](../../../../control/crates/layerfs-store/src/project/export/route.rs) |
| `project.remove` | drops one project from a Store under the root (`RemoveFrame`) | [`project/remove.rs`](../../../../control/crates/layerfs-store/src/project/remove.rs) |
| anything else | `InvalidInput` "the worker has no operation" | `server.rs` |

Each project frame is decoded once into its typed request
([`project/frame.rs`](../../../../control/crates/layerfs-store/src/project/frame.rs)),
and a `store.call` frame into `CallFrame`
([`call.rs`](../../../../control/crates/layerfs-store/src/call.rs)). The worker
then uses its own root as the home, and the Store file under that root as the
Store's path, whatever home or Store path the caller sent (`home_of` and the arms in
`worker_server.rs`). The root
is not a home: `home_id` in the request lends it the caller's home id.

## Hello

| Field | Meaning | Source |
| --- | --- | --- |
| `ready` | always `true` in a hello | [`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs) |
| `protocol` | the worker protocol it speaks; this build is `PROTOCOL = 3` (byte values as frame attachments; 2 sent them as base64); the first workers sent none | `hello.rs` |
| `bundle` | the hash from `bundle.json` beside the worker's executable, or null when run from no sealed bundle | `Hello::own`, `sealed_hash` |

The caller compares the hello with its own build before it sends the
request (`Hello::expected`): same protocol, and the same bundle hash as the
bundle this build deploys. A worker run from no sealed bundle (a test or a
development build) is checked by protocol alone. `worker::hello` greets
without a request; `machine.start` and doctor use it
([`doctor/workers.rs`](../../../../control/crates/layerfs-control/src/doctor/workers.rs)).

## Deploy

```text
 <cache>/layerfs/bundle/<version>/linux-<arch>/     cache = XDG_CACHE_HOME
   bundle.json   {hash, ...}                          or ~/.cache
   layerfs       guest worker (built without `machine`)
   daemon, Sandbox helper, Sandbox image archive
        |
        | machine.start: adapters::start(spec, bundle)
        v
 machine:  layerfs worker   env LAYERFS_WORKER_LISTEN = <bind>:<port>
                                LAYERFS_WORKER_SECRET = <0600 file>
           root /layerfs/data  (LAYERFS_WORKER_ROOT in tests)
        |
        v
 home:     machine/<name>/worker.json {address, secret, started}
```

| Step | What happens | Source |
| --- | --- | --- |
| Find the bundle | `bundle()` reads `bundle.json`; none is `Unsupported` naming `tools/linux_bundle.py` | [`bundle.rs`](../../../../control/crates/layerfs-machine/src/bundle.rs), [`deploy.rs`](../../../../control/crates/layerfs-sandbox/src/deploy.rs) `bundle_dir` |
| Start | the machine adapter starts the machine and its worker from the bundle; a `docker` machine with no image runs its worker container from the bundle's image, loaded into Docker once | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) `start` |
| Record | `worker.json` is written under the home lock; the machine's state becomes `running` | [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) `save_running` |
| Check | the new worker's hello is compared; a stale bundle is refused (see below) | `endpoint.rs` `start` |
| Serve | `layerfs worker` reads its two settings, binds, makes its root `0701` and serves | [`layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs), `server.rs` `serve` |

`LAYERFS_BUNDLE` names another bundle folder; the source marks it internal.

## Lifecycle

| Event | Effect | Source |
| --- | --- | --- |
| `machine.start` | dials the recorded worker; if it answers its secret, done; else starts the machine and worker under `machine/<name>/start.lock` (wait up to 600 s) and waits until ready | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs) `running` |
| any other call | dials `worker.json` once; no worker recorded, or no answer, is `Unavailable` naming `layerfs machine start`; nothing is started | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `machine.stop` | refused `InUse` while Sandboxes run on it; else stops worker and VM under `start.lock` and removes `worker.json` | [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs) |
| idle | nothing: there is no idle stop (#535 D11) | `worker/mod.rs`, `layerfs-cli/src/worker.rs` |
| `native` on macOS or Linux | no worker; `start` and `stop` answer at once | `endpoint.rs` `running`, `power.rs` |

Within one operation of several steps (a push, pull or
remove) the endpoint read first is reused for later steps (`Worker::ready`).

## Version mismatch

| Case | Refusal | What to do |
| --- | --- | --- |
| A recorded worker of another protocol, another bundle, or no protocol | `Unavailable`, `detail {worker, expected}`, "nothing was sent" | `layerfs machine stop <m>`, then `layerfs machine start <m>` (`replace_with`) |
| A worker just started by `machine.start` is still of another build | `Unavailable`, `detail {worker, expected, bundle}`: the bundle was sealed before this build | rebuild with `python3 tools/linux_bundle.py`, then stop and start |
| A wrong secret | the worker's `Denied`; nothing was sent | Not established from source how a recorded secret goes wrong; a fresh `machine.start` writes a new `worker.json` |

`machine.start` never starts over a worker of another build: a recorded
worker that answers its secret counts as running, whatever its build
(`answering` in [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs)).
The refusal names the stop-then-start pair instead. `place::stopped` adds the
same advice to a mismatch met by any other call.

## Related

- [Routing table](routing-table.md): which calls take a worker route.
- [Daemon channel](daemon-channel.md): the other cross-process channel.
- [Call table: Errors](../../api/call-table/errors.md#uncertain-outcomes): `Unknown` after a lost answer.
- [Control: Network, Workers](../network/workers.md): worker setup, loopback and forwards.
- [Control: Machines](../machines/README.md): the adapters `machine.start` and `machine.stop` call.
- [Management: Doctor](../management/doctor.md): the worker checks.

---

← [Routing and workers: Routing table](routing-table.md) · [Up: Routing and workers](README.md) · [Routing and workers: Daemon channel](daemon-channel.md) →
