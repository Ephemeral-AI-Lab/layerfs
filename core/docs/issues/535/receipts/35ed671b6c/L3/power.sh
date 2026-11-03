#!/bin/bash
# item 9: machine power on a VM machine, with a throwaway Store pw pushed there.
# usage: power.sh <machine> <log> <independent-check command...>
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l3
m=$1 log=$2; shift 2
chk() { echo "\$ (independent check) $*" >> $log; "$@" >> $log 2>&1; echo >> $log; }
r() { $S/run.sh $log "$@" >/dev/null; }
r store create pw
r store push pw $m
r store status pw
r machine stop $m
r machine list
r machine status $m
r store status pw
r machine list
chk "$@"
r machine start $m
r machine status $m
r store status pw
r store pull pw
r store remove pw
chk "$@"
echo DONE >> $log
