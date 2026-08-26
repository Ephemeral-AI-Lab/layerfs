#!/usr/bin/env bash
set -euo pipefail

image="layerfs-fuse:frozen-c56ff37"
volume="layerfs_stage2_final011_fault_store"
evidence="$(cd "$(dirname "$0")" && pwd)"
names=(
  layerfs-stage2-final011-fault-pre
  layerfs-stage2-final011-fault-pre-reopen
  layerfs-stage2-final011-fault-post
  layerfs-stage2-final011-fault-post-reopen
)
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

for name in "${names[@]}"; do
  if docker container inspect "$name" >/dev/null 2>&1; then
    echo "container already exists: $name" >&2
    exit 1
  fi
done
if docker volume inspect "$volume" >/dev/null 2>&1; then
  echo "volume already exists: $volume" >&2
  exit 1
fi

docker volume create "$volume" >/dev/null
docker run -d --name "${names[0]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount "${names[0]}"
docker exec "${names[0]}" python3 -c \
  'f=open("/workspace/preack.bin","wb",buffering=0); f.write(b"unacknowledged-dirty-bytes"); f.close()'
docker exec "${names[0]}" test -f /var/tmp/layerfs-owned/spool
docker kill --signal KILL "${names[0]}" >/dev/null
test "$(docker wait "${names[0]}")" = 137
if docker cp "${names[0]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/forced-preack-unexpected-terminal.json" >/dev/null 2>&1; then
  echo "SIGKILL unexpectedly produced a terminal receipt" >&2
  exit 1
fi
docker rm "${names[0]}" >/dev/null

docker run -d --name "${names[1]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount "${names[1]}"
docker exec "${names[1]}" python3 -c \
  'import os; assert not os.path.exists("/workspace/preack.bin"); assert not os.path.exists("/var/tmp/layerfs-owned/spool")'
docker kill --signal TERM "${names[1]}" >/dev/null
test "$(docker wait "${names[1]}")" = 0
docker cp "${names[1]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/forced-preack-reopen-terminal.json"
docker rm "${names[1]}" >/dev/null

docker run -d --name "${names[2]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount "${names[2]}"
docker exec "${names[2]}" python3 -c \
  'import os; f=os.open("/workspace/postack.bin",os.O_CREAT|os.O_EXCL|os.O_RDWR,0o600); os.write(f,b"acknowledged-durable-bytes"); os.fsync(f); os.close(f)'
docker exec "${names[2]}" test ! -e /var/tmp/layerfs-owned/spool
docker kill --signal KILL "${names[2]}" >/dev/null
test "$(docker wait "${names[2]}")" = 137
if docker cp "${names[2]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/forced-postack-unexpected-terminal.json" >/dev/null 2>&1; then
  echo "SIGKILL unexpectedly produced a terminal receipt" >&2
  exit 1
fi
docker rm "${names[2]}" >/dev/null

docker run -d --name "${names[3]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount "${names[3]}"
docker exec "${names[3]}" python3 -c \
  'import os; assert open("/workspace/postack.bin","rb").read()==b"acknowledged-durable-bytes"; assert not os.path.exists("/workspace/preack.bin"); assert not os.path.exists("/var/tmp/layerfs-owned/spool")'
docker kill --signal TERM "${names[3]}" >/dev/null
test "$(docker wait "${names[3]}")" = 0
docker cp "${names[3]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/forced-postack-reopen-terminal.json"
docker rm "${names[3]}" >/dev/null
docker volume rm "$volume" >/dev/null

EVIDENCE="$evidence" IMAGE_ID="$(docker image inspect "$image" --format '{{.Id}}')" \
python3 -c 'import json,os; path=os.path.join(os.environ["EVIDENCE"],"forced-death-oracle.json"); json.dump({"schema":"layerfs-stage2-forced-death-v1","status":"PASS","image_id":os.environ["IMAGE_ID"],"checks":{"dirty_preack_discarded":True,"durable_postack_reopened":True,"preack_sigkill_exit_137":True,"postack_sigkill_exit_137":True,"sigkill_terminal_absent":True,"spool_absent_after_each_reopen":True}},open(path,"x"),indent=2,sort_keys=True); open(path,"a").write("\n")'
