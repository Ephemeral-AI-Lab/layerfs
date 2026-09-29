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
