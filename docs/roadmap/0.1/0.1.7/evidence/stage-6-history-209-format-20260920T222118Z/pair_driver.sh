set -e
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope
CAMP=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z
cp /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content $CAMP/binary-archive/format-v2 && shasum -a 256 $CAMP/binary-archive/format-v2 > $CAMP/binary-archive/format-v2.sha256
cp /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-confirm-20260920T212625Z/binary-archive/shipped-treatment $CAMP/binary-archive/profile-1-control && shasum -a 256 $CAMP/binary-archive/profile-1-control > $CAMP/binary-archive/profile-1-control.sha256
python3 $CAMP/with_locks.py fmt1-control-stride10 python3 $CAMP/collect.py fmt1-control history-stride10 --binary /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-confirm-20260920T212625Z/binary-archive/shipped-treatment --cwd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --seal-repo /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --pre-execute
python3 $CAMP/with_locks.py fmt2-treatment-stride10 python3 $CAMP/collect.py fmt2-treatment history-stride10 --binary /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content --cwd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --seal-repo /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope --pre-execute
echo PAIR_DONE
