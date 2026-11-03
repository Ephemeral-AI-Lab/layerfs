#!/bin/bash
# item 9 for one machine: a Store on it, stop, list/status/store status, start
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l2/env.sh
m=$1; st=pw$2; log=$R/power-$m.log
T="$S/../timeout 115"
$T $S/run.sh $log store create $st >/dev/null
$T $S/run.sh $log store push $st $m >/dev/null
$T $S/run.sh $log machine stop $m
$T $S/run.sh $log machine list
$T $S/run.sh $log machine status $m
$T $S/run.sh $log store status $st
$T $S/run.sh $log store get $st
$T $S/run.sh $log machine list
$T $S/run.sh $log machine status $m
ext() { { echo "# external check ($1) at $(date -u +%H:%M:%S)"; eval "$2" 2>&1 | sed "s#$S#<scratch>#g"; echo; } >> $log; }
ext "after the second list" "$EXT"
$T $S/run.sh $log machine start $m
ext "after start" "$EXT"
$T $S/run.sh $log machine status $m
$T $S/run.sh $log store status $st
