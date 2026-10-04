# Phase 7 read optimization implementation and qualification

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Owner authorizes implementation and asks to qualify retained history in order **stride 10 (17 states), stride 3 (53 states), stride 1 (157 states)**. Authoritative checkout: `codex/phase7-cluster1-storage`, clean starting HEAD `1782b07eb149f6f8f81d4c675533e44ec49ebed6`; current product implementation `6b631aad2`. Active product is SQLite DB-only; historical #300 HTTP/MinIO mechanisms are not implementation scope.

### Stage 1: bounded whole-pack reuse

Replace clear-all overflow in Fetch/delta/pooled body acquisition with one concrete selective cache implementation. Preserve operation-local reader/save ownership, immutable descriptor binding, singleton/count policies, original output and lost-ID ordering, first-wins winner validation, dependency eligibility/cycles/work bounds, private-writer invalidation and atomic publication. Coordinate acquisition with bounded physical pack/group cohorts rather than prefetching an entire frontier larger than retention. Reuse the same authenticated acquired bodies and decoded groups during dependency discovery and reconstruction; avoid a new canonical-base cache or larger buffers.

The owner-required body cap is **2 MiB**. Current source says 4 MiB; tighten it explicitly and never describe historical receipts as having run at 2 MiB. Ordinary and pooled owner multiplicity plus transient acquisitions must remain visible. Existing decoded group/value bounds, publication bounds and construction workers remain unchanged. Necessary rereads (capacity, fresh owner, cold boundary, private generation and winner validation) remain distinct from redundant acquisition.

### Stage 2: backend-neutral partial BLOB read port, implemented by SQLite

Add narrow descriptor/offset/length read requests and a distinct immutable range result; do not use `PersistedPack` to attest partial bytes. SQLite uses existing pinned rusqlite `blob` support: read-only incremental BLOB open/read/checked close inside one transaction attempt, descriptor/BLOB-size matching, checked bounds, no retry or error-driven fallback. C2 owns layout planning and complete encoded compression groups/records plus delta/pooled dependencies; arbitrary canonical slices are not physical read units. Shared directory/decoder validation must preserve framing/domain/range/length/window and canonical identity refusal.

Integrity is a hard implementation constraint: existing pack SHA256 cannot authenticate unread bytes from a slice. Preserve the existing whole-pack read/authentication contract. Initial range plumbing must have an explicit strict strategy that still pays complete authentication inside the operation; optimized scoped reuse may rely only on real same-operation authenticated input under a bounded immutable view. Do not enable cold sparse range reads as equivalent whole-pack authentication, introduce unauthorised format/schema changes, or claim first-touch I/O savings without the required unit-authentication contract. If a stronger range treatment needs a format/contract decision, report its exact blocker and retain the safe implemented port rather than weaken checks.

### Qualification and attribution

Implement in the above order; focused functional checks cover each changed mechanism. Freeze committed clean source before performance. Then use the sole runner, one sample per case per arm, **stride10 -> stride3 -> stride1**. Reference remains unmodified Phase4.5 `7edddbdb8`; matched workload/harness/cold identities, locked release binaries, repository Cargo flags and worktree-local targets required. No unchanged speed retries. Candidate acquisition requires qualified same-harness reference pins; if a reference proof fails, retain the row and mark dependent candidate NOT_RUN rather than fabricate admission.

Performance complete-command caps stay **60/170/170 seconds** respectively; separate combined proofs stay **9.5 seconds**. Init stays 15/9.5 seconds. Cold source/database enforcement must not let setup, preceding states/phases/arms or output writes credit the measured reads. Reuse prepared fixtures/builds through declared seals and prescribed clone mechanics only; no wider caches, extra workers, larger buffers, timeout inflation or reduced workloads.

Measure useful/dependency bytes, body/range acquired bytes, requested/actual VFS bytes/calls, SQL and BLOB calls, decompression/hashing work, owner cache hits/misses/evictions, simultaneously live memory and acquisition/filesystem/save/lifecycle time. Nested spans are not additive. Tests cover sequential/sparse/random demands, duplicate order, crossings, malformed/truncated/mismatched data, dependencies/visibility, first-wins/private invalidation, exact reconstructed output and retained history. Final required core tests/examples, Clippy -Dwarnings, formatting, product boundary and guard self-tests; no retired aggregate preflight/CI claim. Every commit carries exact reproducible parent/staged production LOC comparison and architecture updates.

Evidence remains append-only. Latest53 at `6b631aad2`: lifecycle 69.065836125/74.695010625s, ratio1.081504471904 time PASS; combined proofs8.539828542 PASS/9.507007292 TIMEOUT => joint INCOMPLETE. Earlier Init/history17 passes retain older identities;157/Durable NOT_RUN. Projected savings are hypotheses, not measured equivalence to Phase4.5 or all-seven current-artifact admission.

## Stage 1 implementation and functional checks

