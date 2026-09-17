# Evidence: Stages 1-5 independent acceptance review, round 2 (2026-09-17)

Snapshot `f288d2af7ecdc7e00f7df153073398d333461aa3` (tree
`bf1cfc4cf9aac0e1200a13a5f3654c34b478a6c9`), frozen at `2026-09-17T15:07:01Z`.
Identity was re-checked after every command in this directory ran: HEAD, tree and
`git status --porcelain` were unchanged, and no tracked file was modified. The
reviewer's report is
[`../../component-decoupling/stages-1-5-review-20260917T230700Z.md`](../../component-decoupling/stages-1-5-review-20260917T230700Z.md).

## Manifest

| file | producer | what it records |
| --- | --- | --- |
| `checks.log` | `python3 core/tools/check_product_boundary.py`, `python3 -m unittest discover -s core/tools -p 'test_*.py'`, `python3 -m unittest discover -s tools -p 'test_production_loc.py'`, `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check`, `git diff --check` | each command's output and exit code |
| `cargo-test.log` | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 60 result blocks: 406 passed, 0 failed, 0 ignored, `EXIT=0` |
| `cargo-clippy.log` | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | `EXIT=0` |
| `production_loc-files.log` | `python3 tools/production_loc.py --files` at HEAD | per-file production LOC and physical lines |
| `loc-pre1.json`, `loc-pre1-files.txt` | same counter, `--root` of the pre-Stage-1 tree `579831eb6` | C1/C2 = 0; telemetry 732; reference 65,417 |
| `loc-pre5.json`, `loc-pre5-files.txt` | same counter, pre-Stage-5 tree `4f1b7d847` | core 11,160 |
| `loc-rev.json`, `loc-rev-files.txt` | same counter, HEAD `f288d2af7` | core 18,650 |
| `loc-v016.json`, `loc-v016-files.txt` | same counter, pinned v0.1.6 `44cf74848` | reference 65,417, core 0 |
| `loc-tables.md` | `make_loc_tables.py` | per-file and recursive per-directory C1/C2 tables with cumulative and Stage-5 deltas |
| `loc-rows.json` | same script | the joined per-file comparison rows |
| `commit-loc.json` | `audit_commit_loc.py` | every commit message's `Production LOC:` disclosure, parsed |
| `per-commit-loc-audit.log` | sample of six Stage 1-4 boundary commits, counter vs disclosure | all six reproduce |
| `per-commit-loc-stage5.log` | all twenty Stage-5 production commits, first parent vs committed tree | fourteen reproduce, six drift (F4) |
| `handoff-smoke.log` | the three smoke commands from `stage-5-verification.md` §6, run into fresh `/tmp` paths | all exit 0 |
| `case-selection.log` | `measure_filesystem` for six cases x three modes | `c1`/`pipeline` select distinct cases; `c2` does not (F2) |
| `diagnostics/attributes-case.log` | `filesystem_timing_c1 --case empty` then `--case attributes` | `attributes` times a no-op (F1) |
| `diagnostics/diag-run.log` | the reviewer's own client, probes P1-P8 | ordering scaling, listing bounds, resource ceilings, serial range, attribute-value bound, no-op, entry ceiling |
| `diagnostics/stage15-diag/` | that client's `Cargo.toml`, `Cargo.lock` and `src/main.rs` | external diagnostic client; path dependency on `core/crates/layerfs-content` only; `[workspace]` so it is not a member of any product workspace |
| `ceiling-and-inventory.log` | shell checks | physical-line ceilings, fixture seal inventory, test/example LOC |
| `sa-c1-content.md` | delegated read-only audit | C1 framing, construction, CDC, mapping, edits, memory ledger |
| `sa-c2-storage.md` | delegated read-only audit | C2 seam, SQLite profile, schema, reuse, reads, transactions, chain bounds |
| `sa-telemetry-timing.md` | delegated read-only audit | timer hierarchy, disabled path, clipping, errors, harness flags |
| `sa-adapters-limits.md` | delegated read-only audit | public surface, seven seams, process-local assumptions, limits, placement |
| `audit_commit_loc.py`, `show_commit_loc.py`, `make_loc_tables.py`, `target_summary.py` | review tooling | reproducible LOC parsing and table generation |

## Reproduction

```sh
# checks
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check

# LOC snapshots (git archive, so nothing in the working tree is touched)
for pair in "pre1 579831eb6" "pre5 4f1b7d847" "rev f288d2af7" \
            "v016 44cf748486863ab7c21ca47e731bd88e2b9a7b4a"; do
  set -- $pair
  mkdir -p /tmp/layerfs-review-snapshots/$1
  git archive "$2" | tar -x -C /tmp/layerfs-review-snapshots/$1
done
python3 tools/production_loc.py --root /tmp/layerfs-review-snapshots/rev --files

# the reviewer's diagnostic client (writes only into /tmp)
cd docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T230700Z/diagnostics/stage15-diag
CARGO_TARGET_DIR=/tmp/layerfs-review-diag-target cargo +1.85.1 build --release
/tmp/layerfs-review-diag-target/release/stage15-diag
```

## What this evidence is not

No qualification measurement was taken. Every number in `diagnostics/`,
`handoff-smoke.log` and `case-selection.log` is a **diagnostic**: one sample, warm
in-process fixture, no gate, no cache contract. The only comparative rows cited in
the report come from somebody else's identity-matched receipt at `eb42c1347`, and
they are labelled as such.

## Isolation

Three background read-only audits each wrote exactly one file into this directory
and nothing else; the reviewer wrote the rest. `git status --porcelain` after all
work lists six untracked documentation paths and no modification to any tracked
file, any product source, any test, any fixture or any harness.
