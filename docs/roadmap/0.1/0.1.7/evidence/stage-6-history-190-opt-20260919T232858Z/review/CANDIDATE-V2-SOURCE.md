# Independent review of candidate v2

> Status: Research. Source and retained test-output review; performance review follows separately.

No actionable source correctness defect was identified in the revised batching implementation. It preserves the original sequence of reducer mutations: supplied values are registered first; directory binding effects are observed in original input order; the original `contents` map then supplies ascending final directory values; other supplied values follow. Only immutable-base acquisitions are grouped and reused. This directly removes the cause of the first candidate's quota regression.

`input.check()` rejects a zero base-read batch before the new `chunks` call. The additional parent/metadata windows are capped at `min(base_read_batch, MAXIMUM_READ_DEMANDS)` (4096 maximum). Existing parents are filtered in input order, so binary searches match the returned demand-order answers. Newly allocated/unreachable parents and absent-base builds are excluded from directory base requests. Empty demand vectors do not invoke the provider. A supplied typed value still precedes base metadata, and the content root is still overwritten with that directory's resulting root. Existing parent-kind checks and all earlier validation remain present.

Reuse is deliberately limited: only the final nonempty directory-parent batch's authenticated records survive the directory phase. Earlier omitted values are reread in bounded groups during final overlay. No validation memo is widened, no complete-parent record map is added, and the original contents map remains. At most the retained window and a current window of optional base records coexist, plus bounded serial/reference vectors and the existing lookup working state. Canonical identity and authentication assumptions are unchanged. Grouping reads may change first-error selection when multiple objects are faulty; no successful result or retry is substituted.

## Independent external-test evidence

The coordinator's candidate-v2-focused log reports all four tests passing. The reviewer compared every quota result with the pre-change baseline: **all 56 success/error outcomes match exactly**, including canonical roots and error limit/actual values. This covers the eight first-candidate regressions and the two altered refusal amounts. It is a tested matrix, not proof of every possible resource/input combination; preservation of reducer event order supplies the accompanying source argument.

| Metadata input / version | Batch 1 objects | Batch 3 objects | Batch 64 objects |
|---|---:|---:|---:|
| Omitted / baseline | 23 | 17 | 15 |
| Omitted / candidate v2 | 22 | 10 | 6 |
| Explicit / baseline | 18 | 12 | 10 |
| Explicit / candidate v2 | 18 | 9 | 6 |

For the fixture's full batch, canonical roots remain unchanged and object acquisitions fall from 15 to 6 (9 fewer, exactly 60%). The small-window rows show that complete reuse is not claimed: width 1 only reuses the final parent's base record. All numbers describe public provider acquisitions, not physical I/O or latency. Initial/new/unreachable parents, metadata precedence, removals and actual spill cleanup are covered by the companion tests.

The architecture addendum describes the implemented window reuse and preserved event order accurately. It also retains the rejected immediate-insertion attempt and its quota limitation. No product source, test, benchmark or build was executed or modified by the reviewer.
