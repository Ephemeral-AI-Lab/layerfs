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
