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

## Selected round 20260930-history-v4-r037

R036 issue publication: [#286 comment5900294298](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900294298), report commit `2e9b6809f` (production delta+0). The owner then authorized up to10% storage deviation; code/profile commit `60c7af879` restores payload3/group1 and freezes a separately versioned v4 gate before collection. The old strict receipts are unchanged.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v4-r037 | compound v4 stride10/17 then stride3/53, one sample each | `60c7af879` | **PASS/PASS**: C2+C5 allocated52473856<54278964 (+6.342% over original) and65142784<70427034 B (+1.747%); complete trees/selected content/independent roots/O3/C5/cleanup PASS, driver34.328/58.740s and verifier3.042/8.576s within unchanged bounds; numeric time INELIGIBLE. Generic verifier dispatch lacked v4 and is corrected after this sample. | [Round report](experiments/20260930-history-v4-r037.md) | Publish after commit; corrected harness seal requires a new selection before explicit stride1 |

## Selected round 20260930-history-v4-r038

R037 issue publication: [#286 comment5900732612](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900732612), report/dispatcher commit `f9a8b1b62` (production delta+0). This new selected invocation uses that corrected harness identity and the same low-cost codec product/compilation seal.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-v4-r038 | compound v4 stride10/17 then stride3/53, one sample each at corrected harness seal | `f9a8b1b62`, product `60c7af879` | **PASS/PASS**: C2+C5 allocated52473856<54278964 (+6.342%) and65142784<70427034 B (+1.747%); complete trees/selected content/independent roots/O3/C5/cleanup PASS; driver32.584/58.472s, verifier3.083/9.000s under original bounds; numeric time INELIGIBLE | [Round report](experiments/20260930-history-v4-r038.md) | Publish after commit; next explicit stride1 at same product/harness/compilation seal |

## Explicit round 20260930-history-stride1-v4-r039

R038 issue publication: [#286 comment5900776546](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900776546), selected report commit `0455a7e3c` (production delta+0). Stride1 reused the exact r038 product, compilation, harness and binary seals with a fresh output path and its own single invocation.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-v4-r039 | compound v4 stride1/157, one explicit sample | `0455a7e3c`, product `60c7af879` | **PASS**: C2+C5 allocated86179840<92342273 B (+2.659% over original strict target); all904143 path-states/76726 selected content path-states/157 independent roots/O3/C5/cleanup PASS; driver144.769s<170s, verifier21.370s<30s; numeric time INELIGIBLE | [Round report](experiments/20260930-history-stride1-v4-r039.md) | Publish after commit; current-source Family1 regression next |

## Earlier-family regression round 20260930-init-regression-v4-r040

R039 issue publication: [#286 comment5900830594](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900830594), explicit stride1 PASS report commit `5e775aa09` (production delta+0). With all three v4 history tiers at the same product/harness/binary seals, the required current-product-source Family1 default Init check became due.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-init-regression-v4-r040 | release SDK Init 100 then1,000 files, one sample each | `5e775aa09`, product `60c7af879` | **Functional PASS/PASS**: complete commands1.716/0.140s<15s, separate verifiers0.607/0.055s<9.5s, complete namespace/sampled bytes and cleanup PASS; raw SDK0.043/0.128s; numeric latency INELIGIBLE. Explicit10,000/100,000 NOT_RUN. Together with r038/r039 v4 history, Families1–2 scoped checkpoints pass. | [Round report](experiments/20260930-init-regression-v4-r040.md) | Publish after commit; Family3 matrix next |

## Explicit owner-requested SDK Init tier 20260930-init-100000-v4-r041

R040 issue publication: [#286 comment5900857750](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900857750), functional Init report commit `fc7ed1a30` (production delta+0). The owner requested all four Init tiers; r040's100/1,000 receipts are reused by exact product/harness/binary identity, and the contract runs100,000 before the remaining10,000 tier.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-init-100000-v4-r041 | explicit release SDK Init100,000 files, one sample; first fixture preparation outside timer | `fc7ed1a30`, product `60c7af879` | **Functional PASS**, runner `UNREGISTERED_DIAGNOSTIC`: SDK6.043s, complete command6.063s<15s, separate verifier1.712s<9.5s; all101001 paths and73 selected files/200286236 B, known root and cleanup PASS; numeric INELIGIBLE, no cold2.7s claim. Fixture preparation14.542s outside timer. | [Round report](experiments/20260930-init-100000-v4-r041.md) | Publish after commit; explicit10,000 next |

## Explicit owner-requested SDK Init tier 20260930-init-10000-v4-r042

R041 issue publication: [#286 comment5900890397](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5900890397), explicit100,000 functional report commit `0a8bac6fb` (production delta+0). The remaining10,000 case runs once under the same product, Init harness and release binary seals as r040/r041.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-init-10000-v4-r042 | explicit release SDK Init10,000 files, one sample; first fixture preparation outside timer | `0a8bac6fb`, product `60c7af879` | **Functional PASS**, runner `UNREGISTERED_DIAGNOSTIC`: SDK1.591s, complete command1.610s<15s, separate verifier0.653s<9.5s; all10101 paths and72 selected files/101928859 B, known root and cleanup PASS; numeric INELIGIBLE. Together with r040/r041, all four owner-requested tiers have one functional result. | [Round report](experiments/20260930-init-10000-v4-r042.md) | Publish after commit; Family3 matrix next |

## Owner pause checkpoint after r042 (2026-09-30)

The owner explicitly paused Phase B after the four-tier Family1 observation and requested an issue checkpoint. Families1–2 retain their scoped functional PASS receipts at the low-cost-codec product source; numeric latency is INELIGIBLE under uncontrolled source cache, and explicit10,000/100,000 Init tiers retain `UNREGISTERED_DIAGNOSTIC`. Families3–7 remain NOT_RUN. No Family3 measurement or product change begins until the owner resumes this work. The repository-wide agent rule now rejects spending about50% speed to recover about5% storage; the original strict history failures stay immutable, and the prospectively frozen v4 tolerance applies only to new v4 rows.

**Entrypoint-name erratum (2026-09-30):** The Init driver actually calls `ProjectApi::new(&server).init(...)` inside its SDK timer. The dated plan, runner metadata and r001/r040–r042 receipts incorrectly called that entrypoint `Client::init_project`. The original receipts and their labels remain unchanged; their observed bytes, times and functional outcomes are observations of the actual `ProjectApi::init` call, not evidence for a `Client` method. Forward-looking runner/benchmark guidance now names the real API. The owner-requested pause remains in force; no replacement sample is taken solely to repair a label.

## Family 3 round 20260930-workspace-write-r043

The owner resumed Phase B and explicitly requested the nine-cell Family 3 selection. Runner commit `b7460cb7c` uses the public `WorkspaceApi`/POSIX-FUSE route with a current release SDK driver, daemon image, independent old/new-head verifier, and the frozen #271 master. One append-only attempt was made for each cell. The source and output identities, exact nanoseconds, limits and proof counts are in the [round report](experiments/20260930-workspace-write-r043.md) and [compact receipts](experiments/20260930-workspace-write-r043-receipts.json).

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-workspace-write-r043 | full Workspace write matrix: append/dispersed/repeated × 100/512/4097, one sample each | `b7460cb7c` | **9/9 functional PASS**, exact old/new bytes, known parented Commit, route counts and cleanup; full commands 0.985–10.130 s under unchanged 15/25 s limits; separate verifiers 0.094–0.120 s under 9 s. **9/9 numeric INELIGIBLE** from uncontrolled source/Exec-to-Commit cache; Family 3 speed admission remains open. | [Round report](experiments/20260930-workspace-write-r043.md) | Publish after report commit; next implement enforceable symmetric cache contract before a new selection |

## Family 3 cold-source round 20260930-workspace-write-cold-r044

R043 issue publication: [#286 comment5901345893](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5901345893), functional report commit `56c84c41c` (production delta+0). Method commit `6844f072a` prospectively froze [the Darwin cold-source/Linux direct-backing profile](WORKSPACE-WRITE-COLD-CONTRACT-V1-20260930.md) and a new SDK driver device-read counter before the next selection. The nine case IDs, write schedule, oracle and 15/25/9 s bounds remained unchanged. Prior r043 statuses are unchanged.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-workspace-write-cold-r044 | full Workspace write matrix: append/dispersed/repeated × 100/512/4097, one new-method sample each | `6844f072a` | **9/9 PASS**: zero Store/history residency at launch, 712704 device-read B ≥641433-B frozen floor, direct-I/O source seal, exact old/new bytes, known parented Commit, callback counts and cleanup; full commands 0.974–10.213 s under unchanged 15/25 s; verifiers 0.093–0.117 s under 9 s. No numeric-ineligible or unrun cell. | [Round report](experiments/20260930-workspace-write-cold-r044.md), [compact receipts](experiments/20260930-workspace-write-cold-r044-receipts.json) | Publish after report commit; earlier-family regression checkpoint next |

## Family 1 regression round 20260930-init-regression-r045

R044 issue publication: [#286 comment5901468648](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5901468648), Family 3 PASS report commit `530dfa536` (production delta+0). The once-due earlier-family default Init check used that clean source and one sample per selected case; the explicit 10,000/100,000 tiers were not selected or resampled.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-init-regression-r045 | public release SDK Init 100 then1,000 files | `530dfa536` | **Functional PASS/PASS**: raw SDK34.812/114.956 ms, full commands56.743/127.828 ms<15 s, separate verifiers38.252/49.708 ms<9.5 s, registered scope and cleanup PASS. Numeric latency INELIGIBLE under unchanged Init cache profile; explicit10,000/100,000 NOT_RUN. | [Round report](experiments/20260930-init-regression-r045.md), [compact receipts](experiments/20260930-init-regression-r045-receipts.json) | Publish after report commit; Family 2 regression next |

## Family 2 selected regression round 20260930-history-regression-r046

R045 issue publication: [#286 comment5901496249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5901496249), Family 1 report commit `a12ab932e` (production delta+0). At that clean source, the current v4 stride10/17 and stride3/53 selection used one new sample each. The exact same worktree-local release backend binary was reused by compilation seal, but the changed broad harness identity required a new receipt. Old profiles and the explicit stride1 remain unrun in this selected invocation.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-regression-r046 | compound retained history v4 stride10/17 then stride3/53 | `a12ab932e` | **PASS/PASS**: C2+C5 allocated52,473,856<54,278,964 and65,142,784<70,427,034 B (+6.342%/+1.747% vs original strict target); independent roots, O3, full trees/selected bytes, cleanup PASS; driver32.153/52.446 s<60/170 s, separate verifier3.034/8.407 s<10/20 s. Numeric time INELIGIBLE under unchanged component cache profile. | [Round report](experiments/20260930-history-regression-r046.md), [compact receipts](experiments/20260930-history-regression-r046-receipts.json) | Publish after report commit; explicit stride1 v4 next |

## Family 2 explicit regression round 20260930-history-stride1-regression-r047

R046 issue publication: [#286 comment5901547214](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5901547214), selected Family 2 report commit `b2ea44f0f` (production delta+0). The explicit stride1/157 case used the exact same backend compilation, binary and Family 2 harness seals as r046, with one new invocation and fresh output. The prior selected rows were not resampled.

| Round | Family / selection | Measured source | Results / required gates | Report | Issue progress |
| --- | --- | --- | --- | --- | --- |
| 20260930-history-stride1-regression-r047 | compound v4 stride1/157 explicit | `b2ea44f0f` | **PASS**: C2+C5 allocated86,179,840<92,342,273 B (+2.659% over original strict target), 157 independent roots, 904,143 complete path-states, 76,726 selected content path-states, O3/C5/cleanup PASS; driver136.703 s<170 s, separate verifier21.003 s<30 s. Internal operation timer clipped; numeric time INELIGIBLE. | [Round report](experiments/20260930-history-stride1-regression-r047.md), [compact receipts](experiments/20260930-history-stride1-regression-r047-receipts.json) | Publish after report commit; Families 1–3 scoped checkpoint complete, Family 4 next |

## Post-publication cache audit of r044 (no new sample)

The frozen #273 checkpoint-5 spec says cold host source plus private Linux direct I/O does not prove the full Exec-to-Commit cache domain. The r044 evaluator wrongly promoted that partial proof to numeric `PASS`. The [append-only correction](experiments/20260930-workspace-write-r044-cache-audit.md) supersedes that **numeric admission claim**: r044 remains 9/9 functional/command/verifier/cleanup PASS, but all nine complete-command numeric results are `INELIGIBLE`. The original receipts and r044 report remain unmodified, and no arm was rerun. The r047 earlier-family results still stand; its conclusion that all three families met the full checkpoint does not. Family 4 can proceed with independent functional/fast-path work, while Family 3's numeric speed gate remains open. The owner directed that benchmark/example-only changes should not trigger repeated earlier-family groups, especially Family 2.

## Family 4 first fast attempt r048

The r044 cache correction was published in [#286 comment5901691380](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5901691380), source/evaluator commit `4c20cdfab`. Family 4 code/profile commit `13a9273d8` adds the same-Workspace live lease controls and separate retained-Store proof command, with exact image reuse and no automatic earlier-family runs. The owner explicitly authorized continuing Family 4 on this fast path.

| Round | Family / selection | Measured source | Results / required gates | Report | Next action |
| --- | --- | --- | --- | --- | --- |
| 20260930-workspace-commit-fast-r048 | SDK clean then one-edit retained 4097-write controls v2 | `13a9273d8` | **FAIL / NOT_RUN**: clean outer command15.006 s timeout at unchanged15 s, no final SDK receipt, full pin digest incomplete53/640 reads, product cleanup UNKNOWN; separate external cleanup PASS. One-edit stopped before invocation. Verifier SKIPPED, numeric INELIGIBLE. | [Report](experiments/20260930-workspace-commit-fast-r048.md), [compact evidence](experiments/20260930-workspace-commit-fast-r048-receipts.json) | Separate full pin oracle from fast performance in a prospectively versioned profile, preserve all15/60/9 s bounds and earlier-family proof reuse |

## Family 4 v3 fast controls and retained proof r049

R048 was published in [#286 comment5901873223](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5901873223). New profile/fix commit `9bdc11dc1` prospectively separates full pin bytes into a distinct functional selection, keeping both fast controls at 15 s. No earlier-family benchmark reran.

| Round | Selection | Measured source | Outcomes | Report | Next action |
| --- | --- | --- | --- | --- | --- |
| 20260930-workspace-commit-fast-r049 + separate proof | SDK clean then one-edit retained controls v3, one attempt each; proof reads retained outputs without performance replay | `9bdc11dc1` | Clean **COMPLETE_DIAGNOSTIC**: full13.957 s<15 s, final Exec1.507 ms/Commit5.884 ms known UpToDate, held lease/release/cleanup PASS; independent old/new canonical proof **PASS**103.849 ms<9 s, full pin bytes SKIPPED. One-edit **FAIL**123.118 ms at create InvalidInput from overlong label, no owned resource, proof NOT_RUN. Numeric INELIGIBLE. | [Report](experiments/20260930-workspace-commit-fast-r049.md), [compact receipts](experiments/20260930-workspace-commit-fast-r049-receipts.json) | Bounded shared label fix; rerun only refused one-edit at new driver identity; reuse clean proof and all unchanged earlier-family evidence |

## Family 4 one-edit and full pin oracle r050/r051

R049 was published in [#286 comment5901908056](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286#issuecomment-5901908056), bounded-label/report commit `9d55fa41f`. Only the refused one-edit case was newly attempted; the successful clean control and its proof were reused by unchanged production, operation and verifier scope. The separately registered full pin functional selection then ran once at the same source. Each independent verifier read the retained Store/history without performance replay.

| Round | Selection / role | Measured source | Outcomes | Evidence | Next action |
| --- | --- | --- | --- | --- | --- |
| 20260930-workspace-commit-one-edit-r050 + separate proof | one-edit retained v3 fast control | `9d55fa41f` | **Complete diagnostic + canonical PASS**: full11.826 s<15 s, final SDK Exec4.712 ms/Commit17.603 ms, known new parented Commit, 4098 writes, held lease/release/cleanup PASS; separate canonical verifier99.190 ms<9 s. Full pin bytes SKIPPED here; numeric INELIGIBLE. | [Report](experiments/20260930-workspace-commit-one-edit-r050.md), [compact evidence](experiments/20260930-workspace-commit-one-edit-r050-receipts.json) | Full pin functional oracle next; no clean/earlier-family replay |
| 20260930-workspace-commit-full-pin-r051 + separate proof | full 10 MiB retained G1 pin oracle, declared31 KiB SDK reads, functional-only | `9d55fa41f` | **Functional PASS**: full18.956 s<60 s, all10,485,760 pin bytes/hash after later mutation and two known Commits, checked release/cleanup PASS; separate complete old/new canonical proof99.114 ms<9 s. Not a15 s speed row; numeric INELIGIBLE. | [Report](experiments/20260930-workspace-commit-full-pin-r051.md), [compact evidence](experiments/20260930-workspace-commit-full-pin-r051-receipts.json) | Native lowering/headroom/live/failure group and deterministic SDK stopping proof; unaffected earlier-family evidence remains reused |

- **r052** — [native clone admission failure and permission cause diagnostic](experiments/20260930-workspace-commit-native-r052.md): lowering Denied before edits,55,464,125 ns; seven NOT_RUN, external cleanup PASS; masters prepared/sealed once. Linux copied ancestor501:20 caused correct refusal; assign declared0:0 ownership during setup, then use same masters. No production delta or earlier-family rerun.

- **r053** — [full64 MiB lowering PASS / occupied filler FAIL](experiments/20260930-workspace-commit-native-r053.md): full lowering18,091,738,750 ns, Stage113,870,334 ns, eight replacement bytes, full old/G1/G2/pin and zero cleanup PASS. Occupied Stage helper failed its4 KiB check before Stage (97,458,916 ns), six NOT_RUN. Reclaim released input before the filler baseline; reuse lowering/masters/earlier families. Production delta0.

- **r054** — [remaining7 native controls PASS](experiments/20260930-workspace-commit-native-tail-r054.md),0.11–0.19 s each. Stage31,967,083 ns from completely occupied2 MiB same fund; Commit1,371,875 ns from completely occupied4 MiB. G1/G2, pins, known/unknown no-replay custody, local-C5 same-selector resume and ordinary Base copy PASS. Expected retained states archived, external cleanup PASS; lowering/masters/earlier families reused. Production delta0.

- **r055** — [SDK stopping handshake FAIL](experiments/20260930-workspace-commit-sdk-stopping-r055.md),12,841,067,292 ns<15 s. FD opened but readiness lacked newline; read returned1, intended proof not reached, checked unmount FAIL / SDK deletion PASS. Correct line protocol before new attempt; no product change.
- **r056** — [SDK retained reordered-copy PASS](experiments/20260930-workspace-commit-sdk-reordered-r056.md),19,204,189,375 ns<60 s, Exec16,710,334 ns, Commit31,669,167 ns,4099 writes, all10 MiB G1 pin and separate full canonical proof100,304,875 ns<9 s; checked cleanup PASS, numeric INELIGIBLE. Reuse successful cases/masters/earlier families; production delta0.
