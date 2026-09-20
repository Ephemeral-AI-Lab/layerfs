set -e
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope
BIN=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content
CAMP=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z
run() { python3 $CAMP/with_locks.py "$1-stride10" python3 $CAMP/collect.py "$1" history-stride10 --binary $BIN --cwd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --seal-repo /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --pre-execute --env LAYERFS_STORAGE_RCA_PROBE=1 --env "$2"; }
run q3-1a LAYERFS_STORAGE_STATEMENT_CACHE=
run q3-2b LAYERFS_STORAGE_STATEMENT_CACHE=0
run q3-3b LAYERFS_STORAGE_STATEMENT_CACHE=0
run q3-4a LAYERFS_STORAGE_STATEMENT_CACHE=
echo ALL_DONE
