# #286 r062: rebuilt public SDK retained controls PASS

**2/2 public SDK command/route/retained-custody/separate full canonical proof/checked cleanup PASS** at the changed reconciliation source. This is the one affected checkpoint at measured source2b1302454; previous unchanged controls were not resampled for better numbers. Prelude4097 dispersed writes, dirty-G1 lease before prelude Commit, both Commits, expected4097/4098 writes, final known UpToDate/new parented head, checked release/Status/unmount/Sandbox deletion all included in each15 s complete external command.

| Public SDK retained case | Complete command ns /15 s | Final Exec ns | Final Commit ns | Separate full verifier ns /9 s | Correctness / cleanup | Cache |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| workspace-commit-clean-retained-writes-4097-v3 | 9703319417 | 1594500 | 5356833 | 99629375 | PASS / PASS | INELIGIBLE |
| workspace-commit-one-edit-retained-writes-4097-v3 | 9691391667 | 4402708 | 16508375 | 99560458 | PASS / PASS | INELIGIBLE |

New release SDKef907c0b..., daemon15d1d506... and immutable imageaa7b881c... are source/seal matched in [compact receipts](20260930-workspace-commit-fast-checkpoint-r062-receipts.json). The unchanged independent verifierabfbe9f7... was reused. Source was rebuilt when necessary; no old image was silently used against changed product. Closed10 MiB master cloned independently with hashes. One construction worker. Separate verifiers checked both complete2-path/10 MiB trees, prelude-parent and final Commit parentage, with no performance replay. Full G1 byte read SKIPPED in these fast rows, supported by current native full old/G1/G2/pin proofr061 and unchanged public lease/31 KiB protocol proofr051/r056. Current fast rows do not manufacture a full pin digest.

Numeric cache INELIGIBLE; complete-command bounds and functional proof are separate from numeric admission. No eligible relative speed claim across machine windows. F1/F2 unaffected and reused. One affected F3 representative follows, not the entire9-cell or Family2 group.

```text
python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-commit --out benchmark-results/fs-bench-pro/issue286-workspace-commit-fast-checkpoint-r062
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-commit-fast-checkpoint-r062 --out benchmark-results/fs-bench-pro/issue286-workspace-commit-fast-checkpoint-proof-r062
```

Production LOC135,638 (reference65,417/Core70,221), report delta0.
