#!/bin/bash
# L1 item 7 (#536 §12 item 3, the fixed behaviour since fe6f85013b): the
# clearing calls after intents.sh's kills, one per entity, with the state
# before and after and the copies on both sides.
# - certain (stale) ops: the same push or pull again ("the next operation replaces it");
# - unresolved (killed at the record event = op install uncertain, or in the
#   record window): first a Store call (file list) that must be refused
#   `Unresolved` naming the push/pull; then a new push or pull clears it:
#   record-event kills are cleared by the OPPOSITE call (pu-record by
#   `store pull`, pl-record by `store push`), record-window kills by the
#   SAME call, so both directions are exercised; the copy on the machine the
#   record does not name must be gone afterwards;
# - retire kills: the Store answers on its new machine first (file list),
#   `last_transfer.retired: false`; then the next opposite call deletes the
#   old copy and runs; after a pull the worker keeps no folder for the Store;
# - Sandboxes: `sandbox close`.
#   intents_clear.sh <home> [first entity: skip those before it, to continue a cut run]
H=$1; FROM=${2:-}
want() { [ -z "$FROM" ] && return 0; [ "$1" = "$FROM" ] && { FROM=; return 0; }; return 1; }
l() { LAYERFS_HOME=$H "$LFS" "$@"; }
WC=lfs-$(python3 -c "import json;print(json.load(open('$H/layerfs.json'))['id'][:8])")-docker
st() { l store status "$1" 2>&1 | python3 -c "import json,sys;d=json.load(sys.stdin);print('state',d.get('state'),'machine',d.get('machine'),'op',json.dumps(d.get('op')))" 2>/dev/null || l store status "$1" 2>&1 | head -c 300; }
lt() { python3 -c "import json;print('last_transfer', json.dumps(json.load(open('$H/store/$1/record.json')).get('last_transfer')))" 2>&1; }
copies() { echo "   copies: native=[$(cd $H/store/$1 2>/dev/null && ls | grep -E 'store.sqlite' | tr '\n' ' ')] worker=[$(docker exec $WC sh -c "ls -A /layerfs/data/store/$1 2>/dev/null | tr '\n' ' '; test -d /layerfs/data/store/$1 && echo '(folder exists)' || echo '(no folder)'")]"; }
run() { echo "\$ layerfs $*"; l "$@" 2>&1 | head -c 700; echo " (exit ${PIPESTATUS[0]})"; }
for s in pu-check pu-snapshot pu-send pu-send-mid pu-verify pu-install; do
  want $s || continue
  echo "== $s before: $(st $s)"; copies $s; run store push $s docker; echo "   after: $(st $s)"; copies $s
done
for s in pl-check pl-snapshot pl-send pl-send-mid pl-verify pl-install; do
  want $s || continue
  echo "== $s before: $(st $s)"; copies $s; run store pull $s; echo "   after: $(st $s)"; copies $s
done
for s in pu-record pl-record pu-record-window pl-record-window; do
  want $s || continue
  echo "== $s (unresolved expected) before: $(st $s)"; copies $s
  run file list $s/p main /
  case $s in pu-record) c="store pull $s";; pl-record) c="store push $s docker";; pu-record-window) c="store push $s docker";; pl-record-window) c="store pull $s";; esac
  run $c; echo "   after: $(st $s)"; echo "   $(lt $s)"; copies $s
  run file list $s/p main /
done
for s in pu-retire pl-retire; do
  want $s || continue
  echo "== $s (retire not finished) before: $(st $s)"; echo "   $(lt $s)"; copies $s
  run file list $s/p main /
  if [ "$s" = pu-retire ]; then run store pull $s; else run store push $s docker; fi
  echo "   after: $(st $s)"; echo "   $(lt $s)"; copies $s
done
for n in k-claim-native k-start-native k-claim-docker k-start-docker; do
  want $n || continue
  echo "== $n before: $(l sandbox get $n 2>&1 | head -c 300)"; run sandbox close $n; echo "   after: $(l sandbox get $n 2>&1 | head -c 200)"
done
echo "== doctor"; l doctor --text