Source parent `1782b07eb`. Selective PackCache, explicit 2 MiB cap, protected
physical-cohort admission/consumption, shared decoded-group prefetch, and sealed
ordinary-body reuse across acknowledged save waves implemented. Existing pooled
private-write invalidation, locator/race checks and singleton limits remain.
No Stage 2 range or speed/admission claim follows from this implementation.

Focused 3 cache + 2 locality + 2 immutable carrier tests passed before the final
shared-counter correction. Full core workspace/all-target test initially failed
one pooled work assertion: actual group decompression moved into shared prefetch
but pooled reader counters omitted it. Fixed attribution at the read owner; no
extra decode or integrity removal. Covering full workspace/all-target tests then
passed 479 tests across 88 targets, including examples.
Commands: `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
--all-targets`; owning workspace/all-target Clippy `-- -D warnings`; formatting
`--all --check`; `python3 core/tools/check_product_boundary.py` (444 files);
`python3 -m unittest discover -s core/tools -p 'test_*.py'` (23 tests);
`git diff --check`. Raw initial/repaired test and Clippy logs are retained under
`checks/read-reuse-functional1/`. Performance/proofs remain NOT_RUN for this
new treatment; earlier evidence is not promoted.

## Stage 1 retained-history qualification at `82dd31b24`

One source-matched release/locked pair each for stride10 then stride3; followed
by one stride1 reference attempt. Raw manifests/owner databases remain untouched.
No sampling retries, profile/cache/worker/buffer/budget/workload relaxation.

| Stride/states; arm | Product lifecycle ns | Complete performance ns / cap | Separate proof ns / 9.5s | C2 + C5 allocated B | Verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| 10/17; baseline | 35039103292 | 53129534542 / 60s | 2968669542 PASS | 52473856 | joint PASS |
| 10/17; candidate | 33306699875 | 50671928209 / 60s | 5073271209 PASS | 49594368 | joint PASS |
| 3/53; baseline | 71288805958 | 86535392041 / 170s | 8616643417 PASS | 65142784 | joint INCOMPLETE |
| 3/53; candidate | 69873888833 | 86882979333 / 170s | 9507661125 FAIL | 62611456 | joint INCOMPLETE |
| 1/157; baseline | UNAVAILABLE: incomplete producer after143/157 states | 170014270917 / 170s | NOT_RUN | 73596928 | FAIL_COMMAND_BUDGET; cleanup PASS; proof NOT_RUN |
| 1/157; candidate | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN: unqualified reference |

Stride10 ratio0.9505579979440988, integer333066998750<=385430136212;
strict candidate storage49,594,368B<54,278,964B. JointPASS, source/database cold,
canonical roots/census, proof and cleanup PASS. Candidate final single DB is C2/C5
combined49,594,368B; reference C2=52,428,800B+C5=45,056B=52,473,856B.

Stride3 ratio0.9801523239730849, integer698738888330<=784176865538;
strict candidate storage62,611,456B<70,427,034B; cold/roots/census/cleanup PASS.
Candidate proof9,507,661,125ns is timeout (not9.5s PASS), so jointINCOMPLETE.
Reference proof8,616,643,417ns PASS.

Stride1 reference complete command170,014,270,917ns exceeds170s. Actual producer
child153,425,238,583ns exceeds its remaining153,412,272,750ns; exit-9, no COMPLETE
result. Logs contain143completed state work rows, final before_state144cold boundary;
no final root vector/census/proof. Observed partial allocated73,596,928B is not
completed157storage qualification. Candidate is explicitNOT_RUN: reference pins
cannot be derived. No original/partial receipt promotion or unchanged-arm retry.

Original storage targets remain context, ceilings are the existing registered
54,278,964/70,427,034/92,342,273B strict candidate bounds; no tolerance changed.
SDK/Server transport/daemon/FUSE are N/A: these are direct host C1/C2/C5 cases.
RSS is per-child lifetime only, not phase/cgroup/device-memory attribution.
VFS work is requested/submitted bytes, not physical device bytes.

Raw folders `benchmark-results/fs-bench-pro/issue302-history{17,53,157}-selective-*1`;
compact comparison `checks/read-reuse-history1/comparison.json` retains identity,
raw nanoseconds, arithmetic, all verdicts and omissions. Reproduce with sole
`python3 core/benchmark/fs-bench-pro/runner.py run --case
phase7-sqlite-disposable-history-stride{10,3,1}-v2 --arm baseline --baseline-root
/Users/yifanxu/.codex/worktrees/phase7-cluster1-storage/layerfs/target/phase7-baseline/layerfs
--out <fresh-owned-output>`; candidate uses same selection and qualified
`--reference-pins <baseline-output>/root-pins.json`. Complete commands and separate
verifier argv are in each raw receipt, not reconstructed speed numbers.

Production implementation LOC138233->138465(+232), reference65417 unchanged,
core72816->73048. Earlier Init/history17passes stay pinned older identities;
this is Stage1 only, not all-seven/current-Durable admission. Stage2 remains
implementation work; goalACTIVE.

