#!/bin/bash
# The complete v0.1.6 selected-invocation matrix: every registered regular case
# and the three explicit extended cases, one performance receipt and one
# separate verification receipt each, on one final source and image. The
# measurement lock is this worktree's own (owner direction, 2026-09-21: see
# docs/roadmap/0.1/0.1.7/measurement-isolation.md); it is waited for, never
# stolen, and another worktree is not excluded. Every completed invocation is
# retained exactly as it landed and superseded receipts are archived, not
# deleted.
set -u
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
IMG=$(cat /tmp/v016_img.txt)
PREP=benchmark/fs-bench-pro/shared/prepare_v016_fixture.py
MATRIX=benchmark/fs-bench-pro/shared/v016_matrix.py

echo "=== fixture masters ==="
for spec in \
  "mixed_load_bearing:v016-mixed-development-100mb-5000-k10-v1" \
  "mixed_load_bearing:v016-mixed-development-100mb-5000-k100-v1" \
  "mixed_load_bearing:v016-mixed-development-500mb-30000-k10-v1" \
  "mixed_load_bearing:v016-mixed-development-500mb-30000-k100-v1" \
  "multi_workspace_development:v016-workspace-mixed-100mb-5000-k10-v1" \
  "multi_workspace_development:v016-workspace-mixed-100mb-5000-k100-v1" \
  "multi_workspace_development:v016-workspace-mixed-500mb-30000-k10-v1" \
  "multi_workspace_development:v016-workspace-mixed-500mb-30000-k100-v1" \
  "branch_development:v016-branch-mixed-100mb-5000-k10-v1" \
  "branch_development:v016-branch-mixed-100mb-5000-k100-v1" \
  "branch_development:v016-branch-mixed-500mb-30000-k10-v1" \
  "branch_development:v016-branch-mixed-500mb-30000-k100-v1" ; do
  fam="${spec%%:*}"; case_id="${spec##*:}"
  python3 "$PREP" --image "$IMG" --family "$fam" --case "$case_id" 2>&1 | tail -1
done
python3 "$PREP" --image "$IMG" --family mixed_load_bearing --case v016-mixed-exhaustive-100mb-5000-k100-v1 --extended 2>&1 | tail -1
python3 "$PREP" --image "$IMG" --family multi_workspace_development --case v016-workspace-four-100mb-5000-k100-v1 --extended 2>&1 | tail -1

echo "=== regular matrix: perf + verify ==="
python3 "$MATRIX" --image "$IMG" --seed 1 --mode both --fresh-perf --fresh-verify
echo "=== extended cases ==="
python3 "$MATRIX" --image "$IMG" --seed 1 --mode both --extended --fresh-perf --fresh-verify
echo "=== done ==="
