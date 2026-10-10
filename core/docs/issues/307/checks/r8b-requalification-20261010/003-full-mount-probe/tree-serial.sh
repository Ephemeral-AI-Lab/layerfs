set -euo pipefail
exec timeout --kill-after=1s 13s python3 -B /code/probe.py tree-serial 7 
