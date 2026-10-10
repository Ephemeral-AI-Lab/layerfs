set -euo pipefail
exec timeout --kill-after=1s 20s python3 -B /code/confinement_probe.py /workspaces/2 30421
