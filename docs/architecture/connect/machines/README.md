# Machines

[Architecture](../../README.md) › [Connect](../README.md) › Machines

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker and SSH are **permanently disabled for new work**. Their code
and records remain for inspection and explicit cleanup. See the
[platform restriction](../support-matrix/disabled-platforms.md) for refusals, existing-resource handling and
the source change required for re-enabling. Descriptions of their adapter
mechanics below are retained implementation details, not selectable features.

Where Sandboxes run: one adapter per platform that probes, creates, starts, stops and removes a machine.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Layer | [Connect](../README.md) |
| Crates | `layerfs-machine` |
| Source | [`connect/crates/layerfs-machine/src/platform.rs`](../../../../connect/crates/layerfs-machine/src/platform.rs), [`connect/crates/layerfs-machine/src/adapter.rs`](../../../../connect/crates/layerfs-machine/src/adapter.rs), [`connect/crates/layerfs-machine/src/platforms`](../../../../connect/crates/layerfs-machine/src/platforms), [`connect/crates/layerfs-machine/src/options.rs`](../../../../connect/crates/layerfs-machine/src/options.rs), [`setup/`](../../../../connect/crates/layerfs-machine/src/setup) |
| Uses | [Support matrix](../support-matrix/README.md), [Network](../network/README.md), [Sandboxes](../sandboxes/README.md) |
| Used by | [API: Control](../../api/control/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Platforms and profiles](platforms-and-profiles.md) | The Platform enum, its Profile and options. |
| [Adapter](adapter.md) | The sealed Adapter every platform implements. |
| [Native](platforms/native.md) | The Native platform. |
| [Docker](platforms/docker.md) | The Docker platform. |
| [Apple container](platforms/apple-container.md) | The Apple container platform. |
| [microsandbox](platforms/microsandbox.md) | The microsandbox platform. |
| [Lima](platforms/lima.md) | The Lima platform. |
| [Firecracker](platforms/firecracker.md) | The Firecracker platform. |
| [SSH](platforms/ssh.md) | The SSH platform. |
| [Adding a platform](adding-a-platform.md) | The recipe for a new platform. |

## The picture

```text
 layerfs-control machine calls (records, locks, the home)
      |  Spec + Bundle + Tools: every path and setting from the caller
      v
 setup/calls.rs   create connect start stop remove status
                  host_identity reach forward copy_in copy_out
      |  spec.validate(), worker::check_bundle()   (before any effect)
      v
 adapter(spec.kind)  one match        Platform::profile()  one match
      |                                     |
      v                                     v
 platforms/<p>/adapter.rs  ADAPTER     platforms/<p>/profile.rs  PROFILE
   observe.rs create.rs power.rs            |
   (ssh: connect.rs host_key.rs)            v
      |                                support.rs rows (Support matrix)
      v
 setup/vm.rs      one provider CLI call, sizes, ports
 setup/guest.rs   a root shell in the guest (the Guest enum)
 setup/worker.rs  steps 2-4: prepare, install, launch, ready
 setup/forward.rs detached `ssh -N -L` (ssh, lima, firecracker)
      |
      v
 Running { endpoint: host loopback address + secret (+ forward pid),
           lasting: worker port, firecracker network }
      --> caller writes worker.json and record.json (step 5)
```

## What it does

| Platform | How | Made by | Its kernel | Folder |
| --- | --- | --- | --- | --- |
| `native` | built in | nothing: the host itself | this host | [`native/`](../../../../connect/crates/layerfs-machine/src/platforms/native) |
| `docker` | built in | nothing: one worker container on Docker's kernel | Docker Desktop's VM or the Linux host | [`docker/`](../../../../connect/crates/layerfs-machine/src/platforms/docker) |
| `apple-container` | created | `container run` | an Apple container VM | [`apple_container/`](../../../../connect/crates/layerfs-machine/src/platforms/apple_container) |
| `microsandbox` | created | `msb create` | a libkrun microVM | [`microsandbox/`](../../../../connect/crates/layerfs-machine/src/platforms/microsandbox) |
| `lima` | created | `limactl create` from a template | a Lima `vz` VM | [`lima/`](../../../../connect/crates/layerfs-machine/src/platforms/lima) |
| `firecracker` | created | scripts in a nested `lima` machine | a Firecracker microVM | [`firecracker/`](../../../../connect/crates/layerfs-machine/src/platforms/firecracker) |
| `ssh` | connected | `machine connect` | an existing Linux host | [`ssh/`](../../../../connect/crates/layerfs-machine/src/platforms/ssh) |

