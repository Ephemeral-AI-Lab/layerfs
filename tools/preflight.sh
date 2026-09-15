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

step "rustfmt 1.96 (fmt --all --check)"
cargo +1.96.0 fmt --all --check

step "tools unit tests"
python3 -m unittest discover -s tools -p test_test_fast.py

step "workspace fast suite (RUSTUP_TOOLCHAIN=1.85.1)"
RUSTUP_TOOLCHAIN=1.85.1 tools/test-fast.sh

step "clippy -D warnings (workspace, locked, +1.96.0)"
cargo +1.96.0 clippy --workspace --locked -- -D warnings

step "benchmark harness tests"
python3 -m unittest discover -s benchmark/fs-bench-pro/shared -p 'test_*.py'

printf '\npreflight: all steps passed\n'
