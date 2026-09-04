> **Superseded: Phase 1 verification has been withdrawn by the user.** See [closure decision](phase-1-verification-withdrawal.md) and [final performance report](phase-1-final-results.md). Earlier instructions and pending verification queues below are historical.

# Phase 1 functional repair implementation

The [completion amendment](failure-repair-amendment.md) requires these repairs
before terminal pass. The original 84 performance outcomes remain preserved:
48 raw passes and 36 raw failures, all with their original producing identities.
Corrected collection is in progress; the source-bound checkpoint below distinguishes
confirmed repairs from remaining failures and verification work.

| Shared cause | Minimum repair | Focused qualification |
| --- | --- | --- |
| Structural Commit exceeds final-delta budget | Apply changed directory edges and inode records directly, preserve unchanged roots, add references before removal, traverse only deleted subtrees, and bound batches/cursors/transient paths inside the unchanged budget. | One low-budget namespace regression covers creation, rename, deletion, aliases, exact membership and retained subtree identity. The final stable-source check also rejects a valid long path whose temporary planning charge exceeds a custom limit. |
| Deferred writes exceed aggregate piece allocation | Represent one contiguous offset-zero spool extent with an inline length charged as eight bytes. Fragmented edits promote to the existing fully charged nodes. | One regression covers 100000 records/800000 charged bytes, range semantics, promotion, snapshot preservation, truncation and unchanged error/limit gates. |
| Proxy cannot deliver wide directory | Stream indexed directory response fragments, retaining the 16384-entry per-frame cap and aggregate encoded-byte ceiling. Reassemble the same directory snapshot before existing FUSE pagination; preserve small-response bytes. | One codec regression checks 32002 entries on both directory routes, ordering/metadata, actual frame counts, and malformed/truncated/over-budget streams. |

No file-size, logical-total, memory, spool, Store, evidence or deadline gate was
raised. No workload was shrunk and no benchmark-specific product bypass was
added. Required failure propagation and expected-error oracles remain intact.

The corrected representation changes `PieceTree` from 16 to 24 bytes,
`FileData`/`Data` from 104 to 112 bytes and cached `Node` from 192 to 200 bytes.
Consequently even the 24 read/stat slots have changed lifecycle memory layout.
The other 60 existing slots execute changed write or structural paths. All 84
old outcomes remain valid **original baselines**, but all required corrected
candidate collection; none are relabeled as new-product evidence. Their input
bytes, independently specified oracles and compatible pristine Store preparation
remain reusable. No final verifier or remaining family had run at this point.

The earlier nine-slot observation-only recollection plan is superseded by this
functional-repair invalidation. Candidate rows use the explicit `corrected`
source arm. Reports retain original and corrected arms separately and forbid
PHASE1_TERMINAL_PASS while any required candidate gate remains failed.

The build pipeline now checks cached host depfile source contents against the
requested checkout and invalidates only changed Cargo packages. It retains
unchanged dependency artifacts. The original 712-to-4c host reuse is disclosed:
176 of 177 source inputs were byte-identical; the sole change was excluded by
Linux cfg from the macOS host target. Later target-relevant changes cannot reuse
that old host binary. Failed build/compile attempts and the source-unstable
intermediate namespace test are retained separately; none are called qualified
passing evidence. Each subsequent source/image build is separately authoritative for its runtime work.

Qualification evidence is under
`benchmark-results/fs-bench-pro/phase1-v013/qualification/`, including
`structural-frontier-final-stable`, `compact-spool`,
`proxy-directory-fragments-recheck` and `host-cache-provenance`.


## Current checkpoint: 2026-09-05, targeted gates complete; routine fast collection active

Phase 1 is **not terminally complete**. All **370 active performance samples**
and **29 targeted proofs** are independently qualified. Routine coverage has
48 retained full proofs and 103 qualified fast proofs at this checkpoint;
remaining history and other routine fast checks are still executing.

