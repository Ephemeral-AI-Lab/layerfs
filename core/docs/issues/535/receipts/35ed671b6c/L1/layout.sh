#!/bin/bash
# L1 item 9 (#536 §12 items 5, 6): per home and its docker worker root: the
# layout, any Sandbox record outside sandbox/<name>/, any
# store/*/project/*/sandboxes, the doctor; then layerfs-api/src at HEAD.
#   layout.sh <label=home>...
for arg in "$@"; do
  label=${arg%%=*}; h=${arg#*=}
  echo "== home $label"
  echo "layout: $(cat $h/layerfs.json)"
  echo "-- record.json outside sandbox/<name>/ that look like Sandbox records (backend/runtime_id/route/runtime):"
  (cd $h && find . -name '*.json' -not -path './sandbox/*' -not -path './trace/*' -not -path './log/*' -print0 | xargs -0 grep -l -E '"(backend|runtime_id|route|runtime)"' 2>/dev/null | grep -v '^./machine/[^/]*/record.json$')
  echo "-- paths named sandbox* outside ./sandbox/:"; (cd $h && find . -name 'sandbox*' -not -path './sandbox' -not -path './sandbox/*')
  echo "-- store/*/project/*/sandboxes:"; (cd $h && find . -path './store/*/project/*/sandboxes')
  echo "-- sandbox/ entries: $(ls $h/sandbox 2>/dev/null | tr '\n' ' ')"
  echo "-- doctor:"; LAYERFS_HOME=$h "$LFS" doctor --text 2>&1
  WC=lfs-$(python3 -c "import json;print(json.load(open('$h/layerfs.json'))['id'][:8])")-docker
  if [ "$(docker inspect -f '{{.State.Running}}' $WC 2>/dev/null)" = true ]; then
    echo "-- docker worker $WC (/layerfs/data): sandboxes dirs under store, records outside sandbox/, top tree"
    docker exec $WC sh -c 'cd /layerfs/data && find . -path "./store/*/project/*/sandboxes"; find . -name "*.json" -not -path "./sandbox/*" | xargs -r grep -l -E "\"(backend|runtime_id|route)\"" 2>/dev/null; find . -maxdepth 3 | sort | head -40'
  else echo "-- docker worker $WC: not running (state: $(docker inspect -f '{{.State.Status}}' $WC 2>&1 | head -1))"; fi
done
echo "== size: api/crates/layerfs-api/src at $(git -C "$W" rev-parse --short=10 HEAD)"
git -C "$W" ls-tree -r --name-only HEAD api/crates/layerfs-api/src
