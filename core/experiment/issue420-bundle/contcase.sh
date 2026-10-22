#!/bin/sh
# contcase.sh ARM WRITERS M: one contended sample on a fresh clone of bb.
# ARM remote-bundle: the remote publish only (packs + bundle before the
#   lock, then the lane), no local Save (diagnostic, as the earlier
#   remote-cache1rt-pre); ARM bundle: each writer's local Save on its own
#   copy of the base a master (C12-sized; base b caches NOT_RUN: memory),
#   then encode + zstd + the same remote publish.
# Fresh PG clone (TEMPLATE FILE_COPY) + sync first, local copies last + sync.
set -e
ARM=$1 W=$2 M=$3
R=core/target/issue420/bundle/20261022T194136Z
B=core/experiment/issue420-bundle
NAME=b-$ARM-w$W; DB=$(echo c_$NAME | tr - _)
OUT=/work/$R/contend/$NAME.json
[ -e $R/contend/$NAME.json ] && { echo "receipt exists: $NAME"; exit 1; }
docker exec lfs420-bundle-pg psql -U postgres -qAtc "CREATE DATABASE $DB TEMPLATE bb STRATEGY FILE_COPY"
docker exec lfs420-bundle-pg sync
SQ=-
if [ $ARM = bundle ]; then
  docker run --rm -v lfs420-sqlite:/src:ro -v lfs420-bundle-sq:/sq rust:1.85.1-bookworm timeout 115 sh -c "
    set -e; mkdir -p /sq/contend/$NAME; i=0; while [ \$i -lt $W ]; do mkdir /sq/contend/$NAME/w\$i; cat /src/a-master.sqlite > /sq/contend/$NAME/w\$i/store.sqlite; i=\$((i+1)); done; sync"
  SQ=/sq/contend/$NAME
fi
echo "$NAME start $(date -u +%FT%TZ) $(sysctl -n vm.loadavg)" >> $R/host-load.txt
DOCKER_ENV="-e BTABLE=cb_colref_z3 -e PLAYOUT=fit -e BLEVEL=3" $B/run.sh contend $ARM $W $M /work/core/target/issue420/capture/20261022T184243Z /work/core/target/issue420/fixture/store-r060.sqlite $SQ "host=127.0.0.1 user=postgres dbname=$DB" $OUT > /dev/null
echo "$NAME end $(date -u +%FT%TZ) $(sysctl -n vm.loadavg)" >> $R/host-load.txt
python3 - $R/contend/$NAME.json <<'PY'
import json,sys
d=json.load(open(sys.argv[1])); c=d['correctness']
print(d['arm'],d['writers'],'x',d['per_writer'],'wall',d['wall_ms'],'tput',d['throughput_per_s'],'lat',d['latency_ms'],'wait',d['lane_wait_ms'],'hold',d['lane_hold_ms'],'st',d['statements_per_publish'],'rt',d['round_trips_per_publish'])
print(' errors',c['errors'],c['error_samples'],'dup',c['duplicates'],'chain',c['chain_length'],'linear',c['linear'],'eq',c['chain_equals_acked'],'parents',c['parents_match_lane_head'],'growth',c['growth_ok'],c['store_check'],[(l['integrity_check'],l['object_location_growth']==l['expected']) for l in d.get('local_caches',[])])
PY
