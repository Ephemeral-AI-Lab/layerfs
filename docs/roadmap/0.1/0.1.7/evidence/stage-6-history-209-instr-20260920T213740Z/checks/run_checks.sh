set -x
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope
python3 core/tools/check_product_boundary.py > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/checks/boundary.log 2>&1; echo "boundary=$?"
python3 core/tools/test_check_product_boundary.py > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/checks/boundary-selftest.log 2>&1; echo "boundary_selftest=$?"
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/checks/fmt.log 2>&1; echo "fmt=$?"
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test multi_writer --test pack_watermark --test memory_bounds --test visibility --test persistence_failure > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-instr-20260920T213740Z/checks/storage-tests.log 2>&1; echo "tests=$?"
echo CHECKS_DONE
