#!/bin/bash
# K1 item 7: the clearing calls after intents.sh's kills, one per entity.
# Certain (stale) ops: the documented "next operation replaces it" (the same
# push or pull again); unresolved ops: `store remove` (the call the refusal
# and doctor name); Sandboxes: `sandbox close`. Each: the state before
# (status/get), the call, the state after.
#   intents_clear.sh <home>
H=$1
l() { LAYERFS_HOME=$H "$LFS" "$@"; }
st() { l store status "$1" 2>&1 | python3 -c "import json,sys;d=json.load(sys.stdin);print('state',d.get('state'),'machine',d.get('machine'),'op',json.dumps(d.get('op')))" 2>/dev/null || l store status "$1" 2>&1 | head -c 300; }
run() { echo "\$ layerfs $*"; l "$@" 2>&1 | head -c 600; echo " (exit ${PIPESTATUS[0]})"; }
for s in pu-snapshot pu-send pu-send-mid pu-verify pu-install pu-record iso; do
  echo "== $s before: $(st $s)"; run store push $s docker; echo "   after: $(st $s)"
done
for s in pl-snapshot pl-send pl-send-mid pl-verify pl-install pl-record; do
  echo "== $s before: $(st $s)"; run store pull $s; echo "   after: $(st $s)"
done
for s in pu-record-window pl-record-window; do
  echo "== $s before: $(st $s)"; run store push $s docker; run store pull $s; run store remove $s; echo "   after: $(l store get $s 2>&1 | head -c 200)"
done
for s in pu-retire pl-retire; do
  echo "== $s (no op; the source copy was not retired) before: $(st $s)"
  if [ "$s" = pu-retire ]; then run store pull $s; else run store push $s docker; fi
  echo "   after: $(st $s)"
done
for n in k-claim-native k-start-native k-claim-docker k-start-docker; do
  echo "== $n before: $(l sandbox get $n 2>&1 | head -c 300)"; run sandbox close $n; echo "   after: $(l sandbox get $n 2>&1 | head -c 200)"
done
echo "== doctor"; l doctor --text
