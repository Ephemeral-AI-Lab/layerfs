# Disabled platforms

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker and SSH are permanently disabled for new product work on every
host and for both Store kinds, by [layerfs-project#7](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/7).
Their adapters, platform variants, record readers and historical receipts remain.
There is no environment variable, feature flag or setting that enables them.

The authoritative refusal strings are in
[`platform_policy.rs`](../../../../control/crates/layerfs-isolation/src/platform_policy.rs).
This shared `layerfs-isolation` module is available to both host builds and builds without
machine adapters. The machine support table and readiness probe consume it;
the public dispatcher checks it before opening a home or reaching a worker or
daemon. Lower-level machine create, connect, start, forward and copy calls also
refuse before validating specs, inspecting bundles or invoking platform tools.

| Operation on a disabled platform | Behavior |
| --- | --- |
| Create, connect, start, update | `Unsupported`, with the platform's fixed reason, including an already-running machine |
| Sandbox create; Workspace mount, exec, commit | Refused, including implicit selection from a Store, recorded workers and forwarded daemons |
| Project init; Branch fork, merge, publish; file mutations | Refused when the sqlite Store is on a disabled machine |
| Store push, pull; backup create | Refused when they select a disabled machine; transfer recovery also checks any historical leftover before deleting a copy |
| Machine platforms | Diagnostic row only: `ready: false`, no backend, kernel, sqlite route or selectable options; the same reason in `missing` and PostgreSQL support |
| Machine list, get, status | Retain the recorded state and show the disabled reason without probing provider tools; this is not a live state claim |
| Other read-only inspection | Remains available through existing routes; an unavailable worker is not automatically started |
| Workspace unmount, Sandbox close, machine stop/remove/disconnect, Store/project removal | Existing explicit cleanup remains available with its ordinary ownership, `InUse` and uncertainty checks; it never gains permission to start a disabled machine |

Installing this restriction does not stop existing workloads, delete a record,
move a Store, remove a data disk, or alter historical evidence. A stopped machine
cannot be restarted through LayerFS. Cleanup that needs an unavailable worker or
host can fail; preserve its records and data for explicit operator handling.
SSH disconnect preserves its remote data root. Firecracker remove is still an
explicit destructive operation with the existing ownership checks.

Native macOS, Docker, Lima, Apple Container and microsandbox retain their support
rules and kernel paths. The initial macOS SQLite transfer selection is native ↔
Docker, native ↔ Lima and native ↔ Apple Container (six directions), plus Docker
→ native → Lima. Firecracker and SSH receive refusal checks only. Microsandbox
is excluded only from that initial transfer selection, not from the product.
This document defines the selection; it does not claim those live transfers ran.

Re-enabling either platform requires a new explicit owner decision and a source
change to the shared policy, regenerated `control/SUPPORT.json`, updated setup
and discovery documentation, and focused public-route verification. Retained
adapter tests marked ignored are historical qualification code; remove that
mark only after reauthorization and qualification. Existing receipts keep their
original verdicts and never imply that the current product enables a platform.
