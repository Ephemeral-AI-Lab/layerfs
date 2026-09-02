# LayerFS roadmap

> **Status:** Living roadmap. This checklist is planning material, not part of
> the LayerFS 0.1.0 product or compatibility contract.

This file answers two questions: what is implemented in the current source,
and what should happen next. See [Roadmap architecture](architecture.md) for
sequencing, architecture, acceptance gates, and rationale.

Active maintainer guidance is co-located with its release checklist. See the
[0.1.x development guide](0.1/development.md) and
[benchmark contract](0.1/benchmarking.md). Incompatible multi-agent and
projection work is tracked in the [0.2 roadmap](0.2/README.md).

## Legend

- [x] Implemented and backed by current source or retained evidence.
- [ ] Not complete. Design notes, experiments, or partial code do not count.
- A release is complete only after its source identity, verification, artifacts,
  and documentation are frozen together.

## 0.1.x phase: benchmark completion and optimization

The [0.1.x roadmap](0.1/README.md) completes the benchmark surface and performs
evidence-driven optimization against the existing host-Store, Docker,
authenticated-daemon, real-FUSE, fresh-process, explicit-Commit, and
explicit-End architecture.

- [x] Keep the public SDK and real-FUSE lifecycle as the product boundary.
- [x] Keep container and fixture preparation outside timed regions.
- [x] Keep LayerFS-only iteration separate from final paired Cloudflare runs.
- [ ] Complete the namespace matrix in v0.1.1.
- [ ] Complete admitted prepend, mixed-edit, and Store-footprint work in
  v0.1.2.
- [ ] Complete single-history filesystem-workload coverage in v0.1.3.
- [ ] Complete multi-Layer/multi-Branch history coverage in v0.1.4.
- [ ] Carry the append-only registered matrix through 1.0.0.

New projections, platforms, remote topology, or incompatible contracts do not
belong in this phase.

## Current baseline: LayerFS 0.1.0

### Implemented product surface

- [x] One durable local `LayerStackStore` backed by one SQLite database.
- [x] Ephemeral Workspaces with no Workspace database.
- [x] One SDK `Client` binding one Store, one Monitor, and one Workspace
  manager.
- [x] Named LayerStacks and Branches with typed immutable identities.
- [x] Immutable Layers and Commits in one linear LayerStack history.
- [x] Branch Fork from a Layer or a selected Commit with zero canonical-object
  copies.
- [x] Workspace Create, fresh-process Exec, bounded output, explicit Commit,
  and explicit End or discard.
- [x] Add of a Branch head Commit as the next Layer with zero
  canonical-object copies and head compare-and-swap.
- [x] Supported Layer and Branch-history Diff operations with paged output.
- [x] Typed stale-Workspace reconciliation and conflict choices.
- [x] Content-addressed canonical objects with authenticated reads.
- [x] Content-defined chunking for file-region reuse.
- [x] Persistent extent/tree copy-on-write for structural reuse.
- [x] Bounded candidate memory, spill, object pages, and SQLite transaction
  payloads.
- [x] Host-directory materialization and capture.
- [x] Real Linux FUSE projection through a managed container.
- [x] Docker container create, start, connect, inspect, stop, and remove.
- [x] Capability-authenticated host/container daemon protocol.
- [x] Passive operation Monitor, phase timings, database snapshots, and
  explicit deduplication analysis.
- [x] Public Rust SDK and CLI operation families.
- [x] Store and Branch integrity evaluator.
- [x] End-to-end `fs-bench-pro` campaign through public SDK operations, real
  FUSE, fresh workload processes, and retained raw evidence.
- [x] Versioned 0.1.0 manual, limitations, release contract, benchmark report,
  and artifact/verification templates.

### Released baseline

- [x] Freeze the final 0.1.0 source at
  `243f98a3bf287d6a9f8168891452b7355d45529c`.
