# Retained-history measured-operation optimization plan

> **Status: Current planning checklist; no release candidate exists.**
> Tracking: [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190).
> Direction recorded on 2026-09-20: follow the ordered investigation and
> optimization steps below. The owner subsequently requested execution with
> subagents, authorizing the targeted batching and bounded-reuse product changes.
> Current execution evidence is recorded separately from the original plan below.

## Execution update

The [optimization campaign](evidence/stage-6-history-190-opt-20260919T232858Z/README.md)
freezes raw diagnostic ceilings at 120/240 seconds for stride10/stride3, with
uncontrolled cache residency and no cold/admission claim. Both fresh baseline
and candidate selections are retained. Batching and reuse are implemented in the
working tree and confirmed on both selections with identical Store bytes and
canonical roots. Exact results, checks, LOC and remaining qualification gaps belong
to the campaign and its independent review.

The first implementation was rejected before performance collection because
earlier reducer insertion caused eight previously successful quota cells to
fail. The revised implementation preserves reducer ordering, batches both lookup
passes, and reuses only the final bounded parent window. All 56 quota outcomes
match the baseline exactly. This is not an operation-wide all-parent cache.
See [steps 2–3](evidence/stage-6-history-190-opt-20260919T232858Z/STEP-2-3.md).

## Objective and evidence

Identify and reduce avoidable work inside the retained-history state operation,
preserving canonical results, validation, visibility, storage acceptance and
resource bounds. Iterate on `history-stride10`; confirm on `history-stride3`.
Never tune from `history-stride1`.

The [campaign synthesis](evidence/stage-6-history-190-20260919T225614Z/SYNTHESIS.md)
and [independent review](evidence/stage-6-history-190-20260919T225614Z/REVIEW.md)
retain exact archival accounting and its custody limits. The dominant recorded
envelope is filesystem build/update. It is not yet attributed to an individual
algorithm, and the cross-generation difference is not a matched causal effect.
Fresh detailed measurements remain outstanding.

Two corrections control this plan:

1. **Both versions acquire sibling pages before the unchanged-subtree shortcut.**
   This shared behavior is not evidence of a newly introduced regression.
2. **Old `Namespace` is not all old tree work.** Legacy directory updates execute
   inside `Content`; its namespace subtotal cannot be compared directly with the
   replacement's entire build/update envelope.

See the [version comparison](evidence/stage-6-history-190-20260919T225614Z/FOLLOWUP-VERSION-COMPARISON.md)
and [algorithm clarification](evidence/stage-6-history-190-20260919T225614Z/FOLLOWUP-ALGORITHM-CLARIFICATION.md).
The latter also retracts the unsupported inference that an upper-bound-only check
necessarily recurses into all prefix subtrees. No product arithmetic correctness
defect has been established.

## Existing flows

This diagram describes execution order, not proportional elapsed time.

```text
v0.1.6 workspace Commit              v0.1.7 replacement driver
======================              ========================
Before timer:                       Before timer:
  install + execute mutations         read changed corpus bytes
             |                                   |
-------------+-- timer starts       -------------+-- timer starts
             v                                   v
  capture changed workspace           construct changed content
             |                                   |
             v                                   v
  construct changed files             collect emitted objects
  stream into Store admission                    |
             |                                   v
             v                         assemble filesystem inputs
  dirty-directory/metadata updates                |
  [inside Content interval]                      v
             |                         validate + update directories
             v                         resolve references + inodes
  apply references                               |
  bounded inode frontier                         v
  sorted inode update                  accept collected objects
             |                                   |
             v                                   v
  finish admission + publish                    finish
```

Legacy has pending/spilled inode reuse and batched reference lookup, but also
singleton base lookups. Core already batches validation and some reference work;
the first candidate below targets its remaining singleton directory-parent paths.
Do not draw either implementation as uniformly efficient or uniformly unbatched.

## Ordered work and decision points

