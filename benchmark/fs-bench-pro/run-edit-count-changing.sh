#!/usr/bin/env bash
set -euo pipefail
export LAYERFS_EDIT_FAMILY=count-changing
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/run-edit-same-count.sh" "$@"
