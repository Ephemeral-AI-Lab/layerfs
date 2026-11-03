#!/bin/bash
# usage: lt.sh <test target> <test name>; one run, logged to linux-tests.log
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k3/paths.env
L=$R/linux-tests.log; RES=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue535-verify-417967c19d8d/resources.json
cd "$W"
{ echo "\$ python3 tools/worktree.py cargo --resources <resources.json> --platform linux --timeout 100 -- test -p layerfs-api --features machine --test $1 -- --exact $2"; echo "# at $(date -u +%FT%TZ) load1 $(sysctl -n vm.loadavg | awk '{print $2}')"; } >> $L
t0=$(date +%s)
$S/../timeout 120 python3 tools/worktree.py cargo --resources $RES --platform linux --timeout 100 -- test -p layerfs-api --features machine --test "$1" -- --exact "$2" > $S/lt-$1.out 2>&1; rc=$?
grep -E "^running|^test |test result|panicked|refused|error" $S/lt-$1.out | tee -a $L
echo "exit $rc wall_s $(( $(date +%s)-t0 ))" | tee -a $L; echo >> $L
