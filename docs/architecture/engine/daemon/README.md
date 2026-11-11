# Daemon

[Architecture](../../README.md) › [Engine](../README.md) › Daemon

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Workspace daemon process: startup, control calls, install, Workspace lifetime and forced teardown, over the bridge wire.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-daemon`, `layerfs-bridge` |
| Source | [`engine/crates/layerfs-daemon/src`](../../../../engine/crates/layerfs-daemon/src), [`engine/crates/layerfs-bridge/src`](../../../../engine/crates/layerfs-bridge/src) |
| Uses | [Kernel routes](../kernel-routes/README.md), [Commit](../commit/README.md), [Store calls](../store-calls/README.md), [Workspace](../workspace/README.md), [Overlay](../overlay/README.md) |
| Used by | [Control: Sandboxes](../../control/sandboxes/README.md), [Control: Routing and workers](../../control/routing/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md), [A Commit](../../flows/commit.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Startup](startup.md) | Starting the daemon and reaching readiness. |
| [Control calls](control-calls.md) | Mount, status, export and unmount. |
| [Forced teardown](forced-teardown.md) | Forced unmount and what it reports. |
| [Bridge](bridge.md) | The wire between callers and the daemon. |
| [Operator](operator.md) | Operator calls and the filesystem port. |

## The picture

```text
 controller (holds the control_peer key)     commands in the Sandbox
        | TCP + Noise KK  (Bridge)             | ordinary file calls
        v                                      v
 +------------------------ layerfs-daemon ----------------------------+
 | listener -> slots[connections + 1] -> one worker thread each        |
 |   first record: InitialRecord::Install | Control(Call)  (Startup)  |
 |                                                                    |
 | Application: phase, Store open or install, reserved installer slot |
 |        |                                                           |
 |        v                                                           |
 | Service: registry WorkspaceId -> Entry          (Control calls)    |
 |   Mount, Attach, Commit, Status, Locate, Cleanup, Unmount,         |
 |   ForceUnmount (Forced teardown), backend.* (Operator)             |
 |        |               |                  |                        |
 |        v               v                  v                        |
 |   Store (global,   Owner: overlay     KernelServing (FUSE, Linux)  |
 |   opened direct)   template +         | ExportServing (NFS)        |
 |                    <n>.sqlite each       kernel requests ->        |
 |                                          filesystem port (Operator)|
 +--------------------------------------------------------------------+