What each lifecycle call does, per platform ("refused" is `InvalidInput`
with the fixed text of [Adapter](adapter.md#the-sealed-adapter)):

| Platform | create / connect | start | stop | remove / disconnect |
| --- | --- | --- | --- | --- |
| `native` | refused | nothing (`None`) on macOS and Linux | nothing | refused |
| `docker` | refused | replace and run the worker container | `docker stop` the worker | refused |
| `apple-container` | volume, `container run`, worker | `container start`, worker | `container stop` | container, then volume |
| `microsandbox` | `msb create`, `msb start`, worker | `msb start`, worker | `msb stop` | sandbox, then volume |
| `lima` | disk, instance, boot, worker | `limactl start`, worker | `limactl stop` | instance, then disk |
| `firecracker` | disk, VM folder, boot, `ssh -L`, worker | boot, new `ssh -L`, worker | forward, then VM | forward, VM, folder, disk |
| `ssh` | connect: key, kernel, data root, `ssh -L`, worker | new `ssh -L`, worker | forward and worker | disconnect: worker, forward, `/layerfs/{bundle,state}` |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One crate-private sealed trait, one exhaustive match; no registry or factory. | `pub(crate) trait Adapter` with no default methods and `fn adapter()` in [`adapter.rs`](../../../../connect/crates/layerfs-machine/src/adapter.rs); #535 G2; [connect guide](../../../../connect/AGENTS.md#extension-points) |
| One match for per-platform facts. | `Platform::profile()` in [`platform.rs`](../../../../connect/crates/layerfs-machine/src/platform.rs) |
| The crate knows nothing of the LayerFS home. | [`spec.rs`](../../../../connect/crates/layerfs-machine/src/spec.rs): `Spec` carries `vm_name`, `work_dir`, `tools` and lasting state; the caller writes every record ([`lib.rs`](../../../../connect/crates/layerfs-machine/src/lib.rs)) |
| A spec is checked before any effect. | every call in [`setup/calls.rs`](../../../../connect/crates/layerfs-machine/src/setup/calls.rs) starts with `spec.validate()` |
| A failure after the VM exists keeps it. | `MachineError::kept` / `vm_exists` in [`error.rs`](../../../../connect/crates/layerfs-machine/src/error.rs); each `create.rs` removes only what it made when nothing exists under the name |
| One attempt; a readiness wait is not a retry. | `worker::ready`, `forward::open` (`ExitOnForwardFailure`, one `ssh`) |
| No guest mounts the host home. | Lima `mounts: []`; docker mounts the socket, volume and bundle only; apple-container and microsandbox mount only their data volume |
| Each guest blocks user namespaces but LayerFS's own. | `PREPARE` in [`setup/worker.rs`](../../../../connect/crates/layerfs-machine/src/setup/worker.rs) writes `user.max_user_namespaces=0` and reads it back |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Host-only builds | the crate is `#![cfg(unix)]`; `layerfs-control` reaches it only through its `machine` feature, which the guest worker is built without | [`lib.rs`](../../../../connect/crates/layerfs-machine/src/lib.rs), [`layerfs-control/Cargo.toml`](../../../../api/crates/layerfs-control/Cargo.toml) |
| Missing tool or host fact | probe pushes `tool:<cli>` or `host:<fact>`; create refuses `Unsupported` with `missing` | [`probe.rs`](../../../../connect/crates/layerfs-machine/src/probe.rs), [Adapter](adapter.md#probe) |
| VM size | at least 1 CPU, 256M memory, 1G disk | `Spec::validate` |
| Names | a letter or digit, then `[A-Za-z0-9._-]`, at most 63 | `plain_name` in [`spec.rs`](../../../../connect/crates/layerfs-machine/src/spec.rs) |
| Provider call deadlines | `CALL` 30 s, `CREATE` 120 s, `BOOT` 300 s | [`setup/vm.rs`](../../../../connect/crates/layerfs-machine/src/setup/vm.rs) |
| Guest command and copy | `EXEC` 60 s, `COPY` 600 s | [`setup/guest.rs`](../../../../connect/crates/layerfs-machine/src/setup/guest.rs) |
| Worker readiness | at most `READY` 60 s; a worker that ends is reported with its log's end | [`setup/worker.rs`](../../../../connect/crates/layerfs-machine/src/setup/worker.rs) |
| Bundle | lacks `layerfs`, `layerfs-daemon` or `layerfs-sandbox-helper`: `InvalidInput`, `missing: bundle:<name>`; hash not hex: `InvalidInput` | `check_bundle` |
| Outcome not known | `Unknown`: everything it may have made is kept | [`error.rs`](../../../../connect/crates/layerfs-machine/src/error.rs) |

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`machine_platform.rs`](../../../../connect/crates/layerfs-machine/tests/machine_platform.rs) | `layerfs-machine` | Names parse back; backend, route, worker bind, hosts and options per platform and host. |
| [`machine_adapter.rs`](../../../../connect/crates/layerfs-machine/tests/machine_adapter.rs) | `layerfs-machine` | Built-in machines refuse create and remove; only `ssh` connects and has a host identity; every refusal before any tool runs. |
| [`conformance.rs`](../../../../connect/crates/layerfs-machine/tests/conformance.rs) | `layerfs-machine` | Over `Platform::ALL`: profile vs support table and `SUPPORT.json`; each lifecycle call through the platform's own tool or its fixed refusal; `reach`; `forward`; options vs `Spec::validate`. |

```text
 Scratch dir:  bin/  stand-in tools (shell scripts that log and refuse)
               work/ the machine's work_dir
 Spec.tools.path = bin/   -->  no real VM, Engine, ssh host or root
```

```text
cargo +1.85.1 test --locked -p layerfs-machine --test machine_platform
cargo +1.85.1 test --locked -p layerfs-machine --test machine_adapter
cargo +1.85.1 test --locked -p layerfs-machine --test conformance
```

Each file is `#![cfg(unix)]` and uses the stand-ins of
[`tests/support/standin.rs`](../../../../connect/crates/layerfs-machine/tests/support/standin.rs).
The `forward` kit also needs the system `sh` and `ps`. The commands were not
run while writing this page. Live runs are the receipts each platform page
links.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`connect/README.md`](../../../../connect/README.md): its "Reading the
  tree" and "Adding things" are covered in [Platforms and
  profiles](platforms-and-profiles.md), [Adapter](adapter.md) and [Adding a
  platform](adding-a-platform.md); it stays the layer guide.
- [`docs/versioned/2.0.0/machines.md`](../../../versioned/2.0.0/machines-and-stores.md):
  the user manual for the same calls; its per-platform sections are
  covered in the platform pages, which follow source where they differ.

---

← [Support matrix: Generated JSON](../support-matrix/generated-json.md) · [Up: Connect](../README.md) · [Machines: Platforms and profiles](platforms-and-profiles.md) →
