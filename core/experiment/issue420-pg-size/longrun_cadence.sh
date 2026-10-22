#!/bin/sh
# Round two: CHUNKS x (K one-Commit inserts, then VACUUM as autovacuum would between them).
# Usage: longrun_cadence.sh ARM TABLE CHUNKS K
. "$(dirname "$0")/env.sh"
ARM=$1; T=$2; CH=$3; K=$4; R=$OUT/longrun; mkdir -p $R
sz() { q -At -c "SELECT '$1', pg_relation_size('$T'), pg_indexes_size('$T'), pg_total_relation_size('$T'), (SELECT n_live_tup FROM pg_stat_user_tables WHERE relname='$T')"; }
{ date -u +%FT%TZ; uptime; sz before; i=0
while [ $i -lt $CH ]; do
  docker exec -e PGOPTIONS='-c statement_timeout=30s' $C pgbench -U postgres -n -M prepared -c 1 -j 1 -t $K -f /bench/ins_$ARM.sql size 2>&1 | grep -E "latency average"
  q -c "VACUUM $T" > /dev/null; i=$((i+1)); done
sz after; date -u +%FT%TZ; } > $R/$ARM.round2.txt 2>&1
echo "$ARM $(grep -E 'before|after' $R/$ARM.round2.txt | tr '\n' ' ') lat: $(grep latency $R/$ARM.round2.txt | awk '{print $4}' | tr '\n' ' ')"
