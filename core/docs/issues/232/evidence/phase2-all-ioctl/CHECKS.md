# #232 final source checks and receipt boundary

The all-ioctl v3 performance/functional receipts were taken at the immutable
source `46cf957abf97ad5c340c8faf0dfab7e9fb426658`, with the exact
release binaries, image and harness seal in [PRE_RUN.json](PRE_RUN.json).
The later one-line benchmark-test fixture correction below changes a file
included in the repository's broad harness seal. It does **not** rewrite,
relabel or promote those sampled receipts. Their compiled product, editor,
driver and verifier binaries remain the ones pinned at the sampled source;
no new performance sample was taken after the test correction.

| Check | Result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline` | PASS, all Core workspace unit, integration and doc tests |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --workspace --all-targets -- -D warnings` | PASS |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | PASS |
| `python3 core/tools/check_product_boundary.py` | PASS, 294 production Rust/SQL files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | PASS, 9 tests |
| `cargo +1.85.1 zigbuild --manifest-path core/benchmark/fs-bench-pro/workload/Cargo.toml --locked --offline --release --target aarch64-unknown-linux-musl` | PASS for the generic Linux editor before its image seal |
| `python3 -m unittest discover -s core/benchmark/fs-bench-pro/tests -p 'test_*.py'` | Initial FAIL: one legacy v3 negative test omitted `scenario_id`; after the fixture correction, PASS, 54 tests with 4 skips |
| `python3 core/docs/issues/232/evidence/phase2-all-ioctl/derive.py` | PASS, 56 raw case hashes, identities, LFT1 scopes, verifiers, callbacks and cleanup rechecked |

The legacy negative test intends to ask whether an incomplete v3 case rejects
an absent full-file digest, canonical root or count before opening a Store.
The versioned verifier now requires `scenario_id` as well. The fixture was
given a valid legacy `-exec-v3` scenario ID so each invalid field reaches
the intended check; the verifier implementation did not change.

The Core checks and the corrected benchmark test pass on the same compiled
product/editor behavior used by the v3 campaign. The **only later source
change is test input**, so the exact historical campaign identity remains
the pre-run seal, not the later repository HEAD. The [report](REPORT.md)
still classifies all 56 cold latency rows `INELIGIBLE` and the capped-500 MiB
raw insert aim as missed. No check result upgrades that admission status.
