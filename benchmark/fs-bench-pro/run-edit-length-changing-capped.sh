#!/usr/bin/env bash
set -euo pipefail
exec python3 "$(dirname "$0")/workspace-runner.py" --family edit_length_changing_capped "$@"
