# Apple container

[Architecture](../../../README.md) › [Control](../../README.md) › [Machines](../README.md) › Apple container

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Apple container platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](../README.md) |
| Source | [`control/crates/layerfs-platform/src/platforms/apple_container`](../../../../../control/crates/layerfs-platform/src/platforms/apple_container) |

## How it is made

A created machine: one container of Apple's `container` CLI, macOS on
Apple silicon only. Its main process is a shell that sleeps and ends on
SIGTERM; its data disk is a named volume
([`apple_container/observe.rs`](../../../../../control/crates/layerfs-platform/src/platforms/apple_container/observe.rs)).

```text
 create(spec, bundle)                       apple_container/create.rs
   host_check: macOS, aarch64, `container`   else Unsupported + missing
   container inspect <vm>           present -> Exists
   container volume inspect <vm>-data  present -> Exists
   port = spec.state.port or a free host port
   container volume create -s <disk>M <vm>-data
   container run --detach --name <vm> --cpus N --memory <M>M
     --cap-add SYS_ADMIN --cap-add SYS_RESOURCE --cap-add NET_ADMIN
     --mount type=volume,source=<vm>-data,target=/layerfs/data
     --publish 127.0.0.1:<port>:7070  --label layerfs.role=machine
     <os_image | debian:trixie-slim>  sh -c "trap 'exit 0' TERM; ..."
       fails, no container:  volume delete (one cleanup), Failed
       fails, container exists: Failed, vm_exists (kept)
   worker::bring_up over `container exec`:
     prepare -> install bundle -> launch on 0.0.0.0:7070
     -> ready at host 127.0.0.1:<port>
```

| Call | Does | Source |
| --- | --- | --- |
| `start` | needs the recorded port; `NotFound` if missing; `container start` if stopped or failed; `bring_up` | [`apple_container/power.rs`](../../../../../control/crates/layerfs-platform/src/platforms/apple_container/power.rs) |
| `stop` | `container stop` if running | same |
| `remove` | `container delete --force`, then `container volume delete` | [`apple_container/create.rs`](../../../../../control/crates/layerfs-platform/src/platforms/apple_container/create.rs) |
| `status` | `container inspect`: `status.state` (or `status`) `running`/`stopped`, else failed; through `with_worker` | `observe.rs` |
| `guest` | `container exec [-i] <vm> …` | [`setup/guest.rs`](../../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| `forward` | `Unsupported`: the CLI publishes a port only at create | [`apple_container/adapter.rs`](../../../../../control/crates/layerfs-platform/src/platforms/apple_container/adapter.rs) |

The guest's `/proc/sys` is read-only until remounted; the shared
preparation remounts it before setting `user.max_user_namespaces=0`. No
host folder is mounted.

## Backend and kernel route

| Host | `backend` | `kernel` | `description` |
| --- | --- | --- | --- |
| macos | `namespaces` | `fuse` | an Apple container VM (Apple silicon); Sandboxes run in Linux namespaces and a cgroup |
| linux | `null` | `null` | an Apple container VM (Apple silicon) |
| windows | `null` | `null` | an Apple container VM (Apple silicon) |

## Where Store calls run

| Host | `sqlite` | `postgresql.store_calls` |
| --- | --- | --- |
| macos | `worker` | `in_process` |
| linux | `null` | `in_process` |
| windows | `null` | `in_process` |

Workspace calls also go through the worker: `forward` cannot publish a
Sandbox daemon's port after create.

## PostgreSQL reach

| Host | `postgresql.sandbox` | `postgresql.reason` |
| --- | --- | --- |
| macos | `true` | `null` |
| linux | `false` | "apple-container runs on macOS hosts only" |
| windows | `false` | "apple-container runs on macOS hosts only" |

`reach` uses `Loopback::Lookup`: only for a loopback URL, it reads the
gateway `status.networks[0].ipv4Gateway` from `container inspect` while the
machine runs (192.168.64.1 on vmnet's default network in the G7 receipt)
and puts it in the URL. The guest has no host alias. A host server is
reached there only when it listens on that bridge (`0.0.0.0`), not on
`127.0.0.1` alone (#535 D13).

## Options

| Option | Kind | `Spec` field | Default |
| --- | --- | --- | --- |
| `cpus` | int | `cpus` | set by the caller |
| `memory` | size | `memory` | set by the caller |
| `disk` | size | `disk` (the volume) | set by the caller |
| `image` | text | see [Options](../platforms-and-profiles.md#options) | `os_image` default `debian:trixie-slim` |

## Live receipts

| Receipt | What ran |
| --- | --- |
| [g7-reach](../../../../../core/docs/issues/535/receipts/g7-reach/RECEIPT.md) | the gateway route probed by hand with this platform's flags |
| [g7-live](../../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md) | PostgreSQL Sandboxes reaching a server in Docker Desktop |
| [K2](../../../../../core/docs/issues/535/receipts/fe6f85013b/K2/RESULTS.md) | first terminal pass (container CLI 1.6.1) |
| [L2](../../../../../core/docs/issues/535/receipts/35ed671b6c/L2/RESULTS.md) | final pass: sqlite, PostgreSQL loopback and remote cells, power, worker trace |
| [R2](../../../../../core/docs/issues/535/receipts/3e257b91c3/R2/RESULTS.md) | rerun after the L2-D1 fix |
| [TERMINAL-PASS](../../../../../core/docs/issues/535/TERMINAL-PASS.md) | the merged verdict |
| [machines-live](../../../../../core/docs/issues/project-113/receipts/fb02aa0666/machines-live/RESULTS.md) | release CLI on container 1.6.1 (2026-11-07): create, start, sqlite smoke through the worker, `workspace exec-cancel` and Busy/force, stop and start |

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: host:macos` / `host:apple-silicon` | not a Mac, or not `aarch64` | `observe.rs` `host_check`, adapter probe |
| `missing: tool:container` | no `container` CLI on the search path | same |
| `Exists`: "the volume <vm>-data already exists; it is left alone" | a volume from an earlier machine | `create.rs` |
| `Failed`: "the container CLI's inspect answer is not the shape this adapter knows" | a CLI version with another `inspect` JSON | `observe.rs` |
| `Unavailable`: "… reports no network gateway …; it must be running" | `reach` on a stopped machine | `observe.rs` `gateway` |
| a loopback PostgreSQL server is not reached | it listens on `127.0.0.1` only | `observe.rs` comment, #535 D13 |
| `Unsupported`: "the guest of … still allows user namespaces" | the guest refused the sysctl | [`setup/worker.rs`](../../../../../control/crates/layerfs-platform/src/setup/worker.rs) |

## Related

- [Sandboxes: Native and namespaces](../../sandboxes/native-and-namespaces.md): the `namespaces` backend in the guest.
- [Network: Workers](../../network/workers.md), [Network: Reach](../../network/reach.md), [Network: Forwards](../../network/forwards.md).
- [Adapter](../adapter.md), [Support matrix: Table](../../support-matrix/table.md).

---

← [Machines: Docker](docker.md) · [Up: Machines](../README.md) · [Machines: Lima](lima.md) →
