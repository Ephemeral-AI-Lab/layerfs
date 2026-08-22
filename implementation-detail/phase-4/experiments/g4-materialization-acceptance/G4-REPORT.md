# Phase-4 G4 materialization acceptance report

Status: **G4 STAGE TERMINAL PASS under the user-approved 1-ms absolute-regression materiality rule; v12 remains SEALED TERMINAL REVISE under its frozen relative-only contract**
Date: 2026-08-22
Next boundary: **G4 is closed; stop before G5, which requires separate future authorization; concurrent premature G5 planning remains outside G4 custody**

V12 is the authoritative G4 campaign. It executed exactly once from fresh custody after a real source-hash change, produced 30 records, 50 logical arms, and 76 timed child observations, and is sealed `REVISE`. Primary and independent analysis agree exactly on three failures under the unchanged protected equation: sequences 17, 20, and 26. No result is imported from v9, v10, or v11, and none of those campaigns is reanalyzed or rerun.

After sealing, the user first approved [a 0.500-ms micro-variance decision](USER-APPROVED-MICRO-VARIANCE-DECISION-v1.md), then explicitly clarified that an absolute regression below `1.000 ms` is non-material and directed final closure. The controlling [G4 stage terminal](G4-STAGE-TERMINAL-v1.json), SHA-256 `0297ca2e3b49ddb7d8d2d435713450dcc336397b53cbaaaee9647a46eebcede8`, supersedes the first decision for the materiality floor and stage disposition and records `PASS_WITH_USER_APPROVED_SUB_1MS_MICRO_VARIANCE_POLICY`. V12 remains `REVISE`, and the stage terminal explicitly records `old_relative_only_gate_passed=false`.

For the fixed two-sample estimator, a product-material regression requires both exact conditions: `candidate_sum * 100 > control_sum * 105` and `candidate_sum - control_sum >= 2,000,000 ns`. The three v12 delta numerators are only `452,458`, `571,043`, and `199,208 ns`; all are below `2,000,000 ns`. Every semantic/work/SQL/BLOB/Q/cleanup/durability/resource/custody/bucket/total-wall/cold-label gate remains hard and passed. Three fresh independent read-only lanes—correctness/authority/durability, performance/resources/evidence, and G5-readiness/architecture—reconciled to `PASS` with no source or evidence P0/P1. No v13 package or campaign is needed or authorized.

Source correctness, static closure, resource limits, custody, durability, cleanup under the frozen threat model, buffer evidence, bucket accounting, and artifact integrity all pass. Those passes do not turn the three preregistered adjacent-performance failures into passes; the post-seal user exception is a separate controlling disposition.

## Sealed v12 terminal disposition

The frozen rule for each of the 13 two-pair routes is:

`sum(candidate_ns) * 100 <= sum(control_ns) * 105`.

The CPPC/PCCP order, two samples per role, exact 5% threshold, and raw-sum equation were fixed before measurement. V12 applies no micro-cap, outlier deletion, adaptive sampling, alternative tolerance, or post-outcome issue removal.

| Sequence and route | Control samples / sum (ns) | Candidate samples / sum (ns) | Change | Result |
|---|---:|---:|---:|---|
| 17 — S1 clone/no-op, 100 MiB | 2,795,583 + 2,505,417 = 5,301,000 | 2,797,333 + 2,956,125 = 5,753,458 | **+8.5353%** | `g3-adjacent-degradation-17` |
| 20 — S1 count-change, 1 MiB | 4,555,291 + 3,842,500 = 8,397,791 | 4,396,542 + 4,572,292 = 8,968,834 | **+6.7999%** | `g3-adjacent-degradation-20` |
| 26 — S1 before-publication failure, 1 MiB | 619,458 + 767,750 = 1,387,208 | 607,125 + 979,291 = 1,586,416 | **+14.3604%** | `g3-adjacent-degradation-26` |

Semantic and work counters match exactly for each pair, so these are performance-gate failures rather than correctness mismatches. The remaining adjacent routes pass the same equation. The primary and independent normalized ledgers are byte-identical at `dc563d339401b0e7cdf84b20f1a8da20c99b5f0da849c700e86dceaa9de546b1` and contain exactly the three issues above.

The v12 final-hash causal screen was diagnostic only. It cleared the three v11 protected failures at smaller scale—returned range `+3.427103%`, reopen `+4.741857%`, and symlink rejection `-15.904309%`—but it was never acceptance evidence and does not supersede the full v12 result.

## Retained implementation and static closure

