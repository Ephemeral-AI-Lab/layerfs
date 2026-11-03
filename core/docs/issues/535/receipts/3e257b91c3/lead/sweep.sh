#!/bin/bash
# usage: sweep.sh build | run <pkg> <features|->
W=~/Ephemeral-AI-Lab/layerfs-issue535-connect; export CARGO_HOME=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue535-connect-7500a2864564/cargo-home-macos
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad; mkdir -p $S/k9; cd $W || exit 2
SPECS=("layerfs-api machine" "layerfs-api -" "layerfs-control machine" "layerfs-control -" "layerfs-machine -" "layerfs-sandbox -" "layerfs-cli -" "layerfs-bridge -" "layerfs-store-backup -")
feat(){ [ "$1" = "-" ] || echo "--features $1"; }
if [ "$1" = build ]; then
  for spec in "${SPECS[@]}"; do set -- $spec
    cargo +1.85.1 test --locked --offline -p $1 $(feat $2) --no-run > $S/k9/build-$1-$2.log 2>&1 || { echo "BUILD FAIL $spec"; tail -15 $S/k9/build-$1-$2.log; }
  done; echo built; exit 0
fi
P=$2; F=$3; t0=$(date +%s)
$S/timeout 120 cargo +1.85.1 test --locked --offline -p $P $(feat $F) > $S/k9/test-$P-$F.log 2>&1; r=$?
echo "$P [$F] rc=$r $(( $(date +%s)-t0 ))s; passed $(grep -E '^test result' $S/k9/test-$P-$F.log | awk '{s+=$4} END{print s}') failed $(grep -E '^test result' $S/k9/test-$P-$F.log | awk '{s+=$6} END{print s}')"
[ $r -ne 0 ] && grep -E "panicked|FAILED|^failures|timed out" $S/k9/test-$P-$F.log | head -15
exit $r
