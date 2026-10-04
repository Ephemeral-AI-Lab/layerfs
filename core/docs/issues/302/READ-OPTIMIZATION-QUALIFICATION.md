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
