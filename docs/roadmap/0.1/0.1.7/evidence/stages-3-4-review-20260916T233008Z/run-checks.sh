#!/bin/bash
# Stages 3-4 review: focused checks on the pinned snapshot.
# Each command's exit code is recorded individually; no aggregate pass/fail.
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs || exit 1
export LAYERFS_CONSTRUCTION_WORKERS=1
LOG=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/checks.log
: > "$LOG"
run() {
  local label="$1"; shift
  echo "===== CMD [${label}] : $* =====" >> "$LOG"
  local start=$(date -u +%s)
  ( "$@" ) >> "$LOG" 2>&1
  local rc=$?
  local end=$(date -u +%s)
  echo "----- EXIT [${label}] rc=${rc} wall=$((end-start))s -----" >> "$LOG"
}

echo "REVIEW CHECKS  snapshot=$(git rev-parse HEAD) start=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"

run boundary python3 core/tools/check_product_boundary.py
run coretools_unittest bash -c 'cd core/tools && python3 -m unittest discover -v'
run loc_unittest python3 -m unittest discover -s tools -p 'test_production_loc.py'
run fmt cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
run c1_focused cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test edit_single --test edit_batch --test edit_transitions --test edit_noop --test edit_model --test edit_bounds --test edit_timing --test inode_leaf
run c2_focused cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test delta_payload --test delta_chains --test metadata_pool --test metadata_pool_index --test physical_formats --test policy_capacity --test edit_pipeline
run workspace_test cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
run clippy cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
run loc_files python3 tools/production_loc.py --files
run git_diff_check git diff --check
echo "REVIEW CHECKS END $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
echo DONE_ALL
