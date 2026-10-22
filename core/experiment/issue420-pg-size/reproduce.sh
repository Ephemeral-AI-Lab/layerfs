#!/bin/sh
# Re-creates the size study from scratch, in the order it was run (each step well under 120 s).
# Receipts of the original run: core/target/issue420/size/20261022T191100Z/
set -e
D=$(dirname "$0"); . $D/env.sh
$D/start.sh; sleep 5; DB=postgres q -c "CREATE DATABASE size"
mkdir -p $OUT/bin; cp $WT/core/target/issue420/pg-target/release/issue420-pg $OUT/bin/   # sha256 e8b5032e...
U=postgresql://postgres@localhost/size; S=/fix/store-r060.sqlite
bin schema $U $S
for t in file_digest commit commit_lineage branch layer layer_lineage layer_stack content_signature scope_allocator \
  store_policy store_totals history_meta reach absorbed binding_hint inode_value object_location pack; do bin load $S $U $t; done
bin finish $U $S
q -c "CREATE EXTENSION pgstattuple; CREATE EXTENSION pg_prewarm; CREATE EXTENSION pageinspect"
for f in iv_variants iv_variants2 bh_variants ol_variants pack_variants pack_variants2 a_relaxed pack_variants3; do q -f - < $D/$f.sql; done
q -c "DROP TABLE iv_b64p; DROP TABLE bh_a2; DROP FUNCTION bh_key(bigint,bigint,bytea)"   # superseded arms
q -f - < $D/bench_setup.sql; q -f - < $D/bench_stage.sql
# timing arms: $D/bench_arm.sh ARM TABLES ; growth: $D/longrun_arm.sh, $D/longrun_cadence.sh ; checks: verify.sql
# Publish diagnostic (second run, 2 GB container): TS=<new> MEM=2g ./start.sh; create db; load file_digest binding_hint
# inode_value object_location pack with bin load; bin finish; then publish_layouts.sql, publish_stage.sql and
# ./publish_arm.sh p0|pa|pr|pb <tables> (scripts in pub/).
