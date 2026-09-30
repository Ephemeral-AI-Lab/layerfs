# #286 r054: remaining seven native controls PASS

**7/7 selected native functional controls PASS**, each0.11–0.19 s including docker exec launch/exit, with one constructor. Both closed masters reused, no Project Init repeated, and full64 MiB lowering fromr053 reused unchanged. Source `003ee6322`; exact release binary/build/source/image identities are in the [compact evidence](20260930-workspace-commit-native-tail-r054-receipts.json). Linux ext4/4 KiB backing, default8 MiB product memory Budget. No SDK/FUSE speed claim; uncontrolled cache INELIGIBLE.

| Native component case | Functional command ns / bound | Correctness | Product cleanup | Cache |
| --- | ---: | --- | --- | --- |
| workspace-commit-stage-headroom-quota-2mib-native-v2 | 123258000 / 60 s | PASS | EXPECTED_RETAINED | INELIGIBLE |
| workspace-commit-headroom-quota-4mib-native-v2 | 156553750 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-live-g1-g2-native-v2 | 187460125 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-pin-custody-native-v2 | 132329417 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-known-unknown-native-v2 | 146465750 / 60 s | PASS | EXPECTED_RETAINED | INELIGIBLE |
| workspace-commit-local-c5-native-v2 | 113702375 / 60 s | PASS | PASS | INELIGIBLE |
| workspace-commit-reordered-base-copy-native-v2 | 169159458 / 60 s | PASS | PASS | INELIGIBLE |

The occupied2 MiB Stage began with1,245,184 allocated +851,968 reserved =2,097,152 B, **zero free ordinary quota**. Stage31,967,083 ns converted8,192 B to allocation:1,253,376+843,776=the same2,097,152 B. Full base/candidate bytes and zero canonical Commit calls passed. Dropped Stage selector remains retained; repeated Stage/clean close correctly Busy. That custody was archived before owned external teardown, and is explicitly EXPECTED_RETAINED, not a product clean-close claim.

The occupied4 MiB Commit began at2,498,560 allocated +1,695,744 reserved =4,194,304 B, **zero free ordinary quota**. CommitStaged1,371,875 ns published known G1 and preserved exact live G2. Afterwards2,494,464+851,968=3,346,432 B; reservation/physical ownership was refunded without new ordinary capacity. Dropping the unrelated spare, reclaiming it, committing G2 and checked close left allocation0.

The existing deterministic canonical reply gate proves selected G1 bytes/pin stay isolated from live G2 and known completion credit. Pin admission/frozen names/checked release PASS. Known-C1 metadata denial retained16,384 allocated+851,968 reserved with canonical_calls0; delivered canonical Unknown retained24,576+843,776 with canonical_calls1, and repeated Commit/close refused without replay. Both failure states were independently cloned/archived and externally removed. Local-C5 failure resumed the exact same selector with canonical_calls still1, full bytes and zero-allocation close. Ordinary native backward Base-copy/append/resize/pin proof PASS. All clean controls passed their checked zero allocation/handle/custody assertions.

Reproduction: `python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-commit-native-tail --out benchmark-results/fs-bench-pro/issue286-workspace-commit-native-tail-r054`. Every selected status is retained, separate external container/volume removal PASS. Successful lowering, earlier SDK controls and F1/F2/F3 are explicitly reused for these external test/harness-only changes; no earlier resampling.

Next: public SDK stopping/refusal and arbitrary reordered-copy functional selections, prospectively frozen separately. Family4 complete admission remains open. Production LOC reference65,417 / Core70,219 / combined135,636, delta0.
