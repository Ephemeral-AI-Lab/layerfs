# Network

[Architecture](../../README.md) › [Control](../README.md) › Network

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How callers, workers and daemons reach each other: worker setup, PostgreSQL reach and port forwards.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-platform` |
| Source | [`control/crates/layerfs-platform/src/reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs), [`control/crates/layerfs-platform/src/setup`](../../../../control/crates/layerfs-platform/src/setup); the worker's server and client are in [`worker`](../../../../control/crates/layerfs-machine/src/worker) |
| Uses | — |
| Used by | [Machines](../machines/README.md), [Control: Routing and workers](../routing/README.md) |
| Flows | [A call, end to end](../../flows/call.md), [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Workers](workers.md) | Machine workers and how calls reach them. |
| [Reach](reach.md) | How a Sandbox daemon reaches the caller's PostgreSQL server. |
| [Forwards](forwards.md) | Port forwards that let Workspace calls skip the worker. |
| [Credentials](credentials.md) | Where a PostgreSQL password is resolved and how it travels. |

## The picture

```text
 caller's host                          machine (VM or container)
+--------------------------+           +-----------------------------------+
| caller process           |           |  layerfs worker  (one per machine)|
|  secrets::url  (password |  Workers  |   root /layerfs/data              |
|   resolved here only)    |---------->|   serves store.call, store.file,  |
|  layerfs_platform::reach  | 127.0.0.1 |   sandbox.host, project.*         |
|   (URL rewritten here)   | :<port>   |        |                          |
|                          | + secret  |        | starts                   |
|                          |           |        v                          |
|                          | Forwards  |  Sandbox daemon                   |
|                          |---------->|   127.0.0.1:<guest_port>          |
|                          | 127.0.0.1 |   (Workspace calls)               |
+--------------------------+ :<free>   +-----------------|-----------------+
                                                         | Reach: dials the
                                                         v rewritten URL
                                        PostgreSQL server (caller's loopback
                                        under a host alias, or any host)
```

The worker carries machine-side calls. A forward, where the platform has
one, carries Workspace calls straight to a Sandbox daemon. Reach decides the
URL a daemon dials for a PostgreSQL Store. See the
[glossary](../../glossary.md) for Store, Sandbox, worker and daemon.

## What it does

| Question | Answer | Source |
| --- | --- | --- |
| Where does a worker listen? | The caller always dials a host loopback address (`Endpoint::address`). In the guest the worker binds port 7070 on the platform's `worker_bind`; the provider's port publish, Lima's port forward or an `ssh -L` (`firecracker`, through its Lima host) carries it to the host's `127.0.0.1`. | [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs), [`worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| How does a call reach it? | One new TCP connection per request: the secret first, alone, then one request frame, then events and one answer. | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| How does a Sandbox daemon dial the caller's PostgreSQL server? | `machine::reach` keeps any non-loopback host and rewrites a loopback host to the platform's alias, or refuses. | [`reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs) |
| When do Workspace calls skip the worker? | When `machine::forward` answers an address: `native` (the daemon's own port) and `lima` (a detached `ssh -L`). Elsewhere they go through the worker. | [`calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| Workers are reached on a host loopback address, behind a 256-bit secret in a 0600 guest file. | `tool::secret`, `Guest::write_file`, `worker::launch` ([`tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs), [`guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs), [`worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs)); the server compares the secret before it reads a request ([`server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs)) |
| One attempt per call: no retry, no replay. A request whose answer is lost is `Unknown`, never resent. | `send_once`, `answer`, `lost` in [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs); `tool::run` runs a host command once ([`tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs)) |
| Readiness waits are waits, not retries. | `worker::ready` ([`worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs)) |
| A PostgreSQL password is resolved only in the caller and travels only in one worker request and the Sandbox's private daemon setup file. | [Credentials](credentials.md) |
| No reach or forward message holds any part of a PostgreSQL URL. | `reach::refuse`; `PostgresUrl` has no `Debug` ([`reach.rs`](../../../../control/crates/layerfs-platform/src/reach.rs), [`postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/postgres_url.rs)) |
| A recorded helper pid is ended only while its command line still holds its marker. | `tool::alive`, `tool::end` ([`tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Requests per worker connection | One; the connection then closes. | [`server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| Largest frame / first frame | 512 MiB (`MAX_FRAME`) / 4 KiB (`MAX_HELLO`); a larger request is `TooLarge` before anything is sent. | [`worker/frame.rs`](../../../../control/crates/layerfs-machine/src/worker/frame.rs) |
| Worker readiness wait | 60 s (`READY`), looking every 200 ms. | [`worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| Worker not reached, wrong secret, other build | `Unavailable` / `Denied` / `Unavailable`; the request was not sent. | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| Answer lost after the request was sent | `Unknown`: "it may have run, and was not resent". | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| Network facts verified live | The G7 live receipt covers `lima`, `apple-container` and `firecracker` reaching a PostgreSQL server, at an earlier commit with the support rows lifted locally (it also covered `microsandbox`, removed in 2.0). No G7 receipt covers `docker`. | [G7 live receipt](../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md) |

## Demonstration

These tests use stand-in tools (`support/standin.rs`): no VM or container
is reached. The commands were not run while writing this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`machine_reach.rs`](../../../../control/crates/layerfs-platform/tests/machine_reach.rs) | `layerfs-platform` | Each platform rewrites a loopback host (`native`, and `docker` on a Linux Engine, keep it); other hosts pass unchanged; `sslmode=verify-full` and unreadable URLs are refused; no message holds the password. |
| [`machine_forward.rs`](../../../../control/crates/layerfs-platform/tests/machine_forward.rs) | `layerfs-platform` | `native` answers the daemon's port; `lima` starts a stand-in `ssh -L` that `forward_close` ends; a forward that dies at once fails once; the others and port 0 are refused before any tool runs. |

- `machine_reach`: builds a `Spec` per platform; apple-container gets a
  stand-in `container inspect` that prints a gateway. Needs a Unix host
  (`#![cfg(unix)]`).
  `cargo +1.85.1 test --locked -p layerfs-platform --test machine_reach`
- `machine_forward`: a stand-in `ssh` that sleeps until it is ended; it is
  run once first so a new script's first-run cost does not eat the 600 ms
  settle time. Unix only.
  `cargo +1.85.1 test --locked -p layerfs-platform --test machine_forward`

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`control/README.md`](../../../../control/README.md): its reach and
  forward rules are covered in [Reach](reach.md) and
  [Forwards](forwards.md); the rest belongs to the layer and the
  [Machines](../machines/README.md) pages.
- [`docs/versioned/2.0.0/connect.md`](../../../versioned/2.0.0/connect.md):
  about the LayerFS Connect desktop app, not this layer; historical only
  for these pages.

---

← [Machines: Adding a platform](../machines/adding-a-platform.md) · [Up: Control](../README.md) · [Network: Workers](workers.md) →
