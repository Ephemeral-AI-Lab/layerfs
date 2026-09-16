#!/bin/bash
# Fresh real-example smoke run on the pinned snapshot (admission-ineligible diagnostic).
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs || exit 1
export LAYERFS_CONSTRUCTION_WORKERS=1
OUT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/examples
mkdir -p "$OUT"
LOG=$OUT/examples.log
: > "$LOG"
echo "snapshot=$(git rev-parse HEAD) start=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
run() {
  local label="$1"; shift
  local dir="$OUT/$label"
  local cmd="$*"
  echo "===== CMD [$label]: $cmd --output $dir" >> "$LOG"
  local start=$(date -u +%s%N)
  "$@" --output "$dir" >> "$LOG" 2>&1
  local rc=$?
  local end=$(date -u +%s%N)
  echo "----- EXIT [$label] rc=$rc wall_ms=$(( (end-start)/1000000 )) -----" >> "$LOG"
}
B="cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits --"
run c1-small       cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode c1 --case small --threshold-bytes 131072
run c2-small       cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode c2 --case small --threshold-bytes 131072
run pipeline-chunked cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode pipeline --case chunked --threshold-bytes 131072
run pipeline-grow-1m cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode pipeline --case small-to-large --threshold-bytes 1048576
run pipeline-shrink-1m cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_edits -- --mode pipeline --case large-to-small --threshold-bytes 1048576
run pooled-24      cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_pooled -- --leaves 24 --rows 100
echo "END $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
echo DONE_EXAMPLES
