# Checks: what ran, what did not, and why

> Status: Research; the check record for this round. Nothing here is a release
> claim and nothing here is a product test.

## Ran, and passed

| Check | Command | Result |
|---|---|---|
| harness shared Python tests | `python3 -m unittest discover -s shared -p 'test_*.py'` (in `core/benchmark/fs-bench-pro-storage-content`) | **PASS**, 136 tests, 2.575 s |
| pin generator self-check | `python3 shared/pin_expected.py --self-check` | **PASS** ("no invented constant, split identity refused, table round-trips") |
| this round's counter inventory | `python3 counter_selection.py` | **PASS**; `counter-selection.json` / `.txt` rewritten identically on re-run |
| this round's runner replay | `python3 runner_verdict.py` | **PASS**; `runner-verdict.json` / `.txt` rewritten identically on re-run |
| this round's provenance fingerprint | `python3 state_provenance.py` | **PASS**; `state-provenance.json` / `.txt` rewritten identically on re-run |
| corpus identity | `sha256(checkpoint-manifest.json)` | **PASS**; equals the manifest SHA256 recorded in every retained receipt |
| checkout identity | `git rev-parse HEAD`, `git status --porcelain` | clean worktree at `437683aa0` apart from this round's own untracked evidence directory |

The 136 harness tests are the whole shared Python suite for this harness; none of
them builds or spawns the binary (`grep -l 'BINARY\|subprocess' shared/test_*.py`
is empty), which is why they are runnable in a round that performs no build.

## Not run, and why

| Check | Reason |
|---|---|
| `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked` | no production, core, harness-Rust or test file changes in this commit — the tree changed is `docs/` only. The unchanged production tree's last recorded PASS is L42's (494 core tests, 12 example targets, core Clippy/fmt), with its commit and binary identities; it is **cited, not re-run**, and is not claimed as this round's result. |
| `core/tools/check_product_boundary.py` and its self-tests | same reason: no core source file changed. Cited from L42, not re-run. |
| harness Rust tests (`cargo test` in `core/benchmark/fs-bench-pro-storage-content`) | no harness source changed. Cited from L42 (117 tests), not re-run. |
| `runner.py self-check` | it builds the harness binary when absent (`runner.py:1655-1657`), i.e. it is a build command; this round is prohibited from starting builds it does not need, and the pre-existing harness Clippy/format failures are known and unresolved. |
| a runner run of `history-stride10` / `history-stride3` | a history performance command cannot fit the frozen budgets; the measured walls are 38.101 s / 40.221 s (stride10) and 73.127 s / 80.483 s (stride3), against 15 s ordinary and 25 s declared-exception limits. Recorded `NOT_RUN` with those measured wall times rather than started (AGENTS.md §3.7, `benchmark/AGENTS.md` "Budgets"). |
| `runner.py verify` on a history run | there is no history run directory produced by the runner to verify; the pin re-derivation is replayed in `runner_verdict.py` instead, and is labelled a replay. |

## Known unresolved gaps carried forward

* The unchanged harness Clippy/format failures are **not** rerun and are not
  claimed as passes (L42's note stands).
* No residency or device-read measurement for the corpus exists anywhere in the
  retained evidence; [CACHE-STANCE.md](CACHE-STANCE.md) §3 specifies the build that
  would produce one and does not perform it.
* `history-stride1` has never been measured; its pin-table projection is arithmetic
  and is labelled `NOT_MEASURED`.
