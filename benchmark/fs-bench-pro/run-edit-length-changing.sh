#!/usr/bin/env bash
set -euo pipefail
export LAYERFS_SDK_EDIT_FAMILY=edit_length_changing
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/lib-edit-sdk-runner.sh" "$@"
