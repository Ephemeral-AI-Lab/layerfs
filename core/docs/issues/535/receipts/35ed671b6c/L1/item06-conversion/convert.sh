#!/bin/bash
# L1 item 6: the in-place upgrade (the recorded daemon symlink repointed to
# the 35ed671b6c daemon) and the 35ed671b6c CLI on the 2.0 homes.
#   convert.sh <H6> <H6R> <daemon link> <2.0 CLI> <2.0 bundle>
# Env: LFS (35ed CLI), LAYERFS_DAEMON (35ed daemon), LAYERFS_BUNDLE (sealed 35ed bundle).
H6=$1; H6R=$2; LINK=$3; OLD=$4; OLDB=$5
n() { local h=$1; shift; echo "\$ [35ed] layerfs $*"; LAYERFS_HOME=$h "$LFS" "$@" 2>&1 | head -c 1500; echo " (exit ${PIPESTATUS[0]})"; }
o() { local h=$1; shift; echo "\$ [2.0] layerfs $*"; LAYERFS_HOME=$h LAYERFS_BUNDLE=$OLDB "$OLD" "$@" 2>&1 | head -c 800; echo " (exit ${PIPESTATUS[0]})"; }
lay() { echo "layout: $(cat $1/layerfs.json)  .layout-3: $(test -e $1/.layout-3 && echo yes || echo no)"; }
ln -sfn "$LAYERFS_DAEMON" "$LINK"; echo "daemon link -> $(readlink $LINK)"
echo "== H6R (2.0 home; Sandboxes sr6 on native and sdr7 on docker running)"
lay $H6R
n $H6R store list; n $H6R sandbox list; n $H6R machine list; n $H6R doctor
lay $H6R
echo "-- close both with the build that started them"
o $H6R sandbox close sr6; o $H6R sandbox close sdr7
n $H6R store list
lay $H6R; echo "--- log/migration-2.json"; cat $H6R/log/migration-2.json 2>&1 | head -c 1500; echo
n $H6R doctor --text
echo "== H6 (2.0 home; every Sandbox closed)"
lay $H6
n $H6 store list
lay $H6; echo "--- log/migration-2.json"; cat $H6/log/migration-2.json 2>&1 | head -c 1500; echo
n $H6 file read n6/p lane /v20.txt
n $H6 file read d7/p lane /v20.txt
n $H6 machine status docker
n $H6 machine stop docker
n $H6 machine start docker
n $H6 file read d7/p lane /v20.txt
WC=lfs-$(python3 -c "import json;print(json.load(open('$H6/layerfs.json'))['id'][:8])")-docker
echo "--- docker worker $WC /layerfs/data after the restart"; docker exec $WC sh -c 'cd /layerfs/data && find . -maxdepth 5 | sort | head -60'
n $H6 store status d6
n $H6 doctor --text
echo CONVERT-DONE
