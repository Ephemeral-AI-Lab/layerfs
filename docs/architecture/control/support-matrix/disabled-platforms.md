# Removed platforms

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker, Microsandbox and SSH machines were **removed** in LayerFS 2.0
(owner, 2026-11-08), so that no one mistakes them for supported platforms:
their `Platform` variants, adapters, profiles, support rows, the
`machine connect` and `machine disconnect` calls, and Firecracker's
`machine create` options (`host`, and Lima's `nested`, which existed only to
host it) are gone. Firecracker and SSH had been permanently disabled by
[layerfs-project#7](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/7)
before; no platform is disabled with its code kept any more. There is no
environment variable, feature flag or setting that brings one back. ssh as a
transport (Lima's own `ssh.config`, the `ssh -L` forwards of `lima`) is
unaffected.

The authoritative refusal strings are in
[`platform_policy.rs`](../../../../control/crates/layerfs-sandbox-backend/src/platform_policy.rs):
`removed_reason` answers each removed platform, and `disabled_reason`, the
entry point the public dispatcher calls, answers the same. This shared
`layerfs-sandbox-backend` module is available to both host builds and builds
without machine adapters. The dispatcher checks it before opening a home or
reaching a worker or daemon.

## A record of a removed platform

A home made before 2.0 may still hold a machine record (or a Sandbox record)
whose platform is `firecracker`, `microsandbox` or `ssh`. No code can drive
such a machine, so LayerFS shows it, refuses it and lets the user forget it.
Nothing panics and nothing touches a VM or host.

| Call on a removed platform's machine | Behavior | Source |
| --- | --- | --- |
| `machine list`, `machine get` | Answered, never probed: `platform` as recorded, `backend: "-"`, `supports` all false with the reason, `state: "unsupported"`, `error` the reason, and the record's `ssh` settings, `host` or `vm_name` | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) `to_sdk`, [`list.rs`](../../../../control/crates/layerfs-machine/src/list.rs) |
| `machine remove` | Forgets the record only, under the home lock and the usual `InUse` check (refused while Stores or Sandboxes are recorded on it); answers `{ name, removed: true, note }`, the note saying nothing on its host was touched or, for `microsandbox`, naming the VM and its `<vm>-data` volume and the `msb remove --force <vm>` and `msb volume remove <vm>-data` commands that remove them, and for `firecracker`, naming the microVM's folder `/layerfs/data/firecracker/vms/<vm>` and data disk `/layerfs/data/firecracker/disks/<vm>.ext4` that stay in its Lima host | [`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs) `forget` |
| every other call (`status`, `start`, `stop`, `update`, `sandbox create` on it, Workspace calls on its Sandboxes, Store and project calls on a sqlite Store on it, `store push`/`pull`) | `Unsupported` with the reason, before any effect | the dispatcher's platform policy, `MachineRecord::platform` |
| `machine create <name> firecracker`, `microsandbox` or `ssh` | `Unsupported` with the reason | the dispatcher's platform policy |
| `layerfs import-env` with `LAYERFS_SSH_HOST` or `LAYERFS_SANDBOX=vm:firecracker` / `vm:microsandbox` | Reported as ignored, with the reason | [`import_env.rs`](../../../../control/crates/layerfs-control/src/migrate/import_env.rs) |

The reasons are word for word ``platform `firecracker` was removed in
LayerFS 2.0``, ``platform `microsandbox` was removed in LayerFS 2.0`` and
``platform `ssh` was removed in LayerFS 2.0``. A Lima machine that an old
Firecracker record names as its host still holds that microVM's folder and
disk on its data disk, so `machine remove` refuses it `InUse` (listing the
record in `detail.related`) until `machine remove` has forgotten the
Firecracker record; stopping it is allowed.
[`machine_removed_platforms.rs`](../../../../api/crates/layerfs-api/tests/machine_removed_platforms.rs)
and the shared refusal matrix
([`api/test/disabled-platforms.json`](../../../../api/test/disabled-platforms.json),
run from Rust, the CLI, Node and Python) check this.

Removing the platforms does not stop existing workloads, move a Store,
remove a data disk or alter historical evidence.

## Transfers and bringing a platform back

Native macOS, Docker, Lima and Apple Container retain their support rules
and kernel paths. The initial macOS SQLite transfer selection is native ↔
Docker, native ↔ Lima and native ↔ Apple Container (six directions), plus
Docker → native → Lima. This document defines the selection; it does not
claim those live transfers ran.

Bringing back Firecracker, Microsandbox or SSH would be new platform work
([adding a platform](../machines/adding-a-platform.md)) after an explicit
owner decision. Existing receipts keep their original verdicts and never
imply that the current product enables a platform.
