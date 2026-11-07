# Routing and workers

[Architecture](../../README.md) › [Control](../README.md) › Routing and workers

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Where each call runs: in the caller's process, in a machine's worker, or in a Sandbox daemon.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-control`, `layerfs-cli` |
| Source | [`api/crates/layerfs-api/src/calls`](../../../../api/crates/layerfs-api/src/calls), [`daemon`](../../../../control/crates/layerfs-sandbox/src/daemon), [`api/crates/layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs), [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs), [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs), [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) |
| Uses | [Control: Network](../network/README.md), [Engine: Daemon](../../engine/daemon/README.md) |
| Used by | [Management](../management/README.md) |
| Flows | [A call, end to end](../../flows/call.md), [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Routing table](routing-table.md) | Where each call runs. |
| [Workers](workers.md) | The `layerfs worker` process and its protocol. |
| [Daemon channel](daemon-channel.md) | How Workspace calls reach a daemon. |

## The picture

```text
 front end --> call table --> dispatcher (calls/dispatch.rs)
                                   |
                                   v
                         control family function
                                   |
         +-------------------------+--------------------------+
         | Store call              | Sandbox host operation   | Workspace
         | (branch, commit, file)  | (sandbox.*, workspace.*) | call, daemon
         v                         v                          | reached here
   place::plan(StoreRef)    place::on_machine(machine)        v
     |           |            |              |          direct::call
  Local       Worker        Local          Worker            |
     |           :            |              :               |
     v           v            v              v               v
 store::open  worker op    sandbox::     worker op      daemon pool
 (in process) store.call   dispatch      sandbox.host   (this process)
                 :            |              :               :
                 v            v              v               v
            Store file    daemon pool    daemon pool    Sandbox daemon
            in guest      (this process) (in worker)
```

A dotted edge (`:`) crosses a process boundary over TCP. Both daemon pools
end at the Sandbox daemon. The worker and the daemon pool are described in
[Workers](workers.md) and [Daemon channel](daemon-channel.md); the decision
itself is the [Routing table](routing-table.md).

## What it does

| Call | Runs in | Decided by |
| --- | --- | --- |
| Store call on a PostgreSQL Store | the caller's process | [`place::plan`](../../../../control/crates/layerfs-store/src/place.rs) |
| Store call on a SQLite Store whose machine runs calls locally | the caller's process | `place::plan` → `local` |
| Store call on a SQLite Store on any other machine | that machine's worker, op `store.call` | `place::plan` → `Executor::Worker` |
| Sandbox create, get, close; Workspace mount, exec, commit, unmount, status | the Sandbox's machine: here, or its worker as `sandbox.host` | [`sandbox::route::on_machine`](../../../../control/crates/layerfs-sandbox/src/route.rs) |
| Workspace mount, commit, unmount, status on a Sandbox whose daemon port reaches this host | this process, straight to the daemon | [`sandbox::direct`](../../../../control/crates/layerfs-sandbox/src/direct.rs) |
| `workspace.exec` on such a Sandbox | still the machine's worker (the Sandbox runtime is there) | [`workspace/calls.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs) |