## Owner follow-up: stride1 prospective 190s

The owner requests a modest increase after Stage1's170s reference timeout.
Future stride1-v3 selections use190s complete performance in both arms/profiles.
Stride10/3 stay60/170s; proof9.5s and all other gates remain. This supersedes
the plan's original prospective stride1 cap only. Stage1-v2 receipts above
remain170s FAIL/NOT_RUN; they are not relabelled. See the
[budget ruling](HISTORY-BUDGET-RULING-20261004.md).

## Stage2 strict selected reads — implementation and frozen qualification plan

Implemented against parent `bac4cb1fb`: backend-neutral bounded prefix/selection
port, private authenticated range carrier and SQLite read-only incremental BLOB
I/O with checked close/transaction outcomes. Complete length/SHA256 is paid once
per acquisition, even for selected output; no partial bytes assert a complete
pack digest. C2 parses every directory extent once, coalesces adjacent requested
complete groups and explicitly chooses whole for singleton, small, >=50%-dense
or >4-coalesced-span demands. Descriptor/strategy/extents must match the plan.
All ordinary/native/PREFIX/pooled consumers now use complete groups from either
whole bodies or units. Same2MiB/4096entry body allowance and existing singleton,
decode512KiB/value512KiB/output/chain/publication limits. Reader/save ownership,
first-wins/winner validation and order retained. Private pooled invalidation
retained; pooled selection walks/reconstruction share the save's existing
512KiB group cache. No extra canonical-base memo or buffers/workers/schema.

Source diagnostics now separate whole/selected decisions, complete scan bytes,
prefix+selected materialized bytes and cache work. SQLite reports actual BLOB
open/read/close/read-call time/requested/returned bytes; VFS/device bytes remain
different observations. First cold acquisition does not reduce complete scan
bytes; the hypothesis is less materialization/copying and useful bounded reuse
avoiding subsequent complete acquisitions. Savings remain unmeasured here.

Functional checks: full workspace/all-target492tests PASS, then after final
pooled sharing and strengthened actual-acquisition assertions affected owners
(storage/persistence/project)172tests/36targets PASS. Workspace/all-target
Clippy-Dwarnings, fmt, boundary448files/23selftests and diff checks PASS.
Initial reuse test counted only old read routes and failed; it now includes
selected calls/bytes and preserves same-owner/fresh-owner assertions. Initial
Clippy manual_inspect corrected without disabling a lint. External tests cover
adjacent and random sparse groups, dense promotion, duplicate ordering, native
chunks, PREFIX dependency reconstruction, unselected-byte corruption refusal,
hash-valid malformed directories, raw range crossings/failures, actual SQLite
BLOB work/close and the same body-byte/entry-count bounds. Existing physical
versions and owning publication/transactions remain covered.
Raw checks: `checks/read-ranges-functional1/`.

Prospective final source-matched release pair order is stride10-v2, stride3-v2,
stride1-v3; complete performance60/170/190s, separate proof9.5s each. One sample
per case/arm, immutable archives/matched seals and source/database cold contract;
new qualified reference pins required before each candidate. All incomplete and
unrun rows stay explicit. No unchanged arm retry or old170s row promotion.
This implementation is not all-seven/Init/Durable admission.


## Stage2 ce3c09f24 qualification — stride10, then3, then1-v3

Frozen source ce3c09f24ca057e0ce72375a9de96d91779ace3f, tree
0fd8ea5912a9d0c508badef2f7e0efc2b245dedc, clean release/locked; baseline
7edddbdb8e8512627aed0ed42533ef099d802384. Harness
5f8a9d877189f8327adfd4b3a7488a152954fcd4e19ba86753313a9636c9d0d5;
candidate compilation0d69549aeab252f8b320239160ebf34d5710390816a7b84ce7cba5ba224b8d4a,
reference compilation783c7cc8fdfb7ce823d57ce6e51d9538b8ce3300f2e55d792ec431a94819f456.
Exact binary/product/dependency/fixture/config seals, commands, interference and
cache attestations remain in raw/compact receipts. One sample/case/arm, worktree
lock, reused sealed release builds and immutable source corpus; no prepared
product Store, warm credit or producer work moved outside the command.

| Stride/states; arm | Complete product lifecycle ns | Complete performance ns / cap | Separate proof ns /9.5s | C2+C5 allocated B | Joint disposition |
| --- | ---: | ---: | ---: | ---: | --- |
| 10/17 baseline |33632223167|51377006458 /60s|2824710334 PASS|52428800+45056=52473856|PASS|
| 10/17 candidate |32146486125|47997583291 /60s|5766582375 PASS|49594368 combined|PASS|
| 3/53 baseline |68104961083|84060208208 /170s|8153758542 PASS|65011712+131072=65142784|INCOMPLETE pair|
| 3/53 candidate |66887619334|81908172875 /170s|9507829125 TIMEOUT|62611456 combined|INCOMPLETE|
| 1/157-v3 baseline |UNAVAILABLE: producer incomplete|190020606667 /190s|NOT_RUN|83886080+196608=84082688 partial|FAIL_COMMAND_BUDGET|
| 1/157-v3 candidate |NOT_RUN|NOT_RUN|NOT_RUN|NOT_RUN|NOT_RUN: reference unqualified|

