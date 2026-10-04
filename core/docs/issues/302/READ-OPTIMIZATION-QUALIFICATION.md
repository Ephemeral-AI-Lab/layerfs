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
