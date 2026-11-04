# Native

[Architecture](../../../README.md) › [Connect](../../README.md) › [Machines](../README.md) › Native

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Native platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](../README.md) |
| Source | [`connect/crates/layerfs-machine/src/platforms/native`](../../../../../connect/crates/layerfs-machine/src/platforms/native), [`probe.rs`](../../../../../connect/crates/layerfs-machine/src/probe.rs) (`native_missing`) |

## How it is made

```text
 caller's process (CLI, Node, Python)            this host
 +-------------------------------------------+
 | sqlite Store calls run here (in_process)  |
 | PostgreSQL Store calls run here           |
 +----------------+--------------------------+
                  | sandbox.create (Sandboxes backend)
                  v
 Sandbox daemon: macOS  this user's processes, loopback NFS
                 Linux  Linux namespaces and a cgroup, FUSE
                 listens on 127.0.0.1:<port>  <-- Workspace calls dial it
```

`native` is built in: the host itself, one per home, never created or
removed ([`native/adapter.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/native/adapter.rs)).

| Call | Answer | Source |
| --- | --- | --- |
| `create`, `remove` | `InvalidInput`: "is built in and never created" / "… never removed" | `native/adapter.rs` |
| `connect` | `InvalidInput`: "is created, not connected" | [`setup/calls.rs`](../../../../../connect/crates/layerfs-machine/src/setup/calls.rs) |
| `start` | `Ok(None)` on macOS and Linux (the route is in-process); the adapter's `start` is reached only on Windows and answers `Unsupported` | `setup/calls.rs`, [`native/power.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/native/power.rs) |
| `stop` | `Ok(())`, nothing to stop | `native/adapter.rs` |
| `status` | `running`, no `free_disk`, `down_reason` "missing: …" when the host lacks something | [`native/observe.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/native/observe.rs) |
| `guest`, `copy_in`, `copy_out` | `InvalidInput`: "runs in-process; its files are the host's" | `native/adapter.rs` |
| `host_identity` | `InvalidInput`: "has no ssh host" | `native/adapter.rs` |
| `forward` | `127.0.0.1:<guest port>`, no pid | `setup/forward.rs` `shared` |

## Backend and kernel route

The rows of [`SUPPORT.json`](../../../../../connect/SUPPORT.json) for `native`:

| Host | `backend` | `kernel` | `description` |
| --- | --- | --- | --- |
| macos | `native` | `nfs` | this host; Sandboxes run as this user's processes, served over loopback NFS |
| linux | `namespaces` | `fuse` | this host; Sandboxes run in Linux namespaces and a cgroup |
| windows | `null` | `null` | this host |

The macOS backend is `Backend::HostProcess`, stored and shown as `native`
(#535 G6). Windows has no backend (§13 O5), from
[`native/profile.rs`](../../../../../connect/crates/layerfs-machine/src/platforms/native/profile.rs).

## Where Store calls run

| Host | `sqlite` | `postgresql.store_calls` |
| --- | --- | --- |
| macos | `in_process` | `in_process` |
| linux | `in_process` | `in_process` |
| windows | `worker` | `in_process` |

`native` is the only platform whose sqlite Store calls run in the caller's
process (`ExecutorKind::Local`). On Windows the profile routes them to a
WSL2 worker that is not designed: `start` answers `Unsupported`, "the
native machine <name> on this host runs its Stores in a WSL2 worker, which
is not designed yet".

## PostgreSQL reach

| Host | `postgresql.sandbox` | `postgresql.reason` |
| --- | --- | --- |
| macos | `true` | `null` |
| linux | `true` | `null` |
| windows | `false` | "no Sandbox backend on Windows (O5)" |

`reach` uses `Loopback::Shared`: the daemon shares the caller's loopback,
so the URL is used exactly as given, with none of the checks other
platforms make ([`reach.rs`](../../../../../connect/crates/layerfs-machine/src/reach.rs)).

## Options

None: `PROFILE.options` is empty. `machine update native` settings
(daemon path, image) are records of `layerfs-control`, not `Spec` fields
of this crate.

## Live receipts

| Receipt | Host | What ran |
| --- | --- | --- |
| [K1](../../../../../core/docs/issues/535/receipts/fe6f85013b/K1/RESULTS.md) | macOS | first #535 terminal pass, `native` and `docker` |
| [L1](../../../../../core/docs/issues/535/receipts/35ed671b6c/L1/RESULTS.md) | macOS | final pass: sqlite and PostgreSQL loopback cells, many Stores, `machine platforms` vs `SUPPORT.json` |
| [K3](../../../../../core/docs/issues/535/receipts/fe6f85013b/K3/RESULTS.md) | Linux (a Lima VM standing in for a Linux host) | first pass, Linux rows |
| [L3](../../../../../core/docs/issues/535/receipts/35ed671b6c/L3/RESULTS.md) | Linux (same) | final pass: Linux `native` sqlite and PostgreSQL remote cells, Linux rows vs `SUPPORT.json` |
| [TERMINAL-PASS](../../../../../core/docs/issues/535/TERMINAL-PASS.md) | both | the merged verdict |

Windows: no receipt; the crate is `#![cfg(unix)]` and Windows is not built.

## Troubleshooting

| Symptom | Cause | Source |
| --- | --- | --- |
| `missing: host:root` | Linux caller is not root | `native_missing` in [`probe.rs`](../../../../../connect/crates/layerfs-machine/src/probe.rs) |
| `missing: host:/dev/fuse` | no FUSE device on Linux | same |
| `missing: host:cgroup2` | no `/sys/fs/cgroup/cgroup.controllers` | same |
| `missing: host:macos-or-linux` | a Windows or other host | same |
| `status` is `running` with a `down_reason` | the host lacks one of the above; `native` still answers `running` | `native/observe.rs` |
| a loopback PostgreSQL URL fails | not a machine problem: the URL is used as given | `reach.rs` |

## Related

- [Sandboxes: Native and namespaces](../../sandboxes/native-and-namespaces.md): the backends that run here.
- [Kernel routes](../../../engine/kernel-routes/README.md): [NFS](../../../engine/kernel-routes/nfs.md) on macOS, [FUSE](../../../engine/kernel-routes/fuse.md) on Linux.
- [Network: Reach](../../network/reach.md), [Network: Forwards](../../network/forwards.md).
- [Support matrix: Table](../../support-matrix/table.md).

---

← [Machines: Adapter](../adapter.md) · [Up: Machines](../README.md) · [Machines: Docker](docker.md) →
