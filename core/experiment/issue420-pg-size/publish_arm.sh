#!/bin/sh
# Whole-publish diagnostic for one layout: prewarm its relations, then 60 Commits
# on one client (per-statement latency), then 4 clients x 50 Commits (throughput).
# Usage: TS=... publish_arm.sh ARM TABLE[,TABLE...]
. "$(dirname "$0")/env.sh"
ARM=$1; R=$OUT/publish; mkdir -p $R
docker cp -q "$(dirname "$0")/pub/." $C:/pub
for t in $(echo $2 | tr , ' ') file_digest head st_ol st_ol_b16 st_iv st_iv_b32 st_bh st_bh_b3 st_fd st_pk; do
  q -At -c "SELECT '$t', sum(pg_prewarm(r)) FROM (SELECT '$t'::regclass::oid r UNION ALL SELECT reltoastrelid FROM pg_class WHERE oid='$t'::regclass AND reltoastrelid<>0 UNION ALL SELECT indexrelid FROM pg_index WHERE indrelid IN ('$t'::regclass, (SELECT reltoastrelid FROM pg_class WHERE oid='$t'::regclass))) s"
done > $R/$ARM.prewarm.txt 2>&1
run() { docker exec -e PGOPTIONS='-c statement_timeout=30s' $C pgbench -U postgres -n -M prepared -r "$@" size; }
{ date -u +%FT%TZ; uptime; } > $R/$ARM.load.txt
L0=$(q -At -c "select pg_current_wal_insert_lsn()")
run -c 1 -j 1 -t 60 -f /pub/publish_$ARM.sql > $R/$ARM.c1.txt 2>&1
L1=$(q -At -c "select pg_current_wal_insert_lsn()")
run -c 4 -j 4 -t 50 -f /pub/publish_$ARM.sql > $R/$ARM.c4.txt 2>&1
{ date -u +%FT%TZ; uptime; } >> $R/$ARM.load.txt
echo "$ARM c1 $(grep -E 'latency average' $R/$ARM.c1.txt) wal/commit=$(q -At -c "select (pg_wal_lsn_diff('$L1','$L0')/60)::bigint") | c4 $(grep -E 'latency average|^tps' $R/$ARM.c4.txt | tr '\n' ' ') failed: $(grep -h 'failed' $R/$ARM.c1.txt $R/$ARM.c4.txt | tr -s ' ' | tr '\n' ' ')"
