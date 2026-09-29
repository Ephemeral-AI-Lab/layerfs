# #286 round 20260930-history-stride1-v2-r012

> **Status:** Dated **FAIL/INCOMPLETE**: all157 measured states and C5 publications completed, but strict physical storage and immutable O3 fail, and the independent verifier exceeded its unchanged30 s bound. No family-2 qualification or release admission.

One changed-harness candidate used committed source [`6ab692386`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/6ab692386), the original157-state selection and strict170/30 s driver/verifier deadlines. The only new method is macOS CommonCrypto execution of the **same Git SHA-1 blob OID**; standard vectors and scalar/native differential tests through the largest1,241,221-byte corpus blob passed. Every actual corpus blob still passed SHA1/SHA256 checks before its timed C1/C2 construction; no product bytes, worker count, fixture, policy or cache contract changed. A separate corpus-only diagnostic observed the identical217,646 changed paths/1,711,057,104 blob bytes and 50.899 s scalar vs45.506 s system-SHA1 transition walls. These are **uncontrolled-cache diagnostics**, not a paired speed or product claim.

| Axis | Observed / frozen bound | Result |
| --- | --- | --- |
| Complete driver wall | **167,806,562,041 ns /170,000,000,000 ns** | PASS absolute command budget, margin2,193,437,959 ns; numeric time INELIGIBLE |
| C2/C5 retained custody | **157/157** published C2 saves; C5 **157** Layers/Branches,156 Commits, zero stages | persisted counts PASS; public deep reopen not completed |
| Actual exclusive C2+C5 allocated | **84,520,960 B /<83,947,520 B** | **FAIL**, excess573,440 B |
| Original canonical O3 | **871,337,620 B/104,618 objects** vs **871,588,115 B/104,705** | **FAIL**, −250,495 B/−87 objects; exactly the corrected old-Core full-reference totals, not new pins |
| Separate verifier | **30,016,170,334 ns /30,000,000,000 ns**, killed | **TIMEOUT/INCOMPLETE**; no complete O4/C5 public-reopen PASS |
| Read-only independent root vector check | all **157** candidate trace roots equal committed v2 reference TSV | diagnostic agreement only; native O1 gate not completed |
| Cleanup | closed owners, schema/integrity/sidecars/C5 rows clean | PASS on the completed driver; verifier still incomplete |

C2/C5 allocated **84,238,336/282,624 B**, apparent total **79,507,456 B**. C2 has1,671 packs/11,690 groups, 70,538,523 B pack blob comprising66,732,147 B bodies and3,806,376 B framing, zero unused pack capacity, and63 SQLite freelist pages. The Store's at-run allocated-vs-apparent gap remains substantial; a copied or vacuumed Store is never substituted. Storage is a valid hard FAIL now that all157 states and C5 rows are complete. The original historical O3 pins remain immutable; the observed full totals match the independently corrected older Core's complete reference Store, so only the historical migration comparison is disputed.

The original independent verifier timed out at30 s. A **separate count-driven diagnostic** made a labelled copy of the completed owners and used the exact same30 s watchdog with per-state progress enabled; it reached state114/157. Late completed states spent roughly0.44–0.47 s each, mostly listing/inode walks, so merely increasing the deadline is not the fix. That copy is no candidate gate, and the original verifier trace/receipt are unchanged. Future verifier work must preserve every-state tree/kind/size and the selected content oracle under the same bound, with any identity-based reuse explicit and bounded. The strict resource FAIL and historical O3 mismatch remain independent of verifier completion.

[Original receipt, SHA-indexed raw evidence, C5 file, native trace and separately labelled diagnostics](20260930-history-stride1-v2-r012/evidence-index.json) are retained; the large original C2 Store stays at its indexed local path. Stride10 r008 remains a distinct PASS, stride3 r009 retains O3 FAIL, and earlier stride1 timeouts r010/r011 remain unchanged. No Init repeat or family3–7 case ran. PR #285 stays draft/unmerged, #286 open, and no release admission is claimed.

Harness-only source commit `6ab692386` and this report-only commit each have exact production LOC reference65,417→65,417 (+0), Core70,045→70,045 (+0), combined135,462→135,462 (+0), compared from first-parent/staged/committed snapshots with `tools/production_loc.py --json --root <snapshot>` (counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
