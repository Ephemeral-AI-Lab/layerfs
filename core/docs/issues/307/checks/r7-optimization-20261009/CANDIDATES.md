# R7 candidates

> **Status:** Current planning checklist; no release candidate exists.

Seeded from optimization handbook section 9 at initial product `2b4dc28a6`.
Historical counts are hypotheses to diagnose at the authentic baseline, not new
timings. No candidate is accepted yet. Every kept change must reduce counted
work, pass affected correctness/count/scaling proofs, add no resident allowance
or input-sized state and add no logical/allocated Store/overlay bytes.

| ID | Candidate / group | Counter and historical cause | Risk / affected proofs | Authority / outcome |
| --- | --- | --- | --- | --- |
| H01 | Batched sealed-record read / H | 19076/21182 Commit jobs are record jobs; reduce point round trips to bounded windows | Sealed identity/custody; captured_namespace, product_commit_cost, mounted_commit | Owner 2026-10-09 permits interface changes; pending baseline |
| H02 | Group Content membership questions / H | About 16 is-new and 11 has-header questions/new inode | Replay/validation; filesystem_topology_backed, captured_namespace | Owner direction permits; pending |
| H03 | Producer sorted answer windows / H | Repeated membership questions; expected fewer point reads | Exact seals/windows; captured_namespace_cursor/custody | Pending; alternative to H02, not duplicate cache |
| H04 | Compare inode metadata before patch / H | Patch offered for every captured row | Metadata/identity; captured_namespace_metadata, mounted_install | Pending |
| H05 | Sorted-merge sibling reads / H | 70–86 inode pages/one-file update; grows with base | Canonical tree/reference semantics; Content and Init proofs | Pending; scaling before constants |
| H06 | Narrow territory gate / H | Unrelated mkdir opens moved-directory subtree traversal | Alias/cycle validation; filesystem_topology_model/backed | Pending |
| C01 | Local READ without Store reader / C | Every local READ takes lease | Read/capture custody; mounted_parking/coherence, filesystem_port | Pending |
| C02 | Combine source/facts/release jobs / C | About 5 jobs/request | Atomic permissions/source/custody; native_jobs/coherence | Pending |
| D01 | Status without refused Lifecycle observation / D | 297/310 engine observations refused during R6 Commit | Bounded read-only control and additive wire | Pending |
| D02 | Commit/filesystem admission ordering / D | 30327 jobs during one Commit | Fairness/no starvation; finite_service, mounted_concurrency | Pending |
| B01 | Negative entries/READDIRPLUS/FLUSH / B | Unmeasured at product arm | Immediate namespace/permission coherence; native_coherence/mutation | Owner permits with proof; no TTL/depth increase |
| B02 | Wider directory replies / B | One 64-name window/reply | Exact cookies/custody; native_directory/mounted_install | Pending; no buffer enlargement |
| I01 | Save transaction/batch/pack-search cost / I | #313 hypotheses, current counts needed | Save finish/publication failure; storage/persistence/history | Pending |
| G01 | Partial-cell copy and cache-hit cost / G,F | #313 hypotheses, current counts needed | Exact fragments/truncate/bytes; Workspace and storage | Pending |
| H07 | Parent pointer/ancestry evidence / H | Moved stored directory may list subtree | Changes canonical format and adds stored bytes | PROPOSED ONLY; forbidden in this run, counted benefit pending |

Candidate details use the handbook template: cause sentence with measured units,
counter start/floor/end, growing variables, actual EXPLAIN and runtime profile,
proof binaries/count tests, memory/disk gates and immutable receipts. An
unavailable instrument must be named before diagnosis is closed.

## New count evidence during Stage0 (not a timing baseline)

Host debug/optimized-test captured_commit atcurrentproductb8d76c0a3,
105-host-suite-013-captured_commit, reports a fixed14inode/10entry change over
base310/1738/13162entries. Ownerjobs538/566/538 andstatements2286/2370/2286 are
nearconstant, but Content inodepages8/36/206 andbytesread30513/148953/837817
followunrelatedbase size. Peak scratch103646/320972/667280 also rises.
H05 remains an open countedscalingcandidate; noactualmountedperformance claim,
noedit/acceptance beforeStage0matrix. Need locateunrelatedsiblingread source and
writtenfloor accountingcanonical searchpages before planning afix.

105-host-suite-018-captured_namespace_cursor reports freshfilechanges40/160/640:
recordjobs204/3151/12358, NamespaceGet854/5698/22498. The40-to160 transition is
15.446x jobs for4xfiles;160-to640 is3.922x. It requires source diagnosis of
existing admitted-memory/backing transition versus a badfactor, not an immediate
superlinearalgorithmclaim. Exactraw counters andlimitation retained.

Correction of preliminary speculation above: read-only subagent source diagnosis
finds the cursor cliff is fixed64-entry Memo thrashing with backedrecords atall
sizes, ratherthan an in-memory-to-backed transition. Exactcountcause and bounded
alternative are being refined; noallowedcandidate isclosed onthis preliminary
finding. H05 diagnosis likewise mustdistinguish structurallyrequired authenticated
siblingheaders from repeated demands/decode/scratch; an assumedzero siblingfloor
wouldweaken existingvalidation unless sourceestablishes trustedexistingevidence.

