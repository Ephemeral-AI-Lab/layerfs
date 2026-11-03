#!/bin/bash
# L1 item 6: two 2.0 homes made with the 58b8d1742b CLI and daemon (the
# daemon recorded through a symlink, so an in-place upgrade can repoint it)
# and the 2.0 Linux bundle: H6 (every Sandbox closed after a Commit) and H6R
# (a native and a docker Sandbox left running).
#   make-2.0-homes.sh <H6> <H6R> <data> <2.0 CLI> <daemon link> <2.0 bundle> [docker]
# With a 7th argument `docker`, only the docker parts (after a first run whose
# docker parts were refused: 2.0 needs `machine update docker --image` first).
H6=$1; H6R=$2; DATA=$3; OLD=$4; LINK=$5; export LAYERFS_BUNDLE=$6; ONLY=${7:-}
IMG=sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e
o() { local h=$1; shift; echo "\$ [2.0] layerfs $*"; LAYERFS_HOME=$h "$OLD" "$@" 2>&1 | head -c 1200; echo " (exit ${PIPESTATUS[0]})"; }
ws() { python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('ws') or (sys.argv[1]+':'+d['path']))" "$1"; }
life() { # home store sandbox machine: mount lane, write, Commit, unmount
  local h=$1 st=$2 sb=$3 m=$4
  echo "\$ [2.0] layerfs workspace mount $sb lane"; local out; out=$(LAYERFS_HOME=$h "$OLD" workspace mount $sb lane 2>&1); echo "$out" | head -c 1200; echo
  local w; w=$(echo "$out" | ws $sb) || { echo "no ws"; return; }
  o $h workspace exec "$w" "echo two-oh-$m > v20.txt" --message "2.0 write"
  o $h workspace commit "$w" --message "2.0 commit"
  o $h workspace unmount "$w"
  o $h file read $st/p lane /v20.txt
}
if [ -z "$ONLY" ]; then
for h in $H6 $H6R; do
  echo "== home $h"; echo "\$ [2.0] import-env"; LAYERFS_HOME=$h LAYERFS_DAEMON=$LINK "$OLD" import-env 2>&1; echo
done
echo "== H6 native part"
o $H6 store create n6; o $H6 project init n6/p "$DATA"; o $H6 branch fork n6/p lane main
o $H6 sandbox create n6/p sn6 native; life $H6 n6 sn6 native; o $H6 sandbox close sn6
fi
echo "== H6 docker part"
[ -z "$ONLY" ] && { o $H6 store create d6; o $H6 project init d6/p "$DATA"; o $H6 branch fork d6/p lane main; }
o $H6 machine update docker --image $IMG
o $H6 store move d6 docker
o $H6 sandbox create d6/p sd6 docker; life $H6 d6 sd6 docker; o $H6 sandbox close sd6
o $H6 sandbox list; o $H6 store list
echo "== H6R: one native and one docker Sandbox left running"
if [ -z "$ONLY" ]; then
o $H6R store create r6; o $H6R project init r6/p "$DATA"; o $H6R branch fork r6/p lane main
o $H6R sandbox create r6/p sr6 native
o $H6R store create dr6; o $H6R project init dr6/p "$DATA"; o $H6R branch fork dr6/p lane main
fi
o $H6R machine update docker --image $IMG
o $H6R store move dr6 docker
o $H6R sandbox create dr6/p sdr6 docker
o $H6R sandbox list
for h in $H6 $H6R; do echo "== layout of $h"; cat $h/layerfs.json; echo; (cd $h && find . -maxdepth 4 -not -path './trace*' | LC_ALL=C sort | head -80); done
echo MAKE-DONE
