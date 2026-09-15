#!/bin/bash
# Interim sweep: the 19 registered regular v0.1.6 rows plus the three extended
# cases, seed 1, on the fix image. Superseded receipts are archived, never
# deleted. This is a validation pass; the final gate sweep runs seeds 1-3 after
# the last source change.
set -u
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
IMG=layerfs-bench-infra:168aa1912d09da1a
M=benchmark/fs-bench-pro/shared/v016_matrix.py
echo "=== registered regular rows: seed 1, perf + verify ==="
python3 "$M" --image "$IMG" --seed 1 --mode both --prepare --fresh-perf --fresh-verify
echo "=== three extended cases: seed 1 ==="
python3 "$M" --image "$IMG" --seed 1 --mode both --extended --prepare --fresh-perf --fresh-verify
echo "=== interim sweep complete ==="
