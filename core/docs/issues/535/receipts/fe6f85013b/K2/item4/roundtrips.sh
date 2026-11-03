#!/bin/bash
# #535 phase 8 item 6 (R1/R2), lane K2, on apple-container machine lfs535k2a.
# LAYERFS_TRACE is set on each call; whether a trace file appears is recorded.
# Substitute observation: the guest kernel's TCP counters (/proc/net/snmp
# ActiveOpens/PassiveOpens) and its socket table (/proc/net/tcp) read with
# `container exec` just before and just after each call. Inbound connections
# from outside the guest = dPassiveOpens - dActiveOpens (each guest-loopback
# connection adds one of each; a sqlite Store dials nothing outside).
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k2/env.sh
log=$R/item4/roundtrips.log
N=$(container list | awk 'NR>1{print $1}' | grep "lfs535k2a$")
snap() { container exec $N sh -c 'grep "^Tcp:" /proc/net/snmp | tail -1; awk "NR>1{print \$2, \$3, \$4}" /proc/net/tcp'; }
counters() { container exec $N sh -c 'grep "^Tcp:" /proc/net/snmp | tail -1' | awk '{print $6, $7}'; }
# ports: hex local/remote and state (01 ESTABLISHED, 06 TIME_WAIT, 0A LISTEN)
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
  if [ -s $tf ]; then note "### LAYERFS_TRACE file: $(wc -l < $tf) lines"; cat $tf >> $log; else note "### LAYERFS_TRACE file $(basename $tf): not created"; fi
  note ""
}
src=$(mktemp -d $S/trcsrc.XXXX); echo "hello from the mac" > $src/hello.txt
$S/run.sh $log store create trc3 > /dev/null
$S/run.sh $log project init trc3/p $src > /dev/null
# attempt 1 mounted main (NoSuchBranch); attempt 2 forked before the push, and the push
# refused: a CLI write leaves a write-ahead log (cli-wal-after-fork*.log). Fork after the push.
$S/run.sh $log store push trc3 lfs535k2a > /dev/null
$S/run.sh $log branch fork trc3/p lane main > /dev/null
$S/run.sh $log file read trc3/p main /hello.txt > /dev/null   # warm: first call after push
measured r1-file-read file read trc3/p main /hello.txt
measured r1-file-stat file stat trc3/p main /hello.txt
$S/run.sh $log sandbox create trc3/p trc3 lfs535k2a > /dev/null
ws=$($S/run.sh $log workspace mount trc3 lane | head -1 | python3 -c 'import json,sys;print(json.load(sys.stdin)["ws"])')
$S/run.sh $log workspace exec $ws 'echo warm' > /dev/null
measured r2-exec workspace exec $ws 'echo exec on the VM; uname -n'
measured r2-exec-2 workspace exec $ws 'echo again'
$S/run.sh $log workspace unmount $ws --discard > /dev/null
$S/run.sh $log sandbox close trc3 > /dev/null
$S/run.sh $log store pull trc3 > /dev/null
$S/run.sh $log store remove trc3 > /dev/null
rm -rf $src
