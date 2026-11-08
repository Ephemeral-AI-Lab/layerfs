# Machines

[Architecture](../../README.md) › [Control](../README.md) › Machines

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker is **permanently disabled for new work**. Its code and records
remain for inspection and explicit cleanup. Microsandbox and SSH machines
were removed in 2.0; a record an older home still holds for one is shown,
refused and only forgotten. See the [platform restriction](../support-matrix/disabled-platforms.md) for
refusals, existing-resource handling and the source change required for
re-enabling. Descriptions of Firecracker's adapter mechanics below are
retained implementation details, not selectable features.

Where Sandboxes run: one adapter per platform that probes, creates, starts, stops and removes a machine.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-platform` |
| Source | [`control/crates/layerfs-platform/src/platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs), [`control/crates/layerfs-platform/src/adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs), [`control/crates/layerfs-platform/src/platforms`](../../../../control/crates/layerfs-platform/src/platforms), [`control/crates/layerfs-platform/src/options.rs`](../../../../control/crates/layerfs-platform/src/options.rs), [`setup/`](../../../../control/crates/layerfs-platform/src/setup) |
| Uses | [Support matrix](../support-matrix/README.md), [Network](../network/README.md), [Sandboxes](../sandboxes/README.md) |
| Used by | [Control: Management](../management/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Platforms and profiles](platforms-and-profiles.md) | The Platform enum, its Profile and options. |
| [Adapter](adapter.md) | The sealed Adapter every platform implements. |
| [Native](platforms/native.md) | The Native platform. |
| [Docker](platforms/docker.md) | The Docker platform. |
| [Apple container](platforms/apple-container.md) | The Apple container platform. |
| [Lima](platforms/lima.md) | The Lima platform. |
| [Firecracker](platforms/firecracker.md) | The Firecracker platform. |
| [Adding a platform](adding-a-platform.md) | The recipe for a new platform. |

## The picture

```text
 layerfs-control machine calls (records, locks, the home)
      |  Spec + Bundle + Tools: every path and setting from the caller
      v
 setup/calls.rs   create start stop remove status
                  reach forward copy_in copy_out load_image
      |  spec.validate(), worker::check_bundle()   (before any effect)
      v
 adapter(spec.kind)  one match        Platform::profile()  one match
      |                                     |
      v                                     v
 platforms/<p>/adapter.rs  ADAPTER     platforms/<p>/profile.rs  PROFILE
   observe.rs create.rs power.rs            |
                                            v
      |                                support.rs rows (Support matrix)
      v
 setup/vm.rs      one provider CLI call, sizes, ports
 setup/guest.rs   a root shell in the guest (the Guest enum)
 setup/worker.rs  steps 2-4: prepare, install, launch, ready
 setup/forward.rs detached `ssh -N -L` (lima, firecracker)
      |
      v
 Running { endpoint: host loopback address + secret (+ forward pid),
           lasting: worker port, firecracker network }
      --> caller writes worker.json and record.json (step 5)
```

## What it does

| Platform | How | Made by | Its kernel | Folder |
| --- | --- | --- | --- | --- |
| `native` | built in | nothing: the host itself | this host | [`native/`](../../../../control/crates/layerfs-platform/src/platforms/native) |
| `docker` | built in | nothing: one privileged worker container on Docker's kernel, which runs namespaces Sandboxes (D-8) | Docker Desktop's VM or the Linux host | [`docker/`](../../../../control/crates/layerfs-platform/src/platforms/docker) |
| `apple-container` | created | `container run` | an Apple container VM | [`apple_container/`](../../../../control/crates/layerfs-platform/src/platforms/apple_container) |
| `lima` | created | `limactl create` from a template | a Lima `vz` VM | [`lima/`](../../../../control/crates/layerfs-platform/src/platforms/lima) |
| `firecracker` | created | scripts in a nested `lima` machine | a Firecracker microVM | [`firecracker/`](../../../../control/crates/layerfs-platform/src/platforms/firecracker) |

Microsandbox (`msb`) and SSH machines were removed in 2.0 (owner,
2026-11-08): no `Platform` variant, adapter or profile is left. A home's
record that still names one is handled above this crate, in
`layerfs-machine` ([`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs),
[`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs)):
shown by `machine list` and `get`, refused `Unsupported` elsewhere, and only
forgotten by `machine remove`. See
[disabled platforms](../support-matrix/disabled-platforms.md).

What each lifecycle call does, per platform ("refused" is `InvalidInput`
with the fixed text of [Adapter](adapter.md#the-sealed-adapter)):

| Platform | create | start | stop | remove |
| --- | --- | --- | --- | --- |
| `native` | refused | nothing (`None`) on macOS and Linux | nothing | refused |
| `docker` | refused | replace and run the worker container | `docker stop` the worker | refused |
| `apple-container` | volume, `container run`, worker | `container start`, worker | `container stop` | container, then volume |
| `lima` | disk, instance, boot, worker | `limactl start`, worker | `limactl stop` | instance, then disk |
| `firecracker` | disk, VM folder, boot, `ssh -L`, worker | boot, new `ssh -L`, worker | forward, then VM | forward, VM, folder, disk |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One crate-private sealed trait, one exhaustive match; no registry or factory. | `pub(crate) trait Adapter` with no default methods and `fn adapter()` in [`adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs); #535 G2; [control guide](../../../../control/AGENTS.md#extension-points) |
| One match for per-platform facts. | `Platform::profile()` in [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |
| The crate knows nothing of the LayerFS home. | [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs): `Spec` carries `vm_name`, `work_dir`, `tools` and lasting state; the caller writes every record ([`lib.rs`](../../../../control/crates/layerfs-platform/src/lib.rs)) |
| A spec is checked before any effect. | every call in [`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) starts with `spec.validate()` |
| A failure after the VM exists keeps it. | `MachineError::kept` / `vm_exists` in [`error.rs`](../../../../control/crates/layerfs-platform/src/error.rs); each `create.rs` removes only what it made when nothing exists under the name |
| One attempt; a readiness wait is not a retry. | `worker::ready`, `forward::open` (`ExitOnForwardFailure`, one `ssh`) |
| No guest mounts the host home. | Lima `mounts: []`; docker mounts its data volume and the bundle only (never the Engine socket); apple-container mounts only its data volume |
| No Sandbox command can create a user namespace. | Every VM guest: `PREPARE` in [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) writes `user.max_user_namespaces=0` and reads it back. Every namespaces command, in any guest: the seccomp filter in [`seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs). The `docker` worker never writes the sysctl (it would change the whole Docker host) and relies on the filter |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Host-only builds | the crate is `#![cfg(unix)]`; `layerfs-control` reaches it only through its `machine` feature, which the guest worker is built without | [`lib.rs`](../../../../control/crates/layerfs-platform/src/lib.rs), [`layerfs-control/Cargo.toml`](../../../../control/crates/layerfs-control/Cargo.toml) |
| Missing tool or host fact | probe pushes `tool:<cli>` or `host:<fact>`; create refuses `Unsupported` with `missing` | [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs), [Adapter](adapter.md#probe) |
| VM size | at least 1 CPU, 256M memory, 1G disk | `Spec::validate` |
| Names | a letter or digit, then `[A-Za-z0-9._-]`, at most 63 | `plain_name` in [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs) |
| Provider call deadlines | `CALL` 30 s, `CREATE` 120 s, `BOOT` 300 s | [`setup/vm.rs`](../../../../control/crates/layerfs-platform/src/setup/vm.rs) |
| Exit checks of a host command | every 1 ms at first, growing with its run (an eighth of it) to every 20 ms; the deadline and the kill are unchanged (#66) | `wait_for` in [`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| Guest command and copy | `EXEC` 60 s, `COPY` 600 s | [`setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| Worker readiness | at most `READY` 60 s; a worker that ends is reported with its log's end | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs) |
| Bundle | lacks `layerfs`, `layerfs-daemon` or `layerfs-sandbox-helper`: `InvalidInput`, `missing: bundle:<name>`; hash not hex: `InvalidInput` | `check_bundle` |
| Outcome not known | `Unknown`: everything it may have made is kept | [`error.rs`](../../../../control/crates/layerfs-platform/src/error.rs) |

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`machine_platform.rs`](../../../../control/crates/layerfs-platform/tests/machine_platform.rs) | `layerfs-platform` | Names parse back; backend, route, worker bind, hosts and options per platform and host. |
| [`machine_adapter.rs`](../../../../control/crates/layerfs-platform/tests/machine_adapter.rs) | `layerfs-platform` | Built-in machines refuse create and remove; every refusal before any tool runs. |
| [`conformance.rs`](../../../../control/crates/layerfs-platform/tests/conformance.rs) | `layerfs-platform` | Over `Platform::ALL`: profile vs support table and `SUPPORT.json`; each lifecycle call through the platform's own tool or its fixed refusal; `reach`; `forward`; options vs `Spec::validate`. |

```text
 Scratch dir:  bin/  stand-in tools (shell scripts that log and refuse)
               work/ the machine's work_dir
 Spec.tools.path = bin/   -->  no real VM, Engine, host or root
```

```text
cargo +1.85.1 test --locked -p layerfs-platform --test machine_platform
cargo +1.85.1 test --locked -p layerfs-platform --test machine_adapter
cargo +1.85.1 test --locked -p layerfs-platform --test conformance
```

Each file is `#![cfg(unix)]` and uses the stand-ins of
[`tests/support/standin.rs`](../../../../control/crates/layerfs-platform/tests/support/standin.rs).
The `forward` kit also needs the system `sh` and `ps`. The commands were not
run while writing this page. Live runs are the receipts each platform page
links.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`control/README.md`](../../../../control/README.md): its "Reading the
  tree" and "Adding things" are covered in [Platforms and
  profiles](platforms-and-profiles.md), [Adapter](adapter.md) and [Adding a
  platform](adding-a-platform.md); it stays the layer guide.
- [`docs/versioned/2.0.0/machines.md`](../../../versioned/2.0.0/machines-and-stores.md):
  the user manual for the same calls; its per-platform sections are
  covered in the platform pages, which follow source where they differ.

---

← [Support matrix: Generated JSON](../support-matrix/generated-json.md) · [Up: Control](../README.md) · [Machines: Platforms and profiles](platforms-and-profiles.md) →
