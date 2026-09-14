#!/bin/sh
# Run the follow-up probe (T7) and guarantee the container is removed.
# Usage: ./run_followup.sh <label> [writeback]
set -u
LABEL="fu-${1}"
MODE="${2:-writethrough}"
NAME="v1gen-probe-$LABEL"
DIR=/tmp/v1-generic-probe
IMAGE=rust:1.85.1-bookworm
ARG=""
[ "$MODE" = "writeback" ] && ARG="writeback"

docker rm -f "$NAME" >/dev/null 2>&1
START=$(date +%s)
docker run --rm --name "$NAME" \
  --device /dev/fuse --cap-add SYS_ADMIN \
  --cpus 2 --memory 1g --pids-limit 256 \
  -v "$DIR":/probe -w /probe "$IMAGE" sh -c "
    ulimit -c 0
    uname -a
    gcc --version | head -1
    gcc -std=c11 -O2 -pthread -Wall -Wextra -o /tmp/v1gen-followup followup.c
    sha256sum /tmp/v1gen-followup
    /tmp/v1gen-followup $ARG
    echo \"EXIT=\$?\"
  " >"$DIR/run-$LABEL.log" 2>&1
RC=$?
END=$(date +%s)
STUCK=no
if docker ps -a --format '{{.Names}}' | grep -qx "$NAME"; then
  STUCK=yes
  CID=$(docker inspect -f '{{.Id}}' "$NAME" 2>/dev/null)
  docker run --rm --privileged --pid=host alpine:3.20 sh -c "
    ps -eo pid,args | grep containerd-shim | grep -v grep | grep '$CID' | awk '{print \$1}' | xargs -r kill -9
    sleep 2
  " >>"$DIR/run-$LABEL.log" 2>&1
  sleep 2
  docker rm -f "$NAME" >>"$DIR/run-$LABEL.log" 2>&1 || true
fi
LEFT=$(docker ps -a --format '{{.Names}}' | grep -cx "$NAME" || true)
echo "label=$LABEL rc=$RC stuck=$STUCK remaining=${LEFT:-0} wall=$((END - START))s"
tail -4 "$DIR/run-$LABEL.log"
