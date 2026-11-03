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
| Daemon-needing method tests (C1) | `concurrent_commits`, `lane_merge`, `merge_report` go to `layerfs-daemon/tests` (including store-service's `support/{installed_store,sdk}.rs` by `#[path]`), not store-service. | A store-service dev-dependency on the daemon would build two copies of store-service in one test binary; these tests lived in the daemon before `3cae8b1a1e`. |
| Tests left in api (C1) | `commit_get_pages` (uses `calls`, `home`), `sdk_merge_diagnostics` (uses `sandbox`), `remote_store` (Linux only; deleted later per §9.5) stay in `layerfs-api`. | They drive more than the method table. |
| Family files kept whole (C; superseded by F1h `810dc2e115`, which split store, backup and sandbox per call; project stays one file) | `family/{stores,projects,backups,sandboxes}.rs` are `store/family.rs`, `project/family.rs`, `backup/family.rs`, `sandbox/family.rs` (with the `workspace` calls), not split per op. `home/migrate.rs` and `place.rs` stay whole (not `migrate/{mod,v110}.rs`). | Each `run()` has inline match arms and shared helpers: a per-op split would change function bodies, which a move may not. Splitting is for the typed-model step (F). |
| Places §9.2 does not name (C) | `host/flock.rs` → `common/home/flock.rs`; `host/postgres.rs` → `store/postgres.rs`; `host/registry.rs` → `sandbox/registry.rs`; `base/internal.rs` → `daemon/internal.rs`; `base/recovery.rs` → `daemon/workspace/recovery.rs`; `operator/call.rs` → `daemon/operator.rs`; `host/dispatch.rs` → `sandbox/dispatch.rs` (not merged into `serve.rs`); `host/project.rs` + `host/postgres_init.rs` → `project/init.rs` + `project/postgres_init.rs`; the live facade `sandbox/{provider,create,connection}.rs` and `host/local.rs` → `sandbox/launch/`; `store/route.rs` → `store/{resolve,call}.rs` + `common/place.rs`; `machine/start.rs` → `machine/endpoint.rs`. Dead 1.1 files stay in `host/` and `operator/inspect/` for step D. Renamed modules are imported under their old names (`identity as stores`, …) so bodies did not change. | Moves only; merges change bodies. |
| `calls/` pieces moved down (C) | `calls/json.rs` → `common/json.rs`; `Context` → `common/context.rs` with a `Context::new` constructor (+5); `NAMESPACE` → `store/calls.rs`. | The moved code used them; the dependency must point down (control under api). |
| Tests (C7) | 18 targets that use control internals (including `sdk_merge_diagnostics`, which now drives only `daemon::values`) moved to `layerfs-control/tests` with `support/daemon_binary.rs`; `machine_take_down` stays in api (it calls `calls::call`). | Placed by what they drive. |
| Live code found in step D | `workspace/serve.rs` `observed`, `records`, `refused_entry`, `recorded_all` are reached through `reclaim_gone` (from `sandbox::serve::create`); `sandbox/files.rs` `sandbox_ids` and `sandbox/runtime.rs` `recorded` through them; `daemon/workspace/recovery.rs` `locate`/`cleanup` and the `internal` facade (`with_observations`, `bind`, `attach`, `into_connection`, `install`) are used by the daemon proof tests §9.5 keeps; `SandboxApi::start` is live through `create_provider_postgres`. | Kept. |
| Left in step D | `common/trace.rs` `run`/`take_scope`/`take_waited` have no caller (nothing sets the trace scope), kept for step I observability; store-backup `push`/`pull`/`load`/`copies::check` have no production caller after `host/sync.rs` and `host/copies.rs` went (only `remote_store` and store-backup tests use them), left for step F's `backup/` work (connect was out of the lane's scope); old `process-*.record` loading and the pre-unmount `running_in` probe kept for Sandboxes an earlier bundle left. | Not dead by this step's rule, or out of scope. |
| Worker op `project.remove` (D1) | `{store, project, store_record}`; a pre-#536 worker refuses it and a new worker refuses `host.call`/`host.backend` (`InvalidInput`), one attempt, no replay. | §9.2 worker ops; step I adds the hello version check that refuses a mixed bundle earlier. |
| Retired paths guard | `tools/check_product_boundary.py` refuses files under `layerfs-api/src/{host,family,home,store,worker,fs,branch,machine,sandbox,base,operator,project}/`, `layerfs-control/src/{host,operator}/` and `layerfs-store-service/src/remote/`. | §9.6. |
| Typed records (F1) | `StoreRecord` parses its `StoreRef` lazily so a record of an unknown kind still lists; `View {Full, List}` in `common/outcome.rs`; `MachineRecord` builds without the `machine` feature (platform/state/to_sdk behind it); readers accept records with or without `v`, `to_sdk` drops `v`, nothing writes it until layout 3 (F2). | Result keys stay byte-stable (§10). |
| List state (F1) | `store.list` items end with `state` (`moving`, `down`, `idle`, `in_use`), probed with `Start::Never` once per machine (a failed probe marks that machine's Stores `down`); `sandbox.list` items carry `state` (default `creating`); `machine.list` already did. `op` follows in F3. | §10b. |
| Not yet typed (F1) | `worker.json`, `bundle.json`, the project manifest fields in `sandbox/files.rs` (kept from growing past 953 lines), the conversion code in `common/home/{migrate,import_env,place,layout}.rs`, the worker wire arguments. | Out of F1's scope or size-bound. |
| Sealed `Driver` (`9bdbebbc95`) | store-backup's `Driver` has a private `sealed::Sealed` supertrait implemented by `LocalFolder` and `S3Client` only. | §9.4, G2: a provider is a folder under `bucket/`. |
| Conformance kits (step 9) | `layerfs-machine/tests/conformance.rs` (12 tests over `Platform::ALL`) and `layerfs-sandbox/tests/conformance.rs` (7 over `Backend::ALL`), reading `SUPPORT.json` through `layerfs-sandbox/tests/support/support_json.rs`; each kit's exhaustive matches make a new platform or backend fail to compile until declared. The kit declares bind behaviour per backend itself (no `binds` capability yet). | §11 step 9; tests only, delta 0. |
| Native run folders (F2) | A native Sandbox's local directory is `machine/native/run/<prefix><name>`, not §5's `sandbox/<name>/run/`; the namespaces image cache stays shared at `run/images`. | A per-Sandbox root would unpack the 2.7 GB image once per Sandbox. |
| Sandbox ids (F2) | A Sandbox's `id` is its name; no numbering; local or container name `<prefix><name>`. `create.intent` is gone: an unsettled create answers `Unknown` and the record says `unknown` (F3's `op` replaces this). | One key per Sandbox across home, worker and runtime. |
| `migrate/` split (F2) | `common/home/migrate/{open,volume,v110,v2}.rs`, `mod.rs` declaration-only. | 200-line declaration rule. |
| Layout 2 conversion (F2) | Refused `InUse` (with `detail.related`) while a native runtime runs, any worker Sandbox is recorded with an id, a creator lives, a `create.intent` exists, a Store lease lives, or a move/remove/machine removal is live; otherwise one pass under the home lock with a `log/layout-3.pending` marker so a crash after the flip completes on the next open. Workers clear their layout 2 folders on first start from the new bundle (`guest::retire_layout_2`, marker `.layout-3`). | §5, §11 step 4. |
| `layerfs_api::call` (C1) | Kept as a re-export of `layerfs_store_service::methods::call`; `layerfs_api::fs` and `::branch` are no longer public (nothing used them). | Front ends, examples and remaining tests keep their path. |

