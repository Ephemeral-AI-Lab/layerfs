#!/bin/sh
# Run one probe mode and guarantee the container is removed.
# Usage: ./run_probe.sh <label> [writeback]
# Logs go to run-<label>.log / run-<label>.json.
set -u
LABEL="$1"
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
    grep -m1 'model name' /proc/cpuinfo 2>/dev/null | head -1
    gcc --version | head -1
    gcc -std=c11 -O2 -pthread -Wall -Wextra -o /tmp/v1gen-probe-bin probe.c
    sha256sum /tmp/v1gen-probe-bin
    /tmp/v1gen-probe-bin $ARG
    echo \"EXIT=\$?\"
  " >"$DIR/run-$LABEL.log" 2>&1
RC=$?
END=$(date +%s)

# Recovery: a probe that dies with dirty mappings and a live mount can leave an
# unkillable container. Kill its containerd shim from the Docker Desktop VM,
# which lets containerd reap it and removes it.
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

cat >"$DIR/run-$LABEL.json" <<EOF
{
  "label": "$LABEL",
  "mode": "$MODE",
  "argv1": "$ARG",
  "image": "$IMAGE",
  "image_id": "$(docker image inspect "$IMAGE" --format '{{.Id}}')",
  "kernel": "6.12.76-linuxkit",
  "device": "/dev/fuse",
  "cap_add": ["SYS_ADMIN"],
  "runtime_limits": {"cpus": 2, "memory": "1g", "pids": 256},
  "wall_seconds": $((END - START)),
  "docker_run_returncode": $RC,
  "container_had_to_be_recovered_by_shim_kill": "$STUCK",
  "containers_named_<name>_remaining": ${LEFT:-0},
  "command": "docker run --rm --name $NAME --device /dev/fuse --cap-add SYS_ADMIN --cpus 2 --memory 1g --pids-limit 256 -v $DIR:/probe -w /probe $IMAGE sh -c '<compile probe.c; run /tmp/v1gen-probe-bin $ARG>'"
}
EOF
echo "label=$LABEL rc=$RC stuck=$STUCK remaining=${LEFT:-0} wall=$((END - START))s"
tail -3 "$DIR/run-$LABEL.log"
