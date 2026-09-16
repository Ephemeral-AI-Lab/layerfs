#!/usr/bin/env bash
# RETIRED. This gate is permanently disabled by owner decision.
#
# GitHub Actions was disabled first (ledger L21); this local pre-push gate replaced
# it and is now retired as well (ledger L32), because the repository is in a large
# architecture shift: the gate costs minutes and verifies a tree that is no longer
# the deliverable.
#
# It runs no checks. The zero exit status below is NOT a pass and must never be
# reported as one. Do not restore this gate, and do not reintroduce an equivalent
# aggregate gate, wrapper or workflow; see AGENTS.md §4.
#
# Verify the tree you actually changed, per workspace:
#
#   core (the replacement product):
#     cargo +1.85.1 test  --manifest-path core/Cargo.toml --locked
#     cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
#     cargo +1.85.1 fmt   --manifest-path core/Cargo.toml --all --check
#     python3 core/tools/check_product_boundary.py
#     python3 -m unittest discover -s core/tools -p 'test_*.py'
#
#   root crates/ (reference, kept isolated):
#     cargo test --manifest-path Cargo.toml --workspace --locked
set -euo pipefail

printf '%s\n' \
  'tools/preflight.sh is RETIRED and runs no checks (owner decision, ledger L32).' \
  'Nothing was verified. This zero exit status is not a pass.' \
  'Verify the workspace you changed; see the header of this file and AGENTS.md §4.' >&2

exit 0
