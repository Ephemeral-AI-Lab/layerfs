#!/bin/bash
# The v0.1.6 retained matrix on one final source and image: every registered
# regular case and the three explicit extended cases, seeds 1, 2 and 3, one
# performance receipt and one separate verification receipt each. Every receipt
# is recollected on the final seal, so perf and verify always share one source
# and image. The measurement lock is this worktree's own (owner direction,
# 2026-09-21: docs/roadmap/0.1/0.1.7/measurement-isolation.md); it is waited for,
# never stolen, and another worktree is not excluded. Superseded receipts are
# archived, never deleted.
#
# Cases whose workload is not implemented yet are still registered: they are run
# and their real outcome is retained instead of being relabelled.
set -u
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
IMG=$(cat /tmp/v016_img.txt)
M=benchmark/fs-bench-pro/shared/v016_matrix.py
echo "=== image: $IMG ==="
for seed in 1 2 3; do
  echo "=== regular rows, seed $seed ==="
  python3 "$M" --image "$IMG" --seed "$seed" --mode both --prepare --fresh-perf --fresh-verify
done
echo "=== three extended cases, seed 1 ==="
python3 "$M" --image "$IMG" --seed 1 --mode both --extended --prepare --fresh-perf --fresh-verify
echo "=== final sweep complete ==="