- [x] Publish the annotated `v0.1.0` tag and release record.
- [x] Publish source archives, `Cargo.lock`, `LICENSE`, and `SHA256SUMS`.
- [x] Bind verification and benchmark evidence to the released source.
- [x] Record executables, helpers, and runtime images as not published at
  0.1.0.

## Next: compatibility-preserving 0.1.1 work

The 0.1.1 line must preserve the 0.1.0 schema, identities, canonical bytes,
CDC profile, public SDK behavior, CLI grammar, daemon protocol, and resource
bounds. The detailed and authoritative working list is the
[0.1.1 checklist](0.1/0.1.1/README.md).

- [x] Define the compatibility boundary and one existing-directory lifecycle.
- [x] Record the initial dated planning baseline.
- [ ] Complete the controlled namespace-size admission measurement.
- [ ] Decide whether initialization, localized Commit, both, or neither are
  admitted defects.
- [ ] Add one failing check per admitted defect and fix the shared root cause.
- [ ] Prove every timed tier through real Linux FUSE and one untimed
  materialization/FUSE equality case.
- [ ] Prove one managed Docker lifecycle and attachment-failure cleanup case.
- [ ] Pass compatibility, correctness, resource, benchmark, and release gates.
- [ ] Create the immutable 0.1.1 manual and release record only after a source
  candidate passes.

Extent-aware `copy_file_range`, new wire operations, broad refactors, and new
projection types are deferred from 0.1.1. Patch-compatible follow-up work is
tracked in the [0.1.2 proposals](0.1/0.1.2/README.md); only incompatible work
moves to 0.2.0.

## Proposed compatibility-preserving 0.1.2 work

The [0.1.2 proposal set](0.1/0.1.2/README.md) continues benchmark-driven
optimization after the namespace lifecycle is understood. It owns three known
families: prepend/range-copy, online Workspace capture, and
[total durable Store-footprint efficiency](0.1/0.1.2/store-footprint-efficiency.md).

- [ ] Admit prepend/range-copy work only from public-path transfer evidence.
- [ ] Admit fragmented-write, sparse-growth, or mixed-edit work only from a
  focused failing row.
- [ ] Preserve the existing FUSE/Docker environment and 0.1.x contracts.
- [ ] Retain exact bytes, canonical roots, fresh reopen proof, and existing
  registered scenario meanings.
- [ ] Rerun LayerFS-only payload and namespace matrices after every accepted
  optimization.
- [ ] Run the paired Cloudflare payload campaign once at candidate stability.
- [ ] Measure the 500 MB unique-content control's logical, canonical, SQLite,
  other durable, temporary, and physical-I/O bytes.
- [ ] Reach at most 600 MB total durable Store footprint through a compatible
  mechanism, or retain the exact compatibility or physical lower bound.
- [ ] Count every pack, index, manifest, sidecar, journal, and checksum rather
  than treating a smaller `store.sqlite` as a total Store win.
- [ ] Move only incompatible mechanisms to 0.2.0.

## Draft compatibility-preserving 0.1.3 work

The [0.1.3 README](0.1/0.1.3/README.md) indexes one document per filesystem
workload family. The topology stays fixed at one LayerStack, one genesis Layer,
and one Branch so the workloads—not Layer or Branch fan-out—own the
measurement. New tiered rows use one final Commit; inherited frozen rows such
as `edit16` retain their historical Commit sequence.

- [ ] Freeze deterministic 1/10/100 load tiers, seed-bound schedules, exact
  oracles, and one shared result schema.
- [ ] Cover payload, same-count and count-changing edits, range-copy, namespace
  scale and mutation, file churn, directories, Git, links, and mixed workloads.
- [ ] Optimize only operations with a measured defect or material opportunity.
- [ ] Rerun every registered v0.1.0-v0.1.2 scenario before release.
- [ ] Leave repeated Commit history, Add, multi-Layer Diff, conflicts, and
  Branch fan-out to v0.1.4.

