#!/usr/bin/env bash
set -euo pipefail

image="layerfs-fuse:frozen-c56ff37"
volume="layerfs_stage2_final011_splice_store"
evidence="$(cd "$(dirname "$0")" && pwd)"
names=(
  layerfs-stage2-final011-splice-baseline
  layerfs-stage2-final011-splice-control
  layerfs-stage2-final011-splice-remount
)
common=(--platform linux/arm64 --init --cpuset-cpus 0 --memory 3g --pids-limit 512
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
for output in splice-receipt.json splice-baseline-terminal.json \
  splice-control-terminal.json splice-remount-terminal.json splice-oracle.json; do
  if [[ -e "$evidence/$output" ]]; then
    echo "output already exists: $output" >&2
    exit 1
  fi
done

docker volume create "$volume" >/dev/null
docker run -d --name "${names[0]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount "${names[0]}"
docker exec "${names[0]}" python3 -c \
  'import os; f=os.open("/workspace/file",os.O_CREAT|os.O_EXCL|os.O_RDWR,0o600); assert os.write(f,b"abcdefghij")==10; os.fsync(f); os.close(f); assert open("/workspace/file","rb").read()==b"abcdefghij"'
docker kill --signal TERM "${names[0]}" >/dev/null
test "$(docker wait "${names[0]}")" = 0
docker cp "${names[0]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/splice-baseline-terminal.json"
docker rm "${names[0]}" >/dev/null

docker run -d --name "${names[1]}" --platform linux/arm64 --init \
  --cpuset-cpus 0 --memory 3g --pids-limit 512 \
  --device /dev/fuse:rwm --cap-add SYS_ADMIN --network none \
  --tmpfs /tmp:rw,nosuid,nodev,size=1g,mode=1777 \
  -v "$volume:/var/lib/layerfs" -v "$evidence:/control" "$image" \
  --store /var/lib/layerfs/store.sqlite --mount /workspace \
  --spool /var/tmp/layerfs-owned/spool --ref main --integrity trusted --uid 0 --gid 0 \
  --receipt /var/tmp/layerfs-owned/terminal.json \
  --control-request /control/splice-request.txt \
  --control-receipt /control/splice-receipt.json >/dev/null
wait_mount "${names[1]}"
docker exec "${names[1]}" python3 -c \
  'assert open("/workspace/file","rb").read()==b"abcdefghij"; assert open("/workspace/file","rb").read()==b"abcdefghij"'
docker kill --signal HUP "${names[1]}" >/dev/null
test "$(docker wait "${names[1]}")" = 0
docker cp "${names[1]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/splice-control-terminal.json"
docker rm "${names[1]}" >/dev/null

docker run -d --name "${names[2]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json >/dev/null
wait_mount "${names[2]}"
docker exec "${names[2]}" python3 -c \
  'expected=b"abcXYZfghij"; assert open("/workspace/file","rb").read()==expected; assert open("/workspace/file","rb").read()==expected'
docker kill --signal TERM "${names[2]}" >/dev/null
test "$(docker wait "${names[2]}")" = 0
docker cp "${names[2]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/splice-remount-terminal.json"
docker rm "${names[2]}" >/dev/null
docker volume rm "$volume" >/dev/null

EVIDENCE="$evidence" IMAGE_ID="$(docker image inspect "$image" --format '{{.Id}}')" \
python3 -c 'import json,os; p=os.environ["EVIDENCE"]; splice=json.load(open(os.path.join(p,"splice-receipt.json"))); control=json.load(open(os.path.join(p,"splice-control-terminal.json"))); remount=json.load(open(os.path.join(p,"splice-remount-terminal.json"))); checks={"typed_remount":splice["status"]=="PASS" and splice["remount_required"] is True,"exact_insert_locality":splice["insert_bytes"]==3 and splice["locality"]["cdc_bytes_scanned"]==3 and splice["locality"]["content_payload_bytes_written"]==3,"zero_content_reads":splice["locality"]["content_payload_bytes_read"]==0,"control_root_only":control["mounted"]["lookup_refs"]==control["mounted"]["live_nodes"]==control["mounted"]["inode_mappings"]==1,"clean_remount":remount["status"]=="PASS" and remount["mounted"]["lookup_refs"]==remount["mounted"]["live_nodes"]==remount["mounted"]["inode_mappings"]==1}; receipt={"schema":"layerfs-stage2-mounted-splice-v1","status":"PASS" if all(checks.values()) else "FAIL","image_id":os.environ["IMAGE_ID"],"checks":checks}; out=os.path.join(p,"splice-oracle.json"); json.dump(receipt,open(out,"x"),indent=2,sort_keys=True); open(out,"a").write("\n"); assert receipt["status"]=="PASS"'