```text
Freeze diagnostic protocol and identities
                   |
                   v
Measure validation / directories / references / inodes / save
                   |
                   v
Identify costly repeated page acquisitions and read waves
                   |
                   v
Batch singleton parent-inode demands using existing lookup_many
                   |
                   v
Reuse authenticated base records within the same bounded operation
                   |
                   v
Prove unchanged results/errors + reduced work + matched time effect
                   |
                   v
Confirm on stride3; stop if the required outcome is satisfied
                   |
                   v
Only if evidence warrants: subtree summaries or save streaming
```

| Step | Intended work | Evidence required to advance | Current status |
| --- | --- | --- | --- |
| 0 | Freeze diagnostic command ceilings, cache declaration, identities and selection. | Prospective recorded limits; quiet-machine observations and measurement locks; every history switch recorded set/unset. | Frozen in execution protocol; ordinary gates unchanged. |
| 1 | Run prepared detailed stride10 instrumentation and decompose every state. | Exclusive per-phase sums with named residuals; validation, sorted-page, reference/spill, provider and save counters. Read-wave metrics must actually be observed, not inferred from logical demand counters. | Fresh baseline stride10/stride3 retained; actual provider work recorded. |
| 2 | Batch singleton directory-parent demands with existing `lookup_many`. Validated input already guarantees sorted unique parents. | Fixed shared-ancestor cases retain identical roots and refusal behavior while reducing acquired pages/waves; bounded batch memory; matched stride10 evidence. | Implemented and verified; diagnostics retained. |
| 3 | Retain and reuse authenticated base records through later uses in the same update; consider reuse of validation results only with identical assumptions. | Exact base-root identity, visibility and effective edits; no skipped cycle/alias checks; no unbounded cache; independent correctness proof and matched evidence. | Implemented as final-window reuse; no expanded validation memo. |
| 4 | Confirm the chosen change on stride3. | Exact identities, unchanged storage/correctness requirements, all measured phases and non-passing lines retained. | Confirmed diagnostic work reduction and identical Store bytes; verification target and admission gaps remain explicit. |
| 5 | If sibling acquisition still dominates, design lazy reuse of untouched subtree summaries. | Authenticated metadata sufficient for totals/fill/rebalancing; corruption checks preserved; explicit format/contract ruling if required. | Deferred pending evidence; not a one-line guard change. |
| 6 | If remaining allocation/copy/save cost warrants it, evaluate construction-to-admission streaming. | Correct dependency order and read visibility; bounded buffers; one construction worker; no work moved outside timers. | Deferred pending evidence; no additional worker proposed. |

Steps 2/3 are candidates, not obligations to change code regardless of results.
If step 1 contradicts the hypothesis, record it and revise the next experiment
instead of implementing an unmeasured optimization.

## First candidate: batch shared ancestor reads

Illustrative two-level tree; A, B and C share one leaf. Counts below are page
acquisitions, not disk reads, codec calls or a measured speedup.

```text
Existing singleton path              Proposed bounded batch
=======================              ======================
lookup(A) -> root -> leaf X           needed: [A, B, C, A]
lookup(B) -> root -> leaf X                    |
lookup(C) -> root -> leaf X                    v
                                     sort + deduplicate
3 root + 3 leaf acquisitions                  |
                                             v
                                     lookup_many(A, B, C)
                                             |
                                             v
                                          root once
                                             |
                                             v
                                         leaf X once
                                             |
                                             v
                                     reuse records A, B, C

6 acquisitions in this example       2 acquisitions in this example
```

The core [singleton helper](../../../../core/crates/layerfs-content/src/filesystem/update.rs)
is `lookup_base`; [lookup_many](../../../../core/crates/layerfs-content/src/filesystem/inode/read.rs)
starts each call at the table root. Reuse this existing batch API before designing
new cache machinery. Keep the batch and retained facts within the declared budget;
do not gather an unbounded whole-tree record map.

## Why sibling pruning is a later design step

```text
Shared current behavior in both versions:

                        root
             +-----------+-----------+-----------+
             v           v           v           v
           read A      read B      read C      read D
           check       check       check       check
           reuse       reuse       EDIT        reuse

Desired only with sufficient authenticated metadata:

                        root
             +-----------+-----------+-----------+
             v           v           v           v
           reuse A     reuse B     read/edit C  reuse D
           summary     summary                 summary
```

