#!/usr/bin/env bash
# make_patch.sh <spec.json> <file.rs> <old.txt> <new.txt>
# Builds a patch-runner JSON spec from two text files, keeping Rust
# attribute syntax and octal literals out of any JavaScript channel.
set -euo pipefail
python - "$@" << 'PYEOF'
import json, sys
spec = {
    "root": "D:/workspace/layerfs",
    "patches": [{
        "file": sys.argv[2],
        "old": open(sys.argv[3], encoding="utf-8").read(),
        "new": open(sys.argv[4], encoding="utf-8").read(),
    }],
}
with open(sys.argv[1], "w", encoding="utf-8") as f:
    json.dump(spec, f, indent=1, ensure_ascii=False)
print("spec written:", sys.argv[1])
PYEOF
