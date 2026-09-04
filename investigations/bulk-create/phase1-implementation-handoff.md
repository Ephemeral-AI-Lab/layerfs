# Handoff prompt — implement Phase 1

Implement Phase 1 of the LayerFS optimization plan: one generic, bounded Workspace
Commit engine. Do the implementation and measurements; do not stop at another
plan or code inventory. Phase 2 live-operation optimization is out of scope.

## Read and pin the starting point

Reference worktree:
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-bulk-create-feasibility`
Reference branch: `codex/bulk-create-feasibility`.

Read:
- `investigations/bulk-create/two-phase-optimization-spec.md`
- `investigations/bulk-create/report.md`
- The v0.1.1 `architecture_shift.md` and `namespace-optimization-spec.md`.
- The v0.1.3 bulk-create-delete optimization notes, tiny-file-churn specification,
  testing rules, execution contract, ordinary execution contract and failure-repair
  amendment under `docs/roadmap/0.1/`.

Work in a dedicated Git worktree on a `codex/` branch; reuse the current one only
if it is already dedicated to this implementation. Inspect the actual current
v0.1.3 implementation branch and pin an explicit commit containing the functional
capacity repairs. Do not assume the default branch is latest. The investigation
started at `7a6e119a`, containing repairs `fbf32e84`; its branch also contains
experimental code that must not be blindly promoted. Record the implementation
base, reference-spec revision, reused prototype commits and any differences.
Preserve unrelated changes and do not edit the original checkout.

This task authorizes isolated implementation, tests, measurements and local
commits. Do not merge, push, publish, change release requirements, or implement
Phase 2. Use subagents for bounded independent implementation/review responsibilities,
including performance, generality and correctness. Assign file ownership and do
not run competing builds or measurements among agents.

## Design requirements

Use one common engine for create, delete, content/metadata update, rename,
replacement, hard links and mixed operations. No benchmark names, operation-family
optimization dispatch, file-count/delete-ratio thresholds, survivor-rebuild route,
or per-case tuning rules in the final proposed code. Preserve sparse-edit locality
in large trees; no full namespace/survivor-manifest fallback. Normal tree cases,
semantic operation handling and resource-pressure spilling are allowed.

Implement these checkpoints in order, adjusting only for demonstrated dependencies:

A. Checked candidate-identity handoff into refresh. Avoid fresh per-path discovery
while preserving final-root membership, aliases, kind, length, mode, timestamps,
link counts, stable NodeIds and handles. Bind the handoff to the frozen Workspace
generation and actual final published root. Bound/spill it. Preserve the published
Commit outcome if installation/cleanup fails after publication; do not duplicate
Commit or falsely report rollback.

B. Complete common mutation input and generic batched tree updates. Reuse surviving
directory overlays; retain compact net-reference deltas keyed by stable inode
identity before reclaim loses necessary facts. Reserve/rollback this bookkeeping
with the mutation, retain it for failed-Commit retry, and measure its Exec overhead.
Existing link counts come from base counts plus net changes, not materialized
`Node.paths`. Normalize capture/reconciliation/root-replacement input exactly once.
Merge sorted deltas by affected persistent-tree pages, reuse unchanged children,
and emit final changed nodes without replaying immutable point updates.

C. Bounded final construction and checked admission. Reuse v0.1.1 codecs/balancing,
compact inode-pair storage, exact metadata results, owned object slabs, bounded
queues and carried admission batches. Existing directory/inode builders are not
automatically linear or memory-bounded: do not hide an entire InsertNode tree
behind a streaming iterator. Reuse nonempty-Store authentication/collision checks,
receipts, fault checkpoints and conditional publication. Do not invoke the
empty-Store initializer or its cleanup. Add concurrency only against measured
need within the fixed CPU/memory budget.

D. Integrate and validate the same engine on bulk create/delete, dense updates,
mixed operations, sparse edits in large trees, reconciliation/capture, no-ops,
repeated Commit and failure/recovery cases. Retire experiment selectors from the
final proposed implementation. Keep unrelated live-path prototypes outside this
Phase 1 integration.

## Invariants and bounds

All 100,000 files, prescribed 500 MiB payload, witness tree, metadata operations
and authentic POSIX/FUSE/public SDK route remain unchanged. Historical roots stay
readable. Delete removes reachability from the new root; it does not delete or
alter preexisting SQLite CAS chunks. Preserve external/unmaterialized hard links,
open-unlinked backing, errors, recovery and cleanup.

Keep speculative/superseded preparation private. Preserve and account for normal
final-candidate admission that may commit object batches before root publication;
do not promise all-row rollback or delete historical objects after a late failure.

Retain the spec's resource limits, including two runtime CPUs, 2-GiB memory and
memory+swap, 256 pids, 1-GiB Workspace spool and 8-MiB final-delta allowance, plus
existing Store/candidate/index/scratch limits. Account for simultaneous allocated
capacity across old Workspace state, new bookkeeping, tree buffers, handoff,
workers, slabs, queues and pending admission. Reserve before allocation; spill or
apply backpressure within existing bounds. Finish required cleanup within Commit
and leave no task workers/private leftovers after End.

Do not move construction, validation or cleanup into Exec, its final fsync, a
prior call or End to improve the Commit score. Minimal mutation bookkeeping is
allowed and must be measured. Do not introduce live compilation overlap or other
Phase 2 work.

## Performance objective and initialization comparison

For complete Commit under the two-CPU colocated profile, provisional planning
expectations are create 5–10 seconds (stretch 3–5) and generic delete 0.1–0.5 seconds.
Demonstration targets are medians <=10 seconds and <=0.5 seconds over the prescribed
three seeds. These are research targets, not promises, lower bounds or new release
gates. The existing 33–56 ms delete result is specialized and does not prove the
generic engine's performance.

The additional objective is initialization-like throughput for shared canonical
construction/hash/storage work under matched conditions. Match the exact prescribed
file distribution, paths, metadata/witness, CPU/owner placement, memory, storage,
cache state, resource/worker limits and instrumentation; record and explicitly
account for actual parallelism and producer occupancy as well as semantic
differences. Do not rearrange files to increase initializer parallelism.
The historical 2.766-second initializer used prepared files, 500 decimal MB and
eight host producers; it is neither a fixed target nor a live-create measurement.

Use retained compatible evidence first. On the stable candidate, obtain one
seed-1 100k matched initialization comparator if none exists, using existing
benchmark/public SDK machinery and qualified preparation. Compare it with that
seed's create-Commit sample. Keep preparation and independent verification separate.
Report shared-stage wall/CPU/throughput and Commit-specific checking, refresh and
cleanup costs; account for overlap. Investigate and explain a material remaining
gap even if the broad Commit target is met. Never claim parity from mismatched
resources, omitted checks or a differently shaped workload.

## Experiment and verification discipline

Reuse the existing benchmark infrastructure, generators, selectors, receipts and
qualified inputs. Prepare only selected inputs. Use private exploratory containers
without taking over the existing campaign; serialize this task's own builds,
preparation, performance and verification. Record shared-host interference and
never call these runs frozen-profile qualification. Do not contact task
`01a069c9-2d3f-75d3-ac59-6393e3555b8f` unless an actual coordination conflict occurs.

For each change: state one hypothesis, predict counters, make the smallest useful
implementation, run one small selected case/seed and one relevant correctness
check, then run one 100k sample only when scale evidence is needed. Preserve all
failures with their source identities. Do not rerun old baselines just to reproduce
known timings or repeatedly rerun passing families. Perform broader affected
verification once stable or when a concrete unresolved risk requires it.

Use the spec's acceptance matrix, especially unseen aliases, open-unlinked handles,
reconciliation final-root selection, sparse locality, spill boundaries, early
admission failures, head conflicts and postpublication installation failures.
Ensure fault fixtures still cross the intended admission boundary after changing
batch sizes. Separate performance from canonical and fresh-FUSE verification.

Record complete Commit and lifecycle wall/CPU, non-overlapping phases, structural
visits/emissions/reuse, content encode/hash work, object/copy/spill bytes, transaction
occupancy, collision/accounting checks, refresh traversals and handoff capacity,
simultaneous memory, resource events and cleanup. A counter not implemented is
unavailable, not zero. A favorable timing never overrides a correctness failure.

## Completion

Implement all applicable Phase 1 checkpoints and continue when an experiment
fails; one failed attempt is not proof of impossibility. Deliver reviewable local
commits, a source/evidence ledger, genericity and correctness review results, the
measured target/comparator assessment, and an updated report stating remaining
risks and the proposed integration sequence. If a target remains unmet, identify
and measure the constraint and give the next concrete experiment; do not conceal
the miss or accept a specialized route. Do not claim success with unresolved
mandatory correctness/resource/cleanup failures. Finish with a recommendation
for review, without merging or publishing.
