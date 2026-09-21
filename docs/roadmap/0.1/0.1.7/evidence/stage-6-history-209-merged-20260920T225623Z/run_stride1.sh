set -e
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope
CAMP=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-merged-20260920T225623ZZ
mkdir -p $CAMP/runs $CAMP/checks $CAMP/binary-archive
cp /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/collect.py /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/with_locks.py $CAMP/
cp /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content $CAMP/binary-archive/merged-main && shasum -a 256 $CAMP/binary-archive/merged-main > $CAMP/binary-archive/merged-main.sha256
python3 $CAMP/with_locks.py merged-stride1 python3 $CAMP/collect.py merged history-stride1 --binary /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content --cwd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --seal-repo /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --pre-execute
echo STRIDE1_DONE
