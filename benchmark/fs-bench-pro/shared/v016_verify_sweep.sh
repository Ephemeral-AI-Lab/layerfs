#!/bin/bash
# Re-collect the verification receipts of the v0.1.6 M1 matrix after a
# verification-allowance change. Performance receipts that already landed with
# the current allowance stay in place; nothing is deleted.
set -u
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
IMG=$(cat /tmp/v016_img.txt)
MATRIX=benchmark/fs-bench-pro/shared/v016_matrix.py

python3 "$MATRIX" --image "$IMG" --seed 1 --mode verify --fresh-verify
python3 "$MATRIX" --image "$IMG" --seed 1 --mode verify --extended
echo "=== verify sweep done ==="
