# Family5 native namespace r065 — 10/10 functional PASS

Source `ecca96af962428d1b0f389c828b2448e971cdc30`, tree `d4039ffa8d8ee193fb4bfa673d965e7f9e8e950e`; locked release Linux native Workspace + in-process Server, default8MiB Workspace memory and one construction worker. Product code/codecs unchanged. This is component functional evidence, SDK time N/A and numeric latency INELIGIBLE.

| Exact case ID | Functional child command ns /15,000,000,000 | Correctness | Checked cleanup |
| --- | ---: | --- | --- |
| workspace-namespace-uncached-descendants-3-native-v1 | 206780458 | PASS | PASS |
| workspace-namespace-uncached-descendants-67-native-v1 | 1341718916 | PASS | PASS |
| workspace-namespace-resident-descendants-3-native-v1 | 206207709 | PASS | PASS |
| workspace-namespace-resident-descendants-67-native-v1 | 1410362000 | PASS | PASS |
| workspace-namespace-unrelated-resident-256-native-v1 | 4206949709 | PASS | PASS |
| workspace-namespace-deep-4097-uncached-native-v1 | 500132042 | PASS | PASS |
| workspace-namespace-deep-4097-resident-native-v1 | 483469292 | PASS | PASS |
| workspace-namespace-retained-live-g1-g2-native-v1 | 215163125 | PASS | PASS |
| workspace-namespace-rename-refund-native-v1 | 99988041 | PASS | PASS |
| workspace-namespace-rename-refusal-native-v1 | 93970625 | PASS | PASS |

Every command is under its bound. Move controls verify every old/new/pinned path, mode and byte and unchanged moved inode identities. Both deep controls reach4097-byte paths by component, read the full leaf, rename to4096 and move back; no full-path-limit lift. Live namespace control checks frozen G1/later G2 plus full old pinned tree. Rename refund checks escrow851968→0 and slots0→0 exactly; atomic refusal uses baseline0+8192-byte quota, preserves revision/names, closes both baseline and target. No quota/Budget/worker lift.

| Rename count control | Descendants | Resident nodes | Rename ns (diagnostic) | Upstream calls | Private reads/writes/new pages | Accounted memory delta B |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| workspace-namespace-uncached-descendants-3-native-v1 | 3 | 4 | 3144167 | 2 | 0/0/0 | 695 |
| workspace-namespace-uncached-descendants-67-native-v1 | 67 | 4 | 2378167 | 2 | 0/0/0 | 695 |
| workspace-namespace-resident-descendants-3-native-v1 | 3 | 8 | 1611666 | 1 | 0/0/0 | 704 |
| workspace-namespace-resident-descendants-67-native-v1 | 67 | 72 | 1360875 | 1 | 0/0/0 | 704 |
| workspace-namespace-unrelated-resident-256-native-v1 | 3 | 261 | 2651125 | 2 | 0/0/0 | 695 |

Observed work counts match3 vs67 uncached descendants (2 upstream calls,695B accounted delta), resident3 vs67 (1 call,704B delta), and3 descendants with256 unrelated files resident (2 calls,695B delta). This does not establish asymptotic independence: resident scans, descendant visits and C1 internal visits are UNAVAILABLE because the public counter surface does not expose them. Physical page counts are0 for these resident metadata operations; they do not prove no memory scan. Timing is a single uncontrolled-cache diagnostic, with no speed ratio or improvement claim.

Native executable SHA256 `1344d5206e20d99fd727a86bda4d3be426a18a501bc93b2f08f7bdcffcf6eb4f`; build 3067547417ns /30s, locked-release-build. Fixed Alpine image `sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`; ext4 `ef53 4096`. Independent clones have SHA256 checks in host/Linux and recorded0:0 Linux ownership. Small master reused; wide/deep/unrelated masters created once, closed and sealed. Preparation/cloning/builds remain outside the native child; public work, in-child proof and product cleanup remain inside.

External container removal 165637750ns and volume removal 28468042ns both return0. Native receipt records are functional component commands, not SDK lifecycle latency or a phase-memory gate. All raw files remain at `benchmark-results/fs-bench-pro/issue286-workspace-namespace-native-r065`; [compact receipts](20260930-workspace-namespace-native-r065-receipts.json). Exact reproduction: `python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-namespace-native --out benchmark-results/fs-bench-pro/issue286-workspace-namespace-native-r065` (historical path is append-only; do not execute again into it).

Focused changed-source Linux release Clippy (`-p layerfs-sdk -p layerfs-storage --features rusqlite/bundled --test inherited_workspace -- -D warnings`, explicit existing zig CC) passed. Product boundary scanned357 files; its9 self-tests passed. Native source helper compilation and all ten current controls passed; ordinary earlier-family/unit sweeps were not rerun. New namespace4 and generic native3 Python tests passed before source seal. Host example/verifier checks follow once at final source.

Next: public SDK group r066, all five prospectively frozen cases once, then separate full retained-Store proof<9s. Earlier F1–4 product mechanisms did not change: cite their checkpoint evidence, especially F2; no resampling. The10240 architecture work remains owner-deferred under #276.

Production LOC:135638→135638(delta+0); reference65417/Core70221. Both source and this report-only commit use the exact first-parent/final staged production-only archive counter, excluding tests/examples/harness/docs.