`layerfs worker` is one long-lived process on a machine whose kernel the
caller cannot open a SQLite Store on: every VM and ssh host, `docker` on
every host, and `native` on Windows. Only `machine.start` starts it; it
serves until `machine.stop` ends it
([`worker/mod.rs`](../../../../control/crates/layerfs-machine/src/worker/mod.rs),
[`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs)).

## Invariants

| Rule | Enforced by |
| --- | --- |
| SQLite Store calls run on the Store's kernel: in process on `native` (macOS, Linux), else in that machine's worker. | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) `plan`, `local`; executor per platform in [`layerfs-platform` `platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |
| PostgreSQL Store calls run in the caller's process; the password is resolved per call and never written to a record. | `place::plan` (`StoreRef::Postgres` → `Local`); [`call.rs`](../../../../control/crates/layerfs-store/src/call.rs) `postgresql` |
| Workspace calls go to the daemon; the daemon serves no Store calls. | [`daemon/call.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/call.rs) ("Store calls are not daemon calls"); [`daemon/operator.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/operator.rs) admits one read-model method |
| A worker gets one attempt per call; nothing is resent. A lost answer after the request was sent is `Unknown`. | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) `send_once`, `answer`, `lost` |
| No call starts a machine except `machine.start`; an unreached worker is `Unavailable` naming it. | `place.rs` `recorded`, `stopped` (#535 D11) |
| A guest worker never reads the caller's home: each request carries the records it needs. | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) `with_home`; `sandbox/route.rs` `host_op` |
| A Sandbox host operation is one typed request (`host::Call` with `host::Host`, layerfs-project#108) on both routes; the `sandbox.host` frame and the one args map for layerfs-store and the daemon are built from it by one encoder, in the untyped request's field order. | [`host/call.rs`](../../../../control/crates/layerfs-sandbox/src/host/call.rs), [`host/frame.rs`](../../../../control/crates/layerfs-sandbox/src/host/frame.rs) |
| LayerFS reads `LAYERFS_*` settings only (`LAYERFS_HOME`, `LAYERFS_WORKER_LISTEN`, `LAYERFS_WORKER_SECRET`, …) and names no plugin. | [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) `home`, [`layerfs-cli/src/worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Worker of another protocol or bundle | `Unavailable` with `detail {worker, expected}`; nothing sent; replace it with `machine stop`, then `machine start` | [`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs) |
| Requests per worker connection | one, after the secret frame | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| Worker frame size | 512 MiB (`MAX_FRAME`); first frame 4 KiB (`MAX_HELLO`); larger is `TooLarge` | [`worker/frame.rs`](../../../../control/crates/layerfs-machine/src/worker/frame.rs) |
| Worker connect and secret-answer waits | 10 s each (`CONNECT`, `HELLO_WAIT`), then `Unavailable` | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| Worker wait between request bytes | 30 s (`REQUEST_WAIT`) | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| Machine with no `worker.json` | `Unavailable`: "start it with `layerfs machine start`" | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| Daemon connections per process per Sandbox | 6 (`CONNECTIONS`); the daemon admits 8 (`DAEMON_CONNECTIONS`) | [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs), [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| Wait for a free daemon connection | 60 s (`LEND_WAIT`), then `Busy`; nothing attempted | `daemon/pool.rs` |

## Demonstration

The commands below are written from source and were not run while writing
this page. None needs Docker or root: the workers are stand-ins served on a
loopback port by `worker::serve` in a test thread.

```text
 test --(SQLite Store on "box", platform lima)--> layerfs_api::call
                                                      |
             machine/box/worker.json {address, secret}|
                                                      v
 thread: worker::serve(listener, secret, guest root) <-- store.call frame
```

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`store_routes.rs`](../../../../api/crates/layerfs-api/tests/store_routes.rs) | `layerfs-api` | A Store call goes where its records say: a SQLite Store on a `lima` machine is sent to a stand-in worker with its record; a PostgreSQL Store resolves its `env:` password reference per call; leases are held, counted and cleaned. |
| [`worker_dial.rs`](../../../../api/crates/layerfs-api/tests/worker_dial.rs) | `layerfs-api` | A worker call dials the recorded worker once with no ping and never starts the machine: refused before sending is `Unavailable`, a lost answer is `Unknown`. |
| [`worker_frames.rs`](../../../../control/crates/layerfs-control/tests/worker_frames.rs) | `layerfs-control` | The secret frame comes first and small; an oversized frame is refused from its length; a wrong secret is refused before the request is read; a silent worker is `Unavailable` in time. |
| [`worker_hello.rs`](../../../../control/crates/layerfs-control/tests/worker_hello.rs) | `layerfs-control` | A worker of another protocol or bundle (or an old one whose hello names no protocol) is sent nothing; the matching worker answers. |

| Test | Command | Needs |
| --- | --- | --- |
| `store_routes` | `cargo +1.85.1 test --locked -p layerfs-api --test store_routes` | loopback TCP; one case changes with `--features machine` |
| `worker_dial` | `cargo +1.85.1 test --locked -p layerfs-api --features machine --test worker_dial` | `#![cfg(feature = "machine")]`: without the feature it builds no tests |
| `worker_frames` | `cargo +1.85.1 test --locked -p layerfs-control --test worker_frames` | loopback TCP |
| `worker_hello` | `cargo +1.85.1 test --locked -p layerfs-control --test worker_hello` | loopback TCP; sets `LAYERFS_BUNDLE` to a temporary folder |

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/22-sdk-runtime.md`](../../../../core/docs/architecture/22-sdk-runtime.md): historical only (the host-mediated runtime it describes is retired).
- [`core/docs/architecture/82-sdk-call-channel.md`](../../../../core/docs/architecture/82-sdk-call-channel.md): historical for front ends (the 2.0 call table replaces its channel); its daemon control exchange is covered in [Daemon channel](daemon-channel.md).

---

← [Management: Doctor](../management/doctor.md) · [Up: Control](../README.md) · [Routing and workers: Routing table](routing-table.md) →
