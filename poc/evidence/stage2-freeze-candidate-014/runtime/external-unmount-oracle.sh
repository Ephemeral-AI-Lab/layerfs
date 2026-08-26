#!/usr/bin/env bash
set -euo pipefail

image="layerfs-fuse:frozen-292be84"
source_commit="292be840c31052d85ab6e9441706298af3cd3d15"
source_tree="e3055bcd7a41921879fa149c11918891517e4522"
volume="layerfs_stage2_final014_external_unmount_store"
evidence="$(cd "$(dirname "$0")/.." && pwd)/external-unmount"
names=(
  layerfs-stage2-final014-external-init
  layerfs-stage2-final014-external-dirty
  layerfs-stage2-final014-external-reopen
)
common=(--platform linux/arm64 --init --stop-timeout 30 --cpus 1 --memory 3g
  --pids-limit 512 --device /dev/fuse:rwm --cap-add SYS_ADMIN --network none
  --tmpfs /tmp:rw,nosuid,nodev,size=1g,mode=1777
  -v "$volume:/var/lib/layerfs" "$image"
  --store /var/lib/layerfs/store.sqlite --mount /workspace
  --spool /var/tmp/layerfs-owned/spool --ref main --integrity trusted --uid 0 --gid 0)

wait_mount() {
  local name="$1"
  for _ in $(seq 1 100); do
    docker exec "$name" mountpoint -q /workspace && return
    sleep 0.05
  done
  return 1
}

[[ ! -e "$evidence" ]]
mkdir "$evidence"
for name in "${names[@]}"; do
  ! docker container inspect "$name" >/dev/null 2>&1
done
! docker volume inspect "$volume" >/dev/null 2>&1

docker volume create "$volume" > "$evidence/volume-create.stdout"
docker run -d --name "${names[0]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json \
  > "$evidence/init-run.stdout" 2> "$evidence/init-run.stderr"
wait_mount "${names[0]}"
docker kill --signal TERM "${names[0]}" > "$evidence/init-kill.stdout"
docker wait "${names[0]}" > "$evidence/init-exit.txt"
docker cp "${names[0]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/init-terminal.json"
docker rm "${names[0]}" > "$evidence/init-rm.stdout"

docker run -d --name "${names[1]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json \
  > "$evidence/dirty-run.stdout" 2> "$evidence/dirty-run.stderr"
wait_mount "${names[1]}"
docker inspect "${names[1]}" > "$evidence/dirty-docker-inspect.json"
docker exec "${names[1]}" cat /proc/1/mountinfo > "$evidence/dirty-mountinfo.txt"
docker exec "${names[1]}" python3 -c '
import hashlib
payload = bytes(range(256)) * 4096 + b"external-unmount"
with open("/workspace/dirty.bin", "wb", buffering=0) as target:
    for offset in range(0, len(payload), 64 * 1024):
        target.write(payload[offset:offset + 64 * 1024])
with open("/var/tmp/layerfs-owned/expected.sha256", "x") as output:
    output.write(hashlib.sha256(payload).hexdigest() + "\n")
' > "$evidence/write.stdout" 2> "$evidence/write.stderr"
docker exec "${names[1]}" test -s /var/tmp/layerfs-owned/spool
docker exec "${names[1]}" umount /workspace \
  > "$evidence/external-umount.stdout" 2> "$evidence/external-umount.stderr"
docker wait "${names[1]}" > "$evidence/dirty-exit.txt"
docker logs "${names[1]}" > "$evidence/dirty-daemon.stdout" \
  2> "$evidence/dirty-daemon.stderr"
docker cp "${names[1]}:/var/tmp/layerfs-owned/expected.sha256" \
  "$evidence/expected.sha256"
docker cp "${names[1]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/dirty-terminal.json"
docker inspect "${names[1]}" > "$evidence/dirty-stopped-inspect.json"
docker rm "${names[1]}" > "$evidence/dirty-rm.stdout"

docker run -d --name "${names[2]}" "${common[@]}" \
  --receipt /var/tmp/layerfs-owned/terminal.json \
  > "$evidence/reopen-run.stdout" 2> "$evidence/reopen-run.stderr"
wait_mount "${names[2]}"
docker exec "${names[2]}" sha256sum /workspace/dirty.bin \
  > "$evidence/reopen.sha256"
docker exec "${names[2]}" test ! -e /var/tmp/layerfs-owned/spool
docker kill --signal TERM "${names[2]}" > "$evidence/reopen-kill.stdout"
docker wait "${names[2]}" > "$evidence/reopen-exit.txt"
docker cp "${names[2]}:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/reopen-terminal.json"
docker rm "${names[2]}" > "$evidence/reopen-rm.stdout"
docker volume rm "$volume" > "$evidence/volume-rm.stdout"

python3 - "$evidence" "$source_commit" "$source_tree" <<'PY'
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
source_commit, source_tree = sys.argv[2:]
terminal = json.loads((root / "dirty-terminal.json").read_text())
reopen = json.loads((root / "reopen-terminal.json").read_text())
expected = (root / "expected.sha256").read_text().split()[0]
actual = (root / "reopen.sha256").read_text().split()[0]
mounted = terminal["mounted"]
engine = terminal["engine"]
checks = {
    "external_signal_zero": terminal["signal"] == 0,
    "one_dirty_checkpoint": mounted["checkpoints"] == 1,
    "one_dirty_transaction": engine["transactions_started"] == 1
    and engine["transactions_committed"] == 1
    and engine["transactions_rolled_back"] == 0
    and engine["publication_commits"] == 1,
    "terminal_pass": terminal["status"] == "PASS",
    "source_identity_exact": terminal["source_commit"] == source_commit
    and terminal["source_tree"] == source_tree,
    "terminal_resources_zero": mounted["operation_q_terminal_bytes"] == 0
    and mounted["spool_live_bytes"] == 0
    and mounted["spool_dead_bytes"] == 0
    and mounted["spool_physical_bytes"] == 0
    and engine["connections_terminal"] == 0,
    "root_only_terminal": mounted["lookup_refs"]
    == mounted["live_nodes"]
    == mounted["inode_mappings"]
    == 1,
    "independent_reopen_exact": actual == expected,
    "reopen_pass": reopen["status"] == "PASS",
    "owned_runtime_removed": (root / "volume-rm.stdout").read_text().strip()
    == "layerfs_stage2_final014_external_unmount_store",
}
receipt = {
    "schema": "layerfs-stage2-external-unmount-v1",
    "status": "PASS" if all(checks.values()) else "FAIL",
    "checks": checks,
    "expected_sha256": expected,
    "reopened_sha256": actual,
}
with (root / "oracle.json").open("x") as output:
    json.dump(receipt, output, indent=2, sort_keys=True)
    output.write("\n")
if receipt["status"] != "PASS":
    raise SystemExit(1)
PY