| Evidence category | Current disposition |
| --- | --- |
| Performance | 306 observations retain source `7948df2d`, 30 retain `30d13dee`, 33 use `e0922904`, and the corrected CDC deletion seed uses `e24a3b34`. Exact source/input compatibility preserves unaffected timings; no old result is relabeled as a new-binary measurement. |
| Reliability and CDC boundaries | All 28 reliability subcases plus the separate CDC boundary aggregate are qualified. The original 600.003326232-second active proof and its 123983 cycles retain their original source and VM4 environment and were not rerun. |
| Routine verification | 350 active slots require accepted correctness evidence: 48 retained full proofs plus 302 fast slots. Fast assurance is `fast_iteration_verified`, never `fully_verified`. All 302 unrun exhaustive counterparts remain explicit Phase 2 work. |
| Suppression | 15 exact case IDs remain disabled, with definitions preserved for Phase 2. The new directory-content-scan-100 seed-1 attempt failed at 15.000758416 seconds; its other two seeds were not run. This is not a passing result. |
| Issue disposition | Children #22, #23, #26, #29, #30, #31 and #34 are closed with published evidence. Remaining children and central #21 are open. |

The [fast-verification amendment](phase-1-fast-verification-amendment.md) and
[reference assurance contract](phase-1-fast-reference-qualification.md) govern
routine acceptance. They do not waive expected errors, resource limits, cleanup,
authentic input identities, or the independent checks for content and metadata
covered by the selected profile.

Recent repairs are confirmed, rather than still under investigation:

- **Cancellation/disconnection cleanup:** source `78d0f46d` reaps only owned,
  forcibly terminated process groups after collecting the direct child's exact
  status. The focused Linux check and real cancellation/disconnection proofs
  passed. The original zombie-child failure remains preserved.
- **Read-only Open acknowledgement:** source `957b40c7` waits for the backend Pin
  response. The focused acknowledgement/NotFound test and real contention proof
  `278b3a754568` passed. Exactly 45 affected timing slots were revisited: 42 passed,
  one triggered the existing suppression policy, and two were consequently not run.
- **Replacement Rename attributes:** source `e0922904` invalidates the overwritten
  inode's cached attributes and preserves cached same-inode no-op names. The
  focused check and real hardlink proof `9c319a18a1b8` passed; the original
  `af050e843eb8` failure remains intact. All 33 affected timings were recollected
  and independently qualified, retaining the other 337 observations.
- **CDC deletion fixture collision:** the [recorded fixture correction](findings/cdc-delete-offset-collision.md)
  changes only deletion tier 500, seed 3: ordinal 471 moves from offset 359004 to
  359005. The uniqueness gate is unchanged. The 1500-offset check and the selected
  performance/fast pair passed on `e24a3b34`. Its old timing remains historical
  and recipe-invalid; the original `bf6d01cc7939` fast failure remains failed.

Independent receipts are in `qualification/78-additional-reliability-checkpoint`,
`30d-additional-targeted-checkpoint`, `hardlink-final-checkpoint`,
`readonly-pin-performance-checkpoint`, `rename-cache-performance-checkpoint`,
`cdc-delete-collision-final-checkpoint`, `mixed-final-checkpoint`,
`cdc-family-final-checkpoint`, and `cross-family-final-checkpoint`, relative to
`benchmark-results/fs-bench-pro/phase1-v013/`. Source selection is sealed in
`qualification/fast-source-binding/1788538675483490000/receipt.json`.

Prepared-cache capacity stops remain unexecuted attempts. Explicit maintenance
archived custody/build metadata and removed only completed disposable masters;
future reference inputs and failed diagnostic state were preserved. The 24-GiB
cap was not raised.

The sections below retain earlier checkpoints and their then-current counts as
historical records. They do not supersede this current disposition.

## Historical checkpoint: 2026-09-04, d1325d7f dense-content recovery

