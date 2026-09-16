#!/bin/bash
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs || exit 1
OUT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/oracle-replay
mkdir -p "$OUT"
LOG=$OUT/oracle-replay.log
: > "$LOG"
echo "snapshot=$(git rev-parse HEAD) ref_rev=$(git rev-parse 44cf748486863ab7c21ca47e731bd88e2b9a7b4a) start=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
echo "ref-src-diff-lines: $(git diff 44cf748486863ab7c21ca47e731bd88e2b9a7b4a..HEAD -- crates/layerfs-content/src | wc -l)" >> "$LOG"
for case in join-80-100 repartition-80-100 untouched-sibling height-growth root-collapse batch-normalized interior-multi-level unequal-height-join half-partition-90-90; do
  echo "===== CASE $case" >> "$LOG"
  local_start=$(date -u +%s)
  cargo +1.85.1 run --locked -p layerfs-content --example rope_edit_oracle -- --case "$case" > "$OUT/$case.json" 2>>"$LOG"
  rc=$?
  echo "----- EXIT rc=$rc wall=$(( $(date -u +%s) - local_start ))s" >> "$LOG"
done
echo "END $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
echo DONE_ORACLE