Stride10 ratio0.9558240014457977, integer321464861250<=369954454837;
strict candidate49,594,368B<54,278,964B. Both proofs cover17custody states,
101,477paths,8,631sampled-content paths,51,862,943authenticated bytes; independent
reference roots checked by candidate, producer canonical census agrees.

Stride3 ratio0.9821255055484663, integer668876193340<=749154571913;
strict candidate62,611,456B<70,427,034B. Cold/producer roots/census/storage/cleanup
PASS. Reference proof covers53custody states,306,861paths,26,052sampled-content
paths,127,050,040authenticated bytes. Candidate proof has no complete child
result and is not a semantic/admission PASS despite the time/storage gate.

Stride1-v3 reference source cold attestation PASS (zero resident content pages),
15,864,871,208ns, leaving174,134,608,500ns for producer. Producer ran
174,153,568,708ns, exit-9;154completed state rows and filesystem work in state155.
No complete root vector/census/finalization/proof. Partial84,082,688B is not
completed157storage qualification. CleanupPASS. Candidate explicitlyNOT_RUN
without qualified pins. Original170s rows unchanged; no further cap increase or
unchanged performance retry. Stride1 remains unqualified at190s.

Candidate17actual BLOB open/close34,408,read calls86,532,requested/success
2,930,849,248B,read-call1,569,071,134ns (excludes hashing/planning). Selected
acquisitions23,956scan2,330,175,477B and materialize521,728,962B (22.3901%);
whole decisions10,452. Complete digest scan still pays all pack bytes. Body
cache evictions18,875/916,270,436B. The existing SQL-only observer misses BLOB
opens: its old pack-acquisition count must not be used for candidate17/53;
explicit product BLOB counters are authoritative for that route. VFS counts
remain delegated requests, not device bytes. Subsequent labelled count-only
observer work will cover both routes; these historical rows retain their seals.

Raw `benchmark-results/fs-bench-pro/issue302-history{17,53,157}-ranges-*1`;
[compact comparison](checks/read-ranges-history1/comparison.json) and receipts
retain all outcomes. All six raw inventories/lengths/SHA256 were audited before
copy; compact manifests refer to original raw folders including retained DBs.
Reproduce using sole runner `python3 core/benchmark/fs-bench-pro/runner.py run
--case phase7-sqlite-disposable-history-stride10-v2 --arm baseline --baseline-root
/Users/yifanxu/.codex/worktrees/phase7-cluster1-storage/layerfs/target/phase7-baseline/layerfs
--out <fresh-owned-output>`; next stride3-v2 then stride1-v3; candidate same case
with `--reference-pins <qualified-baseline-output>/root-pins.json`. Exact argv
and separate verifier commands remain in receipts. No SDK/daemon/FUSE scope;
RSS per-child lifetime only. Earlier Init/Durable evidence does not qualify
this source for all-seven admission.

Implementation productionLOC138465->139279(+814); reference65417 unchanged,
core73048->73862, same production_loc.py exact parent/staged archive method.
Next: labelled native proof count diagnostic on retained53stores with BLOB
acquisition attribution, no speed rerun or proof promotion. GoalACTIVE.


## Directed reuse promotion after count diagnostic

Corrected SQL+BLOB count observer2e43336cd native-only53diagnostic: reference
TIMEOUT9,512,798,750ns after52states, candidateTIMEOUT9,503,366,625ns after46.
Whole commands25,467,049,250/24,110,084,667ns; source/database cold and owner
SHA256/closed-state preservation PASS. Diagnostic only; ordinary proof remains
FAIL and no speed sample replaced. Rawissue302-history53-ranges-native-cause1,
compactchecks/read-ranges-native-cause1. The observer counts successful read-only
main.pack.body opens and legacy SELECT acquisitions, fixed bitmap distinct IDs.
Unknown-ID counts0. Cumulative acquisitions36,436/36,575,distinct912/590 and
delegated VFS requested read2,994,085,023/4,123,410,782B across completed stages.
Different completed-state coverage prevents a direct speed/throughput ratio.
Candidate state46pooled pack acquisitions3,158/149,243,528scan bytes; reference
state52=241/8,193,630B. These differing states diagnose repetition, not matched
per-state speed. A selected group still needs a complete digest scan each miss.

