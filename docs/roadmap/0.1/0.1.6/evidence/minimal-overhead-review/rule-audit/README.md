# Issue #130: rules and minimality audit

Review date: 2026-09-14. Scope: the captured #130 issue body and its supporting
proposal, not implementation qualification. Three subagents independently
reviewed foundation/ownership, Commit/scans and minimality; root reconciled
overlapping findings. No product source, governing rule, proposal or issue was
changed. #130 remains deferred until the earlier seven steps and verified
completed closure of #124/#125.

## Verdict

**Do not approve this as a fully specified implementation yet.** The sparse
upper/immutable-root model is compatible with the governing rules in principle,
and #130 explicitly preserves the important lifecycle, identity, publication and
benchmark requirements. It does not contain an explicit instruction to weaken
V1 or restore freezing. Its prerequisite requires those earlier obligations to
be completed first; existing V1 uncertainty is not a newly omitted #130 feature.

There are nevertheless actionable guard/adapter omissions and unselected pager
protocols. Repeating an invariant is not a mechanism that establishes it. The
full set of proposed optimizations is also not established as minimal: the issue
permits choosing fewer changes, while its README turns some costly alternatives
into mandatory execution steps.

The review findings below are design risks or ambiguities, **not observed bugs
in an implemented #130**, and the statuses in the rule matrix are document
coverage judgments, not test PASS results.

## Findings, ordered by implementation risk

### R7 — Packed storage must not broaden a file reader's ownership [P1]

