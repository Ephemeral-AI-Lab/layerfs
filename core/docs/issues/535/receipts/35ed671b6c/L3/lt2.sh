#!/bin/bash
# usage: lt2.sh <package dir> <exe under target/debug/deps> [test args...]; one run in a fresh ext4 /tmp container
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l3/paths.env
L=$R/linux-tests.log; pkg=$1 exe=$2; shift 2
echo "\$ lt-exe.sh plain $pkg debug/deps/$exe -- $*" >> $L
echo "# at $(date -u +%FT%TZ) load1 $(sysctl -n vm.loadavg | awk '{print $2}')" >> $L
$S/../timeout 120 $S/lt-exe.sh plain $pkg debug/deps/$exe -- "$@" 2>&1 | grep -vE '^\s*$' | tail -25 | tee -a $L
echo >> $L