## Draft compatibility-preserving 0.1.4 work

The [0.1.4 README](0.1/0.1.4/README.md) adds multi-Layer, multi-Branch, and
history-depth evidence without changing the established product architecture.

- [ ] Measure bounded Commit-history depths and Branch fan-out profiles.
- [ ] Measure Fork from Layer and Commit, Add outcomes, Layer/Branch Diff,
  query pagination, conflicts, resolution, and head movement.
- [ ] Prove historical immutability, exact reopen, incremental storage growth,
  and bounded resources.
- [ ] Optimize only evidence-backed bottlenecks and rerun the full accumulated
  registry before release.

## 0.2.0: agent Branch integration and portable projections

Work that changes the public contract or frozen 0.1.0 compatibility surface
belongs in a minor release. The [0.2 roadmap](0.2/README.md) makes
[agent Branch reconciliation](0.2/agent-branch-reconciliation/README.md) a
release-defining main task alongside the portable projection foundation.

- [ ] Treat a Branch as a rapidly iterating node or pod shared by cooperating
  agents, while the LayerStack remains main.
- [ ] Permit multiple concurrent Workspaces from different Commits of one
  Branch.
- [ ] Reconcile stale Workspace Proposals automatically into one linear Branch
  history and reserve structured resolution for genuine conflicts.
- [ ] Track changed read dependencies separately from incompatible writes.
- [ ] Make conflict work resumable by another agent without retaining the
  original Workspace process.
- [ ] Validate the exact reconciled candidate before Branch publication.
- [ ] Keep pod-to-main LayerStack checkpoints explicit and less frequent than
  tool-call Commits.

- [ ] Define one projection conformance contract from the proven
  materialization and FUSE behavior.
- [ ] Separate Workspace state, projection, execution binding, and runtime
  lifecycle without speculative one-implementation interfaces.
- [ ] Stabilize typed CLI JSON results for external tools and a future TUI.
- [ ] Add capability-detected Linux reflink materialization with safe streamed
  fallback.
- [ ] Add capability-detected macOS `clonefile`/APFS acceleration with safe
  streamed fallback.
- [ ] Add an OverlayFS projection that feeds the same capture and Commit path.
- [ ] Prove whiteout, opaque-directory, metadata, rename, hard-link, symlink,
  sparse-file, and copy-up semantics.
- [ ] Prove that materialization, FUSE, and OverlayFS produce the same
  canonical root for the same logical result.

## Later: platform and runtime expansion

- [ ] Define a platform-neutral in-process VFS surface.
- [ ] Add a Windows WinFsp projection with explicit Windows/POSIX semantic
  mapping.
- [ ] Evaluate WASM execution through the in-process VFS rather than a kernel
  mount.
- [ ] Select a WASM persistence backend only after a concrete runtime consumer
  exists.
- [ ] Integrate Firecracker through Ephemeral Sandbox rather than placing
  microVM lifecycle code in the LayerFS core.
- [ ] Evaluate virtio-fs or another guest projection behind the same Workspace
  contract.

## Later: portable Store and synchronization

- [ ] Specify a verified, offline LayerFS bundle with selected facts, required
  objects, closure proof, schema identity, and checksums.
- [ ] Implement quiescent Store export and verified import.
- [ ] Prove missing-only admission and unchanged ObjectId/CommitId/LayerId
  identity across import.
- [ ] Design Store-to-Store synchronization around the one-Store model.
- [ ] Exchange bounded fact and ObjectId pages before canonical payload.
- [ ] Transfer only missing immutable objects.
- [ ] Verify object identity and complete root closure at the receiver.
- [ ] Publish destination heads last with compare-and-swap.
- [ ] Make transfer interruption-safe, resumable, and observable through
  receipts.
- [ ] Do not reintroduce BranchStore, Reference/Replica placement modes, or a
  second local database architecture.

## Ecosystem

### DeepSeek Harness plugin

