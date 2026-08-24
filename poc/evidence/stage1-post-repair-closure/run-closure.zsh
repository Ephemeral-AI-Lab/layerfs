#!/bin/zsh
set -eux

export CARGO_TERM_COLOR=never

git rev-parse HEAD
git status --porcelain=v1
git diff --binary HEAD | shasum -a 256
git ls-files -co --exclude-standard -- '*.rs' Cargo.toml Cargo.lock | LC_ALL=C sort | xargs shasum -a 256
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets -- --test-threads=1
cargo build --release --workspace
git diff --check
target/release/layerfs-eval stage1 readiness-only single-file
jq -c '{schema,status,measured_rows_started,git_commit,dirty_tree_blake3,source_tree_blake3,executable_blake3,fixture_master_blake3,store_sqlite_profile,reset_observations_ns,reset_upper_ns}' target/layerfs-stage1-readiness.json
shasum -a 256 target/release/layerfs-eval target/layerfs-stage1-readiness.json
shasum -a 256 poc/evidence/stage1-pre-repair-campaign-20260824/summary.json poc/evidence/stage1-pre-repair-campaign-20260824/rows.jsonl poc/evidence/a02-diagnostic-20260824/result.json
