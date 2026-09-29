# #286 round 20260930-history-stride10-v2-r007

> **Status:** Dated failed candidate checkpoint; strict compound storage remains above its hard ceiling. All raw gates and the adapter's misclassified semantic status are retained.

One new stride10 v2 candidate invocation used committed source [`e4a56f327`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/e4a56f327), following the meaningful C2 pooled-tail close and bounded full-verifier change. No unchanged arm was re-sampled. The same fixed17-state corpus, independent v2 roots, original O3 pins and unchanged strict limits apply.

| Axis | Observed / strict bound | Status |
| --- | --- | --- |
| Complete driver process wall | **34,635,013,875 ns /60,000,000,000 ns** | PASS budget; numeric comparison INELIGIBLE (source cache uncontrolled) |
| Independent verifier wall | **4,242,603,208 ns /10,000,000,000 ns** | PASS budget |
| Full corpus tree/kind/size | **101,477/101,477 listed path-states**, all 17 states | native O4 PASS |
| Selected logical content | **8,631 selected path-states**, SHA256/length checked through public C1/C2 | native O4 PASS |
| Roots, canonical O3, C5 custody | **17/17** independent roots; **380,559,460 B/51,689 objects**; C5 public reopen all17 Layers/16 Commits | PASS |
| Actual exclusive C2+C5 allocated | **51,720,192 B /<49,344,512 B**, excess**2,375,680 B** | **FAIL**, native and Python |
| Cleanup and at-run integrity | C2/C5 closed, clean schema/row/integrity/sidecars, zero live C5 stages | PASS |
| Stride3/stride1 | unselected, receipts retained as NOT_RUN | NOT_RUN |

Actual C2/C5 allocated **51,634,176/86,016 B**; apparent **50,864,128/86,016 B**. C2 has690 packs/2,965 groups, 46,418,112 B blob, 45,326,352 B bodies and1,091,760 B framing. Its pooled-metadata unused capacity is **0 B**, down from r006's 2,384,415 B of unused capacity; the *measured actual allocated* decrease is **1,462,272 B**, while apparent decrease is **2,109,440 B**. The difference is SQLite page allocation and filesystem blocks, so the raw reserved-byte change is never equated to allocated savings. No threshold, buffer/pack limit, worker count, timeout or cache stance changed. The pooled tail is closed before each measured save publishes, inside its transaction.

The native trace has PASS for every correctness/custody gate including complete O4, plus two copies of the strict resource FAIL (performance and independent verifier). The Python adapter's original `correctness_status=FAIL` is a **classification bug**: it uses `parsed.status()`, the worst of all native gate classes, so the resource FAIL masks the passing semantic gates. The overall `status=FAIL` is correct and unchanged. The next source change will compute semantic and resource statuses independently; this receipt will not be rewritten. The full-size verifier method is prospectively disclosed in [the v2 profile amendment](../HISTORY-PROFILE-V2-20260930.md): full C1 listed tree/kind, checked C2 stored canonical widths for unselected whole-file/symlink sizes, public C1 FileState for chunked roots, and public content/hash for the fixed selected set. Its 4.243 s bound completion and native O4 PASS are a valid proof of that **declared** method, not a claim that unselected file bytes were all hashed.

[Exact original receipt, trace, phases, binary/build identity, full SHA inventory and compact raw evidence](20260930-history-stride10-v2-r007/evidence-index.json) are published; the large original C2 file stays at the indexed local path. Earlier r006 INCOMPLETE and v1 FAIL receipts remain. The family-2 storage checkpoint is **NOT_DUE**, and Init is not remeasured until history's required correctness and storage gates pass. Families3–7 remain NOT_RUN, PR #285 stays draft/unmerged, and #286 remains open. No CI or aggregate preflight ran.

Product source commit e4a56f327 records exact production LOC reference65,417→65,417 (+0), Core70,019→70,045 (+26), combined135,436→135,462 (+26). This report-only commit records reference65,417→65,417 (+0), Core70,045→70,045 (+0), combined135,462→135,462 (+0). Both use `python3 tools/production_loc.py --json --root <snapshot>` on exact first-parent/staged/committed snapshots, excluding benchmark/test/docs/tool lines (counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
