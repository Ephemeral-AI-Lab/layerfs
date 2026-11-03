#!/bin/bash
# K1 item 7 (#536 §12 item 3): store.push (native -> docker), store.pull
# (docker -> native) and sandbox.create, each SIGKILLed at each point
# intent_kill.py offers; after each kill: the records, then store.status /
# store.get / sandbox.get / doctor with hashes of every home file and every
# file under the docker worker's /layerfs/data before and after those reads;
# then the clearing call.
#   intents.sh <home> <data folder> <out dir>
# Env: the lane env (LFS, K, PYTHONPATH via K, LAYERFS_BUNDLE, LAYERFS_DAEMON).
H=$1; DATA=$2; O=$3
mkdir -p "$O"
export PYTHONPATH=$K/python/src
X="python3 $R/intent_kill.py"
l() { LAYERFS_HOME=$H "$LFS" "$@"; }
WC=lfs-$(python3 -c "import json;print(json.load(open('$H/layerfs.json'))['id'][:8])")-docker
hashes() {
  (cd "$H" && find . -type f ! -path './trace/*' -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256)
  docker exec "$WC" sh -c 'cd /layerfs/data 2>/dev/null && find . -type f | LC_ALL=C sort | xargs -r sha256sum' | sed 's/^/worker /'
}
show() { echo "\$ layerfs $*"; l "$@" 2>&1; echo "(exit $?)"; }
reads() { # $1 kind (store|sandbox) $2 name
  hashes > "$O/$3.before-reads"
  {
    if [ "$1" = store ]; then show store status "$2"; show store get "$2"; show store list; else show sandbox get "$2"; show sandbox list; fi
    show doctor
  } > "$O/$3.reads.log"
  hashes > "$O/$3.after-reads"
  if cmp -s "$O/$3.before-reads" "$O/$3.after-reads"; then echo "READS UNCHANGED $3"; else echo "READS CHANGED $3"; diff "$O/$3.before-reads" "$O/$3.after-reads"; fi
}
records() { # $1 tag
  { echo "--- home files"; (cd "$H" && find . -type f ! -path './trace/*' | LC_ALL=C sort)
    for f in "$H"/store/*/record.json "$H"/sandbox/*/record.json; do [ -f "$f" ] && { echo "--- ${f#$H/}"; cat "$f"; echo; }; done
    echo "--- worker /layerfs/data"; docker exec "$WC" sh -c 'cd /layerfs/data && find . -type f | LC_ALL=C sort'
    echo "--- native daemons"; pgrep -fl layerfs-daemon | grep -F "$H" ; echo "--- containers"; docker ps -a --format '{{.Names}} {{.Status}}' | grep -F "${WC%-docker}"
  } > "$O/$1.records.log" 2>&1
}
echo "== push kills $(date -u +%FT%TZ)"
for p in check snapshot send send-mid verify install record record-window retire; do
  s=pu-$p
  l store create "$s" >/dev/null && l project init "$s/p" "$DATA" >/dev/null || { echo "setup failed $s"; continue; }
  $X "$H" push "$s" docker "$p" "$O/$s.events.log"; echo "$s exit $?"
  grep -E "SIGKILL|answered|refused" "$O/$s.events.log" | tail -2
  records "$s"; reads store "$s" "$s"
done
echo "== pull kills $(date -u +%FT%TZ)"
for p in check snapshot send send-mid verify install record record-window retire; do
  s=pl-$p
  l store create "$s" >/dev/null && l project init "$s/p" "$DATA" >/dev/null && l store push "$s" docker >/dev/null || { echo "setup failed $s"; continue; }
  $X "$H" pull "$s" - "$p" "$O/$s.events.log"; echo "$s exit $?"
  grep -E "SIGKILL|answered|refused" "$O/$s.events.log" | tail -2
  records "$s"; reads store "$s" "$s"
done
echo "== create kills $(date -u +%FT%TZ)"
l store create cr >/dev/null; l project init cr/p "$DATA" >/dev/null
l store create crd >/dev/null; l project init crd/p "$DATA" >/dev/null; l store push crd docker >/dev/null
for t in claim:native start:native claim:docker start:docker; do
  p=${t%%:*}; m=${t##*:}; n=k-$p-$m; proj=cr/p; [ "$m" = docker ] && proj=crd/p
  $X "$H" create "$proj" "$n:$m" "$p" "$O/$n.events.log"; echo "$n exit $?"
  grep -E "SIGKILL|answered|refused" "$O/$n.events.log" | tail -2
  records "$n"; reads sandbox "$n" "$n"
done
echo "INTENTS DONE $(date -u +%FT%TZ)"
