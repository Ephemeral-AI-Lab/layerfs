# P0-1 — the matched-workload receipt, honestly scoped

> **Status:** Collected receipt for the one honestly matchable family, plus two
> re-verified `NOT_RUN` rows at this tree. Contract: `../CONTRACT.md` §3.
> Identities: source `dcedd7ef1`, clean tree over `core/crates`+`crates`
> (`working_tree_dirty false`), `release`, Rust `+1.85.1`, `--locked`, one sample
> per case per arm, one worker.

## 1. Candidate families and their verdicts

| Family | Verdict | Receipt |
| --- | --- | --- |
| `component.primitives` (core vs v0.1.6 reference) | **matched — collected** | §2 below |
| `pipeline.filesystem` (complete filesystem operation) | **`NOT_RUN`** — reference has no public entry point | §3 |
| `pipeline.c2` (C2-only save path) | **`NOT_RUN`** — surfaces do not match honestly | §4 |

## 2. `component.primitives` — the matched family (collected)

Driver: `tools/stage5_component_comparison.py`, **used unmodified**, the same
driver the governing `eb42c1347` collection used. It starts one process per arm
per case, applies the `release` profile to both, samples once, and refuses to
print a ratio unless both arms agree on all six pinned identities.

| Field | Value |
| --- | --- |
| Reference arm | `crates/layerfs-content/examples/stage5_component_reference.rs`, built from the **root** `Cargo.toml` |
| Candidate arm | `core/crates/layerfs-content/examples/filesystem_primitives_candidate.rs`, built from **`core/Cargo.toml`** |
| Separate dependency graphs | yes — neither arm calls the other and no private implementation is transplanted |
| Cache state (both arms, equal) | *warm in-process fixture; the base tree is built in the arm process before the timed region* |
| Worker count | one (one process per arm per case; the driver adds none) |
| Samples | one per case per arm |
| Timed region | sorted directory merge, sorted inline-inode merge, root encoding |
| Excluded on **both** arms | input normalization; deriving final reference counts; full topology validation; reference ordering preparation; physical persistence |
| Identity gate | six keys: `base_directory`, `base_table`, `base_root`, `updated_directory`, `table`, `root` |
| Budget gate | each arm's complete command inside the ordinary ≤ 15 s budget |

### Rows

| case | files | changes | identity | reference ns | candidate ns | candidate / reference | reference wall s | candidate wall s |
| --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| `small` | 200 | 20 | **MATCH** | 366,708 | 333,209 | **0.908649** | 0.606 | 0.618 |
| `wide` | 2,000 | 200 | **MATCH** | 2,551,750 | 838,458 | **0.328582** | 0.657 | 0.604 |
| `large-few-changes` | 20,000 | 20 | **MATCH** | 3,945,750 | 2,049,292 | **0.519367** | 0.647 | 0.613 |

An identity `MATCH` means both arms agreed on all six pinned identities; a
mismatch records `MISMATCH` and prints no ratio. None occurred.

### Limits of these rows (they are component rows, not a qualification)

- They are a **component** comparison of three primitive steps — sorted directory
  merge, sorted inline-inode merge and root encoding — **not** a complete-operation
  comparison, **not** a cold-cache claim and **not** a storage claim.
- The cache state is a **warm in-process fixture**; the base tree is built inside
  the arm process before the timer. No storage read is measured.
- Every wall time includes the Cargo launch/build check and process startup.
- No ratio is computed across cases and no aggregate claim is made.

### Diagnostic cross-tree comparison (explicitly **not** a gate)

Cross-tree ratios are not stable absolutes. Reported as a **diagnostic** only:

| case | `eb42c1347` (governing) | this tree `dcedd7ef1` | Δ |
| --- | ---: | ---: | --- |
| `small` | 0.652792 | 0.908649 | +0.256 |
| `wide` | 0.272295 | 0.328582 | +0.056 |
| `large-few-changes` | 0.480271 | 0.519367 | +0.039 |

The `small` case moved most. A second diagnostic run of the same driver (X3)
gave `small` 0.883379, `wide` 0.382005, `large-few-changes` 0.411692 at the *same*
source commit — so the same-binary, same-tree spread on `small` alone
(0.908649 → 0.883379) is comparable to the cross-tree delta. **The cross-tree
comparison is therefore not evidence of a regression**, and the earlier
collections are retained unedited rather than used as a baseline.

## 3. `pipeline.filesystem` — `NOT_RUN`, re-verified at this tree

**Claim to re-verify:** the reference has no public entry point that consumes final
sorted bindings plus typed values and returns a filesystem root; its filesystem
update is private to `layerfs-workspace`.

**Re-verified at `dcedd7ef1`:**

