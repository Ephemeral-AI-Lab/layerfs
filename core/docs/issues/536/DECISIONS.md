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

## Implementation choices (not owner decisions)

| Topic | Choice | Why |
| --- | --- | --- |

## Open items

## Phase log
