# LayerFS v0.1.7: Architecture Refactor Migration

> **Status:** Current planning checklist; no release candidate exists.

## Goal

v0.1.7 is the **architecture refactor migration** release: move the internal
architecture onto the shape the [v0.2.0 model](../../0.2/README.md) assumes, so
that 0.2.0 adds new semantics on top of prepared structure instead of replacing
it, and set the tone for that release.

## Direction from 0.2.0

The [0.2 roadmap](../../0.2/README.md) defines the target collaboration model:

```text
LayerStack = main, the globally integrated checkpoint history
Branch     = a rapidly iterating node or pod shared by cooperating agents
Workspace  = one isolated tool-call attempt
```

The [agent Branch reconciliation task](../../0.2/agent-branch-reconciliation/README.md)
records the exact differences between the implemented 0.1 model and that target.
Those differences are 0.2.0's work, not this release's: they change what a
Branch, a Workspace, and an accepted Commit mean. v0.1.7 prepares the
architecture that will carry them.

## Boundary

- v0.1.7 is a 0.1.x release, so the
  [release-policy](../../../general/release-policy.md) promise for the patch line
  applies: public SDK and CLI behavior, daemon protocol, canonical bytes and
  identities, and the Store format are preserved while the internal architecture
  changes. Any exception needs an explicit owner decision recorded here first.
- No 0.2 public semantics, and no 0.2 mechanism, are pre-approved by this
  checklist.
- Projection design focuses on FUSE. APFS-specific projection and clonefile
  acceleration are out of scope for v0.1.7.
- A refactor that cannot fit this boundary moves to 0.2.0.

### Owner scope decision: diff and conflict features

