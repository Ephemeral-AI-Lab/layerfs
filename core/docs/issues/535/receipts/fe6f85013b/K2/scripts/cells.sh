#!/bin/bash
# K2 cell queue: runs cells one after another, one sample each, no retry.
# usage: cells.sh <queue-log> <platform> <machine> <store: sqlite|pg-loop|pg-remote> <db-prefix|-> <C..>...
# Each cell holds the machine lock, then the cell-number lock (the shared home
# names every cell's Store c<nn>), so two queues never run on one machine or
# one Store name at once. Limit 115 s per cell (perl alarm, as #533).
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/k2/env.sh
qlog=$1 platform=$2 machine=$3 store=$4 prefix=$5; shift 5
for c in "$@"; do
  unset LFS535_PG_URL LFS535_PG_PASSWORD
  case $store in
    pg-loop) export LFS535_PG_URL="postgres://postgres@127.0.0.1:55542/${prefix}{cell}" LFS535_PG_PASSWORD=env:LFSPW;;
    pg-remote) export LFS535_PG_URL="postgres://postgres@192.168.2.123:55542/${prefix}{cell}" LFS535_PG_PASSWORD=env:LFSPW;;
  esac
  rec=$R/$platform-$store-$c.json
  echo "START $platform $store $c $(date -u +%H:%M:%S)" >> $qlog
  out=$(cd $K/node && lockf $S/locks/$machine.lock lockf $S/locks/$c.lock perl -e 'alarm 115; exec @ARGV' node test/verify/cells/cell.mjs $c $machine $rec 2>&1); rc=$?
  echo "$out" | sed "s/$LFSPW/<redacted>/g" | tail -3 >> $qlog
  [ -f $rec ] && sed -i '' "s/$LFSPW/<redacted>/g" $rec
  echo "END $platform $store $c rc=$rc $(date -u +%H:%M:%S)" >> $qlog
done
echo "QUEUE DONE $platform $store" >> $qlog