C01:B:L original failed212 sample nevertheless retains AVAILABLE original phase counts:7000 FUSE requests/5000 handoffs/2000inline;18003 rawownerjobs,18002 after Resources observation credit,18001 after typedStatusState1Lifecycle credit (Lifecycle8000/Mutation3000/Read3001/Source4000);2001readergrants with0Storeobjects demanded and6033canonicalcachehits/0misses. This is a numbered C/D round-trip hypothesis to trace at source, not a kept optimization or eligible speed baseline. Warm-to-Mount1129maintenance jobs remain visible and preclude calling that interval isolated Mount.

## Returned H05 diagnosis at owner stop (unimplemented)

Same active parent narrowed source/algebra; independent existing child checked the descriptor-shift subquestion in reused context. No further analysis begins after owner stop. Source locations: Content filesystem/sorted/merge.rs231–296, page.rs452–565, format.rs383–430, object/inode_leaf.rs282–347; external filesystem_failure.rs599–623 preserves malformed branch-summary refusal.

The fixed edit has16 sorted inode keys (14 final values plus2 removals). Base unique inode counts306/1734/13158 derive from bulk+bulk/50+102; hardlink names share inodes. 100-row leaf splitting gives6/34/263 leaves. Large root has4 child branches with64/64/64/71 children. Actual inode pages8/36/206 reconstruct from7/35/204 required unique dependencies plus1/1/2 rereads. Objects13/41/211 additionally include5 directory pages; waves8/8/12 include5 directory waves. Sizes leaf44+81*rows and branch44+40*children reconstruct30513/148953/837817 canonical bytes exactly. Large excess is2604 branch+4094 leaf bytes; small/mid excess4094 each. Unique-ID/site trace is UNAVAILABLE, so these are source/formula-correlated occurrences, not a new instrumented duplicate count.

Current branch rows store(maxserial,childID), not per-child count/fill proof. Authentication alone does not prove child level/max/fill and aggregate equality; skipping unique child validation weakens an existing malformed-tree contract. New canonical summaries/ancestry bytes are forbidden proposals, not an authorized shortcut. Required fanout cost does not block separate waste reduction.

Pending zero-added-state/disk candidate: ordered batch consumers use .position()==0 then Vec::remove(0), shifting C(C-1)/2 descriptors. Reconstructed shifts15/561/6523; maximum8001 at inode fanout127 and32640 at general batch cap256. Consume the same Vec with into_iter().next() or reverse/pop while preserving identity checks, decode/check/edit/shrink/drop ordering, narrowing/lease/read_batch and the existing bound. This is neither a committed plan nor an implemented optimization.

Second pending candidate: inode_leaf decode validates each73-byte value into InodeValue but keeps rawrows; CompactInodes decodes again into a second typed Vec. A shared streaming grammar visitor may remove intermediate rawrow storage/second decode with the same framing/order/kind/length failure contract and canonical bytes. Existing PoolingLeaf still owns rawrow Vec; it is not already a nonallocating summary reader. Reusing rehydrated neighbors needs proof of an existing equivalent frame/lease; new caches/pages are not assumed allowed. No exact leaf-decode/move/site counters, allocator/phase RSS or VFS/device bytes are retained. Scratch103646/320972/667280 is simultaneous charged reservation under4MiB, not RSS. All candidates remain pending and unaccepted.

## Candidates opened after the resume (2026-10-09)

| ID | Candidate | Counter: start → floor | Outcome |
| --- | --- | --- | --- |
| C02a | One owner job for a mutation's ticket and source release | Owner jobs per created file in C01: 18 → 16 by this step; class floor is one job per handed-off request | KEPT, commit `9244dc8c6`: jobs 18002 → 16002, command 2226041875 → 2004655833 ns (receipts 223, 232), disk equal |
| C02b | Fold source acquisition into the first deciding job | 4 Source jobs per created file → 0 | Open; needs the plan to be built inside the job |
| C02c | Release a read-class request's source in its deciding job when no immutable round follows | 2 Lifecycle jobs per created file (LOOKUP, GETATTR) → 0 | Open; needs the custody proof that no consumer uses the source after the decision |
| C03 | Lease-family statements per job | 174 Lease executions per created file, about 10 per job | Open; needs the per-call statement list |
| B03 | GETATTR after CREATE | 1 of 7 requests per created file | Open; compare with the P arm before changing attribute validity |
| D03 | Per-transaction reservation syscalls | about 7 µs per job measured | Measured minor; not pursued now |
| C04 | Attribute-only requests (LOOKUP, GETATTR) retain no FileRead | Owner jobs per created file in C01: 16 → 15; deciding-job statements for getattr 25 → 13 | KEPT on counts, commit `952e0b3bb` (receipts 233, 241): jobs 16002 → 15002, statements 334017 → 302017, disk equal. Wall time not confirmed: 2004655833 → 2202119833 ns in a run where untouched work was also slower |
| D04 | Thread wake-up per owner job (owner idle between jobs; request task parked while the job runs) | Unmeasured: command minus owner wait and service was 59, 199 and 440 ms in three samples; queue wait 17–61 µs per job | Open hypothesis; no instrument. Fewer jobs per request is the allowed response; spinning or more threads is not proposed |