- Full reconstruction reuses the authenticated mapping traversal and batched leaf/chunk acquisition. Closure-off removes only the ordered closure fold; it retains identity-before-grammar validation, mapping/chunk authentication, output hashing, the ordered occurrence commitment, exact topology/length checks, and terminal-Q accounting.
- Reconstruction-only fold, digest, and sink counters are operation-local rather than copied through the shared hot `Metrics` struct. Rejoin segment and maximum-buffer evidence is likewise operation-local; unaffected protected routes do not initialize or update G4-only evidence state.
- R1 and batched M0 use a 1,500-page connection-local SQLite cache. R0 and scalar M0 controls retain 2,000 pages. Both R1 attribution arms use 1,500 pages, preserving closure folding as their work difference. `cache_spill=2000`, `FULL + DELETE`, `temp_store=FILE`, and `mmap=0` remain fixed.
- Rejoin authority still covers the predecessor through `edit_end + 1 MiB`, capped only by file length. Piecewise CDC input preserves that full search without any owned buffer over 1 MiB. `RangeSegments.values` is sized from checked requested-range capacity plus sliced-boundary slack, not the file-wide reference count.
- Exact 1 MiB replacement identity, boundary reading, typed complete fallback, authenticated closure-on/off malformed-error equivalence, requested-visible directory-sync retry, original typed first-cause retention, checked writer/authority counters, lost-ack reconciliation, and post-publication descriptor verification are covered by focused tests.
- M0 writes authenticated bytes with checked counters to an exclusive descriptor-relative temp, data-syncs, applies and metadata-syncs mode, binds length/digest/ordered source sequence before publication, retains descriptor/inode custody through directory sync, and reconciles ambiguous acknowledgement to requested-visible, prior/absent, different, or unresolved.

Final static closure is `PASS` on branch `codex/empty-worktree` at HEAD `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`:

- `cargo test --workspace --all-targets`: **166 passed, 1 ignored, 0 failed**;
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS**;
- `cargo fmt --check`, `git diff --check`, release build, Python compilation, final source audit, and independent methodology audit: **PASS**;
- fragmented-rejoin tests: 4 passed; G4 reconstruction/publication tests: 4 passed;
- symlink preflight/lazy evidence, no-follow authority/fixed buffer, post-`fclonefileat` typed unresolved handling, private-clone isolation, lock/bucket self-check, adapter/analyzer ledger probe, and final causal screen: **PASS**.

The authoritative static artifact is [v12 static closure](v12/STATIC-CLOSURE-v12.json). [The v12 preregistration](v12/PREREGISTRATION-v12.md), [dry run](v12/DRY-RUN-v12.json), and [methodology manifest](v12/METHODOLOGY-MANIFEST-v12.tsv) were frozen before the fresh result root was created. The historical `G4-FINAL-STATIC-CLOSURE-v9.md` is not current G4 status.

## Accepted measurements within the revised campaign

These cells pass their absolute and semantic gates but do not convert the overall campaign to `PASS`:

| Primary 100-MiB cell | Observation | Gate/result |
|---|---:|---|
| R0 scalar closure-derived control | 337.164667 ms | diagnostic control |
| R1 closure-on attribution control | 342.812167 ms | same 1,500-page cache/work boundary |
| R1 closure-off candidate | **237.214083 ms / 421.56 MiB/s** | <=333 ms; 30.803% faster than attribution control |
| R1 fresh-process candidate | **237.381208 ms / 421.26 MiB/s** | <=400 ms; OS cache warm-or-unknown |
| M0 scalar native control | 321.892959 ms | adjacent diagnostic control |
| M0 batched first/full candidate | **307.652375 ms / 325.04 MiB/s** | <=400 ms; sync/publication included |
| protected-seed no-digest read | **10.057750 ms / 9,942.58 MiB/s** | <=50 ms; same-open warm-or-unknown |
| protected-seed digest pass | 83.018417 ms | separately attributed diagnostic |

The 100-MiB reconstruction work shape remains exact: 170 SQL queries; 5,371 returned/authenticated objects; 83 leaf batches; 5,284 chunk BLOB reads/references/native writes; maximum batch 64; 104,926,292 borrowed BLOB bytes; 105,122,401 authenticated canonical bytes; 104,857,600 output bytes; and 5,284 ordered occurrence entries. Closure-off reports zero closure-fold updates/bytes while retaining identical output and occurrence commitments.

M0 reports one data sync, one metadata operation, one metadata sync, one exclusive rename, two directory syncs, one temp create/remove, committed publication, and no ordinary-path reconciliation. Its checked writer records 5,284 calls and 104,857,600 bytes with zero short writes or errors.

## Resource, chronology, custody, and artifact closure