| Evidence | Citation |
| --- | --- |
| `changes` is a **private** module of the reference workspace | `crates/layerfs-workspace/src/lib.rs:4` — `mod changes;` (no `pub`) |
| the input aggregate is a private struct | `crates/layerfs-workspace/src/changes.rs:594` — `struct CandidateInputs<'a>` |
| the operation that builds a candidate root is private | `crates/layerfs-workspace/src/changes.rs:637` — `fn build(` (no `pub`), reached only through `pub(crate) fn build_candidate` (`:331`) and `pub(crate) fn build_remote_candidate` (`:368`) |
| it requires far more than bindings: a Store, a workspace id, a snapshot reader and a spool path | `crates/layerfs-workspace/src/changes.rs:594-601` — `FrozenWorkspaceChanges`, `&LayerStackStore`, `workspace_id: [u8; 16]`, `SnapshotReader` |

**Why this is `NOT_RUN` and not a workaround.** Comparing a native preordered C1
operation against the old whole-Workspace pipeline would measure different work,
and constructing that runtime only to time a primitive is out of scope. Building a
comparator would be a harness change, which the contract forbids.

This is the same reason VF-6 was deferred to Stage 6
(`../stage-5-verification-addendum-20260917.md` §6, owner disposition 2026-09-17).
**The baseline states it on its own evidence rather than inheriting it.**

## 4. `pipeline.c2` — `NOT_RUN`, re-verified at this tree

**The investigation the tasking asked for, and its result.**

| Side | Public surface | What it can honestly do |
| --- | --- | --- |
| **core** | `Store::begin_save` → `SaveOperation::accept` → `finish` (`core/crates/layerfs-storage/src/cas/store.rs:185`, `:291`, `:314`) | accepts already-finalized canonical objects, one `accept` per object, and returns a `SaveOutcome` on acknowledgement |
| **reference** | `LayerStackStore::workspace_admission(workspace_id)` returns a `WorkspaceAdmission` (`crates/layerfs-layerstack-store/src/objects.rs:4459`) | **nothing usable**: see below |

**The reference's admission type is public but inert from outside.**

| Evidence | Citation |
| --- | --- |
| `WorkspaceAdmission` is re-exported publicly | `crates/layerfs-layerstack-store/src/lib.rs:19-24` |
| but it has **no public method at all** | `crates/layerfs-layerstack-store/src/objects.rs:4546` — `impl WorkspaceAdmission` declares only `pub(crate) fn admit_remaining` (`:4547`) and `fn admit_remaining_inner` (`:555`); there is no other `fn` in the block |
| its only input type's constructor is public, but the admission call is not | `admit_remaining(self, objects: DeferredObjectStore)` — `pub(crate)`; `DeferredObjectStore::new()` is `pub` (`objects.rs:2647`) yet there is no public route from it into an admission |
| the page-level admission entry point is also crate-private | `CheckedOutputAdmission::admit_page` — `pub(crate)` (`objects.rs:4049`) |
| the private `build` route confirms the intended consumer is the Workspace | `crates/layerfs-workspace/src/changes.rs:139` — `pub(crate) admission: Option<layerfs_layerstack_store::WorkspaceAdmission>` |

**Why the surfaces do not match.** A matched pair would need both sides to accept
*N supplied canonical objects, one save, one finish* through a public entry point.
Core does exactly that. The reference's nearest public entry point,
`WorkspaceAdmission`, can be **obtained** but never **driven**: every method that
accepts objects is `pub(crate)`, and the only public way to reach object admission
in the reference is through `LayerStackStore::construct_workspace_files`
(`objects.rs:4473`), which is a full construction pipeline keyed to a
`workspace_id` with `initialize`/`step`/`finish` callbacks — **not** "N supplied
canonical objects". Timing that against core's `begin_save`/`accept`/`finish`
would compare a construction pipeline against an admission endpoint.

Therefore: **`NOT_RUN` — the reference exposes no public entry point that admits a
caller-supplied set of already-finalized canonical objects.** Collecting one arm
only, or building a comparator runtime, would violate the contract's "both arms
from their own workspaces, neither calling the other" and "no source changes".

**Consequence for P2-2 (O2, multi-row INSERT).** This row is the vehicle the
optimization study names for the single-row-vs-multi-row A/B. It cannot be a
*match* against the reference on public surfaces; it can still be measured as a
core-only before/after on core's own `c2.ceiling` workload from P0-3 (§5 below).

## 5. What this baseline does and does not establish

- **Does:** on the three component primitives, core is faster than the matched
  reference on all three, identity `MATCH`, one sample per case per arm, warm
  in-process fixture declared and equal in both arms.
- **Does not:** make any complete-operation, storage, cold-cache, release-admission
  or cross-host claim; and it does not close the two `NOT_RUN` rows, which stay
  open with reasons that were re-established on this tree.
- The core-only `c2.ceiling` workload collected for P0-3/P0-2 is the vehicle any
  later C2 claim must be measured against; it has **no reference arm** and must
  never be presented as a matched ratio.
