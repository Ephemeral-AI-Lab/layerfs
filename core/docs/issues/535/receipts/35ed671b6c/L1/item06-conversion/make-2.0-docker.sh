#!/bin/bash
# L1 item 6, docker parts (attempt 3): attempt 2's `store move` of a Store the
# 2.0 CLI had forked a Branch in was refused "unfinished write-ahead log" (the
# 2.0 CLI did not close its Stores: K2 D3, fixed since). So here the Store is
# moved right after Init (no CLI fork on native) and the Branch is forked on docker.
#   make-2.0-docker.sh <H6> <H6R> <data> <2.0 CLI> <2.0 bundle>
H6=$1; H6R=$2; DATA=$3; OLD=$4; export LAYERFS_BUNDLE=$5
o() { local h=$1; shift; echo "\$ [2.0] layerfs $*"; LAYERFS_HOME=$h "$OLD" "$@" 2>&1 | head -c 1200; echo " (exit ${PIPESTATUS[0]})"; }
life() { local h=$1 st=$2 sb=$3 m=$4
  echo "\$ [2.0] layerfs workspace mount $sb lane"; local out; out=$(LAYERFS_HOME=$h "$OLD" workspace mount $sb lane 2>&1); echo "$out" | head -c 1200; echo
  local w; w=$(echo "$out" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d.get('ws') or (sys.argv[1]+':'+d['path']))" $sb) || { echo "no ws"; return; }
  o $h workspace exec "$w" "echo two-oh-$m > v20.txt" --message "2.0 write"
  o $h workspace commit "$w" --message "2.0 commit"
  o $h workspace unmount "$w"
  o $h file read $st/p lane /v20.txt
}
echo "== H6 docker part (Store d7)"
o $H6 store create d7; o $H6 project init d7/p "$DATA"; o $H6 store status d7; o $H6 store move d7 docker; o $H6 branch fork d7/p lane main
o $H6 sandbox create d7/p sd7 docker; life $H6 d7 sd7 docker; o $H6 sandbox close sd7
o $H6 sandbox list; o $H6 store list
echo "== H6R docker part (Store dr7, Sandbox left running)"
o $H6R store create dr7; o $H6R project init dr7/p "$DATA"; o $H6R store move dr7 docker; o $H6R branch fork dr7/p lane main
o $H6R sandbox create dr7/p sdr7 docker
o $H6R sandbox list
for h in $H6 $H6R; do echo "== layout of $h"; cat $h/layerfs.json; echo; (cd $h && find . -maxdepth 4 -not -path './trace*' | LC_ALL=C sort | head -80); done
echo MAKE-DONE
