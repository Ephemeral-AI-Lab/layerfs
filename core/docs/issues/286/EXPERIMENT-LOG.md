# #286 Phase B experiment log

> **Status:** Current planning checklist; no release candidate exists.
> Append-only round index for [#286](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286),
> issued 2026-09-29 against Phase A product `671f4a46f8f354cf764d3d1555529b73a5c939e0`
> and documentation `4a4447db1abcbfccea0bcf1c79b9cc1d80ac2f04`.
> **No Phase B benchmark invocation has been run by this publication.**

Follow the [plan](PHASE-B-PLAN-20260929.md) and
[handoff](HANDOFF-PHASE-B-20260929.md). Append a row/report for every round,
including failed setup, partial sampling, verifier and cleanup failures.
Do not change an old round to match a later fix; append a correction that
names the original record. A declared selection can share one round report,
with separate case/arm/invocation rows. No best-of or unchanged-arm resampling.

## Round index

No collected rounds yet. Replace no historical evidence; append the first
actual round after its invocation.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |

## Required round report

Use `experiments/YYYYMMDD-<family>-rNNN.md`. Below is a template, **not a
sample or PASS receipt**. Record missing fields as null/unavailable with reason.
Keep raw integer bytes/nanoseconds/counts; display seconds additionally.

```text
Round ID / date / family / selected cases and arm order:
Purpose: official gate sample | labelled cause diagnostic | regression
Measured code commit and exact tree:
Last product-changing commit / harness commit:
Product, compilation, dependency, harness, workload and oracle seals:
Release binary hashes / image ID / root Cargo config hash:
Host + Docker/VM/backend/device profile / declared interference:
Metric + threshold + cache contract + gate/profile/receipt versions:
Exact commands, worktree, fresh output paths and original limits:
Preparation: master identity, clone_method or InProcess, excluded work:
Reuse: build_mode, dependency_reuse, reused_proof_identities and omissions:

Per invocation / case / arm:
  Raw receipt/log SHA256 and retained path or published receipt link:
  Operation ns / complete command ns / verifier ns / cleanup ns:
  Available Exec, Commit, lifecycle CPU and mechanism counts:
  C2/C5/index allocated/apparent/logical bytes and total:
  Physical backing/charged/reserved bytes, pins/refunds/custody:
  Available phase-local memory; lifetime metrics named separately:
  Correctness verdict and oracle coverage:
  Hard speed/storage/command-budget verdict and arithmetic:
  Numeric-profile eligibility verdict/reason:
  Cleanup verdict and retained known/unknown owners:
  Overall result, all FAIL/INELIGIBLE/INCOMPLETE/NOT_RUN lines:

Issue / symptom / source-and-receipt cause evidence:
Next concrete shared-cause fix and next executable command:
Code-fix commit (before measurement), if any:
Report commit / first parent / staged tree:
Production LOC: reference before -> after (signed delta),
                Core before -> after (signed delta),
                combined before -> after (signed delta);
counter command/hash and exact snapshot identities:
Current family gate complete? correctness AND required speed/storage:
Earlier-family checkpoint: NOT_DUE | exact reused proof | new result:
Next family / precise unresolved external decision:
Commit-linked #286 progress-comment URL:
```

Commit the result report first, then post the progress comment. A report cannot
contain its own commit hash without changing that hash: record the report
commit/comment URL in the issue, and append that link to this index in a later
real report commit. Do not create an empty bookkeeping commit for this cycle.

## Required issue comment

```text
Round <id>: family <n/name>, selection <cases/arms>
Measured source <commit>; code fix <commit or none>; report <commit + link>
Stats <raw times/bytes/counts>; gates <correctness, required metric,
numeric eligibility, cleanup>; every failed/partial/ineligible case <reason>
Issues/cause <evidence>; next fix/command <concrete action>
Earlier families <not due until n passes | checkpoint results/reused proofs>
Production LOC <reference, Core, combined before/after/delta for each commit>
```

Raw `core/target/` paths are local evidence only. Publish compact receipts or
hash manifests with reproducible source/commands; never label those paths as
published GitHub raw-evidence URLs. Preserve every attempt append-only.

## Collected round 20260929-init-r001

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260929-init-r001 | init_namespace: 100 then 1000 | `10d83af00` | Correctness, command/verifier budgets and cleanup PASS; numeric INELIGIBLE; 10000/100000 NOT_RUN | [Round report](experiments/20260929-init-r001.md) | Posted after report commit; append link in next actual round |

## Collected round 20260930-history-reference-r002

Init r001 issue publication: [#286 comment5893411789](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5893411789), report commit `cbc77ee40` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-reference-r002 | history reference acquisition10/3 + retained157 vector; compound harness/profile implementation | Independent reference `2f07f1f3`; candidate patch committed after acquisition | Root vectors complete; compound gates NOT_RUN; canonical stride3 mismatch preserved; narrow C5/threshold/registry/lock checks PASS | [Round report](experiments/20260930-history-reference-r002.md) | Published after commit; link appended in next actual round |

## Collected round 20260930-history-stride10-r003

Reference/implementation r002 issue publication: [#286 comment5894032481](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5894032481), code/report commit `509668ea6` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride10-r003 | compound history stride10/17 states, one candidate invocation | `509668ea6` | FAIL: C2 dependency encoded-work refusal after6 states; storage parser refuses v17; correctness/storage/cleanup INCOMPLETE, verifier NOT_RUN, time INELIGIBLE | [Round report](experiments/20260930-history-stride10-r003.md) | Published after commit; link appended in next actual round |

## Collected round 20260930-history-stride10-r004

R003 issue publication: [#286 comment5894176598](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5894176598), report commit `8baf47e45` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride10-r004 | compound history stride10/17 states, one changed-source candidate | `d4245911f` | FAIL: strict storage53190656 >=49344512 B; extra directory state2 breaks full tree; roots/canonical/C5/cleanup PASS, numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride10-r004.md) | Published after commit; link appended in next actual round |

## Engineering/reference round 20260930-history-v2-r005

R004 issue publication: [#286 comment5894696260](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5894696260), report commit `597ed6ac2` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v2-r005 | history v2 independent roots and corrected producer; no candidate sample | sealed old Core `6b22835dd` plus diagnostic resume `641f253da` | stride10/3 reference vectors complete; stride1 prefix timed out170.013s and was resumed in65.575s; continuation whole-chain gate FAIL3/157 by design; original stride3/1 O3 pins unchanged; compound candidate gates NOT_RUN | [Round report](experiments/20260930-history-v2-r005.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride10-v2-r006

R005 issue publication: [#286 comment5894988524](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5894988524), method/reference commit `6c494130c` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride10-v2-r006 | compound history stride10/17 states, one v2 candidate invocation | `6c494130c` | INCOMPLETE collector `KeyError('canonical_bytes')` after complete performance; read-only actual C2+C5 allocated53182464 B **FAIL** strict <49344512; roots/O3 native PASS; copied separate verifier timed out at10s, complete semantic proof INCOMPLETE; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride10-v2-r006.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride10-v2-r007

R006 issue publication: [#286 comment5895212384](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5895212384), report commit `b4bea65b4` (production delta+0). Changed product/verifier source `e4a56f327` records production Core +26 and combined +26.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride10-v2-r007 | compound history stride10/17 states, one changed-source candidate | `e4a56f327` | FAIL: C2+C5 allocated51720192 >=49344512 B; full101477-path tree/size and8631 selected content path-states PASS, independent roots/O3/C5/cleanup PASS, verifier4.243s, time INELIGIBLE; adapter semantic label incorrectly FAIL because it includes resource gate | [Round report](experiments/20260930-history-stride10-v2-r007.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride10-v2-r008

R007 issue publication: [#286 comment5895564116](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5895564116), report commit `de407e201` (production delta+0). Changed bounded codec/adapter source `34c3d3e53` records production delta+0.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride10-v2-r008 | compound history stride10/17 states, one changed-source candidate | `34c3d3e53` | **PASS**: C2+C5 allocated47882240 B <49344512 B; complete101477 path-state tree/size,8631 selected content states,17 independent roots/O3/C5/cleanup PASS; verifier4.466s<10s, complete driver44.854s<60s; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride10-v2-r008.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride3-v2-r009

R008 issue publication: [#286 comment5895668329](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5895668329), report commit `2a2453535` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride3-v2-r009 | compound history stride3/53 states, first v2 candidate | `2a2453535` (product seal equal r008) | **FAIL O3**: observed589480854 B/73447 objects vs immutable589423458 B/73476; allocated60477440 B <64024576 B PASS, complete306861-path tree/size+26052 selected content states PASS,53 independent roots/C5/cleanup PASS; verifier16.559s<20s, driver77.536s<170s, numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride3-v2-r009.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride1-v2-r010

R009 issue publication: [#286 comment5895738372](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5895738372), report commit `c30be4b41` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v2-r010 | compound history stride1/157 states, first explicit v2 candidate | `c30be4b41` (product seal equal r008) | **TIMEOUT/INCOMPLETE**: fixed170s child stopped with C2 save157 still active (122 objects), C5 156 Layers, no independent verifier; partial C2+C5 allocated84520960 B is not a complete strict gate, partial O3 status incorrectly labelled FAIL by adapter; all completion/canonical/cleanup claims INCOMPLETE, numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v2-r010.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride1-v2-r011

R010 issue publication: [#286 comment5895866838](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5895866838), report commit `06a158d98` (production delta+0). New SHA-bound oracle-count acquisition source `c130dab26` has production delta+0.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v2-r011 | compound history stride1/157 states, one changed-harness candidate | `c130dab26` | **TIMEOUT/INCOMPLETE** at fixed170s: C2 save157 active with290 object rows, C5 156 Layers, no verifier; partial canonical INCOMPLETE now correctly classified, storage INCOMPLETE; at-run partial C2+C5 allocated84520960 B but cannot decide full-chain gate; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v2-r011.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride1-v2-r012

R011 issue publication: [#286 comment5896046260](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5896046260), report commit `38b557f45` (production delta+0). macOS Git SHA-1 acceleration source `6ab692386` has production delta+0.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v2-r012 | compound history stride1/157 states, one changed-harness candidate | `6ab692386` | **FAIL**: complete driver167.807s<170s and all157 C2/C5 states persisted; strict allocated84520960 >=83947520 B (+573440), canonical871337620 B/104618 vs immutable871588115/104705 FAIL; verifier TIMEOUT30s, full O4 INCOMPLETE; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v2-r012.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride1-v2-r013

R012 issue publication: [#286 comment5896285256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5896285256), report commit `f45599877` (production delta+0). Version18 physical layout + verifier-only page memo source `290ddb180` records Core/combined production+15.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v2-r013 | compound history stride1/157 states, one changed-source candidate | `290ddb180` | **FAIL**: complete driver165.411s<170s, verifier19.951s<30s, all904143 listed paths/76726 selected content states/157 roots/C5 PASS; actual C2+C5 allocated84475904 >=83947520 B (+528384) FAIL; immutable O3 observed871337620 B/104618 vs871588115/104705 FAIL; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v2-r013.md) | Publish after commit; link in next round |

## Collected round 20260930-history-stride1-v2-r014

R013 issue publication: [#286 comment5896609012](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5896609012), report commit `a107054c0` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v2-r014 | compound history stride1/157 states, one changed-source candidate | `6afbfb7b0` | **FAIL**: driver166.455s<170s, verifier19.735s<30s, all904143 listed paths/76726 selected content states/157 roots/C5 PASS; actual C2+C5 allocated84418560 >=83947520 B (+471040) FAIL; immutable O3 observed871337620 B/104618 vs871588115/104705 FAIL; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v2-r014.md) | Publish after commit; link in next real round |

## Collected round 20260930-history-stride1-v2-r015

R014 issue publication: [#286 comment5896898795](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5896898795), report commit `b0fa5bf7a` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v2-r015 | compound history stride1/157 states, one changed-source candidate | `484b99c66` | **FAIL**: driver166.564s<170s, verifier19.548s<30s, all904143 listed paths/76726 selected content states/157 roots/C5 PASS; C2 Store SHA and allocated C2+C584418560 B identical to r014, still +471040 B strict FAIL; immutable O3 observed871337620 B/104618 vs871588115/104705 FAIL; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v2-r015.md) | Publish after commit; link in next real round |

## Collected rounds 20260930-history-v3-r016 and stride1-v3-r017

R015 issue publication: [#286 comment5897087620](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5897087620), report commit `2ca9ec818` (production delta+0). [The prospective O3 method-applicability ruling](HISTORY-O3-APPLICABILITY-RULING-20260930.md) and v3 registry were committed at `2280f580f` before either new invocation; original v1/v2 pins, FAIL statuses and strict physical ceilings remain.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v3-r016 | compound v3 stride10/17 then stride3/53 states, one sample per selected case | `2280f580f` | **PASS/PASS**: strict allocated46718976<49344512 and58368000<64024576 B; full tree/selected content/independent O3+roots/C5/cleanup PASS; driver44.084/75.000s, verifier3.021/8.421s under original bounds; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v3-r016.md) | Publish after commit; link in next real round |
| 20260930-history-stride1-v3-r017 | compound v3 stride1/157, one explicit changed-profile sample at same source | `2280f580f` | **FAIL storage only**: strict allocated84377600>=83947520 B (+430080); all904143 listed paths/76726 selected content states/157 independent O3+roots/C5/cleanup PASS, driver165.224s<170s, verifier20.577s<30s; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v3-r017.md) | Publish after commit; link in next real round |

## Collected rounds 20260930-history-v3-r018 and stride1-v3-r019

R016/r017 issue publication: [#286 comment5897406894](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5897406894), report commit `ada5361d3` (production delta+0). Version21's64 KiB source commit `9e0f0b893` was **not measured**. The [prospective pre-sample correction](HISTORY-O3-APPLICABILITY-RULING-20260930.md) selected version22's128 KiB bound from the already sealed stride10/3 shapes at `c233e0230`, before either r018/r019 invocation; version21 stays readable at its own limit.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v3-r018 | compound v3 stride10/17 then stride3/53 states, one sample each | `c233e0230` | **PASS/PASS**: strict allocated46587904<49344512 and58236928<64024576 B; full tree/selected content/independent O3+roots/C5/cleanup PASS; driver43.466/73.614s, verifier2.891/8.181s under original bounds; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v3-r018.md) | Publish after commit; link in next real round |
| 20260930-history-stride1-v3-r019 | compound v3 stride1/157 at same source, one explicit sample | `c233e0230` | **FAIL storage only**: strict allocated84246528>=83947520 B (+299008); all904143 listed paths/76726 selected content states/157 independent O3+roots/C5/cleanup PASS, driver163.656s<170s, verifier20.885s<30s; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v3-r019.md) | Publish after commit; link in next real round |

## Collected rounds 20260930-history-v3-r020 and stride1-v3-r021

R018/r019 issue publication: [#286 comment5897803588](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5897803588), report commit `08901a008` (production delta+0). The next prospective source `e17f1b43c` returns new pooled writes to the already registered64-KiB v21 grammar, omits only the cleanup-only bounded hint index from new C2, and uses1-KiB pages on new C5; previous formats/stores remain readable, with no threshold or timeout change.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v3-r020 | compound v3 stride10/17 then stride3/53, one sample each | `e17f1b43c` | **PASS/PASS**: strict allocated46469120<49344512 and58142720<64024576 B; full tree/selected content/independent O3+roots/C5/cleanup PASS; driver44.118/73.685s, verifier2.933/8.272s under original bounds; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v3-r020.md) | Publish after commit; link in next real round |
| 20260930-history-stride1-v3-r021 | compound v3 stride1/157 at same source, one explicit sample | `e17f1b43c` | **FAIL storage only**: strict allocated84086784>=83947520 B (+139264); all904143 listed paths/76726 selected content states/157 independent O3+roots/C5/cleanup PASS, driver165.294s<170s, verifier20.612s<30s; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v3-r021.md) | Publish after commit; link in next real round |

## Diagnostic round 20260930-history-full-codec-diagnostic-r022

R020/r021 issue publication: [#286 comment5898183004](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5898183004), report commit `4d649eb98` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-full-codec-diagnostic-r022 | read-only original r021 Store, 6,948 whole-file FULL frames; **no candidate sample** | C2 original SHA `ef1fba79...`, source `e17f1b43c` | Calibrated level9 exact6948/6948; level12 prospective record-width saving322340 B and added codec process CPU1.856136s, both **diagnostic only**; current strict storage FAIL139264 B and family2 status unchanged | [Diagnostic report](experiments/20260930-history-full-codec-diagnostic-r022.md) | Publish after commit; link in next real round |

## Diagnostic correction 20260930-history-codec-workspace-correction-r023

R022 issue publication: [#286 comment5898248569](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5898248569), diagnostic report commit `f785cc923` (production delta+0).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-codec-workspace-correction-r023 | host Zstandard context probe + focused pre-sample test; **no candidate sample** | source r022 wide-probe input corrected, r021 Store unchanged | R022 wide-policy context assertion **CORRECTED**: level12 needs25682968 B at1,000,000 B/dict hint, above fixed16MiB; default131071 B needs4711448 B and fits. R022's6948-frame width/CPU diagnostic unchanged; r021 strict storage FAIL remains. Source selects level12 only for declared default-width policy, level9 for wider policy. | [Correction report](experiments/20260930-history-codec-workspace-correction-r023.md) | Publish after commit; link in next real round |

## Collected rounds 20260930-history-v3-r024 and stride1-v3-r025

R023 issue publication: [#286 comment5898364695](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5898364695), correction commit `4702ab71f` (production delta+0). Product source `9b084cd27` selects level12 for default-width whole-file FULL winners only; all original thresholds and receipts remain unchanged.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v3-r024 | compound v3 stride10/17 then stride3/53, one changed-source sample each | `9b084cd27` | **PASS/PASS**: strict allocated46469120<49344512 and57556992<64024576 B; full tree/selected content/independent O3+roots/C5/cleanup PASS; driver45.489/75.550s, verifier2.945/8.272s under original bounds; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v3-r024.md) | Publish after commit; link in next real round |
| 20260930-history-stride1-v3-r025 | compound v3 stride1/157 at same source, one explicit sample | `9b084cd27` | **FAIL storage only**: strict allocated84758528>=83947520 B (+811008) despite pack bodies −322362 B vs r021; all904143 listed paths/76726 selected content states/157 independent O3+roots/C5/cleanup PASS, driver165.640s<170s, verifier20.458s<30s; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v3-r025.md) | Publish after commit; link in next real round |

## Diagnostic round 20260930-history-block-diagnostic-r026

R024/r025 issue publication: [#286 comment5898567846](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5898567846), report commit `f5ec32d16` (production delta+0). Product source `338bfc21c` then restored the earlier level9 path after level12 increased original APFS allocation. Source `2aaaf8d8f` registered the nine family-3 matrix cells as NOT_RUN and added a labelled per-state block probe without changing production LOC.

| Round | Family / selection | Declared source | Result / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-block-diagnostic-r026 | intended stride1/157 count-driven allocation probe; **no candidate sample** | `2aaaf8d8f`, but stale invoked binary SHA `d0b04009...` instead of built SHA `8f52b440...` | **INCOMPLETE diagnostic**: old binary emitted zero per-state probe counters; exit0, wall167.802s; no new correctness/storage/time verdict; r021/r025 FAIL unchanged | [Diagnostic report](experiments/20260930-history-block-diagnostic-r026.md) | Publish after commit; link in next real round |

## Diagnostic round 20260930-history-block-diagnostic-r027

R026 issue publication: [#286 comment5898878612](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5898878612), report commit `5a8676624` (production delta+0). Corrected target-path resolution and SHA assertion before launch; source remains clean and product code is unchanged.

| Round | Family / selection | Declared source | Result / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-block-diagnostic-r027 | stride1/157 original-owner C2/C5 block progression; **no candidate sample** | `5a8676624`, benchmark-target binary SHA `8f52b440...` | Complete 157 states ×4 probes, exit0, wall165.179s; C2 allocation jumps exactly16,777,216 B at state146 for217,088 B apparent growth, then stays fixed; final files byte-identical to r021, strict official r021 FAIL139264 B unchanged, numeric time INELIGIBLE | [Diagnostic report](experiments/20260930-history-block-diagnostic-r027.md) | Publish after commit; link in next real round |

## Selected round 20260930-history-v3-r028

R027 issue publication: [#286 comment5899036121](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5899036121), diagnostic report commit `4dfa55261` (production delta+0). Prospective product source `6fd7fce46` selects new-Store incremental auto-vacuum and reclaims at most one free page inside each save transaction (+14 Core production LOC); no gate or timeout changed.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v3-r028 | compound v3 stride10/17 then stride3/53, one changed-source sample each | `6fd7fce46` | **PASS/PASS**: strict allocated46473216<49344512 and58146816<64024576 B; full tree/selected content/independent O3+roots/C5/cleanup PASS, driver44.438/72.975s, verifier2.838/8.063s within original limits; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v3-r028.md) | Publish after commit; link in next real round |

## Explicit round 20260930-history-stride1-v3-r029

R028 issue publication: [#286 comment5899224558](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5899224558), selected report commit `2d4f537a5` (production delta+0). Stride1 reused the exact r028 product/harness compilation seal and binary, with its own fresh output and no resampling of a selected cell.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v3-r029 | compound v3 stride1/157, one explicit sample | `2d4f537a5`, product source `6fd7fce46` | **FAIL storage only**: strict allocated84090880>=83947520 B (+143360), 4096 B worse than level9 r021; all904143 paths/76726 selected content states/157 independent roots/O3/C5/cleanup PASS, driver163.758s<170s and verifier20.912s<30s; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v3-r029.md) | Publish after commit; link in next real round |

## Diagnostic round 20260930-history-preallocation-diagnostic-r030

R029 issue publication: [#286 comment5899332158](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5899332158), failure report commit `5c6c2ffe6` (production delta+0). Product source `a49c0d2ed` reverted the incremental-vacuum treatment (-14 Core production LOC), leaving the exact earlier level9 product files. Diagnostic script source `62ed81588` changes production LOC by0.

| Round | Family / selection | Source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-preallocation-diagnostic-r030 | 157-state original C2 allocation-order probe with external watcher; **no candidate sample** | `62ed81588`, level9 binary SHA `8f52b440...` | 73 successful small `F_PREALLOCATE` calls; child166.738s, independent verifier20.733s, final C2+C5 diagnostic allocation78839808 B with bytes SHA-identical to r021, no product gate verdict; official r021 storage FAIL139264 B unchanged | [Diagnostic report](experiments/20260930-history-preallocation-diagnostic-r030.md) | Publish after commit; link in next real round |

## Selected round 20260930-history-v3-r031

R030 issue publication: [#286 comment5899556402](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5899556402), diagnostic report commit `aa1734552` (production delta+0). Product commit `aed28dda1` adds safe, platform-gated, bounded pre-pack reservation paid inside the C2 save (+104 Core production LOC); the benchmark workspace's separate lock was updated at `8b7149420` (production delta+0). Original limits, workload and pins remain unchanged.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v3-r031 | compound v3 stride10/17 then stride3/53, one changed-source sample each | `8b7149420`, product source `aed28dda1` | **PASS/PASS**: strict allocated48279552<49344512 and59899904<64024576 B; complete tree/selected content/independent O3+roots/C5/cleanup PASS, driver42.841/74.208s, verifier2.903/8.310s within original limits; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v3-r031.md) | Publish after commit; link in next real round |

## Explicit round 20260930-history-stride1-v3-r032

R031 issue publication: [#286 comment5899787801](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5899787801), selected report commit `1bbffd2f6` (production delta+0). Stride1 reused the exact r031 product/harness compilation seal and binary, with a fresh output path and its own single invocation.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v3-r032 | compound v3 stride1/157, one explicit sample | `1bbffd2f6`, product source `aed28dda1` | **PASS**: strict allocated78839808<83947520 B (margin5107712); all904143 paths/76726 selected content states/157 independent roots/O3/C5/cleanup PASS, driver164.904s<170s and verifier20.882s<30s; C2/C5 SHA-identical to r021; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v3-r032.md) | Publish after commit; link in next real round |

## Earlier-family regression attempt 20260930-init-regression-r033

R032 issue publication: [#286 comment5899894816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5899894816), stride1 PASS report commit `cdf146ee9` (production delta+0). The frozen order requires one family-1 regression check before advancing to family3.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-init-regression-r033 | default Init 100 then1,000 requested; build failed before either sample | `cdf146ee9` | **BUILD FAIL** `E0004`: service mapper lacks new `StorageError::Io(_)`; build2.998s<30s; 100/1000 and explicit10,000/100,000 all NOT_RUN, no command/verifier/cache/cleanup verdict; family2 candidate PASS receipts unchanged, checkpoint pending | [Round report](experiments/20260930-init-regression-r033.md) | Publish after commit; link in next real round |

## Platform correction 20260930-linux-reservation-correction-r034

R033 issue publication: [#286 comment5900006238](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900006238), build-failure report commit `d5a9fafe8` (production delta+0). Product commit `d4ca6122f` maps the new typed C2 I/O failure to wire `Code::Io` (+1 Core production LOC). A source review then identified that Linux `posix_fallocate` would grow SQLite EOF; r034 changes it to `fallocate(FALLOC_FL_KEEP_SIZE)` and verifies one public save in a locked arm64/musl bundled-SQLite diagnostic build.

| Round | Family / selection | Source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-linux-reservation-correction-r034 | platform semantic verification; **no benchmark sample** | source in this commit | Default cross link INCOMPLETE (missing system `libsqlite3.so`); locked published bundled-SQLite arm64 build PASS, isolated Linux container public-save test PASS1/1; Mac test PASS1/1; current-source family2/Init gates NOT_RUN | [Correction report](experiments/20260930-linux-reservation-correction-r034.md) | Publish after commit; link in next real round |

## Selected round 20260930-history-v3-r035

R034 issue publication: [#286 comment5900153081](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900153081), Linux correction commit `d6f736edc` (+5 Core production LOC). The frozen history profile and strict original-owner gates remain unchanged; source and compilation seals are newly pinned.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v3-r035 | compound v3 stride10/17 then stride3/53, one corrected-source sample each | `d6f736edc` | **PASS/PASS**: strict allocated48279552<49344512 and59899904<64024576 B; complete trees/selected content/independent roots/O3/C5/cleanup PASS; driver44.176/77.213s, verifier2.959/8.520s within unchanged limits; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v3-r035.md) | Publish after commit; link in next real round |

## Explicit round 20260930-history-stride1-v3-r036

R035 issue publication: [#286 comment5900217058](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900217058), selected PASS report commit `57c511a2c` (production delta+0). The explicit stride1 used the same binary/compilation/harness seals with a new output path and one attempted invocation.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v3-r036 | compound v3 stride1/157, one explicit sample | `57c511a2c`, corrected product `d6f736edc` | **TIMEOUT/TARGET_MISS** 170.018s>170s; partial156 Layers/156 Branches/155 Commits, no complete root trace or verifier, partial allocated76742656 B is storage INCOMPLETE, semantic and cleanup INCOMPLETE, numeric time INELIGIBLE; no unchanged resample | [Round report](experiments/20260930-history-stride1-v3-r036.md) | Publish after commit; work stops pending explicit resume |
