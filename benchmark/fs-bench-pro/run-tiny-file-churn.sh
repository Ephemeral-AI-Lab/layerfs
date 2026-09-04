#!/usr/bin/env bash
set -euo pipefail
exec python3 "$(dirname "$0")/workspace-runner.py" --family tiny_file_churn "$@"