## Open items

- `common/place.rs` (Store-call routing) and `common/home/place.rs` (1.1.0 file placing) share a name; rename when `migrate/` is split.
- `sandbox/files.rs` is 953 physical lines after step D, near the 999 ceiling.
- `PROVIDERS.md` stays in `layerfs-api` and describes the facade now in control `sandbox/launch/`.

## Corrections to commit messages

| Commit | Message says | Correct |
| --- | --- | --- |
| `41a2a00525` (F1 merge) | "typed records +545, moves' imports/declarations +167" | behaviour commits +450 (F1a 77, F1d 170, F1e 137, F1f 61, F1g 5), moves +167 (F1b 6, F1c 7, F1h 154); total +617 is right |

## Phase log
- Step C1 (§11 1.1) done at `c73fda2d57`: lane C1 (subagent, branch `issue535-C-methods`) `a6fea806de`, merged `c73fda2d57`. Ruff was not run on the three changed Python tools (not installed); `remote_store`'s new `#[path]` includes were not compiled on Linux (the lane's Linux Cargo home was empty).
- Step C (§11 1.2–1.7, reordered) done at `4fd096335e`: lane C (subagent, branch `issue535-C-control`) C2 `17c17a188c`, C3 `bec9cc372c`, C4 `7f51e79799`, C5 `936fd2175b`, C6 `2997ad6710`, C7 `210fb14a44`, merged `4fd096335e`.
- Step D (§11 step 2) done at `26b453f38c`: lane D (subagent, branch `issue535-D-delete`) D1 `59979c4181`, D2 `4e7e9cd1bd`, D3 `45c5c9aa80`, D4 `5f255a90bf`; production LOC 133797 -> 132043. Retired-path guard added after.
- Sealed `Driver`: `9bdbebbc95` (lead).
- Step F2 (§11 phase 4, layout 3) merged at `9ce7dd5533`: lane F2 (subagent, branch `issue535-F2-layout`) `b7004e5547`, `fb31a90a88`, `db82b917e7`, `6704207235` (merge of `issue535-connect`), `444d12da21` (Workspace names, §11 phase 6 item taken early). Live runs for step K. `core/docs/architecture/nfs-native-lifecycle.md` still says `ownership.lock` (now `sandbox/<name>/lock`).
- Step 9 conformance kits merged at `b055085daf`: kit lane (subagent, branch `issue535-kit`) `503dda0bef`, `c4b135989c`. Linux runs of the kits NOT_RUN on this host; Linux clippy of both crates passed.
- Step F1 (§11 phase 3) done at `41a2a00525`: lane F1 (subagent, branch `issue535-F1-records`) `5caf281e1c`, `72ad49c7d0`, `88fd08ef63`, `5c7a26d7ce`, `908d4a3af1`, `b03e02feb1`, `6d1ab2520b`, `810dc2e115`.
