set -euo pipefail
exec timeout --kill-after=1s 12s python3 -B /code/probe.py node_modules/.pnpm 4 0 0 6
