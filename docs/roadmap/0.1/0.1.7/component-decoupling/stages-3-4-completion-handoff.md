# Completing Stages 3–4: pooling, stored-tree edits and qualification

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Recovery assignment for incomplete [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168)
and [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169).
Read with the [original handoff](stages-3-4-handoff.md),
[file/LOC plan](stages-3-4-file-plan.md),
[implementation report](stages-3-4-report.md) and
[reviewer assignment](stages-3-4-reviewer-handoff.md).
This narrows execution; it does not remove acceptance criteria or claim a fix.
Use the [continuation prompt](stages-3-4-continuation-prompt.md) to dispatch this
work. This document supplies its detailed ownership and proof requirements.

**Current state after `2b2dbc028`:** E pooling has been implemented, with boundary,
chain and measurement gaps. D's oracle exists but its multi-edit harness applies
an extra coordinate shift; D's stored-tree algorithm remains missing. The
[current prompts](stages-3-4-continuation-prompt.md) supersede the earlier E-first
schedule below: D and independent pooling coverage/qualification proceed separately.

## 1. Diagnosis and execution decision

Source inspected at `24ef187d4` confirms the report's two principal omissions:

- `encoding/pool/mod.rs` has no implementation; the C2 FULL/reader paths reject
  the pooled role/lane. A format tag and empty catalogue table are not pooling.
- C1 `apply.rs::stream_chunked` calls `FileView::walk_extents`, forwarding retained
  slices into `EditFrontier` and its `ExtentBuilder`. `split.rs` slices one extent;
  `concat.rs` coalesces two extents. Neither implements stored-tree split/concat.
  Payload IDs survive, but untouched mapping structure is walked and re-derived.

Bounded memory and canonical validity therefore do not establish localized mapping
work or exact v0.1.6 edit roots. This is an algorithm gap, not just a missing test.

Complete the remainder as two focused implementation assignments, followed by
qualification. Keep #168/#169 and their full acceptance requirements. No new cluster,
blanket architecture rewrite, feature expansion or completion-by-test-count target.

| Assignment | Primary ownership | Deliverable |
| --- | --- | --- |
| E: physical metadata | C1 object/inode_leaf grammar; C2 encoding/pool, sqlite/pool, required schema/owner/read/pack wiring | Supplied canonical leaf -> pooled FULL/DELTA -> real save -> reopen -> exact canonical readback |
| D: stored-tree edits | C1 file/edit, mapping traversal and FileView; external edit oracle/tests | Reference-equivalent split/concat/coalesce with stored subtree reuse and proved bounded final-only output |
| Qualification | Existing verification mechanisms, targeted tests, evidence and completion report | Matched successful costs, memory/storage evidence and separate issue acceptance |

With E implemented, start the dedicated D assignment and separately finish pooling
coverage/qualification. D does not wait for those checks; independent pooling
measurements do not wait for D. Combined edit qualification does wait for exactness.
If separate workers are explicitly dispatched, pooling owns object/storage work
and D owns file algorithms. One integration owner handles shared reexports,
error/policy types, lockfile and reports. Nobody reverts another worker's work.
Measurements/builds still obey the resource lock.

Checkpoint boundaries produce reviewable work; they do not authorize ending an
implementation assignment with its required remainder quietly deferred. If an
external run limit interrupts work, preserve a precise continuation (commit/tree,
remaining files, failing case and next action). A run limit is not a technical
blocker or completion. Keep the same acceptance contract on continuation.

## 2. Assignment E: finish metadata pooling

The following describes E's required implementation, now present according to the
completion report. Audit/fix remaining behavior and follow the dedicated
[pooling prompt](stages-3-4-continue-pooling.md) for coverage/qualification; do not
rebuild the component. Preserve payload delta/edit paths and visibility fixes.

1. **Canonical input and pooled FULL round-trip.** Port the checked compact inode
   leaf/value grammar from the pinned reference, including exact header/row layout,
   namespaced meaning where applicable, field checks and references. In C2, assign
   first-encounter ordinals, build the exact value-group body, compute its digest
   before compression, persist catalogue and leaf records atomically, then reopen
   and reconstruct the exact original canonical bytes/ID. Use supplied leaves:
   directory/tree construction and Workspace integration are unnecessary here.
