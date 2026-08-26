#!/usr/bin/env bash
set -euo pipefail

image="layerfs-fuse:frozen-1e33b7188a45"
volume="layerfs_stage2_final010_fault_store"
common=(--platform linux/arm64 --init --cpus 1 --memory 3g --pids-limit 512
  --device /dev/fuse:rwm --cap-add SYS_ADMIN --network none
  --tmpfs /tmp:rw,nosuid,nodev,size=1g,mode=1777
  -v "$volume:/var/lib/layerfs" "$image"
  --store /var/lib/layerfs/store.sqlite --mount /workspace
  --spool /var/tmp/layerfs-owned/spool --ref main --integrity trusted --uid 0 --gid 0)

wait_mount() {
  local name="$1"
  for _ in $(seq 1 20); do
    docker exec "$name" findmnt -rn -T /workspace >/dev/null && return
    sleep 1
  done
  return 1
}

docker volume create "$volume" >/dev/null
docker run -d --name layerfs-stage2-final010-fault-pre "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount layerfs-stage2-final010-fault-pre
docker exec layerfs-stage2-final010-fault-pre python3 -c \
  'f=open("/workspace/preack.bin","wb",buffering=0); f.write(b"unacknowledged-dirty-bytes"); f.close()'
docker exec layerfs-stage2-final010-fault-pre test -f /var/tmp/layerfs-owned/spool
docker kill --signal KILL layerfs-stage2-final010-fault-pre >/dev/null
test "$(docker wait layerfs-stage2-final010-fault-pre)" = 137
docker rm layerfs-stage2-final010-fault-pre >/dev/null

docker run -d --name layerfs-stage2-final010-fault-pre-reopen "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/pre-reopen-terminal.json >/dev/null
wait_mount layerfs-stage2-final010-fault-pre-reopen
docker exec layerfs-stage2-final010-fault-pre-reopen python3 -c \
  'import os; assert not os.path.exists("/workspace/preack.bin"); assert not os.path.exists("/var/tmp/layerfs-owned/spool")'
docker kill --signal TERM layerfs-stage2-final010-fault-pre-reopen >/dev/null
test "$(docker wait layerfs-stage2-final010-fault-pre-reopen)" = 0
docker rm layerfs-stage2-final010-fault-pre-reopen >/dev/null

docker run -d --name layerfs-stage2-final010-fault-post "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount layerfs-stage2-final010-fault-post
docker exec layerfs-stage2-final010-fault-post python3 -c \
  'import os; f=os.open("/workspace/postack.bin",os.O_CREAT|os.O_EXCL|os.O_RDWR,0o600); os.write(f,b"acknowledged-durable-bytes"); os.fsync(f); os.close(f)'
docker exec layerfs-stage2-final010-fault-post test ! -e /var/tmp/layerfs-owned/spool
docker kill --signal KILL layerfs-stage2-final010-fault-post >/dev/null
test "$(docker wait layerfs-stage2-final010-fault-post)" = 137
docker rm layerfs-stage2-final010-fault-post >/dev/null

docker run -d --name layerfs-stage2-final010-fault-post-reopen "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/post-reopen-terminal.json >/dev/null
wait_mount layerfs-stage2-final010-fault-post-reopen
docker exec layerfs-stage2-final010-fault-post-reopen python3 -c \
  'import os; assert open("/workspace/postack.bin","rb").read()==b"acknowledged-durable-bytes"; assert not os.path.exists("/workspace/preack.bin"); assert not os.path.exists("/var/tmp/layerfs-owned/spool")'
docker kill --signal TERM layerfs-stage2-final010-fault-post-reopen >/dev/null
test "$(docker wait layerfs-stage2-final010-fault-post-reopen)" = 0
docker rm layerfs-stage2-final010-fault-post-reopen >/dev/null
docker volume rm "$volume" >/dev/null
