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
(G1), and Step 5.2 (G2). The #533 rows themselves are left as written.

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
| `Sandbox::Native` (1.9) | Renamed `Sandbox::HostProcess` with the three renames G6 lists, and `layerfs-api` `host::runtime::Backend::Native` likewise. | The enums that name the backend name it alike; stored and displayed values stay `"native"`. |

## Corrections to commit messages

History is not rewritten; these rows correct numbers a commit message got wrong.

| Commit | Message says | Recounted (`tools/production_loc.py` on clean archives) |
| --- | --- | --- |
| `990e43f166` (1.7) | `Production LOC: 132097 -> 132096 (delta -1)` | `132097 -> 132093 (delta -4)` |

## Corrections to the 2b doc sweep

2b's `Handles` → `SqliteHandles` sweep also rewrote 29 dated historical records
(`core/docs/issues/{302,307,320,327,352}/…`, including evidence snapshots and
withdrawn source under `307/checks/`, and the fs-bench-pro rollout ledger).
Root AGENTS.md keeps historical evidence unchanged, and the old name was right
when they were written, so step A restores those 29 files to their text before
2b. Current contracts and guides (#303, architecture, handbooks, AGENTS.md,
proposals) keep the sweep.

## Open items

- 1.10 (delete the pre-2.0 Linux native path) not run; needs a shown 2.0 conversion of a pre-2.0 Linux native Sandbox.
- `layerfs-api` `machine_host_start` failed once at the 1.5 staging (one failure at 10.7 s, output not kept) and passed on the next run of the same build. Watch for it; keep the output if it fails again.

## Phase log

- Phase 0 done at `451916521e`.
- Phase 1 done at `791d510b18`: moves 1.1 `aacdd17433`, 1.2 `4398b0fd41`, 1.3 `7035f0c71b`, 1.4 `77c59c524a`, 1.5 `1f40eb9e92`, 1.6 `0cada61acc` (+ 1.6b `236b7aceb2`), 1.7 `990e43f166`, 1.8 `0106ba8285`, 1.9 `791d510b18`; 1.10 not run.
- Phase 2a done at `dffd241aee` (`DaemonStore`, `KernelRoute`, `LAYERFS_KERNEL` gone; `LFSD\x07`).
- Phase 2b done at `c7e14c15a3` (persistence `StoreSelection`, `StoreFile`, one `Handles`; store-service fake path removed).