The checkpoint has **187 compatible performance passes out of 390 prescribed
new-family seed slots**: 187 unique slots have executed successfully and 203
remain unexecuted. The earlier dense-rewrite failure remains historical evidence. The source-compatibility mappings
are prepared but not fully reviewed; these counts do not establish final evidence
eligibility or terminal pass. The original 84 outcomes remain separate history.

| Family | Compatible performance passes | Remaining work |
| --- | ---: | --- |
| Payload create/read | 24/24 | 23 remaining independent verifier slots; retain the original passing payload-create-1m seed-1 proof under its source identity. |
| Tiny-file churn | 60/60 | All 60 current independent verifier slots. The earlier bulk-delete-500 seed-1 proof is invalidated by the later unlink repair and cannot be reused. |
| Directory construction/traversal | 36/36 | 36 independent verifier slots. |
| Git-tool workflow | 12/12 | 12 independent Git semantic/custody/full-tree verifier slots. All prescribed performance seeds passed on e7840da1. |
| Namespace mutation | 12/12 | 12 independent verifier slots. All three namespace-500 performance seeds passed on a40b17e0. |
| Workspace change locality | 43/48 | Finish the remaining two dense-rewrite-100 seeds and three dense-rewrite-500 seeds; all 48 verifier slots. |
| Mixed workload and remaining timed dedup/history families | 0/198 | Collect performance before their full verification. |

Two source-bound proofs are retained: payload-create-1m seed 1 and the sustained
600-second reliability proof. Thus one of 390 timed verifier slots and one of 28
reliability subcases have retained passing proofs; the CDC boundary proof and five
inherited capped verifiers remain outstanding. Retention remains subject to the
reviewed compatibility mappings and does not relabel either proof's source.

## Confirmed repairs and preserved failures

- **Unlink visibility: repaired and runtime-confirmed.** The original Git10
  seed-1 FAIL remains at `attempts/git-tool-10-s1-performance-5abd0cdea1ba`.
  Repair `0763fac6` publishes uncached-parent unlinks before acknowledgement,
  retaining cached-parent batching. Its focused regression failed before the
  change and passed afterward. The selected 34224330 recovery passed; all 12
  Git performance slots subsequently passed on `e7840da1`. The original three
  Git passes and 24 tiny-deletion slots were recollected; the old bulk-delete
  proof remains invalidated. See [the finding](findings/git-unlink-visibility.md).
- **Sustained edit generations: repaired and runtime-confirmed.** The original
  proof at `attempts/workspace-sustained-600s-proof-s1-verify-c3db3ad3ff04` failed
  after 11.138 seconds with worker EINVAL and peer disconnection. Those values
  describe the failed attempt only. Repair `101626e7` retires an edit generation
  after a successful nonempty-to-empty transition while preserving the nonempty
  4096-edit gate. The corrected `e7840da1` proof at
  `attempts/workspace-sustained-600s-proof-s1-verify-01219f621176` passed with
  123983 completed cycles and **600003326232 ns of actual active work**.
  Its complete dispatch/verification/recovery window was 604241658583 ns;
  `qualification/sustained-recovery-e7840da1-validation.json` has no issues or
  violations and confirms verification PASS. Retain this proof once.
- **Large preparation spill lookup: repaired and runtime-confirmed.** The original
  namespace-500 seed-1 preparation hit its unchanged 1800-second gate before
  workload execution; it remains unexecuted/not-run. Seed 2 was interrupted
  during preparation and is also not-run. The retained one-second stack sample
  identified repeated linear spill lookups after the memory index was discarded.
  Repair `a40b17e0` keeps the 64 MiB threshold and transitions to a bounded
  temporary SQLite offset index. Its focused boundary test passed. Corrected
  namespace-500 performance seeds 1/2/3 passed in attempts `6a046f9b5838`,
  `18608b6eec7b`, and `13cbd2dc256b`, with preparation walls of
  127.030938416, 129.223309542, and 138.062860708 seconds respectively.