- Complete wall: **91.262292709 s**, below 120 s with **28.737707291 s reserve**.
- Measured operation-local sum: **7.290316254 s**, below 20 s.
- Maximum whole-child RSS: **20,578,304 bytes**, below 20,971,520 bytes.
- Campaign maximum single buffer: **1,048,576 bytes**. Every candidate route has direct buffer evidence; frozen controls have source-static bounds. Terminal Q is zero for every arm.
- All prospectively repartitioned buckets pass with no overruns. Actual preparation was 72.140823710 s against 75 s; row dispatch was 17.925220915 s against 38 s; the seven actual buckets sum exactly to complete wall, and the caps sum to exactly 120 s.
- Exactly 76 child commands appear in exact global order. The append-only chronology has one campaign-start event, 76 measured-child-complete events, and one rows/analysis/cleanup completion event.
- Both analyzers independently reopen and hash all 76 stdout/stderr pairs, parse all 76 `/usr/bin/time -l` records, rehash each command binary, enforce role-to-binary binding, and derive real/user/system time, RSS, and context switches. Aggregate child evidence is 70.83 s real, 51.64 s user, 14.46 s system, 8,551 voluntary and 37,466 involuntary context switches.
- V11’s four false child/arm bindings are closed by normalizing only the adapter-added `status` and exact `status_adapter` fields for retained raw G3 rows. No other payload field is normalized, and v12 has no binding issue.
- Terminal verification, while the lock remained held, rehashed all four frozen sources plus the live/measured candidate and both measured controls. Source, executable, command, stdout/stderr, and payload custody all match.
- The payload manifest contains 271 entries with zero mismatches; the sealed final artifact inventory contains 277 entries. `work-v12` is absent, residue is zero, and the sealed results directory is mode `0555`.
- Two controlled-cold cells, byte-level physical I/O, and continuous storage peaks remain honestly `Unavailable`; sampled storage is not presented as a continuous peak.

The frozen executable and source identities are:

| Custody item | SHA-256 |
|---|---|
| release/measured candidate | `e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33` |
| frozen G3 control | `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e` |
| frozen protected control | `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5` |
| `phase4_create_edit_benchmark.rs` | `01886da1d413ce73bbeba38f1b5cbc45a939e9d50e69fa7273c1af33f65554cb` |
| `phase4_g3_materialization.rs` | `320ecb529c11de4464ce9a76ce97cc11f60d719d418f33a40d945e5f6dde196a` |
| `canonical_v2.rs` | `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc` |
| `Cargo.lock` | `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8` |

## Cleanup and lock threat-model qualification

The cleanup and lock results pass only under the frozen benchmark-private, mode-0700, no-malicious-same-UID model.

`TempName` retains descriptor/identity custody and validates identity before `unlinkat`, but this remains identity-check-then-unlink. A malicious same-UID actor could substitute after the final check. A post-`fclonefileat` identity-acquisition failure returns typed unresolved cleanup with residue. Therefore the report does **not** claim categorical race-free temp cleanup.

The benchmark lock is held through the fsynced terminal verification. Release rewrites and fsyncs the owned lock, verifies retained descriptor/inode/token, atomically renames the public name into [a sealed release attestation](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/BENCHMARK-LOCK-RELEASE-ATTESTATION-v12.json), fsyncs the parent, verifies the attestation, and leaves the public lock absent. It does not unlink the public pathname. The rename is nevertheless neither an inode-conditional source rename nor a no-replace destination rename, so lock namespace handling is also not generalized to a malicious same-UID adversary.

## Sealed v12 evidence

