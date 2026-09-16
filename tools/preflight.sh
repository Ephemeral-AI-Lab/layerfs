#!/usr/bin/env bash
# Local pre-push gate for this repository.
#
# GitHub Actions is disabled by owner decision (see ledger L21 in
# docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md): the repository
# runs no CI, and .github/workflows/ci.yml was removed. These are the checks that
# workflow used to run, plus the benchmark harness tests, so that "the checks
# pass" still means something before a push.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

step() { printf '\n=== %s ===\n' "$1"; }

step "production LOC counter"
python3 -m unittest discover -s tools -p 'test_production_loc.py'

step "replacement core product-source boundaries"
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'

step "candidate core workspace (locked tests, examples, clippy, fmt)"
# The candidate workspace has its own manifest, lockfile and target namespace;
# the root workspace checks below do not exercise it.
cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --tests
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --tests
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_nested >/dev/null
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_composition >/dev/null
core_stage02="$(mktemp -d "${TMPDIR:-/tmp}/layerfs-stage02.XXXXXX")"
python3 -c 'from pathlib import Path; import sys; Path(sys.argv[1]).write_bytes(bytes(range(256)) * 64)' "$core_stage02/input.bin"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c1 --input "$core_stage02/input.bin" --timings "$core_stage02/c1.json" >/dev/null
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c2 --input "$core_stage02/input.bin" --store "$core_stage02/c2.sqlite" --timings "$core_stage02/c2.json" >/dev/null
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode pipeline --input "$core_stage02/input.bin" --store "$core_stage02/pipeline.sqlite" --timings "$core_stage02/pipeline.json" >/dev/null
rm -rf "$core_stage02"
cargo +1.96.0 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings

step "rustfmt 1.96 (fmt --all --check)"
cargo +1.96.0 fmt --all --check

step "tools unit tests"
python3 -m unittest discover -s tools -p test_test_fast.py

step "workspace fast suite (RUSTUP_TOOLCHAIN=1.85.1)"
# test-fast.sh exits 1 when the suite passes but exceeds its 120 s warm-suite
# ceiling; that ceiling is a development-loop budget, not a correctness gate, and
# a loaded machine routinely exceeds it. A genuine suite failure still fails here.
set +e
suite_output=$(RUSTUP_TOOLCHAIN=1.85.1 tools/test-fast.sh 2>&1)
suite_status=$?
set -e
printf '%s\n' "$suite_output"
if [ "$suite_status" -ne 0 ]; then
  if printf '%s' "$suite_output" | grep -q '^PASS full workspace native tests in'; then
    printf 'preflight: workspace suite PASSED but exceeded the 120s warm-suite ceiling (soft budget) — continuing\n' >&2
  else
    printf 'preflight: workspace fast suite FAILED\n' >&2
    exit "$suite_status"
  fi
fi

step "clippy -D warnings (workspace, locked, +1.96.0)"
cargo +1.96.0 clippy --workspace --locked -- -D warnings

step "benchmark harness tests"
python3 -m unittest discover -s benchmark/fs-bench-pro/shared -p 'test_*.py'

printf '\npreflight: all steps passed\n'
