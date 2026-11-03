#!/bin/bash
# item 7 parity on apple-container: C01 C05 C09 C12 (sqlite) through the CLI
# binary and the Python package; Node's are item 1's receipts. One per cell.
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k2/env.sh
log=$R/parity/parity.log
for via in cli python; do for c in C01 C05 C09 C12; do
  echo "START $via $c $(date -u +%H:%M:%S) load1 $(sysctl -n vm.loadavg | awk '{print $2}')" >> $log
  PYTHONPATH=$K/python/src perl -e 'alarm 115; exec @ARGV' python3 -B $R/parity/cell.py $via $S/workloads.json $c lfs535k2a $R/parity/apple-container-sqlite-$c-$via.json >> $log 2>&1
  echo "END $via $c rc=$? $(date -u +%H:%M:%S)" >> $log
done; done
echo "PARITY DONE" >> $log