Directed product change: on a new group miss while this owner retains another
selected unit of that same <=2MiB pack, explicitly choose whole before I/O.
Descriptor/strategy/full digest/whole frame and retained-unit equality checked;
units replaced, ordinary selective eviction enforces same2MiB/4096entry allowance.
No extra history, buffers, workers, retries or private mutable-pack reuse. The
first sparse acquisition stays selected, existing density/small/singleton
policies retain precedence, reason counterwhole_due_reuse records the real
choice. Source/WaveSource forward demand preference explicitly. External tests
assert acquisition counts, exact sibling bytes and promotion peak bound.
Functional full workspace/alltargets tests/Clippy/fmt/boundary PASS; exact
commands/logs/countschecks/read-ranges-reuse-functional1. Next one prospective
candidate native-only count diagnostic against original closed53store; no proof
promotion. Ordinary source-matched qualification will require new receipts.


## Final reuse promotion22c26f6da — source-matched10/3/1-v3

Full Core workspace/alltargets494tests/92targets PASS; Clippy-Dwarnings,
fmt-all/check, boundary448files/23selftests PASS. `cargo fmt` first invocation
without--all found no virtual-workspace targets; corrected command passed.
Raw Cargo test log ends with its original blank line (retained verbatim);
production/test/docs source diff is whitespace-clean. ProductionLOC139279->
139299(+20), reference65417unchanged, core73862->73882; exact parent/staged
archives using sameproduction_loc.py, committed tree7e1ffb9ab6e362b25ecec7b5df7f480431b18992.

Changed-policy native countdiag: candidateTIMEOUT9,512,228,750ns after43states,
complete26,416,371,792ns; cold/owners preserved PASS. Not proof or speed admission.
At same state43, pooled acquisitions2792->589 and scan bytes130119754->20784043;
state40=2400->339/108368054->11656875. Reconstruction calls/decoded-work counts
remain unchanged at those states. Different window/completion means no overall
wall improvement claim: promotion diagnostic completed43, previous46.
Rawissue302-history53-ranges-reuse-native-cause1; compactchecks counterpart.

Frozen product22c26f6da then new matched performance/proof pairs in10,3,1 order
with corrected BLOB-aware observer. Each registered case got one sample per
eligible arm; no unchanged retry or best-of selection. Clean source, release/
locked archives, one worker and source/database cold contract unchanged.
[Final raw operands/identities/commands/comparison](checks/read-ranges-reuse-history1/comparison.json).

|Stride/states;arm|Product lifecycle ns|Complete performance ns /cap|Separate proof ns /9.5s|Allocated B|
|---|---:|---:|---:|---:|
|10/17;baseline|33655000209|49284228000 /60s|2852601958 PASS|52473856|
|10/17;candidate|32375273334|51899902166 /60s|3787051708 PASS|49594368|
|3/53;baseline|67220889666|81325426291 /170s|8104830125 PASS|65142784|
|3/53;candidate|66147117583|82130659250 /170s|9508026625 FAIL|62611456|
|1/157;baseline|UNAVAILABLE|190019704292 /190s|NOT_RUN NOT_RUN|84082688|
|1/157;candidate|NOT_RUN|NOT_RUN /190s|NOT_RUN NOT_RUN|NOT_RUN|

Stride10jointPASS: ratio0.9619751339458386 (integer323752733340<=370205002299),
reference52428800+45056=52473856B; candidate49594368B combined<54278964B.
Complete cold/producer roots/census/full declared proof/cleanup PASS; proof scope
17custody states,101477paths,8631sampled content paths,51862943authenticatedB.

Stride3timePASS: ratio0.9840262143459385 (integer661471175830<=739429786326),
reference65011712+131072=65142784B; candidate62611456B<70427034B.
Producer roots/census/cold/storage/cleanup PASS; referenceproofPASS8104830125ns,
candidateproofTIMEOUT9508026625ns with no complete verifier result =>joint
INCOMPLETE. No admission inferred from a passing performance/storage gate.

Stride1-v3reference complete command190019704292nsTIMEOUT;154complete state
rows and filesystem state155, no final root vector/census/proof. Observed
83886080+196608=84082688B is partial, not157storage qualification.
CleanupPASS; candidate explicitNOT_RUN because qualified reference pins missing.
Performance envelope190s and proof9.5s remain. Earlier170s and190s attempts
keep their source/harness/outcomes; neither is relabelled or replaced.

Reproduce solely with `python3 core/benchmark/fs-bench-pro/runner.py run --case
phase7-sqlite-disposable-history-stride10-v2 --arm baseline --baseline-root
/Users/yifanxu/.codex/worktrees/phase7-cluster1-storage/layerfs/target/phase7-baseline/layerfs
--out <fresh-owned-output>`, stride3-v2 then stride1-v3; candidate same selection
and--reference-pins from its qualified reference. Raw folders
issue302-history{17,53,157}-ranges-reuse-*1, all6manifest inventories/lengths/
SHA256 audited. Receipts retain driver/verifier argv and complete seals/fixtures/
interference/cache/cleanup observations. No current-source Init/Durable/all-seven
admission claimed. GoalACTIVE: stride3proof and stride1reference gates remain
unfulfilled. Future work must change the measured mechanism; no unchanged retry
or implicit further budget enlargement is authorized by this report.


