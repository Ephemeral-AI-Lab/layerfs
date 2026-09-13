# PR #126 CI failures and repairs

All original failures are preserved. These checks qualify their exact source
only; they are not the final supported-surface or benchmark campaign.

- Run `34780076545`, job `103785353557`, head `5996231830eb05c986f7f9dcbb4b24cc52288281`:
  FAIL at `cargo +1.96.0 fmt --all --check`. Later steps did not run. Log retained
  in `run-34780076545-fmt-failed.log`. Fixed only formatter differences in seven
  files, including inherited benchmark source; no benchmark execution or scenario
  behavior change. Commit `2728aebd38ac5d3b2a8a1379b3259684512284b0`.
- Run `34780263723`, head `2728aebd38ac5d3b2a8a1379b3259684512284b0`:
  fmt PASS, Python test-runner checks PASS, full native suite under Rust 1.85.1
  PASS. Clippy FAIL on redundant `counters: counters` and large publication
  resolution variant. Full and failing-step logs retained separately.
- Targeted repair: field shorthand and boxed Published receipt. No lint gate or
  test oracle disabled. `cargo +1.96.0 clippy -p layerfs-layerstack-store --lib
  --locked -- -D warnings` PASS (`store-clippy-repair.log`), exact
  `staging::tests::publication_lost_ack_reopen_exact_context_and_bounded_history`
  PASS (`boxed-receipt-check.log`). The other Store passes are retained because
  their schema/staging behavior is unaffected by the resolution value's layout.

Full CI must be read back after subsequent changes. Component integration is
still in progress and neither #124 nor #125 is eligible for closure.
