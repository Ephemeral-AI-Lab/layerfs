# LayerFS 0.1.7 Developer Preview

> **Status:** LayerFS 0.1.7 Developer Preview release record.
> [Tag and source](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.7).

## Owner direction

On 2026-10-15 the owner directed: close v0.1.7 and release it, discard the v0.2.0
and v0.3.0 plans, and prepare v1.0.0 as the official release. For the release
gate, the owner chose **release as-is with waivers**: tag the current `main`
after the version bump and documentation. The existing Phase 5 and review
evidence is reused, and every gate that was not rerun at the release commit is
recorded below as an owner waiver. A waiver records that a gate was not run. It
does not turn that gate into a pass.

The tag `v0.1.7` points to the release commit that adds this record. The product
source is the same as at `e4968959b`. That commit and the release commit change
only documentation and version numbers.

## What 0.1.7 is

See the [0.1.7 manual](../../versioned/0.1.7/README.md). In short:
- a rewritten `core/` implementation, where every Sandbox daemon opens one shared
  Store directly;
- per-Workspace overlays, with Commit-time merge and conflict markers;
- the JavaScript SDK `@layerfs/sdk` (29 public methods) and its internal operator
  surface;
- a terminal UI that runs live on the product.

## Compatibility boundary

This is an owner-approved exception to the 0.1.x patch-line promise in the
[release policy](../../general/release-policy.md):
- **Removed.** The v0.1.6 CLI and public Rust SDK are removed. The public surface
  is the JavaScript SDK.
- **Store format.** Stores use schema 16–18 or 22–24 with `layerfs/commit/v3`
  identity. Older Stores, including v0.1.6 schema-10 Stores, are refused without
  migration.
- **Version matching.** The daemon protocol changed. SDK, addon and daemon must
  come from the same tag.
- **Store profile.** Disposable is the only executed profile. Durable is NOT_RUN,
  disabled by the owner.

## Evidence reused

| Evidence | Where | Result |
| --- | --- | --- |
| SDK standard acceptance: 26 VERIFICATION cases plus S328 and XPROCESS, run live | [PHASE5](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/PHASE5-20261015.md) | PASS, except B7 (NOT_RUN) and A9 (historical FAIL, still open) |
| Live UI stories (tour, conflicts, merge, agents) and a fresh browser check | [PHASE5](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/PHASE5-20261015.md) | PASS |
| Publication counter speed/storage screen | [Counter screen](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/COUNTER-SCREEN-20261015.md) | v5 PASS (+1.34% operation time, identical allocation). It is a component diagnostic, not a release benchmark claim |
| Review fixes `c9770e086` (Commit dependency refusal, empty totals update) and `e4968959b` (UI player freeze) | Commit messages on `main` | All `layerfs-persistence` tests pass (100). All `layerfs-daemon` tests pass except two that need setup from other campaigns. Clippy `-D warnings`, fmt and the boundary scan pass. S328 and the merge, conflicts and tour stories pass live |
| Earlier phases | [PHASE2](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/PHASE2.md), [PHASE3](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/PHASE3.md), [PHASE4](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/PHASE4-20261015.md), [DECISIONS](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/DECISIONS-20261015.md) | As recorded |

## Waivers

These release-policy gates were not run, or not fully run, at the release commit.
The owner accepted them for 0.1.7 only.

| Gate | What was not run | Residual risk |
| --- | --- | --- |
| Whole workspace tests | The complete suite was not rerun at the release commit. The changed packages were rerun package by package at `c9770e086`: persistence, daemon, storage, content, bridge, api, project and overlay. `layerfs-fuse`, `layerfs-workspace`, `layerfs-sandbox` and `layerfs-telemetry` were not rerun as whole packages | An untested regression in those packages |
| Daemon tests needing other setup | `host_handoff` and `complete_installed_roots` need `LAYERFS_INSTALL_DAEMON_BINARY` and `LAYERFS_Q1_PREPARED`; NOT_RUN | Host handoff and huge-namespace install are not re-proven |
| FUSE and container-runtime gates on a capable host | No separate gate run. Mounted Workspaces in containers are exercised only by the SDK verification cases and the live stories | FUSE edge cases outside those cases |
| VERIFICATION B7 | No deterministic pause point between merge and publish; NOT_RUN | The live merge/publish race is not proven |
| VERIFICATION A9 | Kept as its historical FAIL; XPROCESS passes as separate evidence | — |
| Durable profile | NOT_RUN, disabled by the owner | Durable behaviour is unverified |
| Published benchmarks | None for 0.1.7. The release makes no performance claim | — |
| Manual completeness | The 0.1.7 manual covers the SDK, operator and UI. There is no CLI to document, and the Store format is described in the SDK reference rather than in a separate storage-format chapter | — |

Known product limits are listed in the
[SDK and UI guide](../../versioned/0.1.7/sdk-and-ui.md#7-known-limits).
They include no restart recovery, deferred zombie reaping and replaced-inode
behaviour, linear deep paging, and no per-file physical attribution.

## Issue disposition

- **Closed as not planned:** the open v0.1.7 issues (research, pilots and
  refactor plans from #155 to #237). They are superseded by this release and the
  1.0.0 plan.
- **Plans discarded:** v0.2.0 (#164) and v0.3.0 (#52, #69) are closed as not
  planned, and `docs/roadmap/0.2/` is archived.
- **Closed as delivered:** #326–#329 (the SDK redesign and its decisions), closed
  earlier on 2026-10-15.
- **Opened for 1.0.0:** [#330](https://github.com/Ephemeral-AI-Lab/layerfs/issues/330)
  (documentation reorganization),
  [#331](https://github.com/Ephemeral-AI-Lab/layerfs/issues/331) (demos) and
  [#332](https://github.com/Ephemeral-AI-Lab/layerfs/issues/332) (benchmark
  preparation). See the [1.0.0 roadmap](../../roadmap/1.0/README.md).
- **Left open:** unversioned issues (#316–#325 and other follow-ups), for 1.0.0
  triage.