## Pooled value retention investigation after25820af5d

Previous goal turn made authoritative progress (product refinement and new
qualification). Current tree/receipts revalidated: stride3ordinaryproofTIMEOUT,
stride1reference190sTIMEOUT/no pins remain. No live measured child remains, and
no unchanged performance arm is retried.

Architecture review: selected results still pay whole digest I/O; prior reuse
promotion reduced same-state pooled acquisition work without a proofPASS. The
next specific problem is the existing512KiB decoded-value owner: unlike the
body cache, it clears all groups on overflow. A deterministic external
PoolReader source fixture fills that exact bound while touching one hot group
between unrelated groups. Before change45decodes for44distinct groupsFAIL;
source path confirms overflow removed the hot group. Rawbefore.log retained.

One directed change: same value cache/map owns recency per entry; selective
least-recent eviction replaces clear-all. No new history/cache, byte/entry
policy, workers, physical format, schema or authentication shortcut. Ceiling
check precedes every hit; fresh decode charges/authenticates all prior work;
private pack invalidation unchanged. Real count fields expose value-cache hits,
evictions and evicted decoded bytes with since/accumulate semantics.
After fixture44decodes/1eviction/exactbytes/bound and ceiling refusalPASS.
Full workspace/alltarget tests,Clippy-Dwarnings,fmt-all/check,boundary448/23
PASS; exact counts/logschecks/pooled-value-retention-functional1. Native count
diagnostic at frozen source next; no proof/performance pass inferred.


###9dac05a34 value-retention frozen diagnostic result

ProductionLOC139299->139358(+59), reference65417unchanged, core73882->73941.
Sameproduction_loc.py/SHA256, exact parent25820af5d and committed
tree13abdc17187a9d6ba01271f941e13e0df33c8a30. Changed production files counted
from git-show parent/staged with the same counter; remaining production scope
verified identical to the full archive-counted22c26f6da tree. No paths moved or
new production classes; external tests/docs excluded.495tests/93targets PASS.

One prospective native-only count child on original closed53store:TIMEOUT
9,507,618,208ns,44completed states; complete28,717,124,250ns within60s diagnostic
bound; source/DBcold/owner hashes preservedPASS. Original row manifests audited.
Same state40value decodes18265->16511; state43=21976->19928(-2048,9.31926%);
state43physical18540records/3144group decodes/119619946decodedB unchanged,
pooled acquisitions589->586, pack scan20784043->20502867B. Current43value-cache
hits163943/evictions19668/evicteddecoded33723153B. These are actual count
differences for the same retained state; not proof admission or overall wall
improvement. Full diagnostic totals have unequal43/44state coverage.
Rawissue302-history53-value-retention-native-cause1, compact
[comparison](checks/pooled-value-retention-native1/comparison.json). Reproduction:
`python3 core/benchmark/fs-bench-pro/diagnostics/run_history_proof_mechanism.py
--out <fresh-owned-output> --states 53 --arm candidate
--baseline-run issue302-history53-ranges-reuse-baseline1
--candidate-run issue302-history53-ranges-reuse-candidate1`. Actual argv has
spaces between flags and values; declaration/receipt pins exact vehicle/command.

Current goal turn is progress: deterministic retention defect reproduced,
bounded implementation/focused regression fixed, fullCorechecks passed, changed
count diagnostic recorded. It did not achieve qualification. Historical22c26f6da
10PASS/3proofTIMEOUT/1reference190sTIMEOUT remain historical; current9dac05a34
source-matched performance/combined proofs NOT_RUN. No assumption that a count
reduction or raw native diagnostic passes the separate combined proof.

Architecture review remains necessary before another tuning change: body
authentication scans all bytes, selected retention and valueLRU cannot remove
full traversal/dependency/canonical verification. Need distinguish remaining
necessary work from redundant decode/acquisition (including value-group
materialization/validation) rather than repeat an unchanged arm or grow caps.
Stride1reference stays unqualified, and it cannot be made faster by candidate
cache code. GoalACTIVE; no further budget/workload/format ruling made.


## Pooled value retention — source-matched qualification at8bfea39f8

Previous goal turn is progress:9dac05a34 cache change/functionals/countdiagnosis.
Clean8bfea39f8 revalidated and frozen (same9dac05a34 product), release/locked
matched case/observer/corpus/cache identities, worktree lock, one worker, same
source/database cold contract. Requested10->3->1-v3 sequence completed with
one sample per eligible arm. Sealed build/corpus reuse is recorded in receipts;
no product work shifted to setup or unchanged arm retry.

