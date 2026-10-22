#!/bin/sh
# pubcase.sh NAME BTABLE LEVEL DICT: one uncontended bundle publish sample.
# Fresh Postgres clone of bb (TEMPLATE FILE_COPY) + sync, then bpub on the
# host cache /sq/replica.sqlite (left unchanged: own numbering rolled back).
set -e
NAME=$1 BT=$2 LV=$3 DICT=$4
R=core/target/issue420/bundle/20261022T194136Z
B=core/experiment/issue420-bundle
DB=pub_$NAME
[ -e $R/pub/$NAME.json ] && { echo "receipt exists: $NAME"; exit 1; }
docker exec lfs420-bundle-pg psql -U postgres -qAtc "CREATE DATABASE $DB TEMPLATE bb STRATEGY FILE_COPY"
docker exec lfs420-bundle-pg sync
echo "pub $NAME start $(date -u +%FT%TZ) $(sysctl -n vm.loadavg)" >> $R/host-load.txt
$B/run.sh bpub /work/core/target/issue420/capture/20261022T184243Z /work/core/target/issue420/fixture/store-r060.sqlite /sq/replica.sqlite "host=127.0.0.1 user=postgres dbname=$DB" $BT fit $LV $DICT /work/$R/pub/$NAME.json
echo "pub $NAME end $(date -u +%FT%TZ) $(sysctl -n vm.loadavg)" >> $R/host-load.txt
