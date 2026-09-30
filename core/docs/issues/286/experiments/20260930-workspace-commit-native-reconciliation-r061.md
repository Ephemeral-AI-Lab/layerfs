# #286 r061: affected native reconciliation checkpoint PASS

**5/5 selected functional controls PASS** at product94af0d79b / measured source2b1302454 after the two-LOC memory-ownership fix. One final affected cohort, not an unchanged-arm repeat: full64 MiB lowering and complete old/G1/new/G2/pin bytes, occupied4 MiB Commit and refund/clean close, deterministic liveG1/G2 with held selection, known canonical/local-C5 fault and same-selector resume, and ordinary normalized Base copy/append/resize all passed. Checked product cleanup PASS for all five; shared closed masters and release binary reused.

| Native component case | Functional command ns / bound | Correctness | Product cleanup | Cache |
| --- | ---: | --- | --- | --- |
| workspace-commit-full-lowering-size-64mib-native-v2 | 18375384209 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-headroom-quota-4mib-native-v2 | 170988208 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-live-g1-g2-native-v2 | 193810042 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-local-c5-native-v2 | 123245042 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-reordered-base-copy-native-v2 | 159363417 / 60 s | PASS | PASS | INELIGIBLE |

Full lowering Stage45,303,250 ns <10 s, final67,108,662 B, exactly8 replacement bytes; complete18,375,384,209 ns <60 s. Occupied4 MiB Commit1,401,625 ns, exactly zero ordinary quota free before it, full G1/G2 correctness/refunds and checked zero allocation close. Exact source/artifact/image/fixture/clone identities and retained child stdout are in [compact receipts](20260930-workspace-commit-native-reconciliation-r061-receipts.json). All native component cache verdicts INELIGIBLE; no SDK speed claim.

Command: `python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-commit-native-reconciliation --out benchmark-results/fs-bench-pro/issue286-workspace-commit-native-reconciliation-r061`. Stage-only2 MiB, pre-reconciliation known/unknown faults, directory-only pin admission and earlier F1/F2/stopping proofs explicitly reused because their execution path did not change. Counts fromr060 reused and not repeated. Next: rebuilt SDK retained controls and one affected F3 representative. Production LOC135,638 (reference65,417/Core70,221), this report delta0.