| Evidence | SHA-256 |
|---|---|
| [Measured terminal](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/MEASURED-TERMINAL-v1.json) | `d3c6dba7cd114817c9153a0426d0a9cc92723bf58a7efc9830877673ff111b31` |
| [Terminal verification](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/MEASURED-TERMINAL-VERIFICATION-v1.json) | `2837c7484238282e03b45876100be9cc4ca4fdfa1931b4cb4e173798809e0478` |
| [Primary analysis](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/PRIMARY-ANALYSIS-v1.json) | `d9431c6cab8755e59397b40f73d48ba80479b4206648ce8381fe21be746d0868` |
| [Independent recomputation](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/INDEPENDENT-RECOMPUTATION-v1.json) | `557462730c05b45ebfa0d0570d364eac8df87efea7744818a77cd44af7e150db` |
| [Complete wall](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/COMPLETE-WALL-v1.json) | `5790efeb0fe9742a0ba8a32f8a40008d126dde6b8ec9f85621723c8dc3c49634` |
| [Commands](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/COMMANDS-v1.json) | `dce7ae21a7158913bfa368508d91db4b87013d774b6cdf3dca9933e1a41f0acd` |
| [Append-only chronology](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/CHRONOLOGY-v1.jsonl) | `40844e12dc522ebb121d1d409d880fdd60d12acb7ac686e3baadac35e5bdfe2f` |
| [Payload manifest](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/PAYLOAD-MANIFEST-v1.tsv) | `d6707758ab4644a5a50ecba65fb1497f7510d7a93d2a33aeef2efbac1bf259db` |
| [Final artifact hashes](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/FINAL-ARTIFACT-HASHES-v1.tsv) | `585be251a1bd1a260a12415790a0e8f4cd59271217c8533639971a11a4c0b012` |
| [Source custody](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/SOURCE-CUSTODY-v1.json) | `4e05be0d9abab10f3c7180485fc336e67b1355d58dcd50a153d957551b6baed2` |
| [Operand custody](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/OPERAND-CUSTODY-v1.json) | `d578eb478a661901f915e9d131b965ef3303c1a7ce3113f8c7479e2f78e2cbe3` |
| [Lock release](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/LOCK-RELEASE-v12.json) | `b8e22f13d77687e6fd108cc6789d40b22017771d917d9e58365e6cd768ae2bb6` |
| [Sealed lock attestation](../../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/BENCHMARK-LOCK-RELEASE-ATTESTATION-v12.json) | `0d3b470f095876c46d9d9e636b85172fc73ba06f1a29eb9184ef31b4eac3d309` |
| [Methodology manifest](v12/METHODOLOGY-MANIFEST-v12.tsv) | `895c4139f7617a87f94810c5a7db8bde6e2418af31aa570c6bea2480b934403b` |
| [Static closure](v12/STATIC-CLOSURE-v12.json) | `c63801f5b7da5206f621e66592782aedf5a402e4b8260549df93b24cfdce5f92` |

## Revision history through v11

| Version | Preserved disposition |
|---|---|
| v1 | Zero-row pre-execution revision. |
| v2 | Partial 15-record execution; terminal `REVISE`. |
| v3 | Complete campaign; revised exact G3 ratio/bucket treatment. |
| v4 | Zero-row pre-execution revision. |
| v5 | Measured numeric `PASS`, but terminal `REVISE` because source/executable static custody did not close. |
| v6 | Measured numeric `PASS` / terminal `REVISE`; native M0 lacked the required ambiguous-acknowledgement, descriptor verification, identity cleanup, and adversarial fault proof. |
| v7 | Freshly proved the durability repair; terminal `REVISE` because RSS was 22,020,096 bytes, above 20 MiB. |
| v8 | Repaired RSS and passed substantive G4/M0/resource gates; terminal `REVISE` because one-shot micro edit/reopen relative inference was not defensible. |
| v9 | Strong complete evidence, but sealed `MEASURED_PROTOCOL_REVISE`; post-v8 micro-caps loosened the original <=5% rule, under which range was +7.82%. |
| v10 | `PRE_EXEC_REVISE_ABORTED_INVALID_EXECUTION`; an invalid draft was interrupted after 22 records / 34 arms, with no terminal artifact and no reusable evidence. |
| v11 | Sealed measured `REVISE`; four adapter-only binding false positives, genuine seq8/25/30 protected failures, and row-bucket overrun 42.268814750 s > 31 s despite 114.060771792 s total. It remains unchanged and is neither reanalyzed nor rerun. |

V12 closes every identified v11 methodology defect: the exact adapter pair is normalized, all 76 child resources and role/binary bindings are independently verified, direct/static <=1 MiB buffer evidence is campaign-wide, the balanced CPPC/PCCP estimator and original <=5% equation remain unchanged, completed-bucket overruns are terminalized, and the prospective 1/75/38/1/1/1/3-second partition sums to exactly 120 seconds and passes. V12 remains `REVISE` solely because the fresh full campaign exposes the three exact adjacent failures reported above.

The controlling post-seal decision classifies those three sub-1-ms regressions as non-material while preserving the sealed result. G4 has a separate audited stage-level terminal `PASS` and is closed at the benchmark-private engine/OS boundary; the report does not call the frozen relative-only equation a pass.

## G5 boundary

`research/phase-4/g5-round-0` already contains concurrent, premature foreign planning work. It is preserved untouched, excluded from G4 hashes and custody, and is not evidence that G4 authorized or completed any G5 activity. The user-approved G4 disposition does not authorize this task to execute G5 implementation or measurement; this report deliberately does not claim that G5 planning is unstarted.
