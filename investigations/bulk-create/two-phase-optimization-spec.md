# Two-phase optimization plan: generic Workspace Commit

Status: **draft for review; no production implementation or integration authorized
by this document**. Date: 2026-09-04. This plan's “Phase 1” means Commit
optimization, not the existing v0.1.3 Phase 1 benchmark-execution campaign.

## 1. Objective and scope

Phase 1 replaces repeated Commit construction, candidate processing and Workspace
rediscovery with one bounded pipeline for **create, delete, content/metadata
update, rename, replacement, hard links and arbitrary mixtures of those operations**.
The same engine must remain efficient for a tiny edit in a large retained tree.

This specification follows the investigation in [report.md](report.md), the
[v0.1.1 architecture shift](../../docs/roadmap/0.1/0.1.1/architecture_shift.md), and
the existing v0.1.3 execution, ordinary-workload and failure-repair contracts. It
changes no workload, release requirement, public SDK semantics, canonical format,
resource ceiling or retained-root guarantee. Production integration requires a
separate decision after measured evidence and review.

“Generic” is an acceptance condition, not an implementation label:

- One mutation-normalization, tree-update, admission and refresh path handles all
  supported operations, including mixed operations in one Commit.
- No benchmark/scenario names, create/delete ratios, file-count thresholds,
  operation-family dispatch, survivor-rebuild selector or per-case tuning knobs
  occur in the product decision path.
- Empty trees, key insertion/removal, split/merge and resource-pressure spilling
  are normal cases inside the same algorithm. They are not alternate workload
  routes. Reuse untouched subtrees instead of rebuilding a full final manifest.
- Experimental whole-candidate A/B comparison is permitted during investigation;
  all `LAYERFS_EXPERIMENT_*` optimization selectors and specialized Commit paths
  must be absent from the final proposed implementation.

Phase 1 includes all work from Commit entry through its acknowledgement: fence,
final-content construction, namespace/inode construction, candidate closure,
checked admission, conditional publication, checked refresh and required cleanup.
Do not improve Commit by moving equivalent construction/validation/cleanup into
Exec, its final fsync, a prior call, or End. Minimal bookkeeping needed to retain
correct mutation facts may occur when a mutation succeeds, but its CPU, bytes and
Exec-time effect must be measured. Live-path tuning and background compilation
across Exec are outside this phase.

## 2. Baselines and target interpretation

All current timings below are single-sample exploratory Linux Host/FUSE results,
with public SDK lifecycle calls, two container CPUs and 2 GiB memory+swap. They
are not frozen macOS/Docker qualification. Reuse raw source-bound evidence;
never relabel it as evidence for a later source.

| Observation | Source / evidence | Commit |
| --- | --- | ---: |
| 100k create, 500 MiB, final new-inode counts and opt-in SQL tracing | `736fa13f`; [receipt](evidence/container-final-create-500-s1/create-final-refs-run.stdout) | 78.739 s |
| 100k delete, specialized survivor construction and directory page reuse | `22b31552`; [receipt](evidence/container-directory-pages-500-s1/run.stdout) | 0.033 s |
| Earlier 100k delete survivor prototype | `b5dd2829`; [receipt](evidence/container-dense-delete-500-s1/run.stdout) | 0.056 s |

Create's 78.739 s contains content **29.904**, late namespace **0.156**, candidate
finish **7.645**, membership planning **2.992**, admission **10.536**, publication
**0.003** and refresh **27.432** seconds. Directory/inode construction also occurs
inside “content”; the 156-ms timer is not total structural work. Even eliminating
refresh entirely would leave approximately **51.31 s**.

The corrected frozen-profile create median remains 188.072 s for the lifecycle,
with 93.181 s median Commit, from source `fbf32e84` and its retained ledger. It
is contextual evidence, not a matched control for these Linux experiments.
Initial investigation pin `7a6e119a` includes those functional capacity repairs.
Before implementation, inspect and pin the actual current repaired implementation
branch in a dedicated `codex/` worktree; do not assume `main` is current or apply
this work to the benchmark owner's checkout. Preserve the experiment sources.

