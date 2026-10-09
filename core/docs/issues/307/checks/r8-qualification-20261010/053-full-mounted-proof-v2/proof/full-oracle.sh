set -euo pipefail
exec timeout --kill-after=1s 85s python3 /code/full_oracle.py --root . --inventory /code/full-inventory.jsonl --inventory-sha256 d058d8c61220950b989772681cee4446575893076318d128a25884849271ba52 --uid 501 --gid 20 --output /tmp/r8-full-oracle.json