Current branch entries carry the key and child ID, while reuse/rebalancing also
needs child counts, byte totals, page size and item count. The engine obtains
and validates those facts from child pages. An early return without them would
remove required work, not prove a correct optimization. A persistent summary
format needs an explicit compatibility ruling; an operation-local memo only
helps where the authenticated facts have already been acquired inside that
operation. See [core page grammar](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs)
and [node reuse/validation](../../../../core/crates/layerfs-content/src/filesystem/sorted/page.rs).

## Measurement, correctness and scope

- Follow the [execution protocol](evidence/stage-6-history-190-opt-20260919T232858Z/PROTOCOL.md)
  and [lane specification](retained-history-storage.md). The original
  [pre-run protocol](evidence/stage-6-history-190-20260919T225614Z/squad-s5/PROTOCOL.md)
  preserves the earlier unresolved budget. Execution now has prospectively frozen
  120/240-second diagnostic ceilings; ordinary gates and verification targets are
  unchanged. No timeout is enlarged after a miss.
- One sample per case per arm, fresh output, append-only evidence, no best-of.
  Use the same instrumentation and declared cache contract across matched arms.
  Unknown or uncontrolled residency cannot support a cold claim. Reuse preparation
  and sealed builds, never measured results or a mutated Store as setup.
- Preserve input validation, authenticated reads, visibility, canonical roots,
  reference counts, ordering and memory bounds, cleanup and explicit errors. Test
  shared-parent lookups, absent records, rename/link/unlink and cycle/alias failures
  as applicable to the actual change; do not weaken validation to lower read counts.
- Validate correctness separately with exact identities. Record all FAIL,
  INCOMPLETE, INELIGIBLE and NOT_RUN outcomes. Existing Clippy/format failures stay
  visible; no CI or aggregate preflight claim.
- No v0.1.6 rerun, no 217-row qualification claim, no stride1 tuning, no raised worker
  counts or changed codec defaults in this plan. The historical comparator remains
  descriptive; any new optimization effect needs an actual matched diagnostic.
- The current owner instruction authorizes the targeted batching and bounded-reuse
  product changes, with issue updates and LOC accounting. It does not automatically
  authorize unrelated formats, codec/worker tuning or waived acceptance checks.

## Completion record required

- [x] Archive/reconcile original spans and reconstruct the historical Commit boundary.
- [x] Record version-comparison corrections and independent review.
- [x] Prepare and build/test harness-only phase instrumentation.
- [x] Resolve and record the diagnostic ceiling before collection.
- [x] Collect fresh detailed stride10 evidence and identify repeated provider acquisitions.
- [x] Obtain the applicable product-change instruction and implement batching/bounded reuse.
- [x] Prove unchanged semantics and reduced work with retained diagnostic evidence.
- [x] Confirm on stride3 and report storage, resources, correctness and timing separately.
- [x] Independent review reproduces the final measurements and all residuals.

At the parent-lookup checkpoint, remaining issue acceptance was distinct from
implementation completion: stride3 verification missed its target, history golden pins were absent,
cache admission is ineligible, and the historical tripwire is not declared resolved.

Close #190 only with measured attribution and a documented disposition. This
planned optimization sequence is not itself a closure or a performance claim.


## Pooled-read continuation — 2026-09-20

The owner directed continuation after parent-lookup PR#194. Three subagents implemented missing pooled-read telemetry, bounded physical-group reuse, and external correctness checks, with independent source/raw-evidence review. The existing512KiB session group cache is shared; no new cache, policy, codec, worker or format change. Details and exact measurements are in [ledger L36](../0.1.6/evidence/issue151-experiment-ledger.md#l36--190-pooled-physical-group-reuse-2026-09-20) and its linked evidence.

The candidate meets stride10 and stride3 verification targets in this diagnostic. Historical operation parity, history pins and eligible cache admission remain open. The preserved parent-lookup checkpoint retains its original misses. No subtree summaries, save streaming, signature reuse or new-row filtering were added. Remaining read costs need measurement before choosing another treatment.


## Bounded ROI round closed — 2026-09-20

