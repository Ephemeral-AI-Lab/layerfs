#!/bin/sh
# Runs the bundle experiment binary on lfs420-bundle-pg's loopback (its
# network namespace), worktree at /work, host-cache volume at /sq; 115 s wall.
WT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/.claude/worktrees/issue420-postgresql
exec docker run --rm --network container:lfs420-bundle-pg -v $WT:/work -v lfs420-bundle-sq:/sq $DOCKER_ENV rust:1.85.1-bookworm timeout 115 /work/core/target/issue420/bundle-target/release/issue420-bundle "$@"
