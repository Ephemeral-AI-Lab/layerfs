#!/bin/bash
# L1 item 5: the eight parity cells, one per command, each under a 115 s alarm.
for b in python cli; do for c in C01 C05 C09 C12; do
  echo "$(date -u +%FT%TZ) start $b $c"
  PYTHONPATH=$K/python/src perl -e 'alarm 115; exec @ARGV' python3 $R/parity_cell.py $c $b $R/workloads.json $R/parity-native-$b-$c.json; echo "$(date -u +%FT%TZ) end $b $c rc=$?"
done; done
echo PARITY-SERIES-DONE
