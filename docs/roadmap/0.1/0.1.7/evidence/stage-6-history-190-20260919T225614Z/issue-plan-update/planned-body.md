# v0.1.7: investigate and optimize retained-history measured-operation cost

> **Status: Current planning checklist; no release candidate exists.**
> We will follow the ordered measurement and optimization sequence below.
> This records intended work, not an implemented fix or a measured speedup.
> Product changes remain subject to the separate owner ruling in this investigation.

## Direction: measure, batch repeated lookups, reuse bounded records, confirm

On 2026-09-20 the owner requested documentation of this direction and an update to
this issue. The plan is drafted locally at:

`docs/roadmap/0.1/0.1.7/retained-history-optimization-plan.md`

It contains ASCII version comparisons, the lookup-batching example, source links,
correctness conditions and the completion checklist. The document and investigation
artifacts are not yet committed/pushed; this issue includes the actionable plan
directly rather than linking to an unavailable GitHub file.

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
Confirm on stride3; stop when the required outcome is satisfied
                  |
                  v
Only if evidence warrants: subtree summaries or save streaming
```

1. **Measure first.** Use the prepared harness-only phase instrumentation on
   stride10. Retain per-state exclusive spans and residuals, plus actual read
   waves/pages, validation, reference/spill and save counters. Logical lookup
   demand is not physical read count. Record missing fields instead of inferring them.
2. **First optimization candidate: batch singleton directory-parent lookups.**
   Deduplicate within the declared memory bound and use the existing `lookup_many`
   API so requests share ancestor acquisition. Product implementation follows only
   after its authorization and evidence support it.
3. **Reuse authenticated base records within the same operation.** Retain records
   for later uses under the identical base-root/visibility contract. Reuse validation
   results only when the effective edits and assumptions match. No unbounded cache
   or skipped cycle/alias checks.
4. **Prove and confirm.** Require identical canonical results and refusal behavior,
   reduced page/read-wave work and retained matched stride10 evidence; confirm on
   stride3. Preserve storage and resource requirements.
5. **Deeper tree work only if it still dominates.** Investigate avoiding sibling
   acquisition through sufficient authenticated child summaries. Current parents
   do not hold every fact needed for reuse/rebalancing; moving a guard above the
   read would not be a valid one-line fix. A format change needs a separate ruling.
6. **Streaming only if remaining cost warrants it.** Evaluate construction-to-save
   handoff after the above, preserving dependency order, visibility and bounded
   buffering. No additional construction workers or work moved outside timers.

These are evidence-driven steps: a falsified hypothesis is recorded and changes the
next experiment. We will not implement a speculative fix just to tick a box.

## What the investigation actually established

The retained selection is `history-stride10`, 17 states:
`range(1,158,10) union {157}`, on the same pinned corpus input history.

The original core trace already contains a coarse decomposition. Archived values,
independently re-derived from raw files, are:

| Core envelope | Exact nanoseconds |
| --- | ---: |
| Content | 1,115,152,085 |
| Store creation | 2,469,708 |
| Harness input assembly | 65,188,166 |
| Filesystem build/update | 23,520,347,667 |
| Save envelope | 7,769,904,041 |
| Unassigned state residual | 93,006,002 |
| **Sum of 17 state children** | **32,566,067,669** |

The exact historical v0.1.6 Commit sum is **11,370,679,212 ns**. The descriptive
excess is **21,195,388,457 ns**, ratio **2.8640389075994275**. This is an unmatched
historical tripwire, not a measured causal regression attributable to one mechanism.
Arithmetic closes with an explicit residual; internal tree/save attribution remains open.

Corrections to the earlier issue wording:

- **Both versions fetch sibling pages before their unchanged-subtree shortcut.**
  This is not a demonstrated new v0.1.7 algorithmic regression. Reference source:
  legacy `crates/layerfs-content/src/tree/batch.rs:609,680-710`; replacement
  `core/crates/layerfs-content/src/filesystem/sorted/merge.rs:180,239-278`.
- **Old Commit already includes content construction, directory/tree work and
  Store admission.** Old directory updates are charged to `Content`, so the old
  `Namespace` subtotal is not comparable to the whole new build/update envelope.
- **The recorded legacy topology is hybrid:** host coordinator, canonical
  construction and SQLite; container workload/FUSE. Commit is not container-owned.
- **Worker counts are not established as the cause.** Legacy predecessor plans
  explicitly force one producer; actual historical counts were not retained.
  Changing the environment variable alone does not parallelize the core raw driver.
- **No product arithmetic correctness defect or unconditional whole-base traversal
  per state has been proved.** The harness full-path scan is inside the input
  envelope above and cannot directly explain the entire excess.
- **Same corpus does not imply identical canonical object populations.** Legacy
  and core totals differ. Also, legacy retained-directory bytes and core database
  bytes have different scopes; this issue does not newly certify all storage tiers.
- Historical lifecycle walls have different boundaries and do not establish a
  paired end-to-end speedup. Existing receipts retain their original identities/statuses.

## Current execution status and limits

- Harness-only detailed instrumentation is prepared and built/tested: final build
  passes and 116 harness tests pass. Clippy still fails at 22 existing statements;
  formatting checks fail on existing styles. All logs are retained, without a CI claim.
- **Fresh detailed stride10/stride3 diagnostics: NOT_RUN.** The handoff says 15/25 s
  complete-command limits apply, while the lane specification lifts the generic
  limit but requires a prospective numerical ceiling. Proposed diagnostic-only
  120/240 s ceilings remain unapproved; this planning update is not a waiver.
- **Private codec-level and equivalent worker/index matched diagnostics: NOT_RUN**
  under current no-product-change scope. Changing caller hints does not isolate the
  Store's persisted-index cost.
- Original core binary/source custody is incomplete and cache residency is unknown;
  legacy declares uncontrolled OS cache. Archived copies are not fresh measurements
  or matched cold-performance evidence.
- One sample per case per arm, fresh output, append-only receipts, quiet-machine
  observations and measurement locks. Reuse setup/builds, never measured results.
  No best-of, cache warming, weakened validation or changed limits after a miss.
- Iterate on stride10, confirm on stride3, **never optimize stride1**. No reference
  rerun, 217-row qualification claim, raised worker count or release claim follows.

The retained-history time comparison remains the one-sided tripwire in
[`retained-history-storage.md`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/retained-history-storage.md)
section 8. Do not re-rule it on the strength of an unmeasured proposal.

## Completion checklist

- [x] Reconcile archived state timing and reconstruct the legacy Commit boundary.
- [x] Independently review raw arithmetic and preserve source/custody limits.
- [x] Prepare harness-only detailed phase instrumentation.
- [x] Document the ordered optimization direction and ASCII explanations.
- [ ] Resolve and record prospective diagnostic command ceilings.
- [ ] Collect fresh detailed stride10 evidence and identify the costly mechanism.
- [ ] Obtain the applicable product-change ruling; implement only the justified step.
- [ ] Prove unchanged semantics and reduced work with matched evidence.
- [ ] Confirm on stride3, retaining every non-passing line.
- [ ] Independent review reproduces final results and a disposition is recorded.

The issue remains open. A plan, successful build or archival arithmetic closure is
not a completed root-cause analysis or a measured optimization.

## Evidence

Local investigation root (not yet committed/pushed):
`docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-20260919T225614Z/`.
It contains `SYNTHESIS.md`, five squad reports, `REVIEW.md`, raw archived files,
hash manifests, checks, and `FOLLOWUP-VERSION-COMPARISON.md` with the source corrections.
Ledger entry: `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md`, L34.

Previously published evidence remains available:

- [Original re-check](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-recheck-20260920T000000Z/README.md).
- [v0.1.6 retained-history report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md).
- [Earlier tier account](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/ALL-THREE-TIERS.md)
  and [verification account](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/VERIFICATION.md),
  retained with the later investigation's missing-receipt qualifications.
