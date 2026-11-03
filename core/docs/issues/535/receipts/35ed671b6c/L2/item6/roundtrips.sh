#!/bin/bash
# #535 phase 8 item 6 (R1/R2), lane L2, on apple-container machine lfs535l2a.
# Copied from K2's item4/roundtrips.sh. R1 is now checked by LAYERFS_TRACE
# (one {event:"worker.connection"} line per connection to the machine's
# worker); the guest kernel's TCP counters (/proc/net/snmp
# ActiveOpens/PassiveOpens) and socket table (/proc/net/tcp), read with
# `container exec` just before and after each call, are kept for comparison
# with K2. Inbound connections from outside the guest = dPassive - dActive.
# The Branch is forked BEFORE the push (K2 D3 fixed: the CLI closes its Stores).
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l2/env.sh
log=$R/item6/roundtrips.log
N=$(container list | awk 'NR>1{print $1}' | grep "lfs535l2a$")
snap() { container exec $N sh -c 'grep "^Tcp:" /proc/net/snmp | tail -1; awk "NR>1{print \$2, \$3, \$4}" /proc/net/tcp'; }
counters() { container exec $N sh -c 'grep "^Tcp:" /proc/net/snmp | tail -1' | awk '{print $6, $7}'; }
note() { echo "$*" >> $log; }
measured() { # label, then the layerfs args
  label=$1; shift
  tf=$S/trace-$label.jsonl; rm -f $tf
  note "## $label"
  note "### guest sockets before"; snap >> $log
  b=$(counters)
  LAYERFS_TRACE=$tf $S/run.sh $log "$@" > /dev/null
  a=$(counters)
  note "### guest sockets after"; snap >> $log
  set -- $b; ba=$1; bp=$2; set -- $a
  note "### counters ActiveOpens/PassiveOpens before $ba/$bp after $1/$2: dActive $(( $1 - ba )) dPassive $(( $2 - bp )) inbound-from-outside $(( ($2 - bp) - ($1 - ba) ))"
  if [ -s $tf ]; then note "### LAYERFS_TRACE file: $(wc -l < $tf | tr -d ' ') lines"; cat $tf >> $log; else note "### LAYERFS_TRACE file $(basename $tf): not created"; fi
  note ""
}
src=$(mktemp -d $S/trcsrc.XXXX); echo "hello from the mac" > $src/hello.txt
$S/run.sh $log store create trc6 > /dev/null
$S/run.sh $log project init trc6/p $src > /dev/null
$S/run.sh $log branch fork trc6/p lane main > /dev/null
$S/run.sh $log store push trc6 lfs535l2a > /dev/null
$S/run.sh $log file read trc6/p main /hello.txt > /dev/null   # warm: first call after push
measured r1-file-read file read trc6/p main /hello.txt
measured r1-file-stat file stat trc6/p main /hello.txt
measured r1-branch-list branch list trc6/p
# a native (in-process) call writes no line
$S/run.sh $log store create trc6n > /dev/null
$S/run.sh $log project init trc6n/p $src > /dev/null
measured r1-native-file-read file read trc6n/p main /hello.txt
$S/run.sh $log store remove trc6n > /dev/null
$S/run.sh $log sandbox create trc6/p trc6 lfs535l2a > /dev/null
measured fix-mount-main workspace mount trc6 main
ws=$($S/run.sh $log workspace mount trc6 lane | head -1 | python3 -c 'import json,sys;print(json.load(sys.stdin)["ws"])')
$S/run.sh $log workspace exec $ws 'echo warm' > /dev/null
measured r2-exec workspace exec $ws 'echo exec on the VM; uname -n'
measured r2-exec-2 workspace exec $ws 'echo again'
$S/run.sh $log workspace unmount $ws --discard > /dev/null
$S/run.sh $log sandbox close trc6 > /dev/null
$S/run.sh $log store pull trc6 > /dev/null
$S/run.sh $log store remove trc6 > /dev/null
rm -rf $src