- **Dense content-only delta: repaired and runtime-confirmed.**
  `workspace-dense-rewrite-100` seed 1 failed Commit after successfully rewriting
  20000 existing files/100 MiB in
  `attempts/workspace-dense-rewrite-100-s1-performance-30c547026e31` on a40b17e0.
  The content-only planner charged the entire path set above 8 MiB and recorded
  202361622 node probes before rejection. Repair
  `d1325d7f44ef205f5fa748130f3b9868973e9edc` selects the existing bounded frontier
  before materializing an oversized content-only plan. The cap and long-path
  guards are unchanged; the focused 16-file/4 KiB-budget regression passed.
  The corrected d132 seed-1 execution passed at
  `attempts/workspace-dense-rewrite-100-s1-performance-7fb7938ebfff`.
  `qualification/dense100-seed1-recovery-d1325d7f-validation.json` returned no
  issues or violations and confirmed receipt/resource/cleanup validity. Recorded
  runtime was 55.872671458 seconds, preparation 7.377930583 seconds, and total
  sample wall 63.608725583 seconds. Independent final verification remains pending.

The next action is the remaining five locality performance slots, then the
remaining families’ broad performance collection. Full verification follows later.
Reports, publication and issue closeout are batched at checkpoints. The selected
builds, original failures, interrupted attempts and invalidated proofs remain
in their ledgers. No failed-attempt duration is used to claim a performance gain.
Shared source compatibility still needs its final review; all prescribed
functional, correctness, resource, cleanup and coverage gates remain required
for `PHASE1_TERMINAL_PASS`, and central issue #21 remains open.

## Resource checkpoint: d1325d7f

All three dense100 seeds and dense500 seed1 passed after the content-frontier
repair. The report checkpoint validates 190 performance passes and two retained
proofs, with no global source-map errors. Dense500 seed2 then failed the unchanged
2 GiB host RSS gate at 2,150,727,680 bytes; seed3 was not started. Its retained
Store shows publication completed before the failure in post-publication rebase.
The original failure is preserved at
`attempts/workspace-dense-rewrite-500-s2-performance-7eed48854a32`.
A minimal rebase lifetime repair is being qualified; runtime recovery is not yet
claimed. The watchdog exits before some operation receipts are drained, so the
failure also retains that evidence incompleteness. The report now recognizes the
exact RSS event and exit125; the changed row alone was revalidated. Full campaign
verification and terminal qualification remain pending.

## SQL-history root cause and genuine performance invalidation

The second dense500 seed2 attempt, on d6fdf964, still crossed the 2 GiB RSS gate.
The rebase copy reduction alone was insufficient. A 57-second diagnostic reused
its already-published Store without repeating fixture preparation or the FUSE
writes. Heap analysis identified an unbounded `SQL_TRACE` vector enabled by the
benchmark's test-instrumentation feature. SQL strings plus vector capacity growth
explain 99.97% of the observed heap growth during rebase.

Commit 8278d817 makes SQL history explicitly opt-in through the existing test
reset function. Query counters and fault features remain enabled. The focused
trace-contract test passed; the identical rebase diagnostic's RSS checkpoint
peak fell from 1,333,968,896 to 234,651,648 bytes. This is a causal diagnostic,
not yet the public dense500 runtime gate. Both prior resource failures remain.

The trace recorder allocated monitoring data inside timed product operations,
contrary to the frozen monitor exclusion. All 191 previously selected performance
passes therefore require clean recollection; they remain preserved at their real
sources as contaminated diagnostics. This is a shared measurement defect, not a
routine rerun of passing work. The two actual independent correctness proofs are
reviewed separately. Their checks are not automatically invalidated by timing
purity. Qualified canonical input Stores and native Git reference data remain
reusable through the exact producer-source checks committed in 6c54f8d7.
