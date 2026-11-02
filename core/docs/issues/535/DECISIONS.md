# #535 decisions log

> **Status:** Record of the owner decisions, gate answers and implementation
> choices for the #535 implementation on branch `issue535-connect`.

Spec: [README.md](README.md). Handoff prompt: [HANDOFF-PROMPT.md](HANDOFF-PROMPT.md).

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
| G1 | Yes: supersede #533 O3, G13 and the "Step-12 deferral closed" PostgreSQL row. A PostgreSQL Sandbox runs on every machine the Store is attached to, where §7.3 marks the platform supported. | owner, 2026-11-03 |
| G2 | Yes, narrowly: supersede #533 "Step 5.2". One crate-private, sealed trait per extension point (`Adapter`, `BackendAdapter`), no default methods, reached through one exhaustive `match`. No public trait, registry, factory or plugin. | owner, 2026-11-03 |
| G3 | Explicit attachment: a PostgreSQL Store record has `machines: { <machine>: { url? } }`; `store.attach <store> <machine> [--url]` and `store.detach <store> <machine>`. | owner, 2026-11-03 |
| G4 | Yes: Workspace calls reach a VM Sandbox's daemon directly (control keys in the home Sandbox record, port forwarded to host loopback). | owner, 2026-11-03 |
| G5 | Defer: one request per worker connection stays; phase 7 is not run. | owner, 2026-11-03 |
| G6 | Split and rename what is internal: shared plumbing to `src/local/`, the backend to `backend/host_process/`, `NativeMacosProvider` → `HostProcessProvider`, `Runtime::MacosNative` → `Runtime::HostProcess`, `Backend::Native` → `Backend::HostProcess`. Stored and displayed values stay `"native"`. The pre-2.0 Linux path goes to `host_process/legacy_linux.rs`. | owner, 2026-11-03 |
| G7 | No: PostgreSQL Sandboxes on `lima`, `apple-container`, `microsandbox`, `firecracker` stay refused, naming the missing fact, until a live receipt records it. | owner, 2026-11-03 |
| G8 | No: sqlite Stores on `docker` keep the Worker executor on every host; the `Kind::route` comment is corrected. | owner, 2026-11-03 |

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

## Implementation choices (not owner decisions)

Choices the spec leaves open, taken to keep its design and the repository
rules.

| Topic | Choice | Why |
| --- | --- | --- |
| #533 supersession form | A new section appended to #533 DECISIONS; the superseded rows keep their text. | Historical records and verdicts stay unchanged (root AGENTS.md); the new section is the pointer. |

## Phase log
