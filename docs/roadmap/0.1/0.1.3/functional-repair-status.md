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


## Current checkpoint: 2026-09-04, read-only pin repair runtime-confirmed

Phase 1 is **not terminally complete**. The latest candidate is
`30d13deeec72b46ff7bc411f1ec08a46990541e1`, incorporating the read-only pin repair
`957b40c7b59fb932ad1e0198b68c515f20168d01` and the expanded qualified fast verifier.
Its sealed build and aggregate fast-verifier qualification passed. The real contention proof
passed in `workspace-shared-path-contention-proof-s1-verify-278b3a754568`; independent
report validation qualified all six new proofs with zero issues or violations. Original failures,
invalidated timings and earlier source identities remain preserved.

| Evidence category | Current disposition | Required next evidence |
| --- | --- | --- |
| Performance | 373 eligible slots had passed before the pin repair. Exactly 328 timings are retained under the reviewed operation-path/source predicate; 42 replacement samples passed; directory-content-scan-100 seed1 hit the15-second budget and seeds2/3 were suppressed. Active performance scope is370. | Independently validate42 new samples and the separate retained budget failure; historical45 timings remain original-source only. Preserve the old 45 observations as original-source evidence. |
| Routine verification | 48 full proofs are retained: 43 new-family and five inherited capped cases. The remaining302 active routine slots may use the authorized qualified fast profile. | Qualify the expanded implementation and collect those302 fast proofs with independent changed-content/metadata/alias checks, authenticated references where reused, and explicit skipped-read scope. Fast results remain `fully_verified=false`. |
| Targeted verification | 20 targeted proofs are qualified, including five source-78 recoveries and repaired contention on30d13dee. | Complete the nine remaining targeted cases; the six new independently validated receipts are sealed in `qualification/78-additional-reliability-checkpoint/`. The CDC boundary and reliability suite retain their targeted gates. |

The [runtime suppression policy](phase-1-runtime-suppressions.md) keeps15 disabled
case definitions for Phase 2. The [fast-verification amendment](phase-1-fast-verification-amendment.md)
and [reference assurance contract](phase-1-fast-reference-qualification.md) authorize
routine fast acceptance; they do not turn a preparation seal into verified input,
waive targeted error/resource/cleanup checks, or authorize relabeling skipped reads
as exhaustive verification. The retained 600-second sustained proof is reused once.

The five qualified source-78 passes are workload cancellation
(`c7a34133754e`), dirty runtime disconnection (`2e969fd5d84f`), corrupt descendant
(`3c8a1f12c1e2`), missing descendant (`fbe2a2784bad`), and parallel read/write
(`2f940d084aa8`). Their attempt directories under
`benchmark-results/fs-bench-pro/phase1-v013/attempts/` retain the original
raw outcomes and successful supervisor cleanup receipts; the separate qualified checkpoint supplies independent validation.

Two recent shared functional findings explain the current work:

- **Canceled descendants were dead but unreaped.** The original cancellation
  failure `workspace-workload-cancel-proof-s1-verify-b46498591d62` and the retained
  diagnostic `qualification/cancel-child-diagnostic-f5d1/process-observations.json`
  show the child becoming a zombie, adopted by daemon PID 1, and remaining past
  the unchanged ten-second disappearance gate. Repair
  `78d0f46d90744bbce729909cdf57f6eafe2eb9e6` reaps only the owned forcibly terminated
  process group after collecting its direct child's exact status. Its isolated
  Linux regression passed (`qualification/group-reap-78d0f46d/focused.json`). The
  corrected cancellation and disconnection runs are among the independently qualified passes above. Normal successful execution does
  not enter the new forced-group reap loop.
- **Read-only Open acknowledged an unretained inode.** The source-78 contention
  attempt `workspace-shared-path-contention-proof-s1-verify-56ec99b0b027` failed with
  ENOENT after successful Open. A concurrent replacement could remove the inode
  before the one-way read pin was processed, while Open had already succeeded.
  Repair `957b40c7b59fb932ad1e0198b68c515f20168d01` waits for the existing backend
  `Pin(node, false, false)` response before exposing the handle, preserving
  read-ahead and the other branches. The focused acknowledgement/NotFound test
  passed (`qualification/readonly-pin-focused/result.json`). Real contention
  recovery passed and was independently qualified; the 45 affected timing slots are being recollected;
  the change is not claimed to have equal instruction cost.

All `qualification/` and `attempts/` paths above are relative to
`benchmark-results/fs-bench-pro/phase1-v013/`. Issues #34 and #21 remain open.
The following sections preserve earlier source-specific checkpoints, including
now-superseded counts and next actions; they are historical records, not the current
campaign disposition.

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