```

## What it does

| The daemon owns | Source |
| --- | --- |
| One global Store, opened directly at startup or after a one-time install | [`application/owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs), [`application/serve.rs`](../../../../engine/crates/layerfs-daemon/src/application/serve.rs) |
| The overlay template and one overlay file and connection per mounted Workspace | [Overlay](../overlay/README.md), [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Kernel serving: the shared FUSE dispatcher (Linux) or one loopback NFSv3 export per Workspace | [`control/serving.rs`](../../../../engine/crates/layerfs-daemon/src/control/serving.rs), [`control/export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| The control registry and each Workspace's binding, activity and kernel half | [`control/registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |

| The daemon never | Where |
| --- | --- |
| Launches or supervises a command. The binary takes only `--config`; "no launcher or shell command mode". Sandbox or an external executor owns launch, streams, exit status and cancellation. | [`application/cli.rs`](../../../../engine/crates/layerfs-daemon/src/application/cli.rs), [`lib.rs`](../../../../engine/crates/layerfs-daemon/src/lib.rs) |
| Serves Store calls. An SDK call off the operator table is answered `SdkRefused` (`UnknownMethod`); `Fork`, `History` and `Branches` are refused `Invalid`. Store calls run in the caller's process. | [`control/operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |

| Control call | Effect | Topic |
| --- | --- | --- |
| `Mount` | Bind a Workspace to a Branch: overlay file, Store reader, registry entry | [Control calls](control-calls.md#attach-and-detach) |
| `Attach` | Make the kernel mount; Ready only when every receive loop serves | [Control calls](control-calls.md#attach-and-detach) |
| `Commit` | Capture, construct, merge or publish, install | [Commit](../commit/README.md) |
| `Status`, `Locate`, `Cleanup` | Observe; settle nothing | [Control calls](control-calls.md#status-and-inspect) |
| `Unmount` | Dirty check, detach, drain, revoke, close, delete the file | [Control calls](control-calls.md#unmount) |
| `ForceUnmount` | One abort write, then the same teardown | [Forced teardown](forced-teardown.md) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| The daemon supervises no commands; Sandbox owns launch and exit status. | [`application/cli.rs`](../../../../engine/crates/layerfs-daemon/src/application/cli.rs), [`lib.rs`](../../../../engine/crates/layerfs-daemon/src/lib.rs), [repository mental model](../../../../AGENTS.md) |
| Terminal unmount detaches once, drains requests, revokes the engine mount, runs one logical Close, then retires the namespace: checked connection close and file deletion. | [`control/detach.rs`](../../../../engine/crates/layerfs-daemon/src/control/detach.rs), [`control/unmount.rs`](../../../../engine/crates/layerfs-daemon/src/control/unmount.rs), [`backing.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/backing.rs) |
| Whole operations run outside the registry mutex; admission is one short step that marks the entry's activity. A Mount's admission also reads each live Workspace's retained maintenance failure in that step, so it waits for a job running on that Workspace's connection. | [`control/registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs), [`control/operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| One attempt per operation. A stop after an effect keeps the exact owner in the entry (`Kernel::Retained`) and reports it; nothing is retried, replayed or settled by a later read. | [`control/detach.rs`](../../../../engine/crates/layerfs-daemon/src/control/detach.rs), [`control/types.rs`](../../../../engine/crates/layerfs-daemon/src/control/types.rs) |
| Command and daemon identities differ, except on the NFS route. | [`application/config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |
| A Store install must be Disposable. | [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Readiness waits are not retries | `Hello { wait_for_store }` waits on a condition variable for the startup phase; Capture/install frontiers wait before effect; nothing is re-sent | [`application/owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs), [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |
| Attach and drain observation deadlines | `READY_WAIT` 5 s, `DRAIN_WAIT` 5 s; expiry keeps owners `Retained`, cancels nothing | [`application/filesystem.rs`](../../../../engine/crates/layerfs-daemon/src/application/filesystem.rs) |
| NFS helper | `HELPER_WAIT` 60 s, then killed and reported | [`control/export_helper.rs`](../../../../engine/crates/layerfs-daemon/src/control/export_helper.rs) |
| Connections | `DaemonLimits.connections` plus one reserved installer slot; a socket with no free slot is dropped | [`application/connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs) |
| Workspaces | `DaemonLimits.namespaces`; one more Mount is `Capacity` | [`control/operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Forced unmount leaves | `Retained` with a `TeardownStage` and `ForcedFacts` when a step is not established: a mount may stay aborted and mounted; Commit knowledge is `Absent`, `Published` or `Unknown` | [`control/force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs), [Forced teardown](forced-teardown.md) |
| Forced unmount refused | No abort control bound (`force:capability`), an active operation, or unrelinquished Commit custody | [`control/force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |
| Dirty Unmount | Refused with at most 256 paths unless `discard` | [`control/dirty.rs`](../../../../engine/crates/layerfs-daemon/src/control/dirty.rs) |
| Platform | FUSE serving on Linux only; macOS serves NFS only | [`application/filesystem.rs`](../../../../engine/crates/layerfs-daemon/src/application/filesystem.rs) |

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`control_root.rs`](../../../../engine/crates/layerfs-daemon/tests/control_root.rs) | `layerfs-daemon` | The FUSE control tree a daemon binds abort controls from is mounted once, reused and never adopted; inside a user namespace the mount is refused and leaves no directory |
| [`forced_unmount.rs`](../../../../engine/crates/layerfs-daemon/tests/forced_unmount.rs) | `layerfs-daemon` | Forced teardown while an external process holds a working directory or a descriptor: the mount is aborted and retained, no process is signalled, and a reference in mount A never holds sibling B |
| [`host_install.rs`](../../../../engine/crates/layerfs-daemon/tests/host_install.rs) | `layerfs-daemon` | Authenticated one-time install streams a sealed Project Init Store, opens it and reads the complete root; an existing Store is refused before upload; an interrupted transfer keeps its partial file and blocks another installer |
| [`duplex.rs`](../../../../engine/crates/layerfs-bridge/tests/duplex.rs) | `layerfs-bridge` | Noise KK duplex: records beyond one window with fixed buffers, wrong peer refused, malformed or corrupted records quarantine, terminal EOF and fence outcomes |

```text
cargo +1.85.1 test --locked -p layerfs-daemon --test control_root
cargo +1.85.1 test --locked -p layerfs-daemon --test forced_unmount
cargo +1.85.1 test --locked -p layerfs-daemon --test host_install
cargo +1.85.1 test --locked -p layerfs-bridge --test duplex
```

| Test | Needs (from source) |
| --- | --- |
| `control_root` | `#![cfg(target_os = "linux")]`; root with `CAP_SYS_ADMIN`; `unshare` for the refusal case, which re-runs an `#[ignore]` test inside `unshare -Urm` |
| `forced_unmount` | `#![cfg(target_os = "linux")]`; root, `/dev/fuse`, `python3`; mounts `fusectl` itself; command identity uid 65534; one mounted test at a time per process |
| `host_install` | No `cfg` gate; Unix APIs, a temporary directory and loopback TCP; no root or Docker |
| `duplex` | Cargo feature `duplex` (default); loopback TCP; on aarch64 the repository's ARMv8 AEAD build flags |

These commands were not run while writing this page. Linux-only daemon test
targets run in the CI mount lane ([`tools/ci_tests.py`](../../../../tools/ci_tests.py)).

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/21-daemon-owner.md`](../../../../core/docs/architecture/21-daemon-owner.md): historical only; its fair owner thread, queue and shared budget were replaced by one lock per Workspace (see [Overlay: Database](../overlay/database.md#statements)).
- [`core/docs/architecture/45-runtime-supervision.md`](../../../../core/docs/architecture/45-runtime-supervision.md): historical only; the host-mediated transport is retired.
- [`core/docs/architecture/47-daemon-upstream.md`](../../../../core/docs/architecture/47-daemon-upstream.md): historical only; the daemon upstream is retired.
- [`core/docs/architecture/70-daemon-application-startup-control.md`](../../../../core/docs/architecture/70-daemon-application-startup-control.md): covered in [Startup](startup.md) and [Control calls](control-calls.md).
- [`core/docs/architecture/80-forced-teardown.md`](../../../../core/docs/architecture/80-forced-teardown.md): covered in [Forced teardown](forced-teardown.md).

---

← [Kernel routes: Coherence](../kernel-routes/coherence.md) · [Up: Engine](../README.md) · [Daemon: Startup](startup.md) →
