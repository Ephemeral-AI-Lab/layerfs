#!/bin/bash
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs || exit 1
export LAYERFS_CONSTRUCTION_WORKERS=1
OUT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/boundaries
mkdir -p "$OUT"
LOG=$OUT/boundaries.log
: > "$LOG"
echo "snapshot=$(git rev-parse HEAD) start=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
BIN=core/target/debug/examples/measure_edits
try() {
  local label="$1"; shift
  echo "===== TRY [$label]: $*" >> "$LOG"
  "$@" >> "$LOG" 2>&1
  echo "----- EXIT [$label] rc=$? -----" >> "$LOG"
}
mkdir -p "$OUT/existing"
try existing-output $BIN --mode c1 --case small --threshold-bytes 131072 --output "$OUT/existing"
try cutoff-100000  $BIN --mode c1 --case small --threshold-bytes 100000 --output "$OUT/bad-100000"
try cutoff-65536   $BIN --mode c1 --case small --threshold-bytes 65536 --output "$OUT/bad-65536"
try cutoff-2097152 $BIN --mode c1 --case small --threshold-bytes 2097152 --output "$OUT/bad-2097152"
try cutoff-262144  $BIN --mode c1 --case small --threshold-bytes 262144 --output "$OUT/ok-262144"
try cutoff-524288  $BIN --mode c1 --case small --threshold-bytes 524288 --output "$OUT/ok-524288"
try timing-off     $BIN --mode c1 --case small --threshold-bytes 131072 --timing off --output "$OUT/timing-off"
echo "END $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
echo DONE_BOUNDARIES