2. **Exact bounded reuse.** Implement the specified Store-owned ordered set with
   reference-equivalent whole-group reset/window and smallest authenticated equal
   ordinal. The fingerprint is only a filter; compare all value bytes. Include
   pending same-save reuse, cold/reopen synchronization, invalidation on failure,
   catalogue chronology and reader visibility. Do not substitute a new persistent
   index table, shrink the retained window or retain unbounded canonical leaves.
3. **Pooled COPY/INSERT and failure completion.** Implement the distinct existing
   metadata delta grammar/selection, group compression, iterative reconstruction
   and intermediate authentication. Enforce its own depth/work bounds; payload
   PREFIX is not a substitute. Complete selected-base/value-group dependencies,
   known-owned failure cleanup, pack chronology and final acknowledgement.

Use the original file plan: `object/inode_leaf.rs`, the real `encoding/pool/`
implementation files, `sqlite/pool.rs` and only necessary existing integration
files. Helpers belong with their responsibility; no general pooling framework.
Reference grammar/algorithms must be read completely before porting them.

Required tests are `inode_leaf`, `metadata_pool`, `metadata_pool_index` and the
relevant existing physical-format, locator, visibility and failure tests. Establish:

- One new leaf round-trips through real SQLite and authenticated reopen.
- Repeated values within/across leaves and saves choose the same ordinals as the
  reference; low/high reuse, 165-value boundaries and retained-window resets work.
- Pooled FULL and winning/losing COPY/INSERT paths preserve exact canonical IDs.
- Bad digest/catalogue/ordinal/base fails once; cleanup leaves retained content
  readable and ordinary readers cannot observe private value groups.
- Store-owned index and query/decode transients have a justified simultaneous bound.

Each stage above must end with a real production round-trip. A role enum, framed
empty pack or generic byte codec does not satisfy the metadata requirement.
All three steps are required before calling E implemented; resource/performance
qualification and any remaining #168 criteria still apply before issue closure.

## 3. Assignment D: replace mapping reconstruction with stored-node COW

Implement checkpoint D of the original handoff; keep the existing real small-file,
transition, replacement-source, timer and storage boundaries where correct.
The reference is `crates/layerfs-content/src/file/rope/edit.rs` and its state/read/
build/codec dependencies at the pinned v0.1.6 revision. Read the complete relevant
implementation and all callers; preserve its algorithm, not merely helper names.

### First validate the executable oracle

The oracle now exists. First align both sides' current-result edit inputs as
specified in the [D prompt](stages-3-4-continue-d.md); a harness-only accumulated
offset cannot diagnose product semantics. Retain/add small external cases that
distinguish the current implementation from the required one. Generate expected
bytes, exact roots/partitions and relevant subtree identities using the sealed
reference in a separate process/fixture workflow.
Do not link legacy runtime code into candidate production or copy the candidate
algorithm as its own oracle. A fresh full construction is not generally the oracle
for the reference's localized-edit partition.

Start with unequal leaf joins (including 80+100 -> 90+90), a one-range change inside
a multi-level tree with untouched siblings, and height growth/root collapse. Then
cover the admitted normalized multi-edit stream and its current-result coordinates.
The existing canonical-page test is retained as a validity test; it cannot be
renamed into reference-root or finality proof.

### Implement the actual tree operations

Use a private representation of two states:

```text
Stored subtree: ObjectId + checked summary
Unfinished subtree: owned decoded entries + checked summary
                          |
        split affected paths / concatenate join boundaries
                          |
 reuse untouched stored children; retain unresolved boundaries
                          |
     prove node final -> seal children -> encode/hash -> emit
```

Preserve reference root/non-root context checks, cumulative offsets, same/different
height joins, coalescing, half partition, child ordering and collapse. A stored
subtree may stay as an ID until a split or join actually needs its boundary.
The desired large->large path must not enumerate every retained extent to rebuild
the mapping. Keep complete construction for genuine complete input and required
representation conversion; it is not a fallback for failed/localized edits.

Do not port both encoded draft maps as a second production mode. An external test
oracle may use the reference's existing machinery; replacement production uses
the one chosen representation and algorithm. Remove the large->large full mapping
walk once its replacement passes the proof gates; no runtime switch or error
fallback keeps both routes alive.

### Prove finality and memory together

