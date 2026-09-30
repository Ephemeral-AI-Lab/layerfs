# Family6 native tail r068 —2/2 PASS;3-profile native checkpoint

Measured clean source0848cac83, required HistoryQuery observer profile2 corrected fromr067; case order/bodies/15s limits unchanged.

| Native selection | Functional child ns /15s | Custody/result | Cleanup |
| --- | ---: | --- | --- |
| mixed known/unknown failures |339,306,250 |PASS: full selected/pinned bytes/modes/mtimes/symlink targets/aliases; public C5 branch oracle; known-before-Commit and Unknown custody, no replay |EXPECTED_RETAINED, copied before checked external teardown |
| mixed known canonical/local-C5 resume |368,167,584 |PASS: same selector and same known Commit, canonical calls1→1; later G2 known Commit, full G1/G2/pin and parentage |PASS: allocated/reserved0 |
| mixed live G1/G2 (r067 reused) |412,191,375 |PASS: deterministic canonical-response gate, old/G1/G2/pins, aliases/refcounts/selected mtimes |PASS: allocated/reserved0 |

All three native profiles have qualified functional coverage at their original bound, SDK time N/A, numeric latency INELIGIBLE. The live profile's relevant source/body and all production code are unchanged by observer-only repair; its original348d07d5f/217d641… binary identity remains inr067 and is not relabelled current or sampled again.

Known-before-Commit failure retains28,672 allocated+851,968 reserved=880,640B, canonical_calls0, branch head absent/genesis unchanged. Unknown canonical response retains36,864+843,776=880,640B, canonical_calls1; read-only public C5 query verifies actual committed root against complete independent mixed oracle. Neither query nor loss of result authorizes replay: another Commit and close remain Busy, submission and pin custody held, accounting complete. Retained Linux state was copied before removal of this owned container/volume. These are custody assertions, not clean-close or phase-memory peak claims.

The local-C5 control freezes Stage and mixed G1 pin, mutates G2, denies allocation on the saving thread after known canonical response, checks absent installed revision and known G1 bytes, restores normal main-thread execution and resumes exactly the same selector. Canonical calls stay1; later G2 creates the second known parented Commit. All names, modes, exact selected mtimes, symlink targets, alias identities/counts and G1/G2 bytes agree; selector/payload/pin cleanup and physical0/0 close pass.

[Compact receipts](20260930-workspace-mutations-native-tail-r068-receipts.json) contain exact source/tree/harness/dependency/binary/image/master/clone seals and every command. Raw path benchmark-results/fs-bench-pro/issue286-workspace-mutations-native-tail-r068 stays append-only; source/harness sealed before selection; one sample per changed-identity failed/unrun case. Small master byte-copy/hash/0:0 ownership reuse is recorded. Checked external container/volume cleanup150,868,375ns/25,235,500ns return0.

Next first SDK group r069 and separate full proof<9s; no earlier benchmarks or passing native arm replay. Unchanged r058 SDK stopping and r05432-lease/saved-root authority proofs are bound by source/compact/raw hashes in registry/run. All earlier families1–5, especially F2, are reused. Production LOC135638→135638(delta+0), reference65417/Core70221; exact first-parent/final staged production-only archive count per commit. No codec, product, quota/Budget/worker/deadline change.270-level and10240 architecture work stay deferred#276.
