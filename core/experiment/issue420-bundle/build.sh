#!/bin/sh
# Builds the bundle experiment crate (Linux, release, -j 2, offline) into its
# own target and cargo home; 115 s wall inside.
WT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/.claude/worktrees/issue420-postgresql
exec docker run --rm -v $WT:/work -w /work/core/experiment/issue420-bundle/rs \
  -e CARGO_HOME=/work/core/target/issue420/bundle-cargo-home \
  -e CARGO_TARGET_DIR=/work/core/target/issue420/bundle-target \
  rust:1.85.1-bookworm timeout 115 cargo build --release --offline -j 2 "$@"
