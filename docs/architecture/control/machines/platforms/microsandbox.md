# microsandbox

[Architecture](../../../README.md) › [Control](../../README.md) › [Machines](../README.md) › microsandbox

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The microsandbox platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](../README.md) |
| Source | [`control/crates/layerfs-platform/src/platforms/microsandbox`](../../../../../control/crates/layerfs-platform/src/platforms/microsandbox) |

## How it is made

A created machine: one `msb` sandbox record (a libkrun microVM) whose data
disk is a named `disk` volume that survives stop and start. The name is its
only identity; every read is `msb inspect` by name
([`microsandbox/observe.rs`](../../../../../control/crates/layerfs-platform/src/platforms/microsandbox/observe.rs)).

```text
 create(spec, bundle)                       microsandbox/create.rs
   `msb` on the search path                  else Unsupported
   msb inspect --format json <vm>   present -> Exists
   port = spec.state.port or a free host port
   msb volume list     fails -> Failed;  lists <vm>-data -> Exists
   msb create --name <vm> --cpus N --memory <M>M
     --mount-named <vm>-data:/layerfs/data:kind=disk,size=<disk>M
     -p 127.0.0.1:<port>:7070  --net public,private,host
     --label layerfs.role=machine  <os_image | debian:trixie-slim>
       fails, nothing made: msb volume remove (one cleanup), Failed
       fails, sandbox exists: Failed, vm_exists (kept)
   msb start <vm> (unless running)
   worker::bring_up over `msb exec <vm> --no-tty --`:
     prepare -> install bundle -> launch on 0.0.0.0:7070
     -> ready at host 127.0.0.1:<port>
```

| Call | Does | Source |
| --- | --- | --- |
| `start` | needs the recorded port; `NotFound` if missing; `msb start` unless running; `bring_up` | [`microsandbox/power.rs`](../../../../../control/crates/layerfs-platform/src/platforms/microsandbox/power.rs) |
| `stop` | `msb stop` if running | same |
| `remove` | `msb remove --force`, then `msb volume remove` (a "not found" volume is fine) | [`microsandbox/create.rs`](../../../../../control/crates/layerfs-platform/src/platforms/microsandbox/create.rs) |
| `status` | `status` `running`; `stopped` or `created` read as stopped; else failed; through `with_worker` | `observe.rs` |
| `forward` | `Unsupported`: msb publishes a port only at create | [`microsandbox/adapter.rs`](../../../../../control/crates/layerfs-platform/src/platforms/microsandbox/adapter.rs) |

`--net public,private,host` lets the guest reach the host, private
addresses and the internet. msb denies the host and private addresses by
default and cannot allow them after create.

## Backend and kernel route

| Host | `backend` | `kernel` | `description` |
| --- | --- | --- | --- |
| macos | `namespaces` | `fuse` | a microsandbox microVM (libkrun); Sandboxes run in Linux namespaces and a cgroup |
| linux | `namespaces` | `fuse` | same |
| windows | `namespaces` | `fuse` | same |

The profile's `hosts` is every named host; it is the one created platform
the table allows off macOS.

## Where Store calls run

| Host | `sqlite` | `postgresql.store_calls` |
| --- | --- | --- |
| macos | `worker` | `in_process` |
| linux | `worker` | `in_process` |
| windows | `worker` | `in_process` |

Workspace calls also go through the worker (`forward` is `Unsupported`).

## PostgreSQL reach

| Host | `postgresql.sandbox` | `postgresql.reason` |
| --- | --- | --- |
| macos | `true` | `null` |
| linux | `true` | `null` |
| windows | `true` | `null` |

`reach` rewrites a loopback host to `host.microsandbox.internal`, the
guest's name for its /30 gateway (`HOST_ALIAS`). It reaches the host
because create allows host egress.

## Options

| Option | Kind | `Spec` field | Default |
| --- | --- | --- | --- |
| `cpus` | int | `cpus` | set by the caller |
| `memory` | size | `memory` | set by the caller |
| `disk` | size | `disk` (the `disk` volume) | set by the caller |
| `image` | text | see [Options](../platforms-and-profiles.md#options) | `os_image` default `debian:trixie-slim` |

`Tools::from_env` adds `~/.microsandbox/bin` after `PATH`, where msb's
installer puts it ([`spec.rs`](../../../../../control/crates/layerfs-platform/src/spec.rs)).

## Live receipts

All on a macOS host. Not run on Linux or Windows.

| Receipt | What ran |
| --- | --- |
| [g7-reach](../../../../../core/docs/issues/535/receipts/g7-reach/RECEIPT.md) | host reach probed by hand with this platform's flags |
| [g7-live](../../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md) | PostgreSQL Sandboxes reaching a server in Docker Desktop |
| [K2](../../../../../core/docs/issues/535/receipts/fe6f85013b/K2/RESULTS.md) | first terminal pass (msb 0.7.11); remote PostgreSQL failed (D1, fixed by private egress) |
| [L2](../../../../../core/docs/issues/535/receipts/35ed671b6c/L2/RESULTS.md) | final pass: sqlite, PostgreSQL loopback and remote cells, many Stores, power |
| [R2](../../../../../core/docs/issues/535/receipts/3e257b91c3/R2/RESULTS.md) | rerun after the L2-D1 fix |
| [32aec70a64 lead](../../../../../core/docs/issues/535/receipts/32aec70a64/lead/RESULTS.md) | `machine remove` refused `InUse` with `detail.related` |
| [TERMINAL-PASS](../../../../../core/docs/issues/535/TERMINAL-PASS.md) | the merged verdict |

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: tool:msb` | `msb` is neither on `PATH` nor in `~/.microsandbox/bin` | adapter probe, `spec.rs` |
| `Failed`: "msb volume list failed, so the data volume … could not be checked" | msb did not list volumes | `create.rs` |
| `Exists`: "the volume <vm>-data already exists; it is left alone" | a volume from an earlier machine | `create.rs` |
| `Failed`: "msb inspect's answer names no status" | an msb version with another JSON | `observe.rs` |
| the guest cannot reach a host or LAN server | the machine was made without host or private egress; msb cannot add it later, so create it again | `create.rs` comment |

## Related

- [Sandboxes: Native and namespaces](../../sandboxes/native-and-namespaces.md).
- [Network: Workers](../../network/workers.md), [Network: Reach](../../network/reach.md), [Network: Forwards](../../network/forwards.md).
- [Adapter](../adapter.md), [Support matrix: Table](../../support-matrix/table.md).

---

← [Machines: Apple container](apple-container.md) · [Up: Machines](../README.md) · [Machines: Lima](lima.md) →