|Stride/states;arm|Product lifecycle ns|Complete performance ns /cap|Separate proof ns /9.5s|C2+C5 allocated B|
|---|---:|---:|---:|---:|
|10/17;baseline|33788754667|50989050667 /60s|2822646750 PASS|52473856|
|10/17;candidate|32226386667|49904542375 /60s|3866133709 PASS|49594368|
|3/53;baseline|67932424584|83358350333 /170s|8241411959 PASS|65142784|
|3/53;candidate|64935143250|81378862709 /170s|9504936125 FAIL|62611456|
|1/157;baseline|UNAVAILABLE|190019101500 /190s|NOT_RUN NOT_RUN|84082688|
|1/157;candidate|NOT_RUN|NOT_RUN /190s|NOT_RUN NOT_RUN|NOT_RUN|

Stride10 ratio0.9537607107631612; integer322263866670<=371676301337 timePASS. Candidate49594368B<54278964B strictstoragePASS, cold/producerroots/census/cleanupPASS. Reference proof2822646750nsPASS; candidate3866133709nsPASS. JointPASS.

Stride3 ratio0.9558784873003643; integer649351432500<=747256670424 timePASS. Candidate62611456B<70427034B strictstoragePASS, cold/producerroots/census/cleanupPASS. Reference proof8241411959nsPASS; candidate9504936125nsFAIL. JointINCOMPLETE.

Reference17 C2=52428800+C5=45056=52473856B;53 C2=65011712+C5=131072=
65142784B. Candidate17/53 singleDB contains bothC2/C5.17proof scope17custody
states/101477paths/8631sampled-content paths/51862943authenticated bytes.
53referenceproof covers53custody/306861paths/26052samples/127050040B. Candidate
53proof has no complete child result; no inference of semantic/admissionPASS.

Stride1-v3reference190019101500ns complete commandTIMEOUT;154complete state
rows and filesystem work155, no completed producer/root vector/census/proof.
Observed83886080+196608=84082688B is partial, not157storage qualification.
CleanupPASS. Candidate explicitNOT_RUN because referencepins absent.190s
performance and9.5s proof unchanged. Older rows keep exactlimits/identities/
outcomes; no fabrication, relabeling, cap inflation or candidate sample.

Rawissue302-history{17,53,157}-value-retention-*1; [comparison](checks/pooled-value-retention-history1/comparison.json) retains source/harness/product/compilation/dependency/binary/corpus/config identities, complete commands/proof scopes and omissions.
All6raw manifests/lengths/SHA256 audited. Reproduce with sole runner
`python3 core/benchmark/fs-bench-pro/runner.py run --case
phase7-sqlite-disposable-history-stride10-v2 --arm baseline --baseline-root
/Users/yifanxu/.codex/worktrees/phase7-cluster1-storage/layerfs/target/phase7-baseline/layerfs
--out <fresh-owned-output>`, thenstride3-v2 andstride1-v3; candidate samecase
with--reference-pins fromitsqualified baseline. Actualargv/pins are in receipts.
SDK/daemon/FUSE N/A; RSSchild-lifetime only, VFSrequestedbytes notdevicebytes.
PriorCore495/93tests/Clippy/fmt/boundary448/23 covers this unchanged product.
No current-source Init/Durable/all-seven admission.

Completion audit remains negative for53proof/157reference. This goal turn
completed a required qualification sequence and yielded authoritative evidence;
it is progress, not successful admission. Next safe source investigation:
decoded-group reuse across dependency discovery/reconstruction and explicit
private-writer invalidation. Need concrete evidence before another retention
change, no newcache/largerbound/format/weakerauthentication. GoalACTIVE.


## Shared decoded-group retention review after8b325c561

Previous goal turn was progress: required source-matched qualification recorded
10PASS/3proofTIMEOUT/1reference190sTIMEOUT. Current tree/records revalidated;
no unchanged performance arm rerun. Source ownership review found ordinary
pending members seal/flush before ready locators expose immutable groups. Only
the separate PooledMetadata private tail appends; it calls pool.release_packs,
and those values do not enter ordinary GroupCache. All private invalidation
remains. Shared groups serve dependency discovery, ordinary reconstruction and
pooled physical-leaf reads under the same caller/source frame/ceiling checks.

GroupCache still used clear-all overflow despite this valid immutable lifetime.
External public-cache fixture filled512KiB, touched a dependency group, then
admitted one group; the touched group vanished (beforeFAIL retained). One
directed change: owner-local recency and only least-recent victims needed for
admission. No extra cache/history or widerbody bound/workers. Checked insertion
returns explicit error for empty/invalid/oversized input or changed immutable
key; same-key identical insertion is idempotent and correctly charged once.
All three decode/discovery callers propagate insertion failure; current frame/
record/canonical validation still surrounds hits, no partial proof shortcut.
AfterfixturePASS selective victim/recent sibling/512KiB/duplicate accounting/
invalid admission preservation. Fullworkspace/alltarget tests,Clippy-Dwarnings,
fmt-all/check,boundary448/23PASS; logschecks/decoded-group-retention-functional1.
Freeze then one native-only53count diagnostic on original retained store; no
proof/performance pass inferred. Existing qualifiedrows stay historical.


###8302782a9 frozen decoded-group diagnostic and counter-scope correction

