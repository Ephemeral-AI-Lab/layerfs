# LayerFS roadmap

> **Status:** Living roadmap. This checklist is planning material, not part of
> the LayerFS 0.1.0 product or compatibility contract.

This file answers two questions: what is implemented in the current source,
and what should happen next. See [Roadmap planning](roadmap-planning.md) for
sequencing, architecture, acceptance gates, and rationale.

## Legend

- [x] Implemented and backed by current source or retained evidence.
- [ ] Not complete. Design notes, experiments, or partial code do not count.
- A release is complete only after its source identity, verification, artifacts,
  and documentation are frozen together.

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

### Release freeze still required

- [ ] Select the final immutable 0.1.0 Git commit.
- [ ] Run every release verification gate from that exact clean source.
- [ ] Record the final Rust toolchain, host, start time, finish time, and
  overall result.
- [ ] Produce source archives and the final checksum manifest.
- [ ] Record CLI, daemon, FUSE helper, and runtime-image artifact identities.
- [ ] Fill every `TO BE FILLED AT RELEASE` field.
- [ ] Verify the final source seal against the retained benchmark evidence or
  rerun the benchmark when the dependency closure changes.
- [ ] Create and publish the `v0.1.0` tag and release record.

## Next: compatibility-preserving 0.1.1 work

The 0.1.1 line must preserve the 0.1.0 schema, identities, canonical bytes,
CDC profile, public SDK behavior, CLI grammar, daemon protocol, and resource
bounds.

### Workspace, FUSE, and Docker

- [ ] Freeze a documented FUSE syscall and error-semantics contract.
- [ ] Run one conformance matrix through both materialization and real FUSE.
- [ ] Prove canonical-root equality across supported projections.
- [ ] Cover create, append, overwrite, truncate, sparse write, rename,
  replace-on-rename, open-unlink, directory operations, symlink, hard link,
  permissions, timestamps, and multiple descriptors.
- [ ] Cover mount failure, daemon disconnect, cancellation, forced cleanup,
  and container failure.
- [ ] Prove no leaked mount, container, process, output reader, or Workspace
  lease on every success and error path.
- [ ] Refactor Workspace lifecycle, projection, execution, and Docker code at
  their existing seams without changing public behavior.
- [ ] Keep each command execution in a fresh process; do not add a persistent
  shell or hidden worker pool.
- [ ] Keep the complete default test suite below two minutes on the reference
  development host.

### Capture and Commit performance

- [ ] Complete the
  [large and mixed-edit capture resilience](next/0.1.1/capture-large-mixed-edit-resilience.md)
  proof.
- [ ] Complete the
  [`copy_file_range` and prepend](next/0.1.1/copy-file-range-prepend.md)
  proof.
- [ ] Coalesce dirty byte ranges and metadata changes per inode.
- [ ] Bound rename, tombstone, hard-link, and dirty-inode planning for large
  repositories.
- [ ] Stop CDC reconstruction after a verified boundary-resynchronization
  point.
- [ ] Splice persistent extents without rewriting the unchanged suffix.
- [ ] Use ID-only object membership before loading canonical payload.
- [ ] Retain bounded memory, spill, transaction duration, and deterministic
  identities for every optimization.
- [ ] Preserve or improve every registered `fs-bench-pro` gate.

## Next minor release: portable projection foundation

Work that changes the public contract or frozen 0.1.0 compatibility surface
belongs in a minor release.

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