| Phase 1 target, complete Commit | Planning expectation | Demonstration gate | Stretch |
| --- | --- | --- | --- |
| Create 100k / 500 MiB | 5–10 s | At most 10 s median over the prescribed three seeds | 3–5 s |
| Delete 100k / 500 MiB | 0.1–0.5 s | At most 0.5 s median over the prescribed three seeds | Approximately 0.05–0.1 s |

These are investigation targets, not promises or new release gates. Faster than
the planning range is acceptable. The generic delete gate is **unproven**:
survivor reconstruction avoids removed keys; a generic delta engine may need to
process 100k removed inode keys. Do not retain a case-specific route to pass it.
If the generic engine misses a target, report the miss and the responsible work;
do not relax semantics, resource limits or universality.

Falsifiable allocation for a 10-second create Commit:

| Non-overlapping critical-path contribution | Budget |
| --- | ---: |
| Content compilation and all directory/inode construction | 4.50 s |
| Candidate closure/finalization | 0.75 s |
| Membership checks and checked admission | 2.25 s |
| Atomic publication | 0.05 s |
| Checked identity-based refresh | 1.00 s |
| Required private-artifact cleanup | 0.75 s |
| Contingency | 0.70 s |
| Total | 10.00 s |

If stages overlap within Commit, report actual intervals and critical-path time;
do not add nested or overlapping timers. Historical initialization consumed about
13 host CPU-seconds: unchanged work on two CPUs already requires at least about
6.5 seconds before Workspace overhead. Three-to-five seconds needs CPU work
reduction, not simply more workers. No worker-count or host-placement change may
be hidden in the comparison.

## 3. Common mutation input and ownership

Freeze an active Workspace generation under the existing quiescence/ownership
protocol. The planner consumes final directory overlays, final inode/content
versions and necessary base identities; it does not replay the full syscall log.

The common logical input is a coalesced binding change:

`(parent inode, name, base binding, final binding)`

plus final changed inode metadata/content and checked link-count effects. Repeated
writes compile only the final version; repeated changes to one name yield one
net binding change. Create-then-delete and changes reverted to their original
state must not emit persistent objects solely for intermediate versions.

Use `DirectoryData.changes` for surviving directories' final bindings. Retain
only the missing information across reclamation: a bounded, charged accumulator
or private stream of **net namespace-reference deltas keyed by stable inode
identity**. Existing nodes use canonical InodeId; new nodes use generation-scoped,
nonreused NodeId until final canonical assignment. Do not retain full deleted
paths, payloads, attributes or dead directory overlays in a second ledger.

Record +1/-1 from identities already resolved by successful shared binding
operations: insertion, unlink and rename/replacement. Coalesce repeated changes;
resolve an existing base record once at Commit. Reserve ledger capacity before
mutation becomes irreversible, and include it in failed-operation checkpoint
rollback. Ledger I/O failure must not leave a successful mutation unrecorded.
Do not hash content, traverse additional trees or emit/admit canonical objects
as mutation-time bookkeeping. Measure its Exec CPU, allocation and latency cost.

Capture creation/linking/clearing already passes through common Workspace
mutators; retain the captured Workspace's newly generated reference ledger when
installing it. Reconciliation and any existing whole-root producer must supply
the same final binding/reference input through their mutations or an authenticated
bounded old/new diff, timed in Commit. This is a source adapter for an existing
semantic operation, not a different optimization engine. Every successful
mutation generation must have complete deltas before reclaim: there is no silent
fallback to repeated deleted-subtree rediscovery for a producer that omits them.
A root-diff adapter must not count a removed binding again if its effect is already
present in the mutator ledger; normalize each semantic binding transition exactly
once. Retain ledger ownership on failed Commit for retry; reset only with successful
base replacement or the applicable Discard/end cleanup transition.

For existing inodes, final references are the authenticated base count plus net
binding additions minus removals, using checked arithmetic. `Node.paths` contains
only materialized aliases and cannot supply existing global link counts. New
inodes may use their complete final binding set when completeness is established.
Renames, replacements and hard-link exchanges must not produce transient zero
counts. Preserve aliases outside the edited or removed subtree. Directory paths
do not define inode-table key ranges; inode IDs are not ordered by path.

Open-unlinked state has separate lifetime ownership: it can be absent from the
new root while its handle, content and private backing remain usable until the
last close. Do not re-admit it as a reachable inode or discard its backing during
refresh. Preserve all existing error, permission and ordering semantics.

