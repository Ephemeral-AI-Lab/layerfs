#!/bin/sh
# usage: run_case.sh <arm pg|pg10> <group history|writes> <outdir>
set -u
ARM=$1; GROUP=$2; OUT=$3
WT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs/.claude/worktrees/rt818-rollout-c3dbae
BIN=$WT/target/debug/deps/fs_call_round_trips-9eab8e660a198019
NAME=$ARM-$GROUP
CAP=lfs818-diag-cap
docker rm -f $CAP >/dev/null 2>&1
rm -f "$OUT/$NAME.pcap"
docker run -d --name $CAP --net container:lfs818-bench-pg --cap-add NET_ADMIN --cap-add NET_RAW \
  alpine:3 sh -c "apk add --no-cache -q tcpdump >/dev/null 2>&1 && exec timeout 110 tcpdump -i eth0 -B 262144 -s 0 -w /tmp/$NAME.pcap tcp port 5432" >/dev/null
i=0
while ! docker logs $CAP 2>&1 | grep -q listening && [ $i -lt 60 ]; do sleep 0.5; i=$((i+1)); done
docker logs $CAP 2>&1 | grep -q listening || { echo "capture not ready"; docker logs $CAP; docker rm -f $CAP; exit 1; }
sleep 1
echo "=== $ARM fs_calls $GROUP $(date -u +%Y-%m-%dT%H:%M:%SZ) load $(sysctl -n vm.loadavg) bin $(basename $BIN) sha256 $(shasum -a 256 $BIN | cut -c1-16) src $(git -C $WT rev-parse --short=10 HEAD) capture $NAME.pcap"
cd $WT
env FS818_ARM=$ARM FS818_GROUP=$GROUP FS818_NETEM_CONTAINER=lfs818-bench-pg \
  LAYERFS_TEST_POSTGRES_URL="host=127.0.0.1 port=61949 user=postgres dbname=rt818_bench sslmode=disable" \
  /usr/bin/time -l $BIN fs_calls --exact --test-threads=1 --nocapture 2>&1
echo "exit $?"
sleep 1
docker exec $CAP pkill -INT tcpdump; sleep 1
docker logs $CAP 2>&1 | tail -3
docker cp $CAP:/tmp/$NAME.pcap "$OUT/$NAME.pcap" >/dev/null
docker stop -t 2 $CAP >/dev/null 2>&1
docker rm -f $CAP >/dev/null 2>&1
ls -la "$OUT/$NAME.pcap"
