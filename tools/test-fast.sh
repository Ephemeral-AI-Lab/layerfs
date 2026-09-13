#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
jobs=${LAYERFS_TEST_JOBS:-4}
[[ "$jobs" =~ ^[1-9][0-9]*$ ]] && (( jobs <= 16 )) || {
  printf 'test-fast: LAYERFS_TEST_JOBS must be 1..16\n' >&2
  exit 2
}

temporary=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-test-fast.XXXXXX")
trap 'rm -rf -- "$temporary"' EXIT

cargo test --manifest-path "$repo/Cargo.toml" --workspace --all-features \
  --locked \
  --no-run --message-format=json >"$temporary/artifacts.jsonl"
started=$SECONDS

python3 "$repo/tools/test_fast.py" "$temporary/artifacts.jsonl" "$jobs"

elapsed=$((SECONDS - started))
printf 'PASS full workspace native tests in %ss with %s bounded jobs\n' "$elapsed" "$jobs"
(( elapsed <= 150 )) || {
  printf 'test-fast: %ss exceeds the 150s warm-suite ceiling\n' "$elapsed" >&2
  exit 1
}
