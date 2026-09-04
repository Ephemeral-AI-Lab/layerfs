#!/usr/bin/env bash
set -euo pipefail
if [[ ${LAYERFS_BENCH_ARCHIVAL:-0} != 1 ]]; then
  exec python3 "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/shared/runner.py" --family edit_length_preserving "$@"
fi
export LAYERFS_SDK_EDIT_FAMILY=edit_length_preserving
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/lib-edit-sdk-runner.sh" "$@"