Owner decision, 2026-09-16: the v0.1.7 replacement omits public logical diff,
three-way reconciliation, conflict inspection and conflict resolution. Their
replacement design and implementation move to
[v0.2.0 issue #164](https://github.com/Ephemeral-AI-Lab/layerfs/issues/164).
This is an explicit feature-scope exception to SDK/CLI parity for those surfaces;
it does not waive canonical identity, data integrity or stale-head protection.

Ordinary snapshot/edit/construction/conditional Commit and Add remain. A stale
captured head/base is rejected explicitly, with no automatic merge, rebase or
fallback into the reference implementation. Path resolution, local equality,
no-op checks, physical delta encoding and authoritative publication/completion state remain
required. See the [deferral review](component-decoupling/diff-conflict-deferral.md)
for exact removal candidates, shared helpers and staging constraints.

Existing root crates remain reference during migration. Preserve released manuals
and historical receipts; explicitly record affected candidate API and benchmark
coverage before release rather than treating omitted cases as passed.

### Owner direction: no retries

Owner decision, 2026-09-17: an operation gets one attempt. A condition that needs
retry is failure. This explicitly replaces reference behavior that retries SQLite
lock acquisition, refreshes invalidated admission state or masks codec failures
by selecting FULL. Selected adapters must disable SDK/query/transaction retries.
Planned representation selection and bounded backpressure before execution remain
ordinary work; re-executing failed work is forbidden.

A lost acknowledgement fails with an unknown persistence outcome. Never resend
the write or delete possibly committed data. A separately requested authoritative
inspection may establish what persisted. Successful versions remain immutable.
The [single-attempt design](component-decoupling/physical-encoding-and-packing.md#one-attempt-no-retries)
defines the details. This is an explicit failure-behavior exception for the
replacement; canonical integrity, valid ordinary workloads and performance gates
remain required. Released manuals and reference code are unchanged by this plan.

### Owner direction: implementation rules and persistence scope

Independent C1-only construction, C2-only save/read and integrated timing are
mandatory acceptance requirements from the first real component slices. They must
use the same production bodies, bounded output and existing layerfs-telemetry,
without Workspace/FUSE/history setup. The [measurement contract](component-decoupling/content-io.md#7-measurement-and-completion)
defines scope, backpressure/overlap and root-cause diagnostics. Timer availability
alone does not establish that the components are independently measurable.

Owner direction, 2026-09-17: replacement production files must stay below 1,000
physical lines (maximum 999); lib.rs/mod.rs retain their stricter 200-line limit
and declaration/delegation-only role. Split large components into cohesive folders;
keep tests/examples outside product source. The core guard now checks Rust and
shipped SQL file lengths. This is separate from per-commit production LOC counting.

No retry, fallback, fsync/fdatasync/sync_all/sync_data, WAL or added crash-durability
work is part of the current target.
Use embedded SQLite's selected MEMORY journal / synchronous OFF profile with zero
busy timeout; preserve runtime atomicity/abort rather than disabling journaling.
Do not patch, fork, vendor or modify third-party dependencies. Cloudflare Durable
Objects is a future placement study because its documented SQLite storage uses WAL.
The [implementation plan and fuller review](component-decoupling/implementation-plan.md)
records the folder map, before/after diagrams, memory/disk owners and staged proof.

### Owner direction: file cutoff and delta depth

The objective is configurable transparency: expose policy values and make their
supported overrides work consistently, with derived capacities and explicit
resource bounds. Finding the best numerical settings is outside this refactor.

Latest owner direction, 2026-09-16: retain defaults of **128 KiB small-file cutoff,
8 whole-file delta links and 4 chunk delta links**, with all three configurable
within an explicitly supported Store profile. This supersedes the earlier 1 MiB
default target and a single numerical delta-depth cap. See the
[co-design decision](component-decoupling/content-storage-co-design.md#file-cutoff-and-delta-depth)
and [performance admission](component-decoupling/content-storage-co-design.md#performance-admission-before-changing-defaults).
The
[simplified payload policy](component-decoupling/content-storage-co-design.md#simplified-payload-storage-model)
uses common selection/reconstruction code with role-specific policy data and
preserves current byte/work limits. First qualify the refactor at unchanged
defaults. Larger values such as 1 MiB/50 remain experimental profiles and cannot
silently increase safety budgets or become accepted merely because a knob exists.

A non-default cutoff can change canonical file roots; physical delta depth alone
does not change canonical identity. Supported override ranges, exact format/schema
compatibility and old Store opening/conversion must be specified before
implementation. Defaults preserve the reference representation policy. This is
not a blanket canonical/format compatibility waiver, silent migration or fallback
authorization. Same-profile determinism, authentication and performance acceptance
remain mandatory. No runtime setting or performance result is claimed here.

## Plan status

**Stage 5 is closed at its implemented scope.** The terminal closure is
`2026-09-17T21:05:53Z`, comment `5721219925`, final HEAD
`249d2b917211d300b93fa3ead418a7eeb56e731b`. An earlier component-scope closure at
`2026-09-17T07:58:02Z` (comment `5711002673`, source of record `f2de7810e`) was
reopened by the round-2 independent review and is **not** the closure of record. The
[Stage 5 terminal handoff](component-decoupling/stage-5-terminal-handoff-20260917.md)
drove [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) to its
terminal condition: every remediation round landed its rows with receipts, each
row was verified by a read-only verification subagent, every finding was
adjudicated and remediated, and the final matrices report **81 PASS / 0 FAIL /
0 PARTIAL-INCOMPLETE / 1 NOT_RUN with a written owner disposition (the
complete-operation comparison, deferred to Stage 6) / 1 NOT_APPLICABLE of 83**
for Stage 5 and **34 PASS / 2 owner-WAIVED of 36** cumulative - see
[stage-5-report.md §16](component-decoupling/stage-5-report.md#16-final-matrices-and-the-verification-pass-round-4-2026-09-18)
and the
[round-4 evidence](evidence/stage-5-terminal-20260918T120000Z/README.md).
The [round-2 independent review](component-decoupling/stages-1-5-review-20260917T230700Z.md)
found Stage 5 not accepted (five blocking items); all of them and the follow-up
findings are remediated and verified. The
[Stage 5 handoff](component-decoupling/stage-5-handoff.md),
with its [exact source/test/LOC plan](component-decoupling/stage-5-file-plan.md),
remains the implementation contract for filesystem trees, inodes, attributes and
reference ordering.
The [Stages 3–4 closure record](component-decoupling/stages-3-4-completion-round-20260917.md)
records completed scopes and explicit unmeasured owner waivers; it is not a
performance baseline claim. Stage 6 (#171) qualifies the whole core and owns the
`VF-6` disposition: under the frozen `structural-complexity` decision (`D1`) the
comparative claim is **withdrawn**, not deferred a second time. Stage 7 (#172)
integrates the later Workspace/runtime shape. No complete-operation performance claim is made by
Stage 5, and nothing is tagged or released by this closure.

Two source-read studies now bound what Stage 6 should measure first: the
[parallelism and batching study](component-decoupling/parallelism-and-batching-study-20260918.md)
(worker pools, SQLite tuning, statement batching) and the
[complexity and round-trip research](component-decoupling/complexity-and-roundtrip-research-20260917.md)
(per-area complexity vs the reference, a risk-tiered optimization register, and
§1a's net answer: the core's architecture is ahead of v0.1.6 while its
configuration layer is behind, and the end-to-end balance is unmeasured).
Neither takes a measurement or makes a performance claim.

**Stage 6 is the active stage (2026-09-19).**
[#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) qualifies the complete
C1/C2 core for correctness, performance and memory, and its
[Stage 6 handoff](component-decoupling/stage-6-handoff.md) is the executable
assignment. The case specification was frozen **before** any harness code or
collection, as §1 of the measurement contract requires, in
[`core/docs/benchmark/fs-bench-pro-storage-content/`](../../../../core/docs/benchmark/fs-bench-pro-storage-content/):
the claim (`structural-complexity`), a 217-case registry over 20 families, the four
measurement axes (time, memory, CPU, space), the copy ladder and cache discipline,
the gate and oracle classes, and the implementation estimate. The harness is built at
`core/benchmark/fs-bench-pro-storage-content/`, which is not product source, so the
production LOC delta for this stage is expected to be **0** and benchmark Python files
are exempt from the product line ceilings. Stage 6 inherits Stage 5's deferred
complete-operation comparison (`VF-6`) and must record, in one place, the owner
decision that withdraws the comparative claim beside the four unmeasured rows the
round-2 review assigned to the Stage 6 owner. No performance claim is made here, and
Stage 6 runs under the measurement contract with one sample per case per arm and no
fault-injection branch in product source.

Design planning (owner direction, 2026-09-16). The
[component-decoupling discussion index](component-decoupling/README.md) organizes
the proposed clusters and their future design documents. The
[shared decoupling proposal](component-decoupling/proposal.md) sets the
repository-wide scope, boundary principles and independent-measurement
requirements for the full internal refactor. CAS/delta/CDC/COW/FUSE is one
worked example within that scope.
The design workstream is tracked in
[#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
The [proposed repository layout](component-decoupling/repository-layout.md)
places the replacement product in core/ and future application adapters in
adapters/, retaining existing crates as a reference until replacement qualification.
The first handoff selects core/crates/layerfs-content and core/crates/layerfs-storage
alongside implemented layerfs-telemetry; later runtime/application packages remain
undecided. [Implementation parent #165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165)
has seven children, with [#166](https://github.com/Ephemeral-AI-Lab/layerfs/issues/166)
and [#167](https://github.com/Ephemeral-AI-Lab/layerfs/issues/167) assigned by the
[Stages 0–2 handoff](component-decoupling/stages-0-2-handoff.md).
Stages 0–2 are now implemented in the candidate workspace: `layerfs-content` (C1)
and `layerfs-storage` (C2) ship a real complete-file construction path, exact CAS
reuse, three pack framings, the four-table SQLite schema and independent timing.
The [Stages 0–2 report](component-decoupling/stages-0-2-report.md) records the
frozen profile, expected-versus-actual production LOC, the run commands, the
observed roots and every declared gap (DELTA, larger cutoffs, pooling, RSS
evidence and the un-induced unknown-outcome case). Stages 6–7 remain open, and no
part of v0.1.7 is claimed complete.

The initial [Stages 3–4 report](component-decoupling/stages-3-4-report.md) records
payload delta/configuration and partial edits, with missing physical metadata
pooling, stored-node split/concat reuse, exact reference-root/finality proof and
comparison evidence. The [current continuation prompt](component-decoupling/stages-3-4-continuation-prompt.md)
now separates D's corrected oracle/stored-tree algorithm from independent pooling
coverage and qualification, after pooling was implemented in the
[completion report](component-decoupling/stages-3-4-completion-report.md).
Combined edit qualification waits for D; pooling checks do not.
[#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) and
[#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169) are **closed**
under the scope and waivers recorded in the
[Stages 3–4 closure record](component-decoupling/stages-3-4-completion-round-20260917.md);
their acceptance criteria were not relaxed, and their unmeasured performance rows
stay unmeasured and owner-waived rather than promoted. No optimization claim
follows from the passing subset or smoke timings, and no required work was moved
to Stage 5/6.
The [agreed cluster 1/2 overview](component-decoupling/cluster-1-2-components.md)
contains three canonical-content components and four physical-storage components,
with shared telemetry and external runtime/workflow ownership. Detailed contracts
and implementation packaging remain open for the next design discussion.

The [integrated content-storage design](component-decoupling/content-storage-design.md)
now covers canonical edits and size transitions, repeated chunk deltas,
compression/packing, database dependency cleanup, Git comparison and mandatory
performance qualification. The detailed
[physical encoding and packing proposal](component-decoupling/physical-encoding-and-packing.md)
now fixes terminology, candidate policy, placement-first compatible append,
metadata pooling/index replacement and concrete cuts. Three further read-only
reviews cover payloads, pack lifetimes and metadata/format behavior. Implementation
and at-least-existing performance remain to be proven.
The final [object save and SQLite persistence proposal](component-decoupling/admission-and-persistence.md)
now covers the remaining two C2 responsibilities. All seven component proposals
are recorded. Next is one consistency review and the first complete-file ->
standalone save -> readback implementation, with format/capacity/receipt and
performance proofs still required. Save ownership is proposed as immediate
try-acquisition; producer backpressure remains bounded. Removing distinct-reuse
diagnostic indexing requires an explicit compatible receipt/API decision first.

The [content I/O contract](component-decoupling/content-io.md) and
[v0.1.6 memory/call audit](component-decoupling/content-io-memory-audit.md)
define the next core-only work: finalized-object streaming, removal of generic
candidate payload staging, fewer copies/transforms/DB calls, and memory bounds
across many files and directories. Multi-edit finality and namespace reference
ordering remain proof gaps; zero temporary storage is not claimed universally.
Host/daemon and remote-SQL placement use neutral boundaries. Workspace mode and
transport remain later integration decisions. The
[ordered component discussions](component-decoupling/content-storage-co-design.md#remaining-co-design-decisions)
track remaining implementation proofs, including concrete schema/profile codes,
writer authority and selected single-attempt remote batch/outcome semantics.

LayerStack, Branch and logical Commit remain in the history/workflow layer,
outside clusters 1 and 2. SQLite transaction mechanics belong to persistence;
sharing a database does not transfer history ownership. The
[remaining co-design decisions](component-decoupling/content-storage-co-design.md#remaining-co-design-decisions)
are ordered from canonical object/read/output contracts through namespace inputs,
configuration/format capacity, SQL sessions, measurements and the first standalone
implementation slice.

Time-only parent/child measurement is specified in
[layerfs-telemetry](component-decoupling/telemetry.md) and was implemented for
[#161](https://github.com/Ephemeral-AI-Lab/layerfs/issues/161) as the first
component of the replacement workspace:
[`core/crates/layerfs-telemetry/`](../../../../core/crates/layerfs-telemetry/README.md).
The standalone crate provides environment-independent trees, injected
parent/child scopes, attachment of independently completed reports and
caller-owned text/JSON output. Adapter integration, async behavior and overhead
qualification are still future work and are not claimed here.

The source-linked inventory, exact interfaces and architecture moves, ordered
implementation slices, proofs and measurement plan remain design deliverables.
They are recorded or linked here before implementation begins.

Tracking issue:
[#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155).

Stages 3-4 closeout: the component-decoupling batch under
[#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165) has a completion
gate table with per-packet evidence, controls and the memory ledger in
[`component-decoupling/stages-3-4-closeout-report.md`](component-decoupling/stages-3-4-closeout-report.md).
