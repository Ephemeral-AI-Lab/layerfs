# 05 — Migration plan from the Phase 4.5 base

> **Status:** Current planning checklist; no release candidate exists.
> Part of the [Phase 7 packet](README.md). Proposal; milestone scope is for
> review and no milestone has started. The crate split in §3 was accepted by the
> owner on 2026-10-03.

> **Superseded 2026-10-05** for cluster two by
> [`../303/07-implementation-validation.md`](../303/07-implementation-validation.md).
> Milestones M1–M5 were not carried out as written. The text below is unchanged
> and is kept as context.

## 1. Base

| Item | Value |
| --- | --- |
| Source base (D1) | `origin/main` `7edddbdb8e8512627aed0ed42533ef099d802384` — Phase 4.5 with Phase B (#284, #286) integrated |
| Not a base | Phase 5 (`codex/issue287-implementation`, `codex/phase5-*`), Phase 6 (`codex/phase6-metadata-experiments` at `203879845`, `codex/phase6-sp1`, `codex/phase6-sp1-strict`) |
| Preserved | Every Phase 5/6 branch, receipt, failure and verdict stays as it is; nothing is rebased, relabelled or deleted |

Production size at the base, from the repository counter
(`python3 core/tools/production_loc.py --detail`, run at `7edddbdb8`): **70,279
production lines in 357 files** for `core`; the reference tree under root
`crates/` is 65,417.

## 2. Impact per crate

| Crate | Production lines at base | Phase 7 | What changes |
| --- | ---: | --- | --- |
| `layerfs-content` (C1) | 13,483 | **Keep** | Target: no format or algorithm change. Touch only if a port signature requires it |
| `layerfs-storage` (C2) | 8,716 | **Rework the storage boundary** | `encoding/` and `pack/` kept. `cas/` is rewired to two ports it defines (object store, locator and metadata bodies) and routes each body by domain. `sqlite/` stays only for the old path and is deleted in M9 |
| `layerfs-history` (C5) | 2,658 | **Keep, provider replaced** | `HistoryCatalog`, records and identities stay. The SQLite provider under `sqlite/` serves the old path until M9; the PostgreSQL provider is written in `layerfs-metadata`. Whether the stage/commit-staged two-step is still needed is reviewed in M0 |
| `layerfs-s3` | — | **New** | S3 client implementing C2's `ObjectStore` ([04 §2](04-storage-interfaces.md#2-object-store-port)) |
| `layerfs-metadata` | — | **New** | PostgreSQL engine implementing C2's locator port and `HistoryCatalog` ([04 §3](04-storage-interfaces.md#3-global-metadata-store-postgresql)) |
| `layerfs-overlay` | — | **New** | Daemon SQLite engine: generation-keyed live state ([02](02-workspace-overlay.md)) |
| `layerfs-workspace` | 26,835 | **Rewrite** | `backing/`, `commit/`, `overlay/`, `runtime/` replaced. POSIX semantics in `filesystem/` are re-expressed over the overlay |
| `layerfs-fuse` | 1,447 | **Keep, rebind** | Same kernel adapter, new Workspace calls |
| `layerfs-daemon` | 2,151 | **Rework** | Becomes the composition root: wires engines into ports, holds credentials, runs Commit and Init |
| `layerfs-server` | 3,996 | **Retire** | Host-side save/read/construction service removed. Namespace Init/import survives as a cluster 1 function over C1, C2, C5 and the two engines; its home crate is D13 |
| `layerfs-bridge` | 6,834 | **Shrink** | Control transport only; payload, metadata and prepared-stream contracts removed |
| `layerfs-api`, `layerfs-sandbox`, `layerfs-telemetry` | 796 / 1,106 / 2,257 | **Keep** | Sandbox bootstraps the PostgreSQL and MinIO containers |

No size is predicted for the result. Each commit reports
`Production LOC: <before> -> <after> (delta …)` with old and replacement
subtotals while they coexist, as root `AGENTS.md` §4 requires. Code that moves
between crates is reported as relocation, and deletion of the old Workspace as
retirement, not as simplification.

## 3. Target layout

File names below are a proposal to show where responsibilities sit; they are
frozen per crate when that crate's milestone starts.

```text
core/crates/
  layerfs-content/          DOMAIN  C1, unchanged
    src/ object/  file/  filesystem/  policy.rs  error.rs

  layerfs-storage/          DOMAIN  C2
    src/ encoding/          unchanged (codec, full, delta, pool)
         pack/              unchanged (assemble, layout, placement)
         cas/               save and read orchestration over the two ports
         port/              object_store.rs  metadata_store.rs   (traits, keys, errors)
         policy.rs  error.rs

  layerfs-history/          DOMAIN  C5
    src/ catalog.rs  records.rs  identity.rs  error.rs

  layerfs-s3/               ENGINE  S3-compatible object store
    src/ sign.rs  request.rs  response.rs  client.rs

  layerfs-metadata/         ENGINE  global PostgreSQL
    src/ connection.rs  schema.rs       connections, schema and its versioning
         locator/                       C2 port: lookup, register, bodies, dependencies
         history/                       C5 port: allocation, branch, commit, layerstack, query

  layerfs-overlay/          ENGINE  daemon SQLite
    src/ connection.rs  schema.rs
         scope.rs                       Workspace row and the three generation numbers
         inode.rs  dentry.rs  block.rs  xattr.rs  handle.rs
         view.rs                        the visibility rule (live view, Commit view)
         capture.rs  retire.rs  dirty.rs

  layerfs-workspace/        RUNTIME  rewritten
    src/ workspace.rs                   open, close, state
         lower.rs                       LowerFilesystem port (the committed base)
         read_plan.rs
         ops/                           lookup, readdir, create, read, write, truncate,
                                        rename, remove, link, symlink, attr
         commit/                        context, construct_file, construct_namespace,
                                        publish, install, outcome

  layerfs-fuse/             RUNTIME  unchanged files (adapter, mount, replies)
  layerfs-daemon/           RUNTIME  wiring, control, execution, lifecycle
  layerfs-bridge/           RUNTIME  control contracts and native transport only
  layerfs-api/  layerfs-sandbox/  layerfs-telemetry/
```

The allowed-dependency table and the relationship graph are in
[01 §3](01-architecture.md#3-components).

**Why engines are separate crates (D8, accepted).** Each engine has a large,
foreign dependency set (an HTTP stack, a PostgreSQL client, or SQLite) and a
different reason to
change from the logic that uses it. A crate boundary means C2 and C5 cannot
reach an HTTP or SQL type even by accident, and each engine can be replaced
alone. `layerfs-overlay` and `layerfs-workspace` are split for the same reason:
one knows SQLite and nothing about filesystems-as-a-product, the other knows no
SQL.

All new production files follow `core/AGENTS.md`: product-only `src/`, external
tests under `tests/`, at most 999 physical lines per file and 200 for `lib.rs`
and `mod.rs`.

## 4. What Phase 6 contributes

Carried as **evidence and decisions**, not as source:

- The strict placement split and the trusted-publisher model (#293).
- C2 storage-parity requirements and the fixture seal at `81f2cf22d`, reusable
  only by exact source, hash and scope (#295).
- The requirement set for unbounded population with bounded residency (#296).
- The overlay hypothesis and its safety questions (#298) — answered here by
  putting payload in the same transaction as metadata.
- Daemon-local execution and the SQL-work instrument (#299): statements and
  transactions per operation are the diagnostic to reuse.
- Read amplification analysis (#300), still deferred.

Not ported: `core/benchmark/phase6-live`, the `born`/`until` row-version schema,
private source custody, and the in-prototype MinIO client.

## 5. Milestones

Both clusters start from the Phase 4.5 base (§1), each in its own worktree. The
detailed implementation plan for each — architecture, file and folder structure,
rollout, final verification and benchmark — is written by a planning agent from
[prompts/cluster1-storage-plan.md](prompts/cluster1-storage-plan.md) and
[prompts/cluster2-sandbox-plan.md](prompts/cluster2-sandbox-plan.md), and lands
in `core/docs/issues/302/` and `core/docs/issues/303/`. The milestones below are
the big picture those plans refine.

Two clusters, developed in parallel ([01 §3](01-architecture.md#3-components)).
**Cluster 1** is fully independent and needs no sandbox. **Cluster 2** is
independent for everything except its last step: mounting a Workspace and
running operations through FUSE need nothing from cluster 1, but Commit does.
Within a cluster a consumer starts only after its provider is finished and tested
alone. Verification runs once per frozen source with the commands that cover the
change.

```text
M0 contracts
 ├─ Cluster 1 (storage)   M1 ports ─> M2 s3 ─┬─> M4 parity ─> M5 Init + history schedules ─┐
 │                                 M3 metadata ┘                                          v
 └─ Cluster 2 (sandbox)   M6 overlay + workspace ops ─> M7 fuse + daemon ─────────> M8 integration: base reads + Commit
                                                                                          └─> M9 retire ─> M10 qualify
```

- [ ] **M0 — Contracts.** Resolve the open decisions. Freeze the three new
  ports, both schemas ([06](06-tables.md)) and the capture context. Extend
  `check_product_boundary.py` to enforce the allowed-dependency table and the
  cluster rule. Read the two risky seams in the source before freezing: C2's
  `cas/` ↔ `sqlite/` boundary (D10), and C1's edit input against overlay block
  runs. *Exit: reviewed documents; production LOC delta 0.*

**Cluster 1 — storage** ([#302](https://github.com/Ephemeral-AI-Lab/layerfs/issues/302)).
**No sandbox, no daemon, no mount. No Commit, conflict or edits: those are not
storage concepts.**

- [ ] **M1 — `layerfs-storage` ports.** Define `ObjectStore` and the locator and
  metadata-body port; rewire `cas/` to route bodies by domain. *Exit: existing
  C2 vectors pass against in-test engines; no network or SQL type in the crate's
  public surface.*
- [ ] **M2 — `layerfs-s3`.** *Exit: put-if-absent, whole and ranged read, head
  and every error class pass against real MinIO; conditional create verified on
  the pinned release; requests and bytes counted.*
- [ ] **M3 — `layerfs-metadata`.** PostgreSQL providers for the locator port and
  `HistoryCatalog`. *Exit: the existing C5 contract tests and the locator
  contract pass against a real, pinned PostgreSQL; two connections exercise
  insert-if-absent and the conditional head update; a lost reply is classified
  `Uncertain`; round trips per operation recorded.*
- [ ] **M4 — Storage parity.** C2 + `layerfs-s3` + `layerfs-metadata` composed in
  an external test. *Exit: identical-content reuse, PREFIX selection, depth-bound
  FULL, cross-pack base, pooled metadata, and corrupt or missing base refusal
  pass with payload on MinIO and metadata in PostgreSQL.*
- [ ] **M5 — Cluster 1 on real workloads.** Namespace Init/import moved onto the
  new stores (D13), then two existing selections that never needed a sandbox:
  the **namespace-init family of `fs-bench-pro`**, and the **deepseek-harness
  history-storage schedules at stride 10, 3 and 1** (#286, #295). This is the
  cluster's acceptance: **meet the earlier baseline for namespace-init speed, and
  for the three strides in speed and storage compression.** *Exit: complete
  namespace and bytes verified by the independent verifier; history storage
  accounted as MinIO object bytes plus PostgreSQL allocation; speed rows taken
  under root `AGENTS.md` §1–§3 with a declared cache contract for both servers
  (Init keeps release binaries and its multi-worker exception; the three strides
  are three selections, one sample each; stride 1 stays run-only).*

**Cluster 2 — sandbox** ([#303](https://github.com/Ephemeral-AI-Lab/layerfs/issues/303)).
**Independent of cluster 1 until M8; it also owns the join and everything after
it.**

- [ ] **M6 — `layerfs-overlay` with `layerfs-workspace` operations.** Built
  together, one operation at a time (create, write, read, truncate, rename,
  remove, link, readdir, attributes): each adds its overlay rows, its Workspace
  semantics and its external tests. The overlay's API has exactly one consumer,
  so it is shaped by that consumer and not designed ahead of it. *Exit: POSIX
  operations pass against an in-test committed base; generation, capture,
  install and retire rules pass; statements and transactions per operation and
  the first page-cache diagnostic recorded.*
- [ ] **M7 — `layerfs-fuse` rebind and `layerfs-daemon` wiring.** The sandbox
  stack runs end to end on an **empty committed base** — a real product state (a
  new project), not a test path. *Exit: a Workspace mounts; generic commands
  through the real mount create, modify and read files held only in the overlay;
  no command recognizer; trust boundary re-proved.*
- [ ] **M8 — Integration with cluster 1.** The only cluster 2 step that depends
  on cluster 1. Reads of a non-empty committed base (`LowerFilesystem` over C1/C2
  and the two engines), and Commit: capture → C1 → C2 → MinIO and PostgreSQL →
  conditional head update in `layerfs-history` → install → retire.
  `layerfs-bridge` reduced to control. *Exit: SDK exec and commit work end to
  end; independent verification of committed bytes, metadata and history; a
  deterministic overlap witness shows captured bytes isolated from later writes
  and later writes preserved after install; Conflict, Refused and Uncertain
  paths exercised.*

**After the join** (still #303)

- [ ] **M9 — Retirement.** `layerfs-server`, bridge payload contracts, the old
  embedded stores and the old Workspace code removed. *Exit: no reference to the
  removed paths; per-commit LOC with relocation and retirement labelled.*
- [ ] **M10 — Final verification.** Cluster 2's acceptance, on the integrated
  product: **complete all seven `fs-bench-pro` families** — `init_namespace`,
  `history_retention`, `workspace_write`, `workspace_commit`,
  `workspace_namespace`, `workspace_mutations`, `workspace_shell_package` —
  against the Phase B baseline under root `AGENTS.md` §1–§3. Receipts from M5 are
  reused by identity where the source is unchanged. *No speed claim for the
  sandbox families exists before this.*

**Why the overlay is not a milestone of its own.** It is a leaf in the
dependency graph, but it has no use and no API of its own apart from the
Workspace, and it only runs inside the sandbox with the daemon. Building it
alone would mean guessing that API. Building it with the Workspace, and bringing
up the real mount in M7, also gives end-to-end feedback before the join.

**Coexistence until M9.** The old host service depends on today's C2 and its
embedded SQLite Store. To keep the tree building while the replacement is
incomplete, the M1 rework should be additive: the existing SQLite code remains
as the old path's implementation of the new ports and is deleted in M9. Whether
`cas/` permits that is part of the M0 source check; if it does not, the old
service is retired earlier and the gap is stated.

## 6. Verification

- This repository runs no CI and no aggregate pre-push gate. Per change:
  `cargo +1.85.1 test`, `clippy` and `fmt` with
  `--manifest-path core/Cargo.toml --locked`, plus
  `core/tools/check_product_boundary.py`; report which ran and which did not.
- Real provider and real kernel for anything that claims them: MinIO for M2 and
  M4, PostgreSQL for M3 and M4, FUSE for M7 and M8. Independent expected state; a candidate never supplies
  its own oracle.
- Diagnostics are count-driven (statements, transactions, rows, requests,
  bytes) and labelled as diagnostics. One sample per case per arm; no repeated
  runs to pick a number.

## 7. Related issues

The design addresses the mechanism behind several open issues. None is closed by
this packet; each still needs its own proof.

| Issue | Relation |
| --- | --- |
| #298 | This design is its proposed resolution (payload in overlay rows instead of reusable backing sources) |
| #299 | Daemon-local Workspace and C1/C2 adopted as the only route |
| #300 | Nice to have, not on the milestone path. Kept possible by the port's range parameter and recorded group offsets; revisited with the M8 measurement ([04 §10](04-storage-interfaces.md#10-ranged-reads-300--nice-to-have)) |
| #245, #261 | Range copy-on-write and many mounted writes become block-row replacement |
| #256 | Fixed dirty-identity and changed-name capacities are replaced by indexed rows |
| #248 | Commit reads final state from frozen rows; C1's deferred-edit allowance remains |
| #249 | Concurrent Exec and Workspaces are permitted by the design; aggregate admission is still to be designed |
| #259, #276 | Unchanged; carried forward |
| #287, #288, #290 | Phase 5 stays paused |