## 4. Generic final-state construction

Implement batched changes at the persistent-tree layer, using existing codecs,
validators, ordering and balancing invariants. Workspace supplies data; it must
not contain a second directory/inode format implementation.

1. Produce sorted, unique-key directory and inode deltas with bounded in-memory
   runs and private spill when needed. Finalize each inode record only after
   final content, metadata and link count are known.
2. Partition deltas by existing child key ranges. Decode and validate affected
   pages, merge their changes, and reuse unchanged children by identity without
   visiting all their entries.
3. Finalize splits, sibling redistribution/merges, root collapse, levels and
   subtree summaries before emitting final pages. Bounded sibling reads required
   by format invariants are allowed; repeated root-to-leaf replay per mutation
   and repeated emission of intermediate persistent pages are not the design.
4. Use the same algorithm for an empty tree, sparse edits, dense inserts,
   removals and mixtures. Neither a global namespace manifest nor a separate
   survivor scan is a fallback for large or difficult inputs.

“Build once” means each selected content version and final changed structural
page is emitted once, with necessary bounded format work. It is not an assertion
that every old page can literally be read once under all balancing conditions.
Measure page visits/decodes/emissions and their relationship to affected keys.

Compile file content from existing Workspace readers and piece plans. Reuse
canonical builders and exact metadata results without changing content identity
or the chunking profile. Already immutable unchanged content is shared, not
recompiled. Metadata-only edits do not read the full file payload. No new content
worker pool is required by this spec: add bounded concurrency only if a measured
serial bottleneck warrants it and the combined CPU/memory budget supports it.

## 5. Final identities and checked Workspace refresh

Extend the internal candidate result with a bounded, generation-bound handoff.
Its exact representation is an implementation detail, but it must bind the
frozen Workspace incarnation/generation, base root, candidate root and existing
Workspace NodeIds to the final inode/content/metadata records they will present.
Cover changed and unchanged materialized nodes, aliases and retained handles;
reuse authenticated base identities where unchanged. Do not build a second
unbounded full inode/attribute map alongside the existing Workspace.

Construction must establish that each retained binding belongs to the candidate
root and that aliases identify the correct same inode. Encoding an object and
recording its ID does not alone prove namespace membership. The handoff must
carry or reference enough validated construction information to discharge the
current kind, length, mode, timestamp, link count, alias and backing checks.
Checking a root hash alone is insufficient. Proof may combine authenticated
unchanged subtrees with validated changed bindings; it must not recreate the old
whole-path rediscovery under another name.

Reconciliation can select a root different from an ordinary candidate. Derive
the handoff from the final selected state, including any bounded authenticated
base/final diff needed by that semantic operation; never reuse an ordinary
candidate handoff for a different root.

After conditional publication, pin/verify the actual committed head/root against
the handoff, then install the final state while preserving Workspace NodeIds,
open handles and permitted continued operations. Invalidate presentation caches
across mutation-generation reset and Workspace replacement. Cleanup remains
accounted inside Commit; required open-unlinked backing is retained and charged.

Publication and presentation are distinct outcomes. If publication succeeds but
handoff installation, refresh or cleanup fails, retain the published Commit ID
and correct outcome/recovery state. Do not report an ordinary pre-publication
rollback or repeat an already-published logical Commit on retry. The current
fallible rebase-after-publication boundary requires explicit verification; a
successful ordinary Commit test does not prove this failure behavior.

## 6. Reuse and bounded checked admission

