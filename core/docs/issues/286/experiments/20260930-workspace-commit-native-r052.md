# #286 r052: native clone admission failure

**Lowering FAIL before any mutation; seven later cases NOT_RUN.** Source `ca9253e0b873aaa3847ac32fd76787b086785799`, native release binary `6971b001c59d01ee5929fa2349b6206bd0e8e95362fefbeedcfeb32eba533542`, locked bundled SQLite dependency feature for this native component binary only; 676,361,375 ns incremental official build. Both small and full64 MiB closed masters prepared successfully once (large preparation415,092,625 ns) and host/Linux byte hashes matched. The lowering child returned `Denied` at native attachment in55,464,125 ns, before edits or Stage. Product cleanup UNKNOWN; owned external container/volume removal PASS. No product slowdown or SQLite incompatibility was observed.

A separate **labelled permission diagnostic**, not another arm sample, copied one closed clone into an owned Alpine volume and measured only Linux uid/gid/mode: `/` and `/work` were0:0 mode755; the copied directory was **501:20 mode755**. Docker cp preserved the host directory owner. Workspace correctly refuses a private-backing ancestor owned by an unrelated uid (`runtime/host.rs::attach`). The correction explicitly assigns the cloned Linux test directory to its declared0:0 caller during untimed setup. It changes no bytes, quota, Budget, product policy, workload or timeout.

| Native component case | Functional command ns / bound | Correctness | Product cleanup | Cache |
| --- | ---: | --- | --- | --- |
| workspace-commit-full-lowering-size-64mib-native-v2 | 55464125 / 60 s | FAIL | UNKNOWN | INELIGIBLE |
| workspace-commit-stage-headroom-quota-2mib-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-headroom-quota-4mib-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-live-g1-g2-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-pin-custody-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-known-unknown-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-local-c5-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |
| workspace-commit-reordered-base-copy-native-v2 | UNAVAILABLE / 60 s | NOT_RUN | UNAVAILABLE | INELIGIBLE |

Reproduction: `python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-commit-native --out benchmark-results/fs-bench-pro/issue286-workspace-commit-native-r052`. Raw invocation/retained-state files stay append-only under that path; cause diagnostic is `benchmark-results/fs-bench-pro/issue286-native-permissions-diagnostic-r052` with checked owned external cleanup. Both are bound by the [compact evidence](20260930-workspace-commit-native-r052-receipts.json). Native component cache INELIGIBLE, no SDK/FUSE or numeric latency claim. Earlier F1/F2/F3 and successful SDK F4 controls are reused unchanged; no rerun.

Next: one corrected-source native selection using those same sealed masters; no repeated Init preparation. Required full SDK reordered-copy and deterministic stopping remain pending. Production LOC reference65,417 / Core70,219 / combined135,636, delta0.
