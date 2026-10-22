#!/bin/sh
# Steady-state growth diagnostic: N more one-Commit inserts, sizes before/after and after VACUUM.
# Usage: longrun_arm.sh ARM TABLE N
. "$(dirname "$0")/env.sh"
ARM=$1; T=$2; N=$3; R=$OUT/longrun; mkdir -p $R
sz() { q -At -c "SELECT '$1', pg_relation_size('$T'), pg_indexes_size('$T'), pg_total_relation_size('$T'), (SELECT n_live_tup||'/'||n_dead_tup||'/'||coalesce(autovacuum_count,0) FROM pg_stat_user_tables WHERE relname='$T')"; }
{ date -u +%FT%TZ; uptime; sz before
docker exec -e PGOPTIONS='-c statement_timeout=30s' $C pgbench -U postgres -n -M prepared -c 1 -j 1 -t $N -f /bench/ins_$ARM.sql size 2>&1 | grep -E "latency average|failed|processed"
sz after; q -c "VACUUM $T" >/dev/null; sz vacuumed; date -u +%FT%TZ; } > $R/$ARM.txt 2>&1
echo "$ARM $(grep -E 'before|after|vacuumed|latency' $R/$ARM.txt | tr '\n' ' ')"
