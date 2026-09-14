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

- Run `34781059906` at `6d907bc43` and run `34782823313` at
  `c95588f30760e08235d56b513e48b92a9defd9b7`: workspace Clippy FAIL on unwired
  private overlay/correspondence code. Both failing-step logs are preserved.
  Complete production integration; suppressing dead-code checks is not the fix.

- Run `34786208371` at `3f4a26d4a555918ea79a90bb69995946fa605abb`:
  native Rust 1.85.1 FAIL in
  `overlay::tests::fixed_inode_metadata_and_linked_ranges_remain_owned_after_live_drop`.
  Its pre-aggregate fixture passed an arbitrary index root as file ranges; the
  shared inline-accounting helper correctly rejected missing range metadata.
  Replace that fixture with a real Inline range tree and verify retained bytes
  after root cleanup. No production validation is weakened. The exact repaired
  check passes on the ongoing integration binary `13b51d2ce155a98d9362900306ddb1840e368f26998824b3f1c9cf49bb45c627`;
  `../phase3-host-operations/lease-ci-attempt02-result.json` records its full
  tested source. This targeted pass is not a final-candidate or full-CI claim;
  the subsequent remote run will qualify the published repair commit.
