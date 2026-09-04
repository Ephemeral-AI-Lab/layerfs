#!/usr/bin/env bash
set -euo pipefail
if [[ ${LAYERFS_BENCH_ARCHIVAL:-0} != 1 ]]; then
  exec python3 "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/shared/runner.py" --family workspace_reliability "$@"
fi
exec python3 "$(dirname "$0")/workspace-runner.py" --family workspace_reliability "$@"