ProductionLOC139358->139408(+50), reference65417unchanged, core73941->73991.
Samecounter/version/exclusions; exact parent8b325c561/staged
dc56e9eafb20ac33a1e1aff9fa0b29a4dcada31e changed-source counts, remaining scope
source-equal to verified9dac05a34. Committedtreeconfirmed.497tests/94targets,
Clippy/fmt/boundary448/23PASS.

Native53countdiagTIMEOUT9,512,102,083ns after45states, whole26,001,995,791ns
within60s diagnosticbound; source/databasecold and ownerpreservationPASS.
Prior/value and current/decoded diagnostics read byte-identical62,611,456B stores:
SHA256cc3c207215d55f2db963a592279cae241dbf1212f517b1f4387956de8216e45f.
Across completedprefix43, physicalgroup decodes3144->1900 and decodedbytes
119619946->69491331;18540physicalrecord calls/19928value decodes unchanged.
Prefix40=2765->1669/104094401->60296801B. This confirms less decompression,
not an overall latency/proof pass; full diagnostic coverage44/45 differs.
Rawissue302-history53-decoded-group-retention-native-cause1, compact
[comparison](checks/decoded-group-retention-native1/comparison.json).

**Counter-scope correction for earlier discussion:** VERIFY_STATE_WORK's
pooled=reader.pooled_read_counters() uses the same reader across all states.
Those pooled counts are cumulative through that state, not the individual
state's work. Earlier wording/interpretation suggesting per-state pooled
counts was inaccurate. Same-prefix count differences remain valid; raw
receipts/counters unchanged. Only wall_ns/paths/sampled/authenticated_bytes in
that line are per-state. HISTORY_ENGINE_WORK verification rows use explicit
snapshot differences and are per-state. Never sum cumulative pooled rows.

Existing disjoint verify-parts spans show the actual remaining cost through
the same43completed states: referencewalk1927503667ns/file-root2280977128ns/
remainingdigest63251375ns; valuecachewalk1760420416ns/file-root4535651711ns/
remainingdigest68545330ns; currentdecodedcachewalk1674827913ns/file-root
4559273459ns/remainingdigest70322083ns. The file-root span includes acquisition,
classification and opportunistic sampled hashing, not just metadata/size SQL.
These sums exclude corpus/oracle/custody/wrapper overhead and cannot manufacture
a complete proof wall. Source-scope metadataMemo counts atprefix43 are identical
17445hits/2997misses/462sourcecalls/2997objects. Shared-group retention addresses
a real defect but file-root acquisition/classification is the larger gap.

Next source/count investigation should distinguish file-root BLOB scan/read
work, dependency reconstruction and actual returned canonical bytes within that
phase, before another policy change. No assumption that adding more retention
will fit9.5s, no extra cache/worker/buffer or weaker authentication. Current830
matched performance/combined proofNOT_RUN; older8bfreeze10PASS/3proofTIMEOUT/
1reference190sTIMEOUT retainidentity/verdict. GoalACTIVE.


## File-root phase count attribution after56d51bcaf

Previous goal turn was progress: shared decoded cache fixed and count-scope
interpretation corrected. Current clean tree/retained phase records revalidated;
file-root phase, not pooled per-state counts, is the larger unresolved gap.
No new performance sample or product-policy change in this step.

First-party observer now snapshots delegated BLOB opens/read/close calls,
requested/returned bytes, operation times and failures. Open counts are matched
read-only main.pack.body attempts; read/close counters cover all BLOB handles.
No payload inspection, profile mutation, retry or dependency patch. VFS bytes
remain requested/delegated bytes, not device bytes. BLOB call timing excludes
C2SHA256/planning/reconstruction work. Calibration on realSQLite checks SQL
and BLOB pack attribution plus valid/invalid reads, request/success byte counts
and failure flags; separate writable/error opens do not become pack acquisitions.

Native proof diagnosticv3 logs walk, file-roots and remaining-digest snapshots
as VERIFY_PHASE_WORK. These are disjoint children of the enclosing per-state
verification snapshot. Marker separation/parser tests prevent accidental
parent+child summation. Pooled rows remain cumulative across reader lifetime.
Partial final-state child phases are retained, but completed-prefix comparisons
exclude them. Missing observer fields mark diagnosticFAIL, not a fabricated0.
Same9.5s native/60s diagnostic envelope, source/database cold, immutable stores,
one child per declared arm; proof scope and all path/kind/size/sample checks
unchanged. File-root phase includes opportunistic sampled hashing.

Changed examples/observer/harness are checked (realSQLite calibration,6Python
diagnostic tests,3vehicle tests,Rustexamples/Clippy/fmt,boundary448/23). Logs
checks/file-root-phase-functional1. Prior497Coretests cover unchanged product
8302782a9; no product LOC change. Freeze changed diagnostic harness then one
labelled53native-only child per arm using original closed stores. Ordinary
performance/combined proof admission remainsNOT_RUN at this new harness.