| Existing implementation | Required use/adaptation |
| --- | --- |
| [`build_initial_directory`, deferred directory machinery](../../crates/layerfs-content/src/filesystem/apply.rs#L366) | Reuse emission/validation concepts; its insert-and-prune loop is not the required generic batched writer. |
| [Directory codecs/balancing](../../crates/layerfs-content/src/tree/directory/edit.rs#L319), [authenticated diff](../../crates/layerfs-content/src/tree/directory/diff.rs#L21), [inode helpers](../../crates/layerfs-content/src/tree/inode/table.rs#L40) | Reuse existing formats, bounded base reads and unchanged-subtree pruning. |
| [`build_initial_inode_table_from_pairs`](../../crates/layerfs-content/src/tree/inode/table.rs#L478) | Already useful for the small survivor experiment; do not use its whole in-memory `InsertNode` tree as the general 100k solution. |
| [`CompactInodePairWriter` / stream](../../crates/layerfs-layerstack-store/src/objects.rs#L1076) | Reuse 64-byte pairs, bounded buffering, checked lengths and consumption. Adapt task-block assumptions; streaming input must not hide an unbounded downstream builder. |
| [`InitializationSlabWriter` / slabs / queue metrics](../../crates/layerfs-layerstack-store/src/objects.rs#L345) | Extract the owned-buffer mechanism where needed: 256-KiB / 512-object slabs and four queued slabs. It cannot serve reads required by structural editing. |
| [`InitializationSegmentAdmission` pending/batch logic](../../crates/layerfs-layerstack-store/src/objects.rs#L2730) | Reuse bounded exact duplicate comparison and batches carried across file/directory boundaries, not its empty-Store constructor. |
| [`insert_initialization_segment_batch`](../../crates/layerfs-layerstack-store/src/objects.rs#L3114) | Reuse prepared insertion and authenticated conflict readback/byte comparison under the generic Store ownership contract. Preserve receipts and all relevant fault hooks. |
| [`NativeImport::portable_metadata`](../../crates/layerfs-layerstack-store/src/layerstack.rs#L1733), [`rope::build` / `build_bytes`](../../crates/layerfs-content/src/file/rope/build.rs#L16) | Reuse exact bounded metadata interning and canonical content construction. The byte-slice shortcut applies only below its actual CDC threshold; it is not a universal file fast path. |
| [`commit_candidate` publication logic](../../crates/layerfs-layerstack-store/src/workspace.rs#L222) | Preserve LayerStack ownership, expected-head validation, collision handling and atomic commit/head publication. |

Keep helpers in their owning content/Store layers and extract only genuinely
shared internal interfaces. No new bulk SDK API, second database owner,
per-operation plugin hierarchy or duplicate benchmark framework is needed.

Use bounded candidate closure: either retain the current reachability validation
or prove an equivalent final-object construction invariant. Never set
`all_reachable` merely because the pipeline emitted an object; superseded
structures or content must not be included silently.

Carry a pending admission batch across structural/directory boundaries under the
existing less-than-4-MiB and at-most-8,191-object machinery, subject to aggregate
ownership bounds. This is not permission to disable checks or simply call the
initializer's bulk mode. Current Workspace fault hooks differ by admission path;
shared extracted code must preserve them and the candidate accounting equations.
SQLite remains the authority for existing object IDs. Equal IDs require the
existing authentication/collision comparisons; duplicates are not new insertions.

No empty-Store assertion, initialization-only membership omission or
`clear_failed_direct_initialization` cleanup may enter Workspace Commit. Deleting
a file changes new-root reachability; **do not DELETE, clear or compact historical
CAS chunks from SQLite as part of unlink or Commit**.

Keep speculative/superseded preparation private. Normal admission of a finalized
candidate may already commit bounded object batches before root publication; a
late failure can leave newly admitted unreachable objects under the existing
contract. Preserve and account for that behavior rather than falsely promising
all-row rollback or deleting retained Store objects. Do not introduce additional
persistent speculative accumulation during Exec.

## 7. Resource and failure invariants

Retain the applicable existing ceilings: two runtime CPUs; 2-GiB cgroup memory
and memory+swap with zero swap/OOM; 256 pids; 2-GiB host RSS where a separate host
exists; existing file/aggregate limits; 4-GiB sample Store including SQLite
scratch; and existing scratch/output/deadline limits. Workspace logical spool
remains capped at **1 GiB** and final-delta ownership at **8 MiB**. The wider
benchmark scratch allowance does not increase the Workspace policy. Preserve
Store candidate-buffer/index ceilings (currently 8 MiB / 64 MiB) as distinct
existing bounds; their presence does not expand the 8-MiB final-delta allowance.

Prepare a simultaneous ownership ledger before implementation: existing Workspace
state, added mutation facts, sort runs/merge cursors, tree pages/siblings, content
buffers, metadata interning, worker/slab queues, candidate/index state, admission
pending state and refresh handoff. Apply the existing category limits plus total
process/container limits; do not give every component an independent “8 MiB” and
then ignore their sum. Count allocated capacity and copies, not only logical
payload. Enforce reservations before growth; detecting an oversized whole tree
after allocation is insufficient.

Private spill must use the existing qualified scratch lifecycle, checked I/O and
bounds; count simultaneous raw, compiled and superseded storage. Join all workers
and complete required cleanup before the owning operation returns. Cleanup errors
must preserve enough ownership for recovery and be visible; no untracked orphan
scratch or worker survives End.

Existing errors, partial-write behavior, failure checkpoints, conditional atomic
publication, old-root readability, alias identity and open-handle lifetime remain
mandatory. The fault fixtures must still cross at least one early committed admission batch
after batch-size changes; a formerly large enough fixture may no longer trigger
the intended boundary. Inject by causal checkpoint rather than old transaction
ordinal alone.

Canonical equivalence follows the existing format/oracle contract;
valid namespace tree shape may differ from an insertion-history-dependent
reference. Do not substitute root equality for content, metadata and alias proof,
or accept logical equality if a required canonical/profile check fails.

## 8. Implementation checkpoints and experiment discipline

**A. Checked identity handoff.** First prove bounded refresh equivalence using the
current candidate construction. Reduce per-path rediscovery, authenticated reads
and duplicate materialization while preserving all presentation checks. A small
mixed/alias fixture and a publication-success/refresh-failure check are required.
Do not claim the whole target from this checkpoint; its maximum measured
opportunity is about 27.43 seconds.

**B. Common delta input and generic tree writer.** Prove mutation-input completeness
before reclamation, then implement sorted/coalesced page updates and unchanged
subtree sharing. Validate sparse and dense mixed changes through the same entry
point before a large performance run. Retire the specialized survivor selector
and repeated immutable point-update route; use the legacy engine only as an
independent test oracle during development.

**C. Bounded construction/admission pipeline.** Integrate compact state, exact
metadata reuse, owned slabs and carried checked admission. First demonstrate fewer
intermediate objects, copies and underfilled transactions; then test elapsed
Commit time. Add concurrency only against a measured remaining CPU/queue issue.

**D. Integrated qualification and recommendation.** Freeze one candidate source,
run the selected large cases with required observations, then independent proof.
Broader affected verification runs once when stable or to resolve a concrete
remaining risk. Deliver a proposed integration sequence and remaining risks;
this document does not authorize merging or publication.

For every experiment, retain a hypothesis, smallest change, predicted counters,
selected case/seed, source and binary identity, input/oracle identity, environment,
commands, outcome and continue/revise/stop decision. Use existing generators,
selectors, receipts and qualified preparation; prepare only selected inputs.
One small case/seed normally precedes one 100k scaling sample. Do not infer 100k
success from small tiers, rerun the old baseline merely to reproduce known times,
or repeatedly rerun passing test families. Failures and unsuccessful prototypes
remain source-bound evidence.

Use dedicated private exploratory containers as authorized, without acquiring
or modifying the other campaign's state. Serialize this investigation's own
build/preparation/performance/verification work. Shared-host interference is
explicitly allowed for exploration and recorded. A frozen-profile qualification
requires its existing coordination and measurement rules; contended exploratory
results cannot be relabeled as qualified. Do not contact another task without
an actual coordination conflict.

## 9. Acceptance and evidence

Use existing selected cases for create/delete and available edit/episode/retained-
history coverage. Add only the smallest focused regression for missing semantic
coverage, not new benchmark workload families. Every row below must exercise the
same planner/writer and bounds:

| Required coverage | Proof |
| --- | --- |
| 100k create and delete, prescribed 500-MiB tree | Full payload/metadata/witness oracle, public SDK/FUSE route, phase timings and bounds; retained SQLite rows and old roots unchanged. |
| Tiny payload edit and metadata-only edit in a large retained tree | Untouched subtree sharing; structural work scales with affected pages and required neighbors, not total entries; metadata-only edit reads no full payload. |
| Mixed create→rewrite→rename→link→unlink; dense updates | One coalesced final state, correct IDs/content/link counts, no case selector. |
| Outside unmaterialized alias, directory rename/replacement, delete/recreate | No lost alias, transient zero-reference deletion or stale path-based identity. |
| Create then delete / write then revert / no-op / second Commit | No intermediate-only admitted content; correct no-op/root/outcome behavior. |
| Reconciliation and whole-root capture/replacement | Complete normalized reference input without double-counting; handoff derived from the final selected root even when it differs from the ordinary candidate. |
| Open-unlinked file, pinned changed inode, continued operations after Commit | Stable handles/NodeIds, retained backing and eventual cleanup. |
| Tree page boundaries, split/merge/root collapse, long names, forced private spill | Format invariants and bounded capacity under one algorithm. |
| Candidate/admission collision or I/O failure; expected-head conflict | No partial root publication; exact error/receipt/recovery behavior; historical objects unchanged. |
| Failure after publication during handoff/refresh/cleanup | Published outcome retained, no duplicate logical Commit, correct recoverability and cache invalidation. |

For the stable candidate, measure the prescribed three performance seeds once
per selected 100k case and report each sample plus median; run independent
canonical and fresh-FUSE proofs separately. Target attainment, correctness,
resource safety, route generality and release qualification are separate statuses.
A good median never cancels a failing correctness or resource gate.

For sparse-change nonregression, use matched source-bound comparisons on the
existing representative large-tree case. Flag a median Commit regression exceeding
the larger of 10% or 2 ms for investigation; this is a review trigger, not
permission to trade away locality. Resolve timing noise with structural counters
and only the additional paired measurement necessary. Do not accept an
input-sized scan because its one warm timing happened to be small.

Mandatory diagnostics: Commit wall/CPU and non-overlapping phase attribution;
Exec/End/whole lifecycle guard timings; affected keys; old pages read/decoded and
final pages emitted; unchanged subtree reuse; content bytes/encode/hash counts;
intermediate objects and copies; spill I/O; candidate/reachable/inserted/reused
accounting; transaction occupancy and collisions; refresh resolutions/reads and
handoff capacity; simultaneous owned memory; process/cgroup CPU, RSS and peak;
scratch allocation; workers/queue waits; cleanup duration and residual resources.
Reuse existing telemetry; minimally extend missing counters. Counter absence
means unavailable, never zero. No full verifier or census runs inside performance.

Phase 1 ends with a measured generic candidate and a recommendation: target met,
target missed with a demonstrated bottleneck, or correctness/resource blocker.
Any mandatory failure remains unresolved work, not an accepted optimization.
No new release guarantee is inferred from this investigation's target table.

## 10. Phase 2 — live operation optimization

After Phase 1 results are reviewed, draft a separate plan for the complete POSIX
workload inside SDK Exec, targeting repeated binding/metadata work, enumeration,
mutable staging and ownership/transport costs through the same generic engine.
Consider bounded private compilation overlapping later writes only in that plan;
measure the complete lifecycle to prove that work was eliminated or overlapped,
not merely moved between calls.

## Review record

Three read-only subagent reviews were requested: performance/measurement,
algorithm generality, and immutable-root/failure/resource correctness. Their
findings are incorporated above; independent review identifies obligations and
risks, and does not establish that unimplemented performance targets are met.
Final reviews on 2026-09-04:

| Independent review | Disposition | Findings incorporated |
| --- | --- | --- |
| Performance and measurement | Approved as draft; performance unproven | Non-overlapping 10-s budget, historical CPU constraint, refresh-only opportunity limit, phase-shifting guard and same-engine scale proof. |
| Generality and sparse efficiency | Approved as draft; implementation gates remain | Compact stable-inode net-reference facts before reclaim, one batched page writer, no survivor/workload selector, exact-once root-diff normalization. |
| Immutable roots, failures and resources | Approved as draft; implementation gates remain | Root/generation-bound checked handoff, unseen aliases/open handles, postpublication failure outcome, aggregate accounting, explicit reconciliation/capture acceptance row. |

No material draft objection remains. Open proof obligations are implementation
completeness and low overhead of reference bookkeeping; bounded generic tree
updates meeting both dense-delete and sparse-edit goals; sound identity handoff
without a second namespace traversal; nonempty-Store failure semantics; and the
measured performance targets. Approval of this draft certifies none of those
unimplemented results. Reviewers performed source/evidence analysis only; no
benchmark or production code was run or changed for this drafting task.
