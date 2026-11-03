# #535 decisions log

> **Status:** Record of the owner decisions, gate answers and implementation
> choices for the #535 implementation on branch `issue535-connect`.

Spec: [README.md](README.md). Handoff prompts: [HANDOFF-PROMPT.md](HANDOFF-PROMPT.md)
(phases 0–2b), [HANDOFF-2.md](HANDOFF-2.md) (from 2c, interleaved with #536).

The owner settled D1–D10 in the design session of 2026-11-03 and approved
the recommended answers to G1–G8 the same day. Each row below is settled;
none is to be re-confirmed.

## Owner decisions

| # | Decision | Taken |
| --- | --- | --- |
| D1 | The engine keeps persistence, daemon, overlay, Workspace and kernel routes; Sandbox and network code move out. | owner, 2026-11-03 |
| D2 | PostgreSQL stays inside `layerfs-persistence`, parallel to SQLite (`backend/{sqlite,postgresql}`). | owner, 2026-11-03 |
| D3 | NFS stays parallel to FUSE. Both become peers over a new route-neutral `layerfs-mount-ports` crate (ports, operations, phase timing moved out of `layerfs-fuse`). | owner, 2026-11-03 |
| D4 | A new top layer `connect/` is created. Supersedes #533 O21b ("the crates stay in `engine/crates/` for 2.0"). | owner, 2026-11-03 |
| D5 | Connect crates: `layerfs-machine`, `layerfs-sandbox`, `layerfs-store-backup`. `layerfs-remote-storage` is renamed `layerfs-store-backup` and absorbs `layerfs-store-service/src/remote/`; no `layerfs-remote-sync` crate. | owner, 2026-11-03 |
| D6 | The machine `Kind` is renamed `Platform`. | owner, 2026-11-03 |
| D7 | One folder per platform; each make/unmake pair in one file (`create.rs`, `power.rs`, ssh `connect.rs`). The public call stays `remove`. | owner, 2026-11-03 |
| D8 | Docker is not treated as a namespaces machine; it keeps its own backend. | owner, 2026-11-03 |
| D9 | Every machine can use both PostgreSQL and sqlite Stores, several of each. | owner, 2026-11-03 |
| D10 | The support matrix is generated (`connect/SUPPORT.json`) and exposed as the call `machine.platforms`. | owner, 2026-11-03 |
| D11 | **Machine lifecycle is manual.** No automatic idle stop of workers or VMs, and no on-demand start: a call on a stopped machine is refused `Unavailable`, naming `layerfs machine start <name>`. Sandbox daemons no longer end themselves when idle. The Store connection reaper (`store/open.rs`, 60 s) stays: it frees a connection and is not lifecycle. **Supersedes** #533 O2 (worker idle stop), #535 R1's on-demand start (R1 keeps "no ping before a worker call") and #373's 120 s Sandbox idle close. Owner's words: "i think we should remove it, if we have start or stop. make the lifecycle management to be mannual"; answers "Refuse, name machine start" and "Remove Sandbox 2-min close". | owner, 2026-11-03, relayed by a forked side session (`local_404fb529`) to this session; scheduled after F2 merges (F2 edits the worker idle files). **Built in `a5ef188cf7`, `b6261beee5`, `c2a4808113`** (code and tests) and the D11 docs commit after them |

## Gates

| Gate | Answer (the spec §3 recommended answer, approved) | Taken |
| --- | --- | --- |
| G1 | Yes: supersede #533 O3, G13 and the "Step-12 deferral closed" PostgreSQL row. A PostgreSQL Sandbox runs on every machine where §7.3 marks the platform supported (no attach; G3). | owner, 2026-11-03 |
| G2 | Yes, narrowly: supersede #533 "Step 5.2". One crate-private, sealed trait per extension point (`Adapter`, `BackendAdapter`), no default methods, reached through one exhaustive `match`. No public trait, registry, factory or plugin. | owner, 2026-11-03 |
| G3 | No attach records or calls; every PostgreSQL Store is usable from every machine where §7.3 / `support.rs` allows it; `Adapter::reach` rewrites the URL per platform and the reach check (schema binding) runs inside `sandbox.create`'s own request; a per-machine URL is deferred, and a machine that cannot reach the server is refused naming the address it tried. (Replaces the earlier recommendation of `store.attach`/`store.detach` and a `machines` field.) | owner, 2026-11-03, revised |
| G4 | Yes: Workspace calls reach a VM Sandbox's daemon directly (control keys in the home Sandbox record, port forwarded to host loopback). The keys go to `sandbox/<name>/control.json` (#536 §5 layout 3). | owner, 2026-11-03 |
| G5 | Defer: one request per worker connection stays; phase 7 is not run. | owner, 2026-11-03 |
| G6 | Split and rename what is internal: shared plumbing to `src/local/`, the backend to `backend/host_process/`, `NativeMacosProvider` → `HostProcessProvider`, `Runtime::MacosNative` → `Runtime::HostProcess`, `Backend::Native` → `Backend::HostProcess`. Stored and displayed values stay `"native"`. The pre-2.0 Linux path goes to `host_process/legacy_linux.rs`. | owner, 2026-11-03 |
| G7 | No: PostgreSQL Sandboxes on `lima`, `apple-container`, `microsandbox`, `firecracker` stay refused, naming the missing fact, until a live receipt records it. | owner, 2026-11-03 |
| G8 | No: sqlite Stores on `docker` keep the Worker executor on every host; the `Kind::route` comment is corrected. | owner, 2026-11-03 |

## SDK and CLI decisions (spec §9a)

| # | Decision | Taken |
| --- | --- | --- |
| P1 | `machine.start <name>` starts a stopped machine and its worker and waits until ready; a running machine answers at once. `machine.stop <name>` stops the worker and, where there is one, the VM; disks stay; refused `InUse` while Sandboxes run on it, with `detail.related` listing them; built-in `native` answers at once. Phase 4. | owner, 2026-11-03 |
| P2 | `machine.platforms` returns `{ host, platforms: [ { platform, how, backend, kernel, sqlite, postgresql: { store_calls, sandbox, reason }, ready, missing, version, options, description } ] }` (spec §9a), the same rows as `connect/SUPPORT.json`; it replaces `machine.list`'s `can_create`, removed in the same commit. Phase 3. | owner, 2026-11-03 |
| P3 | `machine.list` and `machine.get` give each machine `supports: { sqlite, postgresql_sandbox, reason }`, the platform row applied to it. Phase 3. | owner, 2026-11-03 |
| P4 | Every `*.list`, `*.get` and `*.status` call, and `machine.platforms`, never starts a machine: they answer from records and live probes; a stopped machine reads `stopped`. Calls that need the worker still start it on demand. Checked in phase 4. | owner, 2026-11-03 |
| P5 | A docker Sandbox with no image named loads the bundle's `sandbox-image.tar` into Docker once and uses its digest; `--image` still overrides. Phase 4. | owner, 2026-11-03 |

## Interleaving with #536

| # | Decision | Taken |
| --- | --- | --- |
| I1 | #535 and #536 are interleaved on branch `issue535-connect` in the order of [HANDOFF-2.md](HANDOFF-2.md) §5, so each piece is built once in its final place, with one combined terminal pass. Supersedes #536's "starts after #535's terminal pass" and #535's separate phase 8. | owner, 2026-11-03 |
| I2 | R3 (`machine/index.rs` replacing the `sandbox_of` scan) is not built: #536 layout 3 (`sandbox/<name>/`, one read per Sandbox) supersedes it. | owner, 2026-11-03 |
| I3 | #535 phase 4 (`place`, `deploy`, `machine.start`/`stop`, R1, R4–R6) is built inside `layerfs-control` (`common/place.rs`, `sandbox/deploy.rs`), not in `layerfs-api/src/place/`. | owner, 2026-11-03 |

## Supersessions recorded in #533

Phase 0 adds a "Superseded by #535" section to
[#533 DECISIONS](../533/DECISIONS.md#superseded-by-535-owner-2026-11-03)
for O21b (D4), O3, G13 and the "Step-12 deferral closed" PostgreSQL row
(G1), and Step 5.2 (G2). The D11 docs commit adds O2 and the #533 fix
records that built idle stop or on-demand start (P0-5, P0-6's idle reset,
P0-7's start before the home lock, the `start.lock` row's on-demand start).
The #533 rows themselves are left as written.

## Differences between spec §4 and the code

Rows added when the code at the base contradicts a spec fact. The code
wins; the step is adjusted.

| Spec | Code at the base | Effect |
| --- | --- | --- |
| §10 phase 0: "root, engine and api AGENTS.md … the PostgreSQL Sandbox wording" | Root, engine and api AGENTS.md hold no "VMs and ssh later" or other PostgreSQL Sandbox restriction; the restriction is only in #533's README and DECISIONS. | Phase 0 adds the G1/D9 rule to the root "Machines" paragraph instead of replacing old wording. |
| §5, §10 1.8: "the pre-2.0 Linux reopen/stop/delete path goes to `host_process/legacy_linux.rs`" | There is no separate pre-2.0 Linux path. It is the same `Sandbox` type (`backend/host_process/sandbox.rs`), compiled on Linux so that `layerfs-api` `host/discovery.rs` can reopen, stop and delete old Linux native records, plus the Linux `cfg` arms in `runtime.rs`. | `legacy_linux.rs` is not created; `host_process/` stays compiled on Linux and macOS, `provider.rs` macOS only, and `host_process/mod.rs` says the Linux build exists only for pre-2.0 records. Item 1.10 would remove that Linux build. |
| §10 1.10: delete the legacy path "once the 2.0 conversion is shown to reclaim pre-2.0 Linux native Sandboxes" | Not shown: no receipt exists that the 2.0 conversion reclaims pre-2.0 Linux native Sandboxes, and this pass has no pre-2.0 Linux home to show it on. | 1.10 is **not run** and stays open (see Open items). |
| §5: `layerfs-sandbox` `backend/mod.rs` holds `Backend` | The guard's declaration-only rule refuses an enum in `mod.rs`, and `backend/backend.rs` trips `clippy::module_inception`. | `Backend` lives in `backend/kind.rs`, re-exported from `backend/mod.rs` and the crate root (1.2). |
| §5: `docker/` platform files `cli.rs unix.rs windows.rs power.rs network.rs` | The docker machine's adapter was `docker/macos.rs`, serving macOS and Linux since #533 `4c740437cd`. | Renamed `unix.rs`; `power.rs` (start, stop) and `network.rs` (the published port) split out (1.3). |

## Implementation choices (not owner decisions)

Choices the spec leaves open, taken to keep its design and the repository
rules.

| Topic | Choice | Why |
| --- | --- | --- |
| #533 supersession form | A new section appended to #533 DECISIONS; the superseded rows keep their text. | Historical records and verdicts stay unchanged (root AGENTS.md); the new section is the pointer. |
| LOC counter scopes | `tools/production_loc.py` gains a `connect` scope; `layerfs-sandbox`, `layerfs-machine`, `layerfs-store-backup` and the historical `layerfs-remote-storage` are classified by name in every layout. | A pre-move tree then counts them under connect too, so per-crate and per-layer comparisons across a move are like for like (as #463 did for api). |
| Move commits and import lines | A move that splits a file or a crate adds `use` lines, `mod.rs` declarations and rustfmt rewraps; their production LOC is reported per file in the commit (1.2 +19, 1.3 +132, 1.4 +10). No function body changed (checked item by item). | Splitting into folders (D7, spec 1.3) cannot keep every file's imports; the bodies are what the move check protects. |
| Platform folders (1.3) | `observe.rs`, `create.rs` (ssh `connect.rs`), `power.rs` per platform; `network.rs` only where existing code was about ports (docker); lima `template.rs`, firecracker `scripts.rs`, ssh `host_key.rs`. | No empty files; `network.rs` arrives with `reach` and `forward` (phases 5, 6). |
| Store backup module paths (1.6) | `store-service`'s `remote::storage` alias is dropped: callers import `layerfs_store_backup::bucket::…` and `layerfs_store_backup::backup::…`; `push.rs` → `create.rs`, `pull.rs` → `restore.rs` with function names kept. `settings.rs` + `state.rs` → `link.rs` in its own refactor commit (1.6b). | Keeps the move commit a move; the merge is a separate refactor with its own LOC line. |
| `local::os` (1.8) | `local/os/mod.rs` is declared `#[path] mod os_impl`, so `crate::local::os` stays the per-platform alias the moved code already uses. | Avoids touching every `os::` use in the moved files. |
| `Handles` naming (2b) | The unified persistence type is the enum `Handles { Sqlite(SqliteHandles), Postgres(PostgresHandles) }`; the former struct `Handles` is `SqliteHandles`, which keeps `create`, `open` and `seal()`. 2b's doc sweep renamed `Handles::seal()` and the struct to `SqliteHandles` in root and engine AGENTS.md, the architecture docs, the #303 contracts and the handbooks, which matches the code. The spec copy from `layerfs-v2-merge` (§4, §6.1, §10 phase 2) says `Handles` where it means the unified type or the base's struct; the spec is not re-edited. | Current docs name the type that holds the method; the spec text is the owner's. |
| docker on Windows (B) | Docker's profile lists macOS and Linux hosts only; on Windows every Store kind is refused "docker through WSL2 is not designed yet" (`platforms/docker/windows.rs` answers every call as not designed). | The code wins over the handoff's "docker on every host"; adding Windows is one line in `platforms/docker/profile.rs` plus re-blessing `SUPPORT.json`. |
| Profile and adapter files (B) | Each platform's `PROFILE` and `ADAPTER` live in `platforms/<p>/profile.rs` and `adapter.rs`; `HostOs::NAMED`; `OptKind::Name` (firecracker's `host`); `observe` not added to `Adapter` (nothing calls it per platform); `reach`/`forward` arrive in phases 5 and 6. | The guard refuses `const`/`struct`/`impl` in `mod.rs`; no stub that pretends. |
| Bridge enums in machine (B) | The guard allows `layerfs-machine → layerfs-bridge`; machine re-exports the bridge's `KernelRoute` and `StoreKind` (lane B first had copies; `53c326e0bc` replaced them). | One enum per choice (§1 "typed and decided once"). |
| `SUPPORT.json` (B) | Generated by `support_json()`; the `support` test compares it, `LAYERFS_BLESS_SUPPORT=1` rewrites it. Top-level `generated_from` and one `{host, platforms}` block for macos, linux, windows. Unavailable platform rows have null backend, kernel and sqlite. | §5, §9a. |
| Refusal source (B) | `sandbox.create` refuses every (platform, Store kind) pair through `sandbox_allowed`, message ending with the table's reason. The guest build (no `machine` feature) keeps its native/docker-only PostgreSQL check, and `host/local.rs`'s namespaces backstop stays (it runs in the worker, which has no support table). | Every caller-side refusal comes from `support.rs`; the worker has no machine layer. |
| G-connect started early | Lane G-connect (connect crates only: `reach`, `host-gateway`, namespaces binds) ran beside lane C (api only) instead of after F; the files are disjoint. Its table flip (`04fd094d02`, branch `issue535-G-flip`) is held until the api side of 5a lands. | HANDOFF-2 §5a allows parallel lanes on disjoint code; enabling rows before the api serves them would allow calls that fail. |
| `reach` facts (G) | apple-container and microsandbox: no gateway alias is documented in the code or tools, so a loopback URL is refused with the G7 reason (not guessed). firecracker: `172.30.<n>.1` from `spec.state.network`, refused when no network is recorded. Only `localhost`, `127.0.0.1`, `::1` count as loopback. Off native, a URL with `host=`/`hostaddr=` query, a host list or an empty host (Unix socket) is refused. | §7.3 rules; no guessed alias (G7). |
| `reach` after the G7 probe (`78bd14c325`) | lima keeps `host.lima.internal` (a 127.0.0.1-only server answered). firecracker now refuses a loopback URL: its tap address `172.30.<n>.1` is the nested Lima host, and the Lima host has no outbound NAT. apple-container and microsandbox keep refusing, with reasons that state what was measured. No platform's create flags changed. | The receipt (`receipts/g7-reach/RECEIPT.md`, subagent probe lane, 2026-11-03) is the evidence; the alternatives open a host port range or add NAT and are owner calls (open items). |
| Helper spec (G) | Not versioned; now eight fixed `key=path` lines then `bind=<rw|ro>\t<host>\t<guest>` lines. Old and new helpers refuse each other's specs (fail closed). | Same-bundle wire format; a mixed bundle fails visibly. |
| Compatibility wrappers (G) | `Runtime::launch`/`NamespaceProvider::launch` map `ProviderLaunchOptions::Namespaces.store` to one `Bind::store`; `docker::desktop_url` delegates to `PostgresUrl`. The api switches to `launch_with_binds` and `layerfs_machine::reach` in step G and the wrappers are then deleted. Done: `1eb7919076` switched, `88079bab6d` deleted the field, `NamespaceProvider::launch` and `desktop_url`. | Lane G could not edit `api/` while lane C moved it. |
| PostgreSQL route (G, `1eb7919076`) | A Sandbox's host operations go to its machine's executor for both Store kinds. The caller resolves the password and applies `layerfs_machine::reach` (`sandbox/route.rs` `reached`) before choosing the executor, so a refused rewrite starts no machine. The value goes in the request's `postgresql` and host_args only; the worker strips `url`/`password` from the Store record it writes. | §7.4 steps 2, 3, 5; "never resolves a password, never writes the URL". |
| Reach check (G) | On `sandbox.create`, the side that launches reads the schema's binding over the reached URL (`store::postgres::read_binding`) and compares it with the record's `binding`; a connection failure or another binding is `Unavailable` naming the host, never the URL. It runs on every create, not only the first on a machine. | §7.4 step 4; the binding is the Store's identity (#533 step 10b). |
| Binds from the daemon's Store (G) | The launch facade derives binds from the daemon setup's Store (`sandbox/launch/provider.rs` `binds`): a sqlite locator → `[Bind::store]`, PostgreSQL → `[]`; nothing else carries a Store path to the runtime. | One source for what the root binds. |
| PostgreSQL leases (G) | A local PostgreSQL Sandbox call holds the Store's `open/<pid>` lease like sqlite; `store.disconnect` is also refused `InUse` while a Sandbox of the Store is recorded (`inventory::can_retire_store`), because a daemon on another machine holds no lease in this home. `stores_on` still counts sqlite Stores only: a PostgreSQL Store is on no machine, and `machine.remove` already refuses while Sandboxes are recorded on it. | §7.4 "PostgreSQL Stores take open/<pid> leases"; the record check covers remote daemons. |
| docker worker `host-gateway` (G) | On a Linux host the docker worker container is started with `--add-host host.docker.internal:host-gateway`, as Sandbox containers are, because PostgreSQL Sandboxes on `docker` now run from the worker. | `reach` gives `host.docker.internal` for docker on every host. |
| Executor (E) | `common/place.rs` `Executor {Local, Worker}`, `Start {OnDemand, Never}`; `Daemon` waits for step H. `Worker::ready()` (a readiness wait) is used only by `store.move`/`store.remove`, which must start their machine before marking the Store. | §6.4; no unused variant. |
| R1 boundary (E) | `Unreached` covers every failure up to and including the worker's answer to the secret (connect, hello, closed, `Denied`, a stopping worker's `Unavailable`); only that, on an OnDemand caller's first call, starts the machine, then the same framed request is sent once. | One attempt: nothing that may have reached the worker is resent. |
| No-start reads (E) | Before E, `sandbox.get` (via `list_workspaces`) and `workspace.status` started a machine. Now `sandbox.get` on a down machine answers `workspaces: []` and `down_reason`; `workspace.status` is refused `Unavailable` naming `machine start`. | P4. |
| `machine.stop` race (E) | `InUse` is checked under the home lock, but a `sandbox.create` claiming its name after the check can race the stop; there is no `stopping` marker. | Step F's `op` intent on the machine record closes it. |
| Deploy wire (E) | `sandbox.host` carries one `deploy` choice resolved by `sandbox/deploy.rs` `resolve(Side::Home | Side::Worker, ..)`; the old `docker`/`native` fields are gone, so caller and worker must come from one bundle. PostgreSQL Sandboxes still launch from the caller until step G. | G8, §10 phase 4. |
| Docker default image (E) | `machine/image.rs` loads `<bundle>/sandbox-image.tar` once with `layerfs_machine::load_image` (one attempt, 300 s), records `loaded_image {id, bundle}` on the docker machine and uses it for Sandboxes and the worker container; a new bundle hash loads again; missing file → `Unsupported` naming it. | P5. |
| `StoreRef` (2c) | `api/crates/layerfs-api/src/store/store_ref.rs`, not `place/store_ref.rs`; `kind_of` returns the bridge's `StoreKind`. PostgreSQL `schema`/`synchronous_commit` stay optional in `PgServer` and are required (`store_ref::required`) only where `identity_of` required them. | #536 step C moves `store/` into `layerfs-control` `store/` (I3); a fixture without schema (`sandbox_machines`) showed stricter parsing would change routing. |
| `Sandbox::Native` (1.9) | Renamed `Sandbox::HostProcess` with the three renames G6 lists, and `layerfs-api` `host::runtime::Backend::Native` likewise. | The enums that name the backend name it alike; stored and displayed values stay `"native"`. |

## Corrections to commit messages

History is not rewritten; these rows correct numbers a commit message got wrong.

| Commit | Message says | Recounted (`tools/production_loc.py` on clean archives) |
| --- | --- | --- |
| `990e43f166` (1.7) | `Production LOC: 132097 -> 132096 (delta -1)` | `132097 -> 132093 (delta -4)` |
| `86d4ec0652` (2c) | per-file split "store_ref.rs +76, call sites +14" | store_ref.rs +81, call sites +9 (total +90 is right) |
| `78bd14c325` (G7 reach) | "support.rs +5 …, firecracker adapter -5" | support.rs +4, firecracker adapter -4 (total 0 is right) |

## Corrections to the 2b doc sweep

2b's `Handles` → `SqliteHandles` sweep also rewrote 29 dated historical records
(`core/docs/issues/{302,307,320,327,352}/…`, including evidence snapshots and
withdrawn source under `307/checks/`, and the fs-bench-pro rollout ledger).
Root AGENTS.md keeps historical evidence unchanged, and the old name was right
when they were written, so step A restores those 29 files to their text before
2b. Current contracts and guides (#303, architecture, handbooks, AGENTS.md,
proposals) keep the sweep.

## Open items

- Step E live checks outstanding: R1 against real stopped/idle workers; `machine.start`/`stop` on lima, apple-container, microsandbox, firecracker, ssh, docker; a real `docker load` (classic and containerd image stores); the new deploy wire with a sealed bundle.
- `backup.restore` and `backup.list` still parse each Store record twice (R4 leftover).

- PostgreSQL Sandboxes on `docker` created before `1eb7919076` were launched by the caller; their later calls now go to the docker worker, which has no record of them. 2.0 is unreleased, so no conversion is written; close such a Sandbox with the build that made it.
- Step G live checks for step K: a PostgreSQL Sandbox end to end on native Linux (namespaces, no bind), docker (macOS and Linux, through the worker) and ssh, including a reach check that meets another schema.
- `ProviderCapabilities` has no `binds` capability yet (§6.3 lists one).
- Unverified live: `host-gateway` from the Linux worker inside Docker Desktop; a server bound only to 127.0.0.1 is not reachable through the bridge gateway on a Linux host.
- G7 owner calls (probe facts in `receipts/g7-reach/RECEIPT.md`; until decided, these platforms refuse a loopback URL and keep their PostgreSQL rows refused):
  - apple-container: keep refusing, or rewrite loopback to the gateway `container inspect` reports (`networks[0].ipv4Gateway`, 192.168.64.1 here) and require the server to listen on the bridge with `pg_hba` allowing 192.168.64.0/24.
  - microsandbox: keep refusing, or create machines with `--net public,host` (opens every host port to the guest; `msb modify` cannot add it, so existing machines are recreated) and rewrite to `host.microsandbox.internal`.
  - firecracker: keep refusing, or build NAT on the Lima host (ip_forward + masquerade for 172.30.0.0/16) and rewrite to 192.168.5.2 (`host.lima.internal` on the Lima host). A real Firecracker run is NOT_RUN (no binary, kernel or rootfs on this host).
- The probe lane's throwaway PostgreSQL container left an anonymous Docker volume, `ae17ce8d6d4f5beec46e7c0ecf85b8d314294d29a131220e00f2729f18d03588` (only the throwaway database); its `docker volume rm` was denied by the permission classifier, so the owner removes it by hand.
- Kit notes (#536 step 9): `connect` on a built-in platform is refused "…is created, not connected" (wording only); `Spec::validate` does not check docker's `os_image` against the profile.

- 1.10 (delete the pre-2.0 Linux native path) not run; needs a shown 2.0 conversion of a pre-2.0 Linux native Sandbox.
- `layerfs-api` `machine_host_start` failed once at the 1.5 staging (one failure at 10.7 s, output not kept) and passed on the next run of the same build. Watch for it; keep the output if it fails again.

## Phase log

- Phase 0 done at `451916521e`.
- Phase 1 done at `791d510b18`: moves 1.1 `aacdd17433`, 1.2 `4398b0fd41`, 1.3 `7035f0c71b`, 1.4 `77c59c524a`, 1.5 `1f40eb9e92`, 1.6 `0cada61acc` (+ 1.6b `236b7aceb2`), 1.7 `990e43f166`, 1.8 `0106ba8285`, 1.9 `791d510b18`; 1.10 not run.
- Phase 2a done at `dffd241aee` (`DaemonStore`, `KernelRoute`, `LAYERFS_KERNEL` gone; `LFSD\x07`).
- Phase 2b done at `c7e14c15a3` (persistence `StoreSelection`, `StoreFile`, one `Handles`; store-service fake path removed).
- Fix `48a50e4950`: two imports 2b left unused under `--features postgresql` (`host/postgres_init.rs`); the default-feature check set did not see them.
- Phase 2c done at `86d4ec0652` (api `StoreRef`; no `get_text("kind")` left in `layerfs-api`). **Phase 2 done at `86d4ec0652`.**
- Step B done at `53c326e0bc`: lane B (subagent, branch `issue535-B-support`) `56f6831a0e`, `1082648ca7`, `0a71736fd0`, `633e53f8b1`, merged `1b74839960`; api side `53c326e0bc` (lead). **Phase 3 done.**
- G-connect (5a, connect side) merged at `ba00cddfea`: lane G (subagent, branch `issue535-G-reach`) `23786fbd52`, `87bd0bfaff`, `2f35e9d6cb`, `510fd6400c`; `04fd094d02` held on `issue535-G-flip`.
- Verify lane (subagent, report-only, worktree `layerfs-issue535-verify` at `44623d029a`): Node/Python SDK tests on macOS, Linux sandbox/api/control targets (FUSE targets in a container with the nightly mount flags) and PostgreSQL targets against a throwaway server. It found two defects, fixed in `45b804329c` and rerun there (observed_application 9/9, postgres_connect 1/1; `init_postgresql` passed on macOS): `disposable()` refused every PostgreSQL `project.add` since 2b, and `observed_application` hard-coded a daemon locator since 2a. NOT_RUN there: `namespaces_userns` (writes the shared Docker VM's sysctl), `namespaces_lifecycle` (skips by design without blocked userns), the SDK live-exec halves (need an lfc VM).
- Step G api side (phase 5a) done: `1eb7919076` (route, reach check, leases, binds, docker worker host-gateway), `b5a66257c8` (store_routes test stale since E), flip merged `c45e2b4ed1`, wrappers deleted `88079bab6d` (lead). Live runs: step K.
- G7 probe lane (subagent, no product code): lima PASS; apple-container and microsandbox refuse loopback as created; firecracker network path simulated, real run NOT_RUN. Receipt committed under `receipts/g7-reach/`; `reach` fixed in `78bd14c325`.
- Step E (phase 4 in `layerfs-control`) done at `30842a0560`: lane E (subagent, branch `issue535-E-place`) `9eed8f696e`, `e8d330116e`, `5fd14480df`, `c6f3dc96d9`, `9de6d4c77d`, `ae4d9de019`, `aed6ad02a8`. R3 not built (I2). **Phase 4 done** (live checks open).
- Lane F2 (subagent, branch `issue535-F2-layout`: `b7004e5547`, `fb31a90a88`, `db82b917e7`, its merge of connect `6704207235`, `444d12da21`) merged at `9ce7dd5533`; its decisions are in [#536 DECISIONS](../536/DECISIONS.md) (`16b4c4a329`). Deleted test: `worker_store_record.rs`.
- **D11 done** (lead, ahead of F's other parts, H and the rest, as the owner ordered): `a5ef188cf7` (worker and VM idle stop removed: `machine/worker/idle.rs`, the server's gate, the CLI worker's power-off, `Platform::guest_stop`, the launch script's stop argument), `b6261beee5` (on-demand start removed: a call on a stopped machine is `Unavailable` naming `layerfs machine start <name>`; `endpoint::start_after` and `place::Start` gone), `c2a4808113` (the daemon's idle exit removed: `application/idle.rs`, `DaemonLimits::idle_exit_s`, `IDLE_EXIT_S`; DaemonSetup magic `LFSD\x07` -> `LFSD\x08`; guard `lifecycle_violations` in `tools/check_product_boundary.py`), then the docs commit. Production LOC 133430 -> 133239 -> 133159 -> 133082. Deleted tests: `worker_idle.rs`, `worker_idle_edge.rs`, `worker_idle_probes.rs`, `machine_platform::an_idle_worker_resets_a_microvm_and_powers_other_vms_off`, `observed_application::an_idle_daemon_exits_and_a_workspace_keeps_it`. Added: `worker_lifecycle.rs` (a worker with no requests keeps serving, no real waiting), `store_reaper.rs` (the reaper still closes an unused Store), the guard test `test_d11_idle_stop_and_on_demand_start_cannot_return`; `machine_no_start`, `worker_dial` and `docker_default_image` now expect the refusal naming `machine start`.
- D11 side effects recorded: `store move` and `store remove` refuse a stopped machine instead of starting it; `machine start` on a firecracker machine still starts its lima host inside that one explicit call; the conformance kits (`layerfs-sandbox`, `layerfs-machine` `tests/conformance.rs`) needed no change (they never drove idle stop or on-demand start); #533 O36's image still holds `/usr/sbin/poweroff`, which nothing calls now. The step-G api side was built before D11 arrived (`1eb7919076`..`88079bab6d`), so "before the step-G api side" could not be honoured; D11 did not touch it. The H-connect merge (`issue535-H-forward`) is held until after D11.