- [ ] Define a DSH plugin using only the public LayerFS SDK.
- [ ] Map harness runs to Branch, Workspace, Execution, Commit/discard, and End.
- [ ] Guarantee cleanup on success, failure, timeout, and cancellation.
- [ ] Expose LayerFS typed IDs and operation receipts in harness results.
- [ ] Keep persistent shells, private SQL, internal object APIs, and direct
  daemon calls out of the plugin.

### Ephemeral Sandbox

- [ ] Define the Workspace-projection handoff between LayerFS and Ephemeral
  Sandbox.
- [ ] Keep Docker, Firecracker, networking, egress, resource enforcement, and
  VM lifecycle owned by Ephemeral Sandbox.
- [ ] Prove that Sandbox runtime choice does not change LayerFS content
  identity or Commit results.

### AgentsGit

- [ ] Define an explicit mapping from LayerFS Branch/Commit state to Git
  repository state.
- [ ] Keep Git diff, merge, rebase, review, and pull-request promotion owned by
  AgentsGit.
- [ ] Preserve the distinction between a LayerFS Commit and a Git Commit.
- [ ] Share correlation IDs without sharing mutable databases.

### TUI

- [ ] Keep the TUI outside the 0.1.x core and dependency graph.
- [ ] Start with a read-only Store, LayerStack, Branch, Commit, Workspace,
  execution, timing, and deduplication browser.
- [ ] Consume only the public SDK or stable typed CLI JSON.
- [ ] Add mutation only after lifecycle, cancellation, conflict, and container
  APIs are stable.

## Continuous benchmark checklist

- [ ] Treat the benchmark registry as append-only: once admitted, a scenario's
  fixture, operation sequence, timing boundary, oracle, and schema remain
  frozen through 1.0.0.
- [ ] Replace a defective benchmark with a new scenario ID or schema version;
  retain and explain the old row and evidence.
- [ ] Run every previously registered scenario for each later 0.1.x candidate.
- [ ] Keep public SDK or CLI operations as the timed product boundary.
- [ ] Keep fresh workload processes; do not introduce a persistent execution
  shell for benchmark speed.
- [ ] Keep environment and container preparation outside the timed region for
  every compared product.
- [ ] Record source identity, runtime identity, cache policy, acknowledgement
  boundary, distributions, and raw evidence.
- [ ] Measure end-to-end latency, phase latency, CPU time, maximum RSS, bytes
  read/written, Store growth, semantic bytes, and object reuse.
- [ ] Extend create workloads across file sizes and file-count distributions.
- [ ] Extend deterministic edits across overwrite, insert, delete, append,
  prepend, and mixed edit sizes.
- [ ] Add directory rename, subtree move, recursive delete, hard-link, symlink,
  permission, and metadata-only cases.
- [ ] Measure initial, warm, and incremental materialization.
- [ ] Run the same workloads across each supported projection and compare final
  canonical roots.
- [ ] Keep read throughput as a projection diagnostic when shell/bootstrap
  time dominates the user-visible row.
- [ ] Do not add a multi-agent performance benchmark to the public comparison.
- [ ] Never discard valid slow samples or rely on unfair caches, prewarmed
  Workspaces, or hidden implementation-only operations.

## Definition of done for every roadmap item

An item may be checked only when all applicable conditions hold:

- [ ] The public operation and ownership boundary are explicit.
- [ ] Correctness tests fail before the change and pass after it.
- [ ] Real FUSE or the relevant real adapter is exercised when applicable.
- [ ] CPU, memory, transaction, paging, and cleanup bounds are measured.
- [ ] Canonical identities and Store integrity are unchanged or deliberately
  versioned.
- [ ] The public SDK and CLI expose the behavior without private shortcuts.
- [ ] Focused tests, dependent tests, Clippy, formatting, and full release gates
  pass.
- [ ] Current-source benchmark evidence shows no unexplained regression.
- [ ] Documentation, limitations, and raw evidence are updated together.
