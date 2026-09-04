#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
exec python3 "$here/verify-selected.py" --family payload_create_read "$@"