The owner authorized one attribution step and at most one low-complexity change. Native sampling required no new production telemetry and identified repeated catalogue SQL preparation. Only `group_for` now reuses the existing bounded prepared-statement cache; SQL, current results and validation remain unchanged.

[Ledger L37](../0.1.6/evidence/issue151-experiment-ledger.md#l37--190-catalogue-statement-reuse-bounded-roi-round-2026-09-20) records the single-sample matched results, exact work proof and limits. Both operation samples improve, while stride10 complete-command wall regresses. Candidate verification meets both targets. The one-helper implementation is net −1 production LOC.

**Stop optimization here.** No broader data/value cache, pack-query treatment, summaries or save streaming is included or recommended by this round. Existing O3 pins, cache qualification and historical timer tripwire remain separate open acceptance work. Earlier receipts retain their original results and misses.

## Owner's one-second follow-up — 2026-09-20

The owner reopened exploration and clarified that **one second is a worthwhile saving**. The earlier multi-second screening rule does not apply to future candidate decisions. Three subagents explored and tested within-batch New-row filtering in zero-count reference processing, with unchanged authenticated validation and bounds.

[Ledger L38](../0.1.6/evidence/issue151-experiment-ledger.md#l38--190-zero-count-filter-tested-against-the-owners-one-second-bar-2026-09-20) records the one-sample pair, structural work proof, separate verification and independent review. The candidate failed the revised one-second criterion and was reverted with its temporary phase/test; archived patches and receipts remain. PRs #194/#195/#196 are unchanged. Both new allocation-target misses and the continuing O3/cache gaps are recorded plainly.

No stride3 or full-suite continuation for this rejected variant. Future one-second candidates remain worthwhile when justified by measured removable work and proportionate complexity; this experiment establishes no such next candidate. Subtree summaries and save streaming remain untested and unjustified by this result.

The owner additionally accepts small allocated-storage overages when they accompany worthwhile time reduction. Record the byte overage and time benefit together; preserve historical numeric misses and apply the conditional disposition. This does not rescue the rejected filter, whose speed difference is below one second.

## Legacy-code comparison — 2026-09-20

The owner requested one more subagent review of v0.1.6 `crates/` against current `core/`. [Ledger L39](../0.1.6/evidence/issue151-experiment-ledger.md#l39--190-legacy-code-review-and-next-experiment-ordering-2026-09-20) records source identities, three reviews, counter-evidence and the final recommendation: one isolated live group-compression-level pair next, with selective pack-range reads as the larger follow-up after fetch/parse attribution. Both experiments remain NOT_RUN. No default or product change was made by the review.

The earlier stop recommendation and rejected New-row filter remain historical decisions about their own rounds. The current recommendation reflects the owner's one-second and small-allocation-tradeoff rulings. It does not claim parity with legacy or remove the outstanding qualification gaps.

## Group-level live experiment — 2026-09-20

The owner authorized the isolated codec experiment. [Ledger L40](../0.1.6/evidence/issue151-experiment-ledger.md#l40--190-group-compression-level-1-live-pair-2026-09-20) records the matched stride10/stride3 pairs, time/space tradeoff, independent canonical-inventory proof, separate verification, validation and limitations. Retain group level1 while keeping payload level3 and all prior optimizations, workspaces, workers, integrity checks and bounds unchanged. Stale source comments and the storage architecture description are corrected alongside the constant.

The owner's one-second and small-allocation-tradeoff rulings apply; numeric allocation misses and O3/cache gaps remain visible. Pack-range reads remain a separate, unrun direction requiring fetch/parse attribution. No level sweep or stride1 optimization follows from this result.

## Data-access continuation handoff — 2026-09-20

The next agent should use the [#190 data-access, reuse-boundary and execution-pipeline handoff](issue190-data-access-handoff.md). It preserves the four merged improvements and latest measured baseline, prioritizes provider attribution and selected-group reads, and makes reuse/pipeline changes conditional on evidence. It also records the resolved lock protocols and the owner's instruction to stop cross-task communication. Creating this handoff runs no new experiment and changes no product source.
