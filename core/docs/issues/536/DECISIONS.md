# #536 decisions log

> **Status:** Record of the owner decisions, gate answers and implementation
> choices for the #536 implementation on branch `issue535-connect`
> (interleaved with #535).

Spec: [README.md](README.md). Work order: [535/HANDOFF-2.md](../535/HANDOFF-2.md) §5.
The #535 log ([535/DECISIONS.md](../535/DECISIONS.md)) holds the shared rows
(interleaving, phase logs of #535 steps). Each row below is settled; none is
to be re-confirmed.

## Owner decisions

| # | Decision | Taken |
| --- | --- | --- |
| C1 | The management layer is a crate named `layerfs-control`, in the api layer (`api/crates/layerfs-control`): the library under the call surface `layerfs-api`. `connect/` keeps drivers only. | owner, 2026-11-03 |
| C2 | Sandboxes are recorded at the top of the home, `sandbox/<name>/`; the name is the only identity. The 1.1-era numbered state (`store/<s>/project/<p>/sandboxes/<n>/`), `machine/<m>/sandbox/<name>/` and `machine/native/sandbox-root/` are retired into it. | owner, 2026-11-03 |
| C3 | Workspaces are children of their Sandbox (`sandbox/<name>/workspace/<id>.json`) and may have an optional user-chosen name, unique within the Sandbox. | owner, 2026-11-03 |
| C4 | PostgreSQL Stores are usable from every machine automatically; there is no `store.attach`/`store.detach` (#535 G3, §7.4). Reach is rewritten per platform and checked inside `sandbox.create`. | owner, 2026-11-03 |
| C5 | sqlite Stores move by two caller-driven calls: `store.push <store> <machine>` (native → machine) and `store.pull <store>` (machine → native). Between two machines: pull, then push. Guests never fetch a Store; no `store.copy`. | owner, 2026-11-03 |
| C6 | Projects are data in a Store; a Sandbox is compute bound to one project on one machine; a project may have many Sandboxes. | owner, 2026-11-03 |
| C7 | Backup to buckets stays a separate operation family (`layerfs-store-backup`). | owner, 2026-11-03 |

## Gates (recommended answers, owner-approved 2026-11-03)

| Gate | Answer | Taken |
| --- | --- | --- |
| H1 | `store.push <store> <machine>` and `store.pull <store>` replace `store.move` (no alias). `store.push` is refused unless the Store is on `native`; `store.pull` is refused when it already is. | owner, 2026-11-03 |
| H2 | Free functions over a `Home` handle: `<entity>::<op>(&Home, Request, &mut Events) -> Result<Record, Refusal>`. No `Control` object holding managers. | owner, 2026-11-03 |
| H3 | Store-call implementations (`fs/`, `branch/`, `base/dispatch.rs`) move to the engine, `layerfs-store-service/src/methods/`. | owner, 2026-11-03 |
| H4 | Persisted states are facts and one `op` intent only; derived states come from one `state()` per entity. | owner, 2026-11-03 |
| H5 | Intents live inside the entity's own record (`op`); owner identity is always `{pid, pid_start, pid_boot}`. | owner, 2026-11-03 |
| H6 | No new call to settle `Unknown`: `machine.status`, `store.status` and `sandbox.get` carry `op`; clearing is an explicit existing call, never a read. | owner, 2026-11-03 |
| H7 | No reconciler and no new family: one report-only top-level `layerfs doctor` (CLI) and `layerfs.doctor()` (Node, Python), outside the call table; it changes nothing. | owner, 2026-11-03 |
| H8 | Labels, `status --json`, per-entity idle policies: deferred. | owner, 2026-11-03 |
| H9 | Per-machine PostgreSQL URL: deferred; such a machine is refused with the address tried. | owner, 2026-11-03 |
| H10 | Converting a 2.0 home: refuse while anything is live, naming each; then convert in one pass under the home lock; guest leftovers are deleted by the worker on its first start from the new bundle. | owner, 2026-11-03 |
| H11 | `project.init` into a sqlite Store on a non-native machine is allowed through that machine's worker. | owner, 2026-11-03 |

## Interleaving

| # | Decision | Taken |
| --- | --- | --- |
| I1 | #536 is interleaved with #535 on branch `issue535-connect` (order: [535/HANDOFF-2.md](../535/HANDOFF-2.md) §5) with one combined terminal pass recorded in `535/TERMINAL-PASS.md`. Supersedes "starts after #535's terminal pass". | owner, 2026-11-03 |

## Differences between the spec and the code

Rows added when the code contradicts a spec fact. The code wins; the step is adjusted.

| Spec | Code | Effect |
| --- | --- | --- |
| §4, §9.2, §11 step 2: `base/{internal,recovery}.rs` are reachable only from tests | `internal.rs` `workspace()`, `trace`, `take_telemetry`, `split_provider` and the `Control` re-export path are live (host/workspace.rs, discovery.rs, trace.rs, local.rs, sandbox.rs, pool.rs); `recovery.rs` `bind`/`attach` back `WorkspaceApi::mount`. | Their live parts move with step C (to `daemon/` and `workspace/`); only the dead parts are deleted in step D. |
| §9.2: the `sandbox/{provider,create,close}.rs` facade is deleted | `provider.rs` (`with_runtime`, `create_provider_file`, `create_provider_postgres`, `start`), `create.rs` (`SandboxApi`, cause/phase, `hello`, `connect_address`) and `connection.rs` (not named in the spec) are the live launch path (host/sandbox.rs, host/local.rs, host/pool.rs). Only `close.rs`, `SandboxApi::create` and `ProviderSandbox::status` are test-only. | The live facade moves to `sandbox/launch.rs` and `daemon/`; step D deletes `close.rs` and the test-only parts with their tests. |
| §4: "most of host/discovery.rs" is reachable only from 1.1 names | Only about 45 of 991 lines are dead (`sandbox_ids`, `gone_container`, `local_running`). | The live part moves (sandbox/files.rs and helpers needed earlier); step D deletes the three functions. |
| §9.2 lists `host/postgres.rs` among files with dead parts; `host/flock.rs`, `sandbox/connection.rs`, `machine/start.rs` have no destination | Every `host/postgres.rs` function has a live caller; flock is used by home/lock, machine/start, discovery and unmount. | Destinations chosen in step C (recorded under Implementation choices when made). |
| §9.2: `host/sandbox.rs` `reclaim` deleted | `host/sandbox.rs::reclaim` is live (create → `workspace::reclaim_gone`); only `host/workspace.rs::reclaim` is dead. | Only the workspace one is deleted. |
| §9.2: `sandbox/exec.rs` → `daemon/workspace/exec.rs` | It sends no daemon request; it takes the facade's `ProviderSandbox` and only tests and examples use it. | Deleted with its users in step D, not moved to `daemon/`. |
| §9.4, §11 1.5: store-service `remote/{settings,state}.rs` move to control | Since #535 1.6/1.6b (`0cada61acc`, `236b7aceb2`) they are `layerfs-store-backup/src/backup/link.rs`; store-service `remote/` is gone. | Step C/F moves the home-record half of `link.rs` from store-backup into control `backup/`. |
| §11 step 1 order (1.2 home, names, outcome, trace; then 1.3 …) | Files moved early need later ones: `host/registry.rs`, `host/flock.rs`, `store/store_ref.rs`, `discovery` helpers; `calls::json` and `calls::Context` (which stay in api by §9.1) are used by home/records, home/migrate, worker/frame, machine/calls, store/route and family/*; `worker/` needs store/, family/sandboxes and host/dispatch. No sub-step after 1.1 builds alone. | Step C: 1.1 as written; then one crate-level move of everything else in `layerfs-api/src` except `calls/` and `lib.rs` into `layerfs-control` (with `json.rs`, `Context` and `scope::NAMESPACE` moved down to control), then the by-entity reorganisation as moves inside the one crate in §11's groups. Each commit builds and is delta 0. |
| §9.1: `layerfs-api` depends on control and bridge only; §11 1.3 repoints `layerfs-cli/src/worker.rs` | `lib.rs` re-exports `layerfs_project::store::*`; the guard allows `layerfs-cli` only `{layerfs-api, layerfs-bridge}`. | api re-exports what the front ends use from control; front ends keep their edges. |
| §9.5 test placement | `method_refusal_order` and `refresh_lookups` drive the method table (store-service); `commit_get_pages` and `sdk_merge_diagnostics` do not; tests that drive `layerfs_api::calls::call` test the call surface; `init`, `local_install`, `store_projects` test `layerfs_project` through api re-exports; `concurrent_commits`, `lane_merge`, `merge_report` need `layerfs-daemon`. | Tests are placed by what they drive, recorded per commit. |

## Implementation choices (not owner decisions)

| Topic | Choice | Why |
| --- | --- | --- |

## Open items

## Phase log
