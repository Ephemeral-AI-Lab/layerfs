#!/bin/sh
# pubcase-wl.sh NAME BTABLE LEVEL DICT: as pubcase.sh, but the host cache is
# a fresh byte copy of /sq/replica.sqlite written last + sync (the declared
# cache state of the reused pg/20261022T190000Z cache-local receipt).
set -e
NAME=$1 BT=$2 LV=$3 DICT=$4
R=core/target/issue420/bundle/20261022T194136Z
B=core/experiment/issue420-bundle
DB=pubwl_$NAME
[ -e $R/pub/wl-$NAME.json ] && { echo "receipt exists: $NAME"; exit 1; }
docker exec lfs420-bundle-pg psql -U postgres -qAtc "CREATE DATABASE $DB TEMPLATE bb STRATEGY FILE_COPY"
docker run --rm -v lfs420-bundle-sq:/sq rust:1.85.1-bookworm timeout 115 sh -c "rm -f /sq/wl.sqlite; cat /sq/replica.sqlite > /sq/wl.sqlite; sync"
echo "pubwl $NAME start $(date -u +%FT%TZ) $(sysctl -n vm.loadavg)" >> $R/host-load.txt
$B/run.sh bpub /work/core/target/issue420/capture/20261022T184243Z /work/core/target/issue420/fixture/store-r060.sqlite /sq/wl.sqlite "host=127.0.0.1 user=postgres dbname=$DB" $BT fit $LV $DICT /work/$R/pub/wl-$NAME.json
echo "pubwl $NAME end $(date -u +%FT%TZ) $(sysctl -n vm.loadavg)" >> $R/host-load.txt
