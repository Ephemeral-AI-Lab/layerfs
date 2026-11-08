# Disabled platforms

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker is permanently disabled for new product work on every host and
for both Store kinds, by [layerfs-project#7](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/7).
Its adapter, platform variant, record readers and historical receipts remain.
There is no environment variable, feature flag or setting that enables it.

Microsandbox and SSH machines were **removed** in LayerFS 2.0 (owner,
2026-11-08), so that no one mistakes them for supported platforms: their
`Platform` variants, adapters, profiles, support rows and the
`machine connect` and `machine disconnect` calls are gone (SSH had been
disabled by layerfs-project#7 before). See
[removed platforms](#removed-platforms-microsandbox-and-ssh) for a record a
home still holds for one. ssh as a transport (Lima's own `ssh.config`, the
`ssh -L` forwards of `lima` and `firecracker`) is unaffected.

The authoritative refusal strings are in
[`platform_policy.rs`](../../../../control/crates/layerfs-sandbox-backend/src/platform_policy.rs):
`disabled_reason` answers Firecracker's and, through `removed_reason`, the
removed platforms'. This shared `layerfs-sandbox-backend` module is available
to both host builds and builds without machine adapters. The machine support
table and readiness probe consume it; the public dispatcher checks it before
opening a home or reaching a worker or daemon. Lower-level machine create,
start, forward and copy calls also refuse before validating specs, inspecting
bundles or invoking platform tools.

| Operation on a disabled platform | Behavior |
| --- | --- |
| Create, start, update | `Unsupported`, with the platform's fixed reason, including an already-running machine |
| Sandbox create; Workspace mount, exec, commit | Refused, including implicit selection from a Store, recorded workers and forwarded daemons |
| Project init; Branch fork, merge, publish; file mutations | Refused when the sqlite Store is on a disabled machine |
| Store push, pull; backup create | Refused when they select a disabled machine; transfer recovery also checks any historical leftover before deleting a copy |
| Machine platforms | Diagnostic row only: `ready: false`, no backend, kernel, sqlite route or selectable options; the same reason in `missing` and PostgreSQL support |
| Machine list, get, status | Retain the recorded state and show the disabled reason without probing provider tools; this is not a live state claim |
| Other read-only inspection | Remains available through existing routes; an unavailable worker is not automatically started |
| Workspace unmount, Sandbox close, machine stop/remove, Store/project removal | Existing explicit cleanup remains available with its ordinary ownership, `InUse` and uncertainty checks; it never gains permission to start a disabled machine |

Installing this restriction does not stop existing workloads, delete a record,
move a Store, remove a data disk, or alter historical evidence. A stopped machine
cannot be restarted through LayerFS. Cleanup that needs an unavailable worker or
host can fail; preserve its records and data for explicit operator handling.
Firecracker remove is still an explicit destructive operation with the
existing ownership checks.

## Removed platforms: Microsandbox and SSH

A home made before 2.0 may still hold a machine record (or a Sandbox record)
whose platform is `microsandbox` or `ssh`. No code can drive such a machine,
so LayerFS shows it, refuses it and lets the user forget it. Nothing panics
and nothing touches a VM or host.

| Call on a removed platform's machine | Behavior | Source |
| --- | --- | --- |
| `machine list`, `machine get` | Answered, never probed: `platform` as recorded, `backend: "-"`, `supports` all false with the reason, `state: "unsupported"`, `error` the reason, and the record's `ssh` settings or `vm_name` | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) `to_sdk`, [`list.rs`](../../../../control/crates/layerfs-machine/src/list.rs) |
| `machine remove` | Forgets the record only, under the home lock and the usual `InUse` check (refused while Stores, Sandboxes or hosted machines are recorded on it); answers `{ name, removed: true, note }`, the note saying nothing on its host was touched or, for `microsandbox`, naming the VM and its `<vm>-data` volume and the `msb remove --force <vm>` and `msb volume remove <vm>-data` commands that remove them | [`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs) `forget` |
| every other call (`status`, `start`, `stop`, `update`, `sandbox create` on it, Workspace calls on its Sandboxes, Store and project calls on a sqlite Store on it, `store push`/`pull`) | `Unsupported` with the reason, before any effect | the dispatcher's platform policy, `MachineRecord::platform` |
| `machine create <name> microsandbox` or `ssh` | `Unsupported` with the reason | the dispatcher's platform policy |
| `layerfs import-env` with `LAYERFS_SSH_HOST` or `LAYERFS_SANDBOX=vm:microsandbox` | Reported as ignored, with the reason | [`import_env.rs`](../../../../control/crates/layerfs-control/src/migrate/import_env.rs) |

The reasons are word for word ``platform `microsandbox` was removed in
LayerFS 2.0`` and ``platform `ssh` was removed in LayerFS 2.0``.
[`machine_removed_platforms.rs`](../../../../api/crates/layerfs-api/tests/machine_removed_platforms.rs)
and the shared refusal matrix
([`api/test/disabled-platforms.json`](../../../../api/test/disabled-platforms.json),
run from Rust, the CLI, Node and Python) check this.

## Transfers and re-enabling

Native macOS, Docker, Lima and Apple Container retain their support rules
and kernel paths. The initial macOS SQLite transfer selection is native ↔
Docker, native ↔ Lima and native ↔ Apple Container (six directions), plus
Docker → native → Lima. Firecracker receives refusal checks only. This
document defines the selection; it does not claim those live transfers ran.

Re-enabling Firecracker requires a new explicit owner decision and a source
change to the shared policy, regenerated `control/SUPPORT.json`, updated setup
and discovery documentation, and focused public-route verification. Retained
adapter tests marked ignored are historical qualification code; remove that
mark only after reauthorization and qualification. Bringing back Microsandbox
or SSH would be new platform work ([adding a platform](../machines/adding-a-platform.md)).
Existing receipts keep their original verdicts and never imply that the
current product enables a platform.
