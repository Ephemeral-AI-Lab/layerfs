# Component decoupling: cluster design discussions

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Design issue: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Release: [v0.1.7 checklist](../README.md), tracked by
[#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155).

## Start here

- [Shared proposal](proposal.md): lightweight decoupling, explicit ownership,
  independent component measurement, compatibility and migration requirements.
- [Proposed repository layout and migration](repository-layout.md): product under
  core/, future application adapters alongside it, and an independent candidate
  workspace while the existing crates remain a temporary reference. Only
  layerfs-telemetry is agreed as a candidate crate; all other package boundaries
  remain open.
- [Clusters 1 and 2 co-design review](content-storage-co-design.md): extraction
  obstacles, independently measurable operations, destructive simplification
  candidates, measurement ownership and proof gates.
- [Time-only telemetry implementation specification](telemetry.md): injected
  parent/child scopes, environment-independent timing trees, optional subtrees
  returned with ordinary responses, caller-owned JSON output, worked cases,
  crate layout and acceptance; the standalone timer was implemented for
  [#161](https://github.com/Ephemeral-AI-Lab/layerfs/issues/161) at
  `core/crates/layerfs-telemetry/`.
  Actual transport integration requires compatibility and overhead qualification;
  broader monitoring, detached tracing and collectors stay separate.
- [Architecture overview](../architecture-overview.md): the source-linked
  starting inventory; recheck source when deciding a boundary.

The proposal has been saved here for discussion before implementation. Owner
direction: group related components into clusters, design the boundaries
between clusters, and decide which internal relationships should remain coupled
or become independently accessible. Each cluster gets its own design document
as that discussion develops.

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

## Candidate clusters to discuss

This is an initial grouping, not an accepted dependency graph or implementation
plan. Merge or split rows after tracing actual callers, ownership and workflows.
Clusters 1 and 2 now have source-backed review documents. Other filenames remain
planned document names.

| Candidate cluster | Components and responsibilities to examine | Document / planned filename |
| --- | --- | --- |
| Logical content and CAS contract | Canonical bytes/IDs, logical object contract, namespace and file representations, CDC, persistent structural COW, pure Diff/reconciliation algorithms | [Cluster 1 review](canonical-content.md) |
| Physical object storage | CAS implementation and exact reuse, authentication, delta/compression, packs, locators, SQLite reads/writes and physical admission | [Cluster 2 review](object-storage.md) |
| Workspace state and snapshots | Live COW, private edits, backing, dirty tracking, frozen generations, checkpoint/completion state and cleanup | `workspace-state.md` |
| History and Commit coordination | LayerStack/Branch/Commit operations, orchestration of candidate construction, reconciliation policy, conditional publication, retry and recovery | `history-commit.md` |
| Filesystem and execution runtime | FUSE projection, mounts/handles, daemon/control transport, process execution, cancellation and host/sandbox bindings | `filesystem-runtime.md` |
| Public interfaces and observation | SDK/CLI compatibility, typed requests/results, operation receipts, diagnostics and resource accounting | `interfaces-observation.md` |

The candidate-construction boundary is an early cross-cluster discussion: it
consumes frozen Workspace input, runs canonical algorithms, and emits objects
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
   implementation slices, existing proofs to reuse, missing checks, and rollback.
7. **Decisions and open questions:** clearly distinguish proposals from accepted
   choices, with links to related clusters and the release checklist.

Component and whole-cluster diagnostics use the same production functions as
the integrated pipeline. A cluster benchmark may include internal interactions
while substituting only declared external dependencies. Keep integrated phase
attribution and public end-to-end qualification as complementary evidence.
Follow the shared proposal's measurement rules; no diagram or document grants a
new benchmark exception, changed transaction boundary or performance claim.

## Discussion order

Begin with canonical content and object storage together, because the boundary
between candidate construction and Store admission currently prevents a clean
measurement of construction without Store writes. Then discuss Workspace input
ownership and history/publication, followed by runtime and public integration.
This is a discussion order; implementation sequencing follows the resulting
dependency and compatibility analysis.

Create and link each cluster document when its discussion begins. The shared
proposal remains the home for common principles; this index tracks the grouping
and links; each cluster document owns its detailed decisions.
