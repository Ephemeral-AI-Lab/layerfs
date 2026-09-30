# #286 r045: Family 1 default regression after Family 3

**Functional PASS/PASS** for the registered public SDK `ProjectApi::init` 100/1,000-file default selection. The source-cache-uncontrolled Init profile still makes numeric latency `INELIGIBLE`; this does not change the separate Family 3 cold-qualified result. Explicit 10,000/100,000-file tiers are visible as `NOT_RUN` in this default selection; their earlier owner-requested r041/r042 observations remain historical.

Clean measured source `530dfa5363c7e0bd38d6ab6daf94a30da65302b5` (tree `d3642c234b8bf681ccc8fb0c6add4a7082794470`), product seal `e67bbe4e80924ee49d044d90e6d90fe24f95f42ec5866295c5c99fa54d62b47b`, harness seal `aa620053c306727791654f6879f84a57b2b15ef383f4bb9b37d1bc4024f3d75f`, locked release driver `06a031846776ad00716d4a90c5a1166ccc25eccda55b19e68a1e5a45e079c0e8` and independent verifier `7c9fe69fd4a7977a78bf9b9f9b0169eba7fefddba5ca6ce55ccc4243dd74bab3`. The worktree-local incremental build took 0.100 s within its 30 s bound, and the family cycle took 0.398 s within its recommended 30 s. Prepared immutable fixtures were reused outside timers; each call created a fresh Store/history owner.

| Files / case ID | Raw SDK call ns | Complete command ns / 15 s | Separate verifier ns / 9.5 s | Verified paths; selected files / bytes | Functional / cleanup | Registration | Numeric cache |
| --- | ---: | ---: | ---: | --- | --- | --- | --- |
| 100 / `namespace-100-compact-v3` | 34,812,375 | 56,742,834 | 38,251,666 | 102; 53 / 3,354,003 B | PASS / PASS | registered default | INELIGIBLE |
| 1,000 / `namespace-1000-compact-v3` | 114,956,291 | 127,827,791 | 49,708,208 | 1,011; 70 / 6,430,827 B | PASS / PASS | registered default | INELIGIBLE |
| 10,000 / `namespace-10000` | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | explicit tier | NOT_RUN |
| 100,000 / `namespace-100000` | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | explicit tier | NOT_RUN |

The separate verifier reopened Store/history, checked all path/kind entries and the registered sample's complete metadata/content; it did not hash all fixture bytes. Both calls returned known roots. No daemon/FUSE scope or cold 2.7 s Init target applies. Source cache remained uncontrolled, so no numeric speed PASS is claimed. No default case failed or was resampled.

Command: `python3 core/benchmark/fs-bench-pro/runner.py run --family init_namespace --out benchmark-results/fs-bench-pro/issue286-init-regression-r045`. The raw append-only output is local to this worktree, manifest SHA-256 `1aef828f5f503cb925233afc446c9aa02cf2ab27737dec5494f77c08ea734465`; [compact receipt index](20260930-init-regression-r045-receipts.json) retains exact case statuses and hashes. `runner.py verify --run benchmark-results/fs-bench-pro/issue286-init-regression-r045` returned PASS. Next is the once-due Family 2 selected and explicit stride1 regression. This report-only commit has production LOC reference 65,417 → 65,417, Core 70,219 → 70,219, combined 135,636 → 135,636 (delta +0), counted on the exact first-parent/staged `crates` and `core/crates` snapshots with `tools/production_loc.py --json --root`.
