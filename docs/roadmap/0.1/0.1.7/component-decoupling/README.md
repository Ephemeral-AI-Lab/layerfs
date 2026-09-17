# Component decoupling: cluster design discussions

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Design issue: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Release: [v0.1.7 checklist](../README.md), tracked by
[#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155).

## Start here

- [**Stage 5 closure (2026-09-18)**](stage-5-report.md#16-final-matrices-and-the-verification-pass-round-4-2026-09-18):
  the terminal handoff reached its terminal condition - 81 PASS / 0 FAIL /
  0 PARTIAL-INCOMPLETE / 1 NOT_RUN with a written owner disposition / 1
  NOT_APPLICABLE of 83 for Stage 5, 34 PASS / 2 owner-WAIVED of 36 cumulative,
  every remediated row verified by a read-only verification subagent and every
  finding adjudicated. Evidence:
  [`../evidence/stage-5-terminal-20260918T120000Z/`](../evidence/stage-5-terminal-20260918T120000Z/).
- [Stage 5 terminal handoff](stage-5-terminal-handoff-20260917.md): the routing
  that drove the closure; now a historical record of the loop, the ledger and
  the terminal checklist.
- [Round-2 independent acceptance review](stages-1-5-review-20260917T230700Z.md):
  Stage 5 **not accepted** (64 PASS / 9 FAIL / 8 PARTIAL of 83) and the cumulative
  core feature-complete but unqualified - the verdict the terminal rounds
  remediated. Its evidence is
  [`../evidence/stages-1-5-review-20260917T230700Z/`](../evidence/stages-1-5-review-20260917T230700Z/).
- [Round-1 review and remediation](stages-1-5-review-20260917T160000Z.md) with
  [`stage-5-remediation-handoff-20260917.md`](stage-5-remediation-handoff-20260917.md):
  the findings the current tree has already closed.
- [Stage 5 continuation](stage-5-continuation-handoff.md): the earlier continuation
  routing; historical, superseded by the terminal handoff above.
- [Stage 5 blocker investigation](stage-5-blocker-investigation-20260917.md):
  public-API diagnostic evidence, reachable reference APIs and recovery rationale.
- [Stage 5 and complete Stages 1–5 reviewer handoff](stages-1-5-reviewer-handoff.md):
  independent stage and cumulative verdicts, actual structure/LOC, all criteria,
  simplification, performance/memory evidence, capacity limits and exact future
  FUSE/host/remote/container adapter boundaries. This is a prompt, not acceptance.
- [Stage 5 handoff](stage-5-handoff.md): complete filesystem trees, scoped inodes,
  portable/generic attributes and reference ordering, with canonical/topology/resource proof gates,
  real storage/timers, external tests and source-backed optimization qualification.
- [Stage 5 exact file/LOC plan](stage-5-file-plan.md): 39 new focused filesystem
  files, existing C1/C2 integration paths, per-file/directory ranges and actual-size
  reporting. No new crate or generic scratch framework.
- [Stage 5 completion report](stage-5-report.md) and its
  [partial verification contract](stage-5-verification.md): the implemented C1/C2
  filesystem scope, the inode-leaf compatibility gate and its evidence, and the
  criteria that remain open. Performance gates are not yet frozen; the current
  continuation requires prospective correction before qualification.
- [Stages 3–4 closure record](stages-3-4-completion-round-20260917.md): completed
  stage scopes and explicit unmeasured owner-waived rows; no inherited speed claim.
- [Prior Stages 3–4 continuation prompts](stages-3-4-continuation-prompt.md):
  D oracle/tree implementation and independent pooling coverage/qualification,
  then combined acceptance. These describe earlier execution; the closure record
  supersedes their unfinished-work status.
- [D implementation prompt](stages-3-4-continue-d.md): fix compared edit coordinates,
  then complete stored-node split/concat, exact-root/finality and localization proof.
- [Pooling coverage prompt](stages-3-4-continue-pooling.md): real index boundary,
  persisted replay, actual chain depth and independent resource/performance evidence.
- [Final acceptance prompt](stages-3-4-continue-acceptance.md): combine completed paths,
  qualify the final source and report criteria, LOC, simplifications, metrics and limits.
- [Implementation issues](implementation-issues.md): parent #165 and seven native
  sub-issues #166–#172, with stage mapping and dependencies.
- [Stages 0–2 handoff](stages-0-2-handoff.md): executable agent assignment for
  #166/#167, exact source/test file map, per-file/directory LOC estimates, required
  commands and independent timing acceptance. Full text is also embedded in #167.
- [Stages 0–2 implementation report](stages-0-2-report.md): what C1/C2 actually
  shipped for #166/#167 — frozen profile and capacities, the real file tree with
  recommended-versus-actual LOC per file and directory, the run commands and
  observed roots, the ownership/error evidence and every declared gap.
- [Stages 1–2 reviewer handoff](stages-1-2-reviewer-handoff.md): independent review
  of actual structure/LOC, every acceptance criterion, further simplifications,
  timing/efficiency evidence, bounded memory and memory safety.
- [Stages 3–4 handoff](stages-3-4-handoff.md): complete #168/#169 together through
  physical encoding, localized edits, transitions and independently timed proof;
  includes verification cases and the qualified v0.1.6 optimization comparison.
- [Stages 3–4 file plan](stages-3-4-file-plan.md): exact starting source/test paths,
  current versus recommended final LOC per file and directory, and honest deviation
  reporting. Existing C1/C2 packages are extended; no new crate is planned.
- [Stages 3–4 reviewer handoff](stages-3-4-reviewer-handoff.md): independent criteria,
  structure/LOC, simplification, performance/memory evidence and explicit limits
  audit for revisions, files, directory dimensions, workspace and underlying storage.
- [Stages 3–4 completion handoff](stages-3-4-completion-handoff.md): focused recovery
  assignments for missing physical pooling, stored-node split/concat and finality,
  followed by the required comparison and memory evidence.
- [Implementation plan and full review](implementation-plan.md): proposed folder
  structure, 999/200-line rules, before/after component diagrams, bidirectional I/O,
  memory/disk ownership, FUSE/process/cloud integration and ordered implementation
  slices. Records no retry/fallback, no WAL/durability work and no third-party patches.
- [Canonical objects](canonical-objects.md): component 1's identity, framing,
  checked decoding/construction and minimal read/output contract; source-backed
  reuse and simplification decisions, with no separate validation component.
- [File content](file-content.md): complete and known-edit inputs for empty,
  small and large files; current-result coordinates, replayable replacement ranges,
  immutable COW finalization, transitions, delta cooperation and concrete cuts.
- [Filesystem tree and metadata](filesystem-tree.md): native checked logical
  inputs, sorted directory/inline-inode COW, attributes, bounded reference ordering
  and source-backed cuts; independent of any future Workspace shape.
- [Finalized-object handoff](finalized-object-handoff.md): shared C1/C2 output
  fields, finality, ownership/release, backpressure and distinct construction/storage
  completion; before/after simplifications and required qualification.
- [Physical encoding and packing](physical-encoding-and-packing.md): C2 terms,
  payload candidates/selection, compression, physical inode-value pooling, bounded
  index replacement and placement before assembly; concrete cuts and no-retry rule.
- [Object save and SQLite persistence](admission-and-persistence.md): one save
  owner, bounded transactions, four tables, required indexes, read visibility,
  indexed failed-save cleanup and single-attempt local/remote completion.
- [Generic content I/O contract](content-io.md): finalized-object streaming,
  whole-operation bounds across many files/directories, demand-driven reads and
  local/host/daemon/remote-SQL placement. No generic payload spill/scratch layer;
  multi-edit finality and namespace ordering still need proof.
  Its [measurement contract](content-io.md#7-measurement-and-completion) requires
  standalone C1, standalone C2 and integrated timing from the first real slices,
  with explicit scopes and root-cause diagnostics.
- [Content I/O and memory audit against v0.1.6](content-io-memory-audit.md): pinned
  source comparison, core optimization targets and allocation/lifetime ledger;
  distinguishes source bounds from unproven SQL/process residency claims.
- [Integrated content-storage design](content-storage-design.md): complete
  cluster 1/2 data flow, edit and size-transition algorithms, repeated chunk
  deltas, compression/packing, database cleanup, Git comparison, timer wiring
  and required performance qualification, reviewed by three subagents.
- [Shared proposal](proposal.md): lightweight decoupling, explicit ownership,
  independent component measurement, compatibility and migration requirements.
- [Proposed repository layout and migration](repository-layout.md): product under
  core/, future application adapters alongside it, and an independent candidate
  workspace while the existing crates remain a temporary reference. Only
  layerfs-telemetry is implemented; the first handoff selects layerfs-content and
  layerfs-storage. Later runtime/application package boundaries remain open.
- [Clusters 1 and 2 co-design review](content-storage-co-design.md): extraction
  obstacles, independently measurable operations, destructive simplification
  candidates, measurement ownership and proof gates.
- [Cluster 1/2 overall architecture](cluster-1-2-components.md): the agreed
  seven-component overview, ASCII diagrams and shared timing/placement model;
  detailed contracts, measurements and crate boundaries remain open for discussion.
- [Content policy and database tables](content-storage-policy-and-tables.md):
  logical WHOLE_FILE/CHUNK roles, physical FULL/DELTA encoding, shared policy,
  queryable object descriptors and base-reference integrity.
- [Diff/conflict deferral](diff-conflict-deferral.md): v0.1.7 omissions, retained
  correctness mechanisms and the v0.2.0 work tracked in
  [#164](https://github.com/Ephemeral-AI-Lab/layerfs/issues/164).
- [Current cluster 1/2 source audit](cluster-1-2-source-audit.md): three detailed
  reviews, ASCII component/operation diagrams, algorithm/resource contracts,
  concrete removal candidates, no-fallback rules, timer wiring and coverage limits.
- [Time-only telemetry implementation specification](telemetry.md): injected
  parent/child scopes, environment-independent timing trees, optional subtrees
  returned with ordinary responses, caller-owned JSON output, worked cases,
  crate layout and acceptance; the standalone timer was implemented for
  [#161](https://github.com/Ephemeral-AI-Lab/layerfs/issues/161) at
  `core/crates/layerfs-telemetry/`.
  Actual transport integration requires compatibility and overhead qualification;
  broader monitoring, detached tracing and collectors stay separate.
- [Architecture overview](../architecture-overview.md): the source-linked
  starting inventory for the **reference** tree under `crates/`; recheck source
  when deciding a boundary. It does not describe the replacement under `core/`.
- [Replacement-core architecture](../../../../../core/docs/architecture/):
  source-backed description of the `core/` packages (C1 `layerfs-content`,
  C2 `layerfs-storage`, telemetry) — boundaries, algorithms, on-disk formats and
  declared limits. It lives with the product it describes; see
  [`core/docs/architecture/`](../../../../../core/docs/architecture/README.md) —
  ten papers covering the boundary, objects, files, filesystem, storage, limits,
  importing, representations, delta hints and counters.
- [Parallelism and batching study (2026-09-18)](parallelism-and-batching-study-20260918.md):
  what the reference does with worker pools and SQLite tuning, what `core/` does
  instead, and which differences are real. Source reads only, no measurement, no
  recommendation; it exists to tell [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
  what to measure first and what not to assume.


The proposal has been saved here for discussion before implementation. Owner
direction: group related components into clusters, design the boundaries
between clusters, and decide which internal relationships should remain coupled
or become independently accessible. Each cluster gets its own design document
as that discussion develops.

## Settled boundary for clusters 1 and 2

```text
History / workflow: LayerStack, Branch, logical Commit
  owns expected-head rules, history records, publication and completion
                    |
             coordinates calls
               /          \
              v            v
    CLUSTER 1                CLUSTER 2
    canonical content        physical object storage
    file / namespace COW     encoding / compression / packing / SQLite
```

LayerStack, Branch and logical Commit are outside both core clusters. The
reference crate name layerfs-layerstack-store does not define the replacement
boundary. Cluster 2 owns object persistence and SQL transaction mechanics;
history/workflow owns its history tables and mutations. Sharing one SQLite
database or an existing atomic transaction does not merge those responsibilities.

Cluster 1 must construct content from explicit inputs. Cluster 2 must save/read
and authenticate objects without creating a LayerStack, Branch, logical Commit,
Workspace or mount. The agreed default policy is configurable 128 KiB/8/4;
the task is configurable transparency, not finding optimal numbers. Successful
published versions are never rolled back. Diff/conflict/resolution stays deferred.
An operation gets one attempt: conditions requiring retry return failure. This
includes SQLite busy handlers and SDK retries. Unknown write outcomes never
authorize replay or deletion; see the [single-attempt contract](physical-encoding-and-packing.md#one-attempt-no-retries).
The current implementation target uses embedded SQLite with MEMORY journaling and
synchronous OFF; no WAL or added crash-durability subsystem. Runtime transaction
atomicity remains required. Provider choices must meet this policy, not silently
relax it. Required unsupported capabilities fail explicitly.

The future Workspace may have a completely different shape or be absent. C1/C2
have native callable contracts with stable logical inputs, authorized identities,
explicit resources and object read/output capabilities. Old Workspace code supplies
behavior/cost evidence, not a required command planner, lifecycle or backing model.
Each cluster must be independently extractable and measurable, then composable
with local, remote, container or later callers through the same contracts.

The I/O direction is direct finalized output into bounded storage admission.
Limits apply across the whole workload, not independently to unlimited files.
Small files share batches/packs/transactions; slow consumers apply backpressure.
No generic payload spill/scratch service is planned. The filesystem proposal
retains narrow compact reference ordering; C2's existing physical value index also
uses temporary backing. Its chosen replacement is a bounded Store-owned ordered
set with identical reuse semantics, pending resource/performance proof. No blanket
zero-temporary-storage claim is made.

## Remaining co-design discussions

The [ordered decision checklist](content-storage-co-design.md#remaining-co-design-decisions)
tracks implementation decisions and proof gates. All seven individual-component
proposals are now recorded; use this dependency order for qualification:

1. Finalized-object handoff: five proposed answers are recorded; finish exact
   APIs, reference ownership, producer/singleton capacities and implementation proofs.
2. File content: proposed inputs and cuts are documented; prove decoded-boundary
   finality/memory, exact-root equivalence and supported-caller compatibility.
3. Filesystem tree and metadata: the sorted/direct-value design and compact
   ordering decision are documented; qualify checked-input integration, record
   layout, format invariants and actual resource/performance behavior.
4. Physical encoding + packing: proposed decisions are recorded; qualify checked
   capacities/formats, batched candidate reads, bounded index replacement and
   compatible append with placement before assembly. No runtime retry paths.
5. Admission + SQLite: proposal recorded for single-owner saves, four tables,
   bounded transactions, indexed cleanup and unknown outcomes. Qualify concrete
   schema/profile compatibility, receipt changes and selected backend behavior.
6. Measurement + first extraction: complete-file construction -> real save ->
   readback, then many-file bounds, module layout and reference-retirement gates.

The [file-content](file-content.md) and [filesystem-tree](filesystem-tree.md)
proposals now apply the handoff to all three C1 responsibilities. Their algorithm
and resource proofs remain open. Both C2 detailed proposals are now recorded.
The fuller [consistency/placement review and implementation sequence](implementation-plan.md)
are now recorded. Implement complete-file construction -> standalone storage ->
authenticated readback first. Exact Rust APIs,
schema/profile identifiers and dependency layout remain implementation decisions,
not additional component-design phases.
Public LayerStack/Branch/Commit semantics are a later
history/workflow discussion, not additional cluster 2 components.

Reader/output/memory ownership is folded into the object contract, not a separate
component or design workstream. The current implementation scope is logical
content plus database storage. Existing Workspace/transport observations are
reference context; a later environment may supply an entirely different source
and lifecycle. Core speed claims cannot include deferred Workspace RPC cuts.

## Two levels of design

**Between clusters:** define a small explicit contract: inputs/outputs, allowed
dependencies, resource ownership, lifetime, failures, cancellation and completion.
One cluster must not reach through that contract into another's mutable state or
storage/runtime internals. Cross-cluster workflows keep their ordering visible.

**Within a cluster:** keep related algorithms and structures close when they
share an invariant, ownership or a useful optimization. Separate an internal
boundary when it enables an actual independent change, measurement or test.
Use ordinary functions and existing types/traits; independent measurement does
not require a separate crate or runtime plugin for each component.

A cluster should have a coherent purpose, ownership story and measurable
operation. Start from these properties rather than current crate names or the
host/daemon process split. Host versus daemon is a deployment view to document
for each cluster and its transport interactions.

## Cluster map and later discussions

Clusters 1 and 2 have agreed responsibility groups and source-backed design
reviews; their remaining contracts are listed above. The surrounding groups are
still candidates for later discussion, not fixed crates or implementation plans.
Other filenames below remain planned document names.

| Candidate cluster | Components and responsibilities to examine | Document / planned filename |
| --- | --- | --- |
| Cluster 1: canonical content and CAS contract | Canonical bytes/IDs, logical object contract, namespace and file representations, CDC, persistent structural COW, ordinary reads and local equality/validation | [Cluster 1 review](canonical-content.md) |
| Cluster 2: physical object storage | Exact reuse, authentication, delta/compression, packs, locators, SQLite object reads/writes and physical admission; excludes LayerStack/Branch/logical Commit | [Cluster 2 review](object-storage.md) |
| Workspace state and snapshots | Live COW, private edits, backing, dirty tracking, frozen generations, checkpoint/completion state and cleanup | `workspace-state.md` |
| History and Commit coordination | LayerStack/Branch/Commit operations, orchestration of candidate construction, conditional publication, explicit failure and outcome inspection | `history-commit.md` |
| Filesystem and execution runtime | FUSE projection, mounts/handles, daemon/control transport, process execution, cancellation and host/sandbox bindings | `filesystem-runtime.md` |
| Public interfaces and observation | SDK/CLI compatibility, typed requests/results, operation receipts, diagnostics and resource accounting | `interfaces-observation.md` |

The candidate-construction boundary is an early cross-cluster discussion: it
consumes neutral stable input from the caller, runs canonical algorithms, and emits objects
to a selected destination; orchestration and storage effects have distinct
owners. Assign its implementation home after tracing those responsibilities.
Likewise, history owns publication semantics while storage owns the SQL
transaction mechanism. Separating these responsibilities must preserve required
atomicity.

Shared contracts have one definition and one documented owner; consuming
documents link to it. Avoid a catch-all shared module that allows every cluster
to access everything else. Public interfaces and observation may remain thin
adapters over other clusters rather than gaining their own runtime framework.

FUSE is the v0.1.7 projection focus. APFS-specific projection and clonefile
acceleration are outside scope. Existing public routes retain the compatibility
requirements in the shared proposal and release checklist.

## Contents of each cluster document

Use the following outline when a cluster is discussed; keep it proportional to
the actual decisions.

1. **Purpose and component inventory:** current source locations, callers,
   proposed owner, and responsibilities outside the cluster.
2. **External contract:** inputs/outputs, required dependencies, permitted calls,
   lifetime/error/cleanup rules, and host/daemon placement where relevant.
3. **Internal structure:** current and proposed dependency diagrams; coupling to
   keep, coupling to remove, and the reason for each boundary.
4. **State and resources:** mutable owners, read views, caches/backing, bounds,
   concurrency, cancellation, and any transaction/publication obligations.
5. **Independent measurements:** production entry points for each selected
   component and the cluster operation; fixtures/providers; timer/cache scopes;
   bounded outputs; correctness oracles; included and excluded dependency work.
6. **Integration and migration:** affected public paths, compatibility,
   implementation slices, existing proofs to reuse, missing checks, and cleanup
   of failed unpublished attempts; no rollback of successful versions.
7. **Decisions and open questions:** clearly distinguish proposals from accepted
   choices, with links to related clusters and the release checklist.

Component and whole-cluster diagnostics use the same production functions as
the integrated pipeline. A cluster benchmark may include internal interactions
while substituting only declared external dependencies. Keep integrated phase
attribution and public end-to-end qualification as complementary evidence.
Follow the shared proposal's measurement rules; no diagram or document grants a
new benchmark exception, changed transaction boundary or performance claim.

## Discussion order

Finish the [remaining cluster 1/2 contracts](content-storage-co-design.md#remaining-co-design-decisions)
first. Apply the handoff proposal to file construction and filesystem-tree ordering
while closing its exact APIs and resource proofs. Workspace state and
LayerStack/Branch/Commit workflows follow, then runtime and
public integration. Only the integration boundary with these later owners is
part of the present content-storage co-design.
This is a discussion order; implementation sequencing follows the resulting
dependency and compatibility analysis.

Create and link each cluster document when its discussion begins. The shared
proposal remains the home for common principles; this index tracks the grouping
and links; each cluster document owns its detailed decisions.
