# Phase 0 measurement contract (frozen before collection)

> **Status:** Frozen contract for the Phase 0 baseline of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178), the opening act
> of [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
> Written into the evidence directory **before any receipt was collected**, as
> required by the Phase 0 tasking. Nothing in the collection below may relax it;
> a change after collection would need a new stamp and a new directory.
>
> This is a **measurements-only** campaign. It contains no product, test, harness
> or example source change, no optimization and no tuning.

## 1. Frozen tree and toolchain

| Field | Frozen value |
| --- | --- |
| Repository | `Ephemeral-AI-Lab/layerfs` |
| Source commit | `dcedd7ef13338366bd80446fc7a9008cca69d683` (`main`) |
| Working tree | clean tracked tree; `git status` and `git rev-parse HEAD` recorded in `tree.txt` at collection time |
| Toolchain | `cargo +1.85.1` / `rustc 1.85.1 (4eb161250 2025-03-15)` |
| Locking | `--locked` on every Cargo invocation, both workspaces |
| Profile | `release` for anything timed |
| Host | Apple M3 Max, 14 CPUs, 36 GiB RAM, macOS; results are host-specific and carry no cross-host claim |

Identities are pinned **per receipt**, not only here: each receipt records the
source commit, the dirty flag over `core/crates` + `crates`, the product, the
compilation seal (profile + toolchain + `--locked`), the harness identity, and
the workload definition (its parameters or a verbatim spec). A rebuilt artifact
needs a rebuilt matched arm; a harness change invalidates the pair.

## 2. Sample and evidence discipline

1. **One sample per case per arm.** No best-of, no n3, no re-runs to improve a
   number. Any repeated run in this directory is a **determinism diagnostic**
   and is labelled as one beside the gate sample.
2. **Fresh `--output` per run.** Receipts are append-only and are never
   overwritten; failed and discarded attempts stay on disk with their exit code.
3. **Cache state is declared and equal in both arms.** The component-primitive
   family uses the precedent's declared state: *warm in-process fixture; the base
   tree is built in the arm process before the timed region*. The C2-only family
   uses: *the fixture's canonical objects are constructed by the harness process
   before the timed region; the Store is created inside the timed region*. No
   warm/cold pooling anywhere, and no cold-OS-cache claim is made by any row.
4. **Worker count is one** for every timed arm (one process, one thread,
   `LAYERFS_CONSTRUCTION_WORKERS=1` where the vehicle reads it). No row raises it.
5. **Measurement lock.** Nothing else resource-sensitive runs during a
   collection; the complete command's wall time is recorded next to its exit code.
6. **Budget.** Each complete command (product timer + lifecycle + cleanup) must
   fit the ordinary **≤ 15 s** budget, with a declared exception list ≤ 25 s.
   Exceptions are declared per row in the manifest; a command that does not fit
   is recorded as `NOT_RUN` with its measured wall time rather than shrunk.
7. **No source changes of any kind** — product, test, harness or example. If a
   needed counter or knob does not exist, that is reported as a finding with a
   proposed separate change, never hacked in. A pragma set for a run is allowed
   only when that run is labelled **diagnostic**; gate samples stay on the
   unmodified configuration.

## 3. P0-1 — matched-workload family definitions

| Family | Reference arm | Candidate arm | Verdict scope |
| --- | --- | --- | --- |
| `component.primitives` | `crates/layerfs-content/examples/stage5_component_reference.rs` built from the root `Cargo.toml` | `core/crates/layerfs-content/examples/filesystem_primitives_candidate.rs` built from `core/Cargo.toml` | matched — **collect** |
| `pipeline.filesystem` | none: the reference has no public entry point that consumes final sorted bindings/typed values and returns a root | `core` `build_filesystem` / `update_filesystem` | `NOT_RUN` — re-verified at this tree, call sites cited |
| `pipeline.c2` | reference admission entry points (`LayerStackStore::workspace_admission`, `WorkspaceAdmission`, `admit_page`) | `Store::begin_save` / `accept` / `finish` | `NOT_RUN` — re-verified at this tree, call sites cited |

`component.primitives` cases are the precedent's three, unchanged:

| Case | Files | Change pairs | Identity keys that must MATCH |
| --- | ---: | ---: | --- |
| `small` | 200 | 20 | `base_directory`, `base_table`, `base_root`, `updated_directory`, `table`, `root` |
| `wide` | 2,000 | 200 | same six |
| `large-few-changes` | 20,000 | 20 | same six |

**Identity gate.** A case fails if any of the six identities differs; the driver
prints `MISMATCH` and no ratio. **Budget gate.** Each arm's complete command must
fit §2.6. **No aggregate claim.** No ratio is computed across cases, and no
complete-operation, storage, cold-cache or release-admission claim follows.

The earlier collections at `eb42c1347` (governing) and `3b4941f1e` (superseded)
are retained unedited; any comparison against them in this directory is labelled
**diagnostic**.

## 4. P0-2 — the spilling question, its method and its limits

**Question.** Does SQLite page-cache spilling occur at core's transaction sizes
(up to 8,191 rows / `4 MiB − 1` canonical bytes) under the default 2 MiB page
cache with `journal_mode = MEMORY`?

**Method, in the order the tasking prefers it.**

1. **Reachable status counter.** Determine whether the pinned `rusqlite 0.40.2`
   exposes any SQLite status counter that distinguishes a cache spill (a
   `sqlite3_db_status` / `sqlite3_status` route), and whether the product's own
   surfaces expose it. If it is reachable, use it as the primary observable.
2. **`PRAGMA cache_spill` read-back.** Record what the pragma reports and, in the
   receipt, state exactly what that value does and does not establish (it is a
   policy flag, not an occurrence counter).
3. **Labelled diagnostic A/B.** Only if (1) yields nothing: the same save at the
   default cache versus a raised `cache_size`, comparing the observables that do
   exist — `SaveOutcome.commits`, elapsed (diagnostic grade), journal behaviour,
   and RSS as a diagnostic. A raised-cache arm is a **diagnostic**, never the gate
   sample; the gate sample stays on the unmodified configuration.

**The workload for P0-2 is the P0-3 `c2-ceiling` workload**, so the gate sample
is collected once and shared between P0-2 and P0-3.

**Honest outcomes.** If no observable exists without a code change, the answer is
`UNDETERMINED — no reachable observable without instrumentation`, with the
proposed instrumentation reported as a finding. Constants alone decide nothing.

## 5. P0-3 — workload set for the counter baseline (core only)

Fixed before collection; kept fixed for every later Phase 1 comparison.

| Workload id | Vehicle (unmodified example) | Parameters | Anchor for |
| --- | --- | --- | --- |
| `c1.empty` | `core` `filesystem_timing_c1` | `--case empty` | trivial re-emit row |
| `c1.directory-update` | `core` `filesystem_timing_c1` | `--case directory-update` | C1 tree navigation |
| `c1.inode-update` | `core` `filesystem_timing_c1` | `--case inode-update` | C1 tree navigation |
| `c1.hardlink-move` | `core` `filesystem_timing_c1` | `--case hardlink-move` | C1 tree navigation, two parents |
| `c1.subtree-remove` | `core` `filesystem_timing_c1` | `--case subtree-remove` | release-heavy counting (P1-10) |
| `c1.attributes` | `core` `filesystem_timing_c1` | `--case attributes` | attribute patch route |
| `fs.c1.directory-update` | `core` `measure_filesystem` | `--mode c1 --case directory-update --entries 200` | C1 update counters |
| `fs.c2.directory-update` | `core` `measure_filesystem` | `--mode c2 --case directory-update --entries 200` | admission counters |
| `fs.pipeline.directory-update` | `core` `measure_filesystem` | `--mode pipeline --case directory-update --entries 200` | per-phase elapsed |
| `edits.c1.*` | `core` `measure_edits` | `--mode c1` × `small`, `chunked`, `small-to-large`, `large-to-small`, `batch` | edit route `nodes_read` (P1-6, P1-7, P1-8, P1-9, P1-14) |
| `edits.c2.*` | `core` `measure_edits` | `--mode c2`, same five cases | storage-only counters |
| `edits.pipeline.*` | `core` `measure_edits` | `--mode pipeline`, same five cases | integrated counters + per-phase elapsed |
| `c2.ceiling` | `core` `measure_pooled` | chosen so rows = **8,191** and canonical bytes ≤ `4 MiB − 1`, at the default policy | P2-1/O1, P2-2/O2, the spilling question |
| `c2.small` | `core` `measure_pooled` | the same shape at a fraction of the ceiling (≈ 1/8) | transport term at ordinary size |
| `order.default` | `s5term grid` shape, `maximum_pending_records =` the default 4,096, base 4,000 files, 2,000 rename pairs | re-implementation of the public-API probe in this evidence directory | whether spills happen in normal operation |
| `order.forced64` | the same shape at the forced `maximum_pending_records = 64` | same client | P1-5 / P1-13's anchor |

**Counters to record per workload**, wherever the vehicle exposes them:
`ObjectWork` (`objects_read`, `objects_emitted`, `bytes_read`, `read_waves`),
`SortedWork` for directories and inodes (`pages_read`, `pages_created`,
`pages_reused`, `peak_scratch_bytes`), `ReferenceWork` (`rows_spilled`,
`peak_pending`, `runs.rows_read`, `runs.rows_written`, `runs.runs_created`,
`runs.merges`, `runs.peak_run_bytes`, `runs.peak_live_runs`), the edit route's
`nodes_read`, `ValidationWork` (`entries_examined`, `objects_read`), storage
counters (packs created, commits, pooled values), and the timer's per-phase
elapsed as **diagnostic-grade single samples**.

A counter that the vehicle does not expose is recorded as `NOT_EXPOSED` with the
reason — never estimated and never hacked in. `SortedWork.pages_read` is recorded
with the standing note that it currently **undercounts batched decodes**
(#178 P1-11).

**Determinism.** For each counter, one labelled determinism re-run of the same
workload is allowed as a **diagnostic**; the receipt states, per counter, whether
the two runs agreed. The gate sample is the first run.

## 6. Acceptance (restated from the tasking)

- This contract exists before any receipt. ✔ (this file)
- P0-1: every matched family collected with identities/limits/cache state/workers
  declared, or `NOT_RUN` with a current-tree reason.
- P0-2: spilling determined, or `UNDETERMINED` with the method's limits and the
  proposed instrumentation.
- P0-3: the counter baseline exists for the frozen workload set, with the Phase 1
  anchor counters present and determinism labelled.
- Every command's exit code and wall time recorded; every failure retained.
- No commit touches `core/crates`, `crates/`, tests, examples or tools.
