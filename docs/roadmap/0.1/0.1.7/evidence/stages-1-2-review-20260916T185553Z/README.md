# Evidence: Stages 1–2 independent review

Raw evidence for `docs/roadmap/0.1/0.1.7/component-decoupling/stages-1-2-review-20260916T185553Z.md`.
Nothing here is product source; the repository tree was not modified by the review
except for this directory and the review document itself.

| File | What it is | Command that produced it |
| --- | --- | --- |
| `identity.txt` | Frozen reviewed state: HEAD, tree, lockfile and counter hashes, parent tree | `git rev-parse`, `git write-tree`, `shasum -a 256` |
| `loc-per-file.csv` | Per-file production LOC and physical lines, `before` from tree `2242e867`, `after` from tree `69d34cbe` | `python3 tools/production_loc.py --files`, materialized from `git archive` |
| `loc-directories.csv` | Recursive directory rollups for both snapshots | same |
| `loc-counter-audit.txt` | Independent probe of the counter's classification (nested comments, strings, `#[cfg(test)]` forms, SQL comments, SQL under `src/`) | reviewer probe script |
| `checks-python.txt` | Boundary guard, core tool tests, counter tests, full `tools` discovery | the exact commands in the header of the file |
| `checks-structure.txt` | rustfmt check and test-target inventory | `cargo fmt --check`, `ls`, `cargo test -- --list` |
| `checks-cargo.txt` | Package and workspace test runs, clippy | `cargo +1.85.1 test --locked`, `cargo +1.96.0 clippy -D warnings` |
| `isolation.txt` | Manifests, external-crate uses, feature graph, product-source marker scan | `cat`, `grep`, `cargo tree -e features` |
| `deps-safety.txt` | Strict test-marker scan, dependency-use audit, `unsafe` inventory, dead-API symbol counts | `grep`, reviewer symbol-count script |
| `memory-and-calls.txt` | Filesystem/spool surface, declared capacity constants, workspace-allocation sites, per-call connection sites, the `flush_batch` clone site | `grep`, `sed` |
| `experiment-visibility-and-cost.txt` | E1: unrelated reader sees unfinished-save output. E2: per-call read cost and batching value | `/tmp/rev-harness` `rev-harness` binary (path dependency on the candidate crates) |
| `experiment-race.txt` | 15 s two-writer contention experiment: acquisitions, stale-baseline observations, post-race readability | `/tmp/rev-harness` `race` binary |
| `harness-run/` | Store databases produced by the two harness experiments (`vis.sqlite` for E1, `cost.sqlite` for E2), kept as raw artifacts rather than re-generated | `/tmp/rev-harness` `rev-harness` binary |
| `smoke/` | Six real `measure_components` runs: `{small,chunked}-{c1,c2,pipeline}` text transcripts, JSON timing trees and SQLite stores | the three modes documented in `core/crates/layerfs-storage/README.md` |

## Limits of this evidence

- All timings are single observations of one input per mode, with no warmup
  contract, no declared cache state and no comparator. They are diagnostics, not
  measurement-campaign receipts, and are not `admission_eligible`.
- The debug smoke runs use the example's default profile; the harness runs used a
  release build. The two must not be pooled.
- No RSS, heap or RSS-like instrument was used; memory conclusions rest on
  declared bounds, live-ownership assertions and source inspection.
- The two SQL classification gaps in `tools/production_loc.py` are reported in
  `loc-counter-audit.txt` and do not affect the totals for this tree.
