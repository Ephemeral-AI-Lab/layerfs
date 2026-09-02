# LayerFS 0.2 roadmap

> **Status:** Draft incompatible-minor-release roadmap. No public contract,
> Store migration, benchmark row, or release candidate is frozen yet.

## Problem statement

LayerFS 0.1 proves one local Store, immutable history, one writable Workspace
per Branch, explicit Commit, and compare-and-swap Layer publication. That is a
sound single-writer baseline, but it is not the intended agent topology.

LayerFS 0.2 treats:

```text
LayerStack = main, the globally integrated checkpoint history
Branch     = a rapidly iterating node or pod shared by cooperating agents
Workspace  = one isolated tool-call attempt
```

Many agents must be able to work from different snapshots of one Branch at the
same time. Their results need cheap automatic reconciliation, linear accepted
Branch history, structured conflict work that another agent may resume, and a
separate checkpoint boundary into the LayerStack. A routine stale head must not
become a Git-style manual pull/rebase loop.

At the same time, 0.2 remains the first release allowed to extend projection
and automation contracts beyond the frozen 0.1.x surface.

## Goal

Establish an agent-native Branch integration model and a portable Workspace
projection contract without changing canonical identity accidentally.

The target lifecycle is:

```text
many concurrent agent tool calls
  -> one private Workspace per call
  -> immutable candidate Proposal
  -> one ordered integration stream per Branch/pod
  -> automatic reconcile or structured resolution ticket
  -> validated linear Branch Commit history
  -> explicit pod checkpoint into the LayerStack/main history
```

`Proposal` and `resolution ticket` are working design terms, not frozen public
type names.

## Main tasks

### 1. Agent Branch reconciliation and conflict policy

The [agent Branch reconciliation task](agent-branch-reconciliation/README.md) is a
release-defining 0.2 task.

- [ ] Permit multiple concurrent Workspaces from one Branch.
- [ ] Replace the long-lived exclusive Branch lease with short, ordered Branch
  integration.
- [ ] Reconcile an older Workspace result against the latest Branch head
  without replaying every intervening Commit.
- [ ] Make stale-head compare-and-swap loss an internal retry when changes
  reconcile cleanly.
- [ ] Preserve one linear accepted Commit history per Branch.
- [ ] Automatically combine independent, identical, and otherwise commuting
  filesystem changes before producing a conflict.
- [ ] Distinguish write/write conflict from changed read dependencies that
  require revalidation.
- [ ] Represent genuine conflicts as bounded, structured, resumable work rather
  than conflict markers or process-local state.
- [ ] Let another authorized agent resolve a conflicted Proposal without
  requiring the originating Workspace process to survive.
- [ ] Validate the exact reconciled tree before it becomes the Branch head.
- [ ] Keep a conflicted Proposal from blocking unrelated Branch progress.
- [ ] Keep Branch-to-LayerStack reconciliation as a separate, explicit
  pod-to-main checkpoint.

### 2. Projection conformance and portable acceleration

- [ ] Define one conformance contract shared by materialization, FUSE, and new
  projections.
- [ ] Preserve visible filesystem results, dirty-frontier capture, inode and
  link behavior, canonical roots, Commit results, cleanup, and resource bounds.
- [ ] Add capability-detected Linux reflink acceleration with safe streamed
  fallback.
- [ ] Add capability-detected macOS `clonefile`/APFS acceleration with safe
  streamed fallback.
- [ ] Add an OverlayFS projection through the same capture and Commit path.
- [ ] Prove whiteout, opaque-directory, rename, metadata, hard-link, symlink,
  sparse-file, open-unlink, and copy-up behavior.

### 3. Typed automation surface

- [ ] Replace CLI debug-preview results with stable typed JSON outcomes.
- [ ] Expose accepted, automatically reconciled, needs-revalidation,
  needs-resolution, busy, and failed outcomes distinctly.
- [ ] Give agents bounded access to Base, Incoming, Current, and WorkingTree
  conflict variants.
- [ ] Expose Proposal, reconciliation, validation, and publication receipts
  through public SDK operations rather than private Store calls.
- [ ] Make retryable concurrency outcomes machine-distinguishable from success.

### 4. Compatibility, evidence, and release closure

- [ ] Decide and document Store-schema, SDK, CLI, daemon, and migration changes
  before implementation freezes them accidentally.
