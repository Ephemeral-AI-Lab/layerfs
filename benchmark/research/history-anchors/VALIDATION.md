# Validation record

Validated on 2026-09-04 against LayerFS `main` at
`1e81e9b8cf871324341c221a51b0a0239c580da9`.

## Research crate

- `cargo fmt --check`: passed.
- `cargo test`: 4 passed, 0 failed.
- `cargo clippy -- -D warnings`: passed.
- two independent release runs: 56 rows each.
- `python3 verify.py --output results/analysis.json`: both runs passed every
  frozen gate; all rows reported exact correctness and unchanged Store state.

## Root workspace

- The full unfiltered workspace run reached one unchanged CLI integration test
  failure: `standalone_cli_keeps_one_sdk_client_through_workspace_end`. The
  managed runner blocks the local context transport used by that test; the
  history-anchor patch touches no CLI, SDK, daemon, or transport code.
- Re-running the workspace with only that named environment-blocked test
  filtered out, using a fresh executable target directory, passed 236 tests;
  one repository-declared large-spill test remained ignored.
- An initial cached-target attempt produced a zero-filled non-executable test
  artifact in this managed filesystem. A clean target under `/tmp` rebuilt and
  ran the same test suite successfully. This is an environment note, not a
  source failure.

## Scope audit

- no product source changed;
- no Store schema or canonical identity changed;
- no public API or semantics changed;
- no runtime dependency was added;
- no v0.1.4 benchmark ID, registry, frozen scenario, or release evidence changed;
- repository search found no existing Commit-history skip/anchor index on this
  baseline.
