#!/usr/bin/env bash
set -euo pipefail
if [[ ${LAYERFS_BENCH_ARCHIVAL:-0} != 1 ]]; then
  exec python3 "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/shared/runner.py" --family edit_canonical_chunk_count "$@"
fi
export LAYERFS_SDK_EDIT_FAMILY=edit_canonical_chunk_count
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/lib-edit-sdk-runner.sh" "$@"
