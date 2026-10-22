#!/bin/sh
# One arm, one sample of each case: prewarm the arm's relations (buffer mode),
# then point lookups, range lookups (if scripted), then 60 one-Commit inserts.
# Usage: bench_arm.sh ARM TABLE[,TABLE...]
. "$(dirname "$0")/env.sh"
ARM=$1; TABLES=$2; R=$OUT/bench; mkdir -p $R
docker cp -q "$(dirname "$0")/bench/." $C:/bench
for t in $(echo $TABLES | tr , ' ') ol_keys iv_keys bh_keys pk_keys; do
  q -At -c "SELECT '$t', sum(pg_prewarm(r)) FROM (SELECT '$t'::regclass::oid r UNION ALL SELECT reltoastrelid FROM pg_class WHERE oid='$t'::regclass AND reltoastrelid<>0 UNION ALL SELECT indexrelid FROM pg_index WHERE indrelid IN ('$t'::regclass, (SELECT reltoastrelid FROM pg_class WHERE oid='$t'::regclass))) s"
done > $R/$ARM.prewarm.txt 2>&1
{ date -u +%FT%TZ; uptime; } > $R/$ARM.load.txt
run() { docker exec -e PGOPTIONS='-c statement_timeout=30s' $C pgbench -U postgres -n -M prepared -c 1 -j 1 -r "$@" size; }
[ -f "$(dirname "$0")/bench/get_$ARM.sql" ] && run -t 20000 -f /bench/get_$ARM.sql > $R/$ARM.get.txt 2>&1
[ -f "$(dirname "$0")/bench/rng_$ARM.sql" ] && run -t 5000 -f /bench/rng_$ARM.sql > $R/$ARM.rng.txt 2>&1
run -t 60 -f /bench/ins_$ARM.sql > $R/$ARM.ins.txt 2>&1
{ date -u +%FT%TZ; uptime; } >> $R/$ARM.load.txt
for f in get rng ins; do [ -f $R/$ARM.$f.txt ] && printf '%s %s %s\n' $ARM $f "$(grep -E 'latency average|failed' $R/$ARM.$f.txt | tr -s ' ' | tr '\n' ' ')"; done