For each emission rule, state why no remaining admitted edit or future join on
either side can alter that node. A node being valid/full or lying before an edit
offset is not sufficient. Model same-height repartition and unequal-height joins,
including later root collapse. If more context is needed, retain it as a bounded
decoded boundary; do not emit speculative objects and prune them afterwards.

Give an explicit live-state bound over supported height/fanout, both boundary
paths, replacement-builder levels, pending output and authenticated read state.
Charge capacities and transient coexistence. A stream of edits must not retain
one unfinished tree per edit. Neither passing a few cases nor stopping emission
until all edits end establishes this bound.

Required gates:

- Exact reference root and partition for the same base/profile/admitted edit stream.
- Unchanged old roots stay readable; untouched subtrees retain IDs where the
  reference does so; emitted new objects are reachable from the final root.
- Instrumented external providers/consumers show path/boundary work rather than
  a full mapping walk for eligible localized cases. Keep unavoidable no-op reads
  and replacement scans visible; no unconditional O(edit bytes) assertion.
- Deterministic join/fanout/height tests, multiple edits, no-ops, transitions,
  slow consumers and late failures pass with bounded simultaneous ownership.
- C1-only and integrated timing use that same production implementation.

If a finality rule fails, keep its exact counterexample and revise the rule before
implementation continues. A bounded-memory full mapping reconstruction does not
satisfy this gate, and reference equivalence cannot be removed from #169.

## 4. Qualification is a third missing deliverable

Qualify each component after its own correctness gates. Independent pooling
qualification does not depend on D. Final combined edit qualification requires
D's corrected oracle and implemented algorithm. The integration owner verifies
whether earlier component evidence remains eligible for the final source seals.

The current verification document has smoke-sized cases, leaves cache state to
later per-case declaration and has no collected matched reference campaign. Its
listed cases do not cover the missing pooling families or establish exact localized
reference partitions. It is insufficient by itself to close the original gates.

Before new benchmark work, append a clearly versioned specification/addendum with
exact cases, source/harness/oracle identities, cache/index state, acknowledgement,
correctness/resource/numerical gates and budgets. Preserve old smoke receipts and
their interpretation. Reuse the existing harness; no new generic benchmark system.

Required focused coverage:

| Area | Compare/observe |
| --- | --- |
| Pooling | New/reused values, high/low reuse, index turnover, cold/reopen synchronization, pooled FULL/DELTA, total DB/pack/index footprint and memory |
| Stored-tree edits | Fixed-size edit in multiple physically realized tree sizes, single/multiple edits, boundaries/root collapse, exact roots, nodes read/encoded and temporary ownership |
| Integration | Successful default and accepted override cases, full save-to-ack/readback, copy/query/codec/assembly work and enabled/disabled timing |

Collect actual process/allocator/SQL memory with declared scope and coverage where
required; capacity assertions alone cannot be renamed observed peak memory. Compare
equivalent successful v0.1.6 operations only. Unmatched public surfaces are explicit
diagnostics; they cannot establish an existing-or-better claim. Use the repository's
single-sample, cache, worker, lock, append-only evidence and lifecycle-budget rules.
Do not postpone a Stage 3–4 acceptance requirement merely by relabelling it Stage 6.

Audit the other reported deviations before closure, including same-save delta-base
eligibility and admitted-FULL cache candidate quality. These are not automatically
acceptable because they produce correct bytes: show unchanged required policy or
the explicit decision and matching storage/performance evidence for a change.

## 5. Completion discipline

Use affected core tests, examples, boundary/LOC-tool checks, formatting and Clippy
directly under current rules. Never run/restore tools/preflight.sh or an equivalent
aggregate gate. Preserve no retry/fallback/fsync/WAL, third-party pins, external
tests and the 999/200 physical-line rules. Report exact per-commit production LOC,
per-file/directory actual versus estimates, commands and every remaining gap.

Append a completion report linking the independent E and D proof results and the
qualification receipts. Use the [reviewer prompt](stages-3-4-reviewer-handoff.md),
including the supported-limits audit. Close each issue only when every criterion
is met; fixing these two implementation gaps alone does not establish all criteria.
Keep the release parent and Stage 5+ work open. This assignment creates no runtime
fallback, history/Workspace architecture or unsupported performance claim.
