#!/bin/bash
# usage: lt2.sh <exe under target/debug/deps> <test name>; one run in a fresh ext4 /tmp container
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k3/paths.env
L=$R/linux-tests.log
echo "\$ lt-exe.sh plain api/crates/layerfs-api debug/deps/$1 -- --exact $2" >> $L
echo "# at $(date -u +%FT%TZ) load1 $(sysctl -n vm.loadavg | awk '{print $2}')" >> $L
$S/../timeout 120 $S/lt-exe.sh plain api/crates/layerfs-api debug/deps/$1 -- --exact $2 2>&1 | grep -vE '^\s*$' | tail -12 | tee -a $L
echo >> $L