Anchors: [issue lines 88–98](issue130.md#L88), [proposal's compact range proposal](proposal.md#L97).
Rules: §5 independent ownership and reclamation, §9 bounded retention; selected
specification's retained-range contract.

Packing records changes the unit that a page lease retains. Counterexample:
files A and B have inline range/token descriptors in one packed leaf. A retained
file-only reader keeps that leaf alive; B is then deleted or replaced. Reusing
the previous dedicated-file-root lease would also retain the old leaf's external
ownership of B's potentially large payload for as long as A's reader survives.
The desired saving in metadata would introduce unrelated payload retention.

A full Commit snapshot legitimately owns its entire captured namespace. The
counterexample concerns a file-only retained reader, which should not acquire
that broad ownership accidentally.

Smallest singleton correction: under a temporary page/root lease, decode the
target record and acquire only its token/base-context ownership, then release
the parent page. Fragmented shared-range storage needs an equivalent bounded,
file-scoped cursor/ownership contract; copying every piece into RAM is not a
solution. Add the A-reader/B-large-payload retirement check. Until that is
selected/proved, keep existing file-scoped range roots. The 106-byte canonical
correspondence optimization does not introduce raw-payload edges and is the
safer initial packing change. This is a new-design counterexample, not a claim
that the current separate per-file representation has this bug.

### R1 — Tighten the zero-work Commit guard [P2]

Anchors: [issue lines 83–87](issue130.md#L83), [proposal lines 124–128](proposal.md#L124).
Rules: §7 incremental C2 and §8 exact captured/comparison context.

The public issue allows skipping construction when captured state is represented
by the canonical predecessor. That is ambiguous between unchanged logical
sequence and merely equal canonical bytes. The earlier detailed Commit review
used the narrower sequence guard, but that requirement was not carried into the
issue's executable description.

Counterexample: replace bytes with equal bytes carrying a new Origin, Commit
produces UpToDate, then perform a small edit for C2. Reusing the old predecessor
description solely because output bytes/root match leaves correspondence for
the old occurrence. The next Commit can lose its localized predecessor anchors
and reconstruct unchanged data.

Smallest correction: zero-work reuse requires a valid captured logical sequence
equal to the applied covered sequence for the same Workspace/canonical context.
Keep existing staging, expected head/base and V4 outcome resolution. Dirty but
equal-output attempts still update captured metadata-only correspondence even
when canonical encoding can be reused. Add the equal-byte/new-Origin ->
UpToDate -> localized C2 regression to the selected checks.

### R2 — The memory-only pager bootstrap is not selected [P2]

Anchors: [issue lines 93–98](issue130.md#L93), [proposal lines 138–156](proposal.md#L138).
Rules: §§5,6,9 and §11's requirement to select ownership/buffering mechanisms.

The proposal promises stable evictable IDs, no unbounded RAM location map and no
mandatory backing before spill. It does not select where initial ownership and
location records live or how they transition without a whole-graph conversion.

Counterexample to a naive implementation: a retained snapshot owns a resident
page when the cache fills. Evicting the only authoritative bytes loses its view;
pinning all PageCells defeats bounded residency; converting the complete graph
on first spill/capture defeats the affected-path/acquisition bound. A shared
cache also needs scoped identities, actual retained-owner charging, reserved
release tickets and a lock order that does not span callbacks or physical I/O.

Smallest correction for the initial cut: keep the existing disk-owned metadata
graph and defer only genuinely unused resources. If measurements later justify
the pager, first select its bounded resident catalog, transition/rollback,
read lease, ownership and eviction protocol and its failure checks. Do not claim
memory-only D0/D1 until that mechanism exists.

### R3 — Resident-only writes need an explicit fsync path [P2]

Anchors: [issue lines 93–98](issue130.md#L93); the proposal preserves fsync only
generally in its specification-boundary paragraph.
Rules: §5 buffering/fsync and §9 supported filesystem behavior and error handling.

Counterexample: acknowledge a 17-byte ordinary write held only in memory, then
fsync before cache pressure causes eviction. An unchanged descriptor-only sync
path may sync empty backing and report success without processing the resident
bytes or the required transfer/error boundary.

Smallest correction: select how explicit flush/fsync processes resident-only
payload and metadata, reports partial I/O/writeback errors, and retains ownership
on failure. Add the write -> fsync-before-eviction case. This preserves the
existing fsync contract; it does not introduce restart durability or require
ordinary writes/Commit capture to flush synchronously. Keeping the disk-owned
backend initially avoids introducing this new boundary in the first cut.

### R4 — Separate name-order merging from live numeric-cookie resume [P2]

Anchors: [issue lines 168–211](issue130.md#L168) and [240–246](issue130.md#L240).
Rules: §2 supported open/namespace semantics, §5 incremental metadata and §6
bounded snapshot/cursor ownership.

The O(L+K) name-order merge describes a fixed captured view. Live readdir keeps
numeric-cookie and concurrent mutation semantics. These are distinct interfaces.

Counterexample from an existing oracle: read to EOF, create a new name that sorts
before the previous lexical frontier, then resume from the old numeric cookie.
The current live contract expects the newly created entry to be reachable.
Reusing the lexical frontier would miss it; pinning the old root changes live
behavior. Re-seeking/revalidating on new roots adds real work beyond two initial
seeks.

Smallest correction: retain the existing live cookie-order adapter and its
after-EOF/delete-recreate/partial-page tests. Use the streaming merge directly
for fixed snapshot/internal views. Define any live batching adapter separately
and count its residual seeks, cookie lookups and revalidation; do not silently
apply the fixed-view complexity bound to arbitrary concurrent live enumeration.

### R5 — Make the execution list match the optional, minimal scope [P2]

Anchors: [issue lines 114–117](issue130.md#L114) versus
[proposal lines 572–583](proposal.md#L572). Rule: §11 smallest satisfying
implementation, plus the owner's minimal-overhead objective.

The issue says a simpler measured solution may omit exploratory representations.
The README instead makes tiny-payload conversion and a shared pager mandatory.
That commits to eviction/location/refcount, ID-domain and source-promotion
machinery before measuring the cheaper changes at existing interfaces.

Smallest correction: one selected initial cut, explicit optional follow-ups with
a measured reason. Keep dual change indexes, cookie/alias maps, existing ID
mapping and repaired payload intervals initially. Do not make replacing all of
them a condition of success. Lazy constructors alone do not produce zero-FD
Begin: the current root metadata is written during initialization.

The same execution list also says to integrate default public dispatch and
prove capture/C1/C2 as if they were new #130 deliverables. They must already be
delivered by its #124/#125 prerequisites. Replace that instruction with preserving
the delivered integration and rerunning only evidence invalidated by the selected
optimization. Otherwise the follow-up can accidentally restart completed work.

### R6 — Carry forward affected capacity and natural-overlap evidence [P2]

Anchors: [issue lines 396–415](issue130.md#L396) and
[454–488](issue130.md#L454). Rule: §10 required evidence and source validity.

Held-builder checks establish operation progress, but the builder does no work
while held. Sequential small-call results do not expose contention during real
construction/eviction. A shared pager/admission change can pass those checks and
still introduce foreground latency spikes or a large-scale ownership problem.

Smallest correction: explicitly carry forward the delivered natural-overlap
qualification and required million-changed-file/spill evidence. Reuse passes
when their exercised source/fixture/environment remains valid; if a selected
optimization changes those dependencies, rerun the affected original proof and
report foreground p50/p95/p99, first post-capture write work and resource peaks
under its existing contract. The 100k accounting examples are not a substitute
for the million-file rule. No new numeric threshold, repetition policy, or
unconditional full-suite rerun is proposed.

## Rule-by-rule coverage

| Governing section | Coverage in #130 | Audit disposition |
|---|---|---|
| §1 Current mutable state; no per-update history | Sparse current upper, superseded changes replaced, temporary history forbidden | Explicitly preserved; no contradiction found |
| §2 Non-pausing lifecycle; mount/inode/descriptor identity | Continued-operation timeline, brief installation, live descriptor ownership, no reset/remount | Explicitly preserved; live cursor adapter requires R4 |
| §3 Consistent capture, C1/C2 and one unresolved attempt | Independent roots, exact coverage, attempt gate separate from operations | Explicitly preserved in intent; R1/R2 require precise mechanisms |
| §4 One authority and host/container placement | Host root/backing/construction; separate fresh ownership scopes; existing service reuse | Explicitly preserved; lazy pager metadata authority needs R2 |
| §5 Incremental storage, ownership, fsync and reclamation | Range sharing, bounded cache/owners, retained readers and cleanup | Intent preserved; pager/fsync protocols need R2/R3 and packed-reader ownership needs R7 |
| §6 Bounded acquisition and evictable snapshots | Root retention; no map conversion, cache drain or delayed whole-copy | Explicit prohibition preserved; resident bootstrap remains unselected under R2 |
| §7 Existing encoder and incremental C2 | Same canonical pipeline, changed-only scan, predecessor descriptions | Preserved except ambiguous zero-work guard R1 |
| §8 Exact stage/publication/retry and preserved live progress | Same candidate/context, V4 receipts, no stage-as-inactive | Explicitly preserved; R1 must not reuse stale correspondence |
| §9 Aggregate memory/ownership and supported kernel visibility | Actual charges, retained-owner accounting, V1 inherited from completed prerequisites | No V1 waiver found; pager/resource/fsync mechanisms require R2/R3/R7 |
| §10 Correctness/capacity/performance evidence | Source/custody, preserved failures, no new gates, #122 excluded | Generally preserved; make affected overlap/million-file evidence explicit under R6 |
| §11 Select concrete mechanisms; smallest implementation | Existing machinery preferred; some proposals marked optional | Not yet minimal/selected consistently; R5 and R2 |
| §12 Existing reuse references | Existing daemon, builder, Index/Payload, compact forms and receipts are referenced | Guidance rather than another acceptance gate; minimum cut below maximizes reuse |

The owner-added ordering constraint is explicit: #130 cannot start before the
seven-step campaign and verified #124/#125 closure, and cannot absorb their
mandatory unfinished work. The review does not move that boundary.

## Smallest credible initial implementation

```text
Keep:
  delivered snapshot boundary and FUSE/SDK semantics
  current root Index and disk ownership backend
  existing ID, cookie, alias and dual-change maps
  repaired Payload intervals and existing canonical builder
  exact stage/publication/receipt coordinator

Change first:
  narrow logical-sequence-equals-coverage clean shortcut
  lazy unused payload/index backing and empty construction journals
  106-byte singleton correspondence in the existing parent row
  build one small range leaf once, rather than 3–5 successive updates

Measure residual public-call time, physical disk and I/O
  -> choose another change only for an identified remaining cost
```

The root Index stays because Begin currently writes its root metadata. This cut
does not promise zero private files, memory-only operation, equality with all
v0.1.5 timings, or an already achieved storage target. It is smaller and avoids
multiple new state machines. Dense byte-aware pages/compact range storage are
the next candidates if the remaining measured page cost requires them; a full
pager, identity virtualization and tiny-source promotions are separate choices.

## Review inputs and verification

Pinned issue: `issue130.json`/`issue130.md`, updated at
`2026-09-14T03:34:29Z`. `proposal.md` and `governing-rules.md` preserve exact
reviewed text. `inputs.json` records hashes and source HEAD
`8d6352128248d2598af20641127371f983d66a4e`. Source inspection is explanatory;
this is not an atomic product candidate seal and active unrelated edits were
preserved.

Independent reports: [foundations](foundations-review.md),
[Commit and scans](commit-scan-review.md), [minimality](minimality-review.md).
The root merged overlapping cursor findings and qualified omissions as missing
mechanisms rather than fabricated runtime failures. No benchmark or V1 probe was
run, no issue was edited, and no production test PASS is claimed.
