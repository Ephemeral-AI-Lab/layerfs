# #286 r060: fixed8192 completion and actual10240 custody PASS

**8192 successful Workspace Commit PASS** at the unchanged8 MiB memory Budget/64 MiB disk quota: complete12,900,220,000 ns <60 s, Commit1,554,850,709 ns <10 s. Full independent old/new/original-pin325,680-byte comparisons, one known canonical call, checked pin/handle release and zero allocation/reservation clean close all PASS. Source94af0d79b includes only the two-LOC deletion-key ownership fix; no codec/format or additional work/worker.

**10240 scoped known-canonical/local-C5 custody observation PASS**, complete16,972,545,167 ns <60 s, attempted Commit1,921,662,876 ns. Actual canonical root/Commit succeeded, local reconciliation still refused with KnownCommitLocalFailure and no installed revision. Full canonical saved/new and original held-pin bytes matched independent oracles; canonical_calls1; subsequent Commit/clean close Busy, no replay. Physical custody4,513,792 allocated+843,776 reserved B, accounting complete. Expected retained state was archived and owned external container/volume removal PASS. **This is not successful Workspace Commit or product clean close at10240.** The frozen plan requires the actual success or exact known-result/local-C5 refusal outcome; this proves the latter and does not manufacture or relax the old capacity boundary.

| Native component case | Functional command ns / bound | Correctness | Product cleanup | Cache |
| --- | ---: | --- | --- | --- |
| workspace-commit-native-count-writes-8192-v2 | 12900220000 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-native-count-writes-10240-v2 | 16972545167 / 60 s | PASS | EXPECTED_RETAINED | INELIGIBLE |

[Compact receipts](20260930-workspace-commit-native-counts-r060-receipts.json) retain all identities, cloned prepared inputs, source/compilation/dependency seals, release binaries and complete counters. Same prepared count master reused once, no repeated Init. Numeric cache INELIGIBLE, functional component roles, not SDK time or speed samples. Originalr059FAIL and charged diagnosis remain immutable.

Next is one prospectively frozen affected-reconciliation cohort and the public retained SDK controls at the rebuilt source; then one affected Family3 representative. No Family2 replay: its in-process C1/C2/C5 storage operations do not run Workspace reconciliation, and their codec/history implementation is unchanged. Stage-only/pin-directory/stopping/unrelated failures also explicitly reuse unaffected proof scope. Production LOC reference65,417 / Core70,221 / combined135,638; fix delta+2, PhaseB delta+199 from135,439.

Receipt erratum appended at checkpoint: the count master is **327,680 bytes**, as specified by fixture.before, prepared.json and the test's explicit assertion. The earlier prose's325,680 was a transcription error; no input/oracle/receipt was changed. A separate labelled10240 cause diagnostic (no replacement arm/latency claim) using the exactr060 binary confirmed `known=true installed_revision=NA class=capacity`, post-unwind Budget1,081,884 B and retained4,513,792+843,776 B. Its compact diagnostic reference is linked in the final checkpoint.
