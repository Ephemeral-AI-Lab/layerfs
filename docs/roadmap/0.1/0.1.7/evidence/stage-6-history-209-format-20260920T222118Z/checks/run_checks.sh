set -x
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/checks/fmt.log 2>&1; echo "fmt=$?"
python3 core/tools/check_product_boundary.py > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/checks/boundary.log 2>&1; echo "boundary=$?"
python3 core/tools/test_check_product_boundary.py > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/checks/boundary-selftest.log 2>&1; echo "boundary_selftest=$?"
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/checks/clippy.log 2>&1; echo "clippy=$?"
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/checks/workspace-test.log 2>&1; echo "workspace_test=$?"
cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked > /Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-209-format-20260920T222118Z/checks/harness-test.log 2>&1; echo "harness_test=$?"
echo CHECKS_DONE
