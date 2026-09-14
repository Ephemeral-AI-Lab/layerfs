#!/bin/sh
# Run one v1-unmeasured probe and guarantee the container is removed.
# Usage: ./run_probe.sh <label> <source.c> [probe argv ...] [TIMEOUT=<sec>]
#
# Detached run + bounded wait + log capture, because a probe that blocks in an
# uninterruptible kernel wait cannot run a signal handler; the only recovery is
# SIGKILL of the process, which `docker rm -f` (or the containerd-shim kill
# below) performs. Partial logs are always captured, even on timeout.
set -u
LABEL="$1"; SRC="$2"; shift 2
TIMEOUT="${TIMEOUT:-180}"
NAME="v1un-$LABEL"
DIR=$(cd "$(dirname "$0")" && pwd)
IMAGE=rust:1.85.1-bookworm
ARGV="$*"
IMGID=$(docker image inspect "$IMAGE" --format '{{.Id}}')

docker rm -f "$NAME" >/dev/null 2>&1
START=$(date +%s)
docker run -d --name "$NAME" \
  --device /dev/fuse --cap-add SYS_ADMIN \
  --cpus 2 --memory 1g --pids-limit 256 \
  -v "$DIR":/probe -w /probe "$IMAGE" sh -c "
    ulimit -c 0
    uname -a
    gcc --version | head -1
    gcc -std=c11 -O2 -pthread -Wall -Wextra -o /tmp/v1un-bin $SRC
    sha256sum /tmp/v1un-bin
    /tmp/v1un-bin $ARGV
    echo \"EXIT=\$?\"
  " >/dev/null

TIMEDOUT=no
i=0
while [ "$i" -lt "$TIMEOUT" ]; do
  if ! docker ps --format '{{.Names}}' | grep -qx "$NAME"; then break; fi
  sleep 1
  i=$((i + 1))
done
if docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
  TIMEDOUT=yes
  # Record where it is stuck before killing anything.
  docker exec "$NAME" sh -c '
    echo "=== STUCK: task list at timeout ==="
    ps -eo pid,stat,wchan:24,args 2>/dev/null | head -20
    for t in /proc/[0-9]*/task/*; do
      echo "--- $t $(cat $t/comm 2>/dev/null) state=$(awk "/^State:/{print \$2}" $t/status 2>/dev/null) wchan=$(cat $t/wchan 2>/dev/null)"
    done
    echo "=== STUCK: kernel stacks of the probe ==="
    cat /proc/*/task/*/stack 2>/dev/null | head -40
    echo "=== STUCK: mounts mentioning the probe ==="
    grep -E "v1un" /proc/mounts
  ' >>"$DIR/run-$LABEL.log" 2>&1
fi

docker logs "$NAME" >"$DIR/run-$LABEL.log" 2>&1
EXITCODE=$(docker inspect -f '{{.State.ExitCode}}' "$NAME" 2>/dev/null || echo "?")
if [ "$TIMEDOUT" = yes ]; then
  printf '\n=== PROBE TIMED OUT after %ss: killed with docker rm -f (SIGKILL) ===\n' "$TIMEOUT" >>"$DIR/run-$LABEL.log"
fi

STUCK=no
docker rm -f "$NAME" >/dev/null 2>&1
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
END=$(date +%s)

cat >"$DIR/run-$LABEL.json" <<EOF
{
  "label": "$LABEL",
  "source": "$SRC",
  "argv": "$ARGV",
  "image": "$IMAGE",
  "image_id": "$IMGID",
  "device": "/dev/fuse",
  "cap_add": ["SYS_ADMIN"],
  "runtime_limits": {"cpus": 2, "memory": "1g", "pids": 256},
  "wall_seconds": $((END - START)),
  "probe_timeout_seconds": $TIMEOUT,
  "probe_timed_out": "$TIMEDOUT",
  "exit_code": "$EXITCODE",
  "container_had_to_be_recovered_by_shim_kill": "$STUCK",
  "containers_named_${NAME}_remaining": ${LEFT:-0},
  "command": "docker run --rm --name $NAME --device /dev/fuse --cap-add SYS_ADMIN --cpus 2 --memory 1g --pids-limit 256 -v $DIR:/probe -w /probe $IMAGE sh -c 'gcc -std=c11 -O2 -pthread -o /tmp/v1un-bin $SRC && /tmp/v1un-bin $ARGV'"
}
EOF
echo "label=$LABEL exit=$EXITCODE timedout=$TIMEDOUT stuck=$STUCK remaining=${LEFT:-0} wall=$((END - START))s"
tail -8 "$DIR/run-$LABEL.log"
