#!/bin/bash
# R3 item: LAYERFS_TRACE on Linux docker. In the shared docker home (machine
# running): a throwaway sqlite Store `tr` is created, Init, pushed to docker
# (untraced); then ONE docker-routed Store call (`branch.list`) runs in its
# own process with LAYERFS_TRACE naming a fresh file, and the file's lines are
# counted (expected: exactly one `worker.connection` line). Then pulled back
# and removed (untraced). As root in lfs535r3x.
X=$(dirname "$(readlink -f "$0")")
H=/root/lfs535r3-dockerhome
T=$X/receipts/trace/linux-docker-tracecheck.jsonl
src=$(mktemp -d)
c() { echo "\$ node call.mjs <home> $1 '$2'"; echo "# at $(date -u +%FT%TZ)"; (cd "$X" && timeout 115 ./node call.mjs "$H" "$1" "$2"); echo "exit $?"; echo; }
unset LAYERFS_TRACE
c store.create '["tr"]'
c project.init "[\"tr/p\",\"$src\"]"
c store.push '["tr","docker"]'
rm -f "$T"
echo "## the traced call (LAYERFS_TRACE=$T)"
export LAYERFS_TRACE=$T
c branch.list '["tr/p"]'
unset LAYERFS_TRACE
echo "## trace file: $(wc -l < "$T") line(s)"; cat "$T"; echo
python3 - "$T" <<'PY'
import json,sys
lines=[json.loads(l) for l in open(sys.argv[1]) if l.strip()]
wc=[l for l in lines if l.get("event")=="worker.connection"]
keys=sorted(set(k for l in lines for k in l))
print("lines", len(lines), "worker.connection", len(wc), "keys", keys)
print("VERDICT", "PASS" if len(lines)==1 and len(wc)==1 and set(keys)>={"at","event","op","worker","sent","ms"} else "FAIL")
PY
c store.pull '["tr"]'
c store.remove '["tr"]'
rm -rf "$src"