- [ ] Preserve 0.1 canonical bytes and object identities unless a separately
  reviewed format change is required.
- [ ] Define reconnect and crash recovery for pending Proposals and resolution
  tickets.
- [ ] Add deterministic concurrent-agent correctness scenarios before adding a
  public performance comparison.
- [ ] Retain latency, queue time, retries, conflicts, validation, CPU, RSS,
  Store growth, object reuse, transaction bounds, and cleanup evidence.
- [ ] Rerun the complete frozen 0.1.x registry against the 0.2 candidate.

## Required ownership boundaries

LayerFS owns:

- Workspace filesystem snapshots and captured mutations;
- immutable Proposals, Branch Commits, Layers, and canonical objects;
- Branch integration ordering and filesystem reconciliation;
- structured conflict evidence and resolution state;
- projection behavior and exact publication outcomes.

LayerFS does not own:

- agent selection, planning, prompting, or delegation;
- semantic code review or product-intent arbitration;
- Git repositories, pull requests, or Git commit history;
- network policy, model-provider integration, or microVM lifecycle.

An external agent orchestrator may choose who resolves a ticket and which
validation to run. It must not need private SQL or mutable access to LayerFS
internals.

## Release sequence

### Phase A: freeze semantics before schema

- [ ] Specify Branch, Workspace, Proposal, accepted Commit, resolution ticket,
  and LayerStack checkpoint state transitions.
- [ ] Specify cumulative three-root reconciliation and rolling merge-base
  advancement for old Workspaces.
- [ ] Specify conflict invalidation, retry, cancellation, and crash recovery.
- [ ] Freeze deterministic correctness fixtures and public outcomes.

### Phase B: concurrent Branch integration

- [ ] Admit multiple Workspace snapshots per Branch.
- [ ] Add the Branch integration sequencer and clean automatic reconciliation.
- [ ] Prove exact linear history under controlled concurrent submission.
- [ ] Prove that stale clean candidates do not expose `HeadMoved` as manual
  agent work.

### Phase C: structured resolution and revalidation

- [ ] Add durable candidate and resolution state.
- [ ] Apply resolution choices to the visible candidate before validation.
- [ ] Preserve unaffected choices when the Branch advances and invalidate only
  intersecting dependencies.
- [ ] Prove that a conflicted Proposal does not block unrelated acceptance.

### Phase D: projection and automation contracts

- [ ] Complete projection conformance, clone acceleration, and OverlayFS.
- [ ] Stabilize typed SDK and CLI outcomes for agent automation.
- [ ] Exercise the same concurrent reconciliation cases through every admitted
  projection.

### Phase E: candidate evidence and release

- [ ] Pass focused concurrency, failure, reconnect, projection, and integrity
  suites.
- [ ] Pass the accumulated 0.1.x registry without unexplained regression.
- [ ] Freeze migrations, manuals, release evidence, artifacts, checksums, and
  source identity together.

## Non-goals

- A general CRDT filesystem.
- Git merge, rebase, index, conflict-marker, or pull-request semantics.
- A line-oriented source-code merge engine inside the storage core.
- One LayerStack Add per tool call.
- A merge-commit DAG for routine intra-Branch integration.
- Blocking every Branch Commit while one Proposal awaits resolution.
- Hiding unvalidated semantic combinations behind a clean filesystem merge.
- Adding agent orchestration to LayerFS.

## Exit criteria

LayerFS 0.2 is complete only when:

- [ ] Multiple agents can execute in concurrent Workspaces on one Branch.
- [ ] Independent stale candidates integrate automatically into a deterministic
  linear Branch history.
- [ ] Changed dependencies cause explicit revalidation and incompatible writes
  cause structured resolution.
- [ ] Resolution survives tool-call and Workspace teardown or has an explicit,
  proved resumable boundary.
- [ ] The exact post-reconciliation candidate is validated before publication.
- [ ] Branch progress, retry, and conflict behavior remain bounded under the
  admitted contention profiles.
- [ ] Pod-to-main LayerStack checkpoints remain explicit, atomic, and free of
  silent overwrite.
- [ ] Every admitted projection produces the same canonical root for the same
  logical result.
- [ ] Public SDK and CLI surfaces expose every lifecycle without private
  shortcuts.
- [ ] Store integrity, fresh reconnect, cleanup, and frozen 0.1.x regressions
  pass from a clean candidate.
