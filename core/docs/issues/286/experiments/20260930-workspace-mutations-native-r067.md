# Family6 native r067 — partial, observer profile refusal

Source348d07d5f, tree4ffe66e7be3986ebc67cacd4e2e82b1049873ab6, clean locked release. Current family result INCOMPLETE; cache numeric INELIGIBLE.

| Exact native selection | Command ns /15s | Functional result | Product cleanup |
| --- | ---: | --- | --- |
| mixed-live-g1-g2 |412,191,375 |PASS: complete old/G1/G2/pin trees, modes/mtimes/targets/aliases/refcounts, known parents |PASS: zero allocated/reserved |
| mixed-known-unknown-custody |112,667,500 |FAIL: independent branch observer request refused Unsupported in first fault clone |UNKNOWN/unqualified after panic; retained Linux case copied, owned external teardown PASS |
| mixed-local-c5-resume |N/A |NOT_RUN: previous case failed |N/A |

The observer at phase_b_mutations.rs:452 used Request.profile1 (content) for HistoryQuery::GetBranch. The public Request::validate contract requires HISTORY_PROFILE2 for both HistoryQuery/HistoryCommand and returns Unsupported before dispatch for a mismatched profile (request.rs:497–522). This is a test observer construction error, not a mutation/product correctness failure. Before that observer, the known-before-Commit disposition, absent installed revision, submission custody and refusal to replay/close assertions had passed, but branch, full pin and physical custody checks were not completed and cannot be labelled PASS. The second fault clone never ran.

Correct the observer to HISTORY_PROFILE. Register an explicit native-tail selection (the same final two frozen profiles/bounds/order) to avoid rerunning the successful mixed live arm. Collect tail once asr068; then first SDK group and separate proof asr069. Reuse this r067 live proof with its original source/binary labels; observer-only correction leaves its measured path and all production libraries unchanged. No Budget/worker/quota/deadline/profile-gate relaxation, compression or earlier-family rerun.

Release native build10,563,068,958ns /30s; archived binary217d64100271fe5c410ebb070b2faa0db8a344600eb16d51bb381c2abb20d506. Small fixture reused and independent byte-cloned/hash-checked. External container/volume cleanup return0,156,156,417ns/21,706,458ns. [Compact receipts](20260930-workspace-mutations-native-r067-receipts.json) bind source/harness/dependency/image/fixture identities, all rows/commands, original failure stdout/stderr. Raw benchmark-results/fs-bench-pro/issue286-workspace-mutations-native-r067 stays append-only.

Focused Python mutation7/namespace5/native3 checks passed before source seal; native release compilation passed. At final changed source, run covering Clippy/fmt/boundary once. Production LOC135638→135638(delta+0), reference65417/Core70221, exact first-parent/staged production-only archive counter per commit.

Owner-directed270-level validation/reservation limitation is [reported to#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903016828) and deferred. Family6 is the active work; no all-seven, numeric or release admission is claimed.
