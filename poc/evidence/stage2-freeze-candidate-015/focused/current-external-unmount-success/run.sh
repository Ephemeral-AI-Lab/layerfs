#!/usr/bin/env bash
set -euo pipefail

repo="/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty"
evidence="$repo/poc/evidence/stage2-freeze-candidate-015/focused/current-external-unmount-success"
image="layerfs-fuse:frozen-7e82abc"
image_id="sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0"
source_commit="7e82abcd7320f6a214be336d82488ba0527b6025"
source_tree="df13d88eb7e7d2471971b0c58ca6425bb81b0b03"
volume="layerfs_stage2_c015_focus_external_success_store"
init_name="layerfs-c015-focus-external-init"
dirty_name="layerfs-c015-focus-external-dirty"
reopen_name="layerfs-c015-focus-external-reopen"
events="$evidence/events.jsonl"

common=(
  --platform linux/arm64
  --init
  --stop-timeout 30
  --cpus 1
  --memory 512m
  --pids-limit 512
  --device /dev/fuse:rwm
  --cap-add SYS_ADMIN
  --network none
  -v "$volume:/var/lib/layerfs"
  "$image"
  --store /var/lib/layerfs/store.sqlite
  --mount /workspace
  --spool /var/tmp/layerfs-owned/spool
  --receipt /var/tmp/layerfs-owned/terminal.json
  --ref main
  --integrity verified
  --uid 0
  --gid 0
)

event() {
  python3 - "$events" "$1" "${2:-}" <<'PY'
import datetime
import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
row = {
    "event": sys.argv[2],
    "detail": sys.argv[3] or None,
    "time_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "time_ns": __import__("time").time_ns(),
}
with path.open("a") as output:
    output.write(json.dumps(row, sort_keys=True) + "\n")
PY
}

wait_ready() {
  local name="$1"
  local output="$2"
  local attempt
  local started
  started="$(python3 -c 'import time; print(time.time_ns())')"
  for attempt in $(seq 1 200); do
    if docker exec "$name" mountpoint -q /workspace \
      && docker logs "$name" 2>/dev/null | grep -q '"backend":"layerfs-fuse"'; then
      python3 - "$output" "$name" "$attempt" "$started" <<'PY'
import datetime
import json
from pathlib import Path
import sys
import time

ended = time.time_ns()
Path(sys.argv[1]).write_text(json.dumps({
    "container": sys.argv[2],
    "attempts": int(sys.argv[3]),
    "started_ns": int(sys.argv[4]),
    "ended_ns": ended,
    "elapsed_ns": ended - int(sys.argv[4]),
    "ready_time_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
}, indent=2, sort_keys=True) + "\n")
PY
      return 0
    fi
    sleep 0.05
  done
  return 1
}

cleanup_on_error() {
  local status=$?
  if [[ $status -ne 0 ]]; then
    event "failure-trap" "exit=$status" || true
    docker rm -f "$init_name" "$dirty_name" "$reopen_name" >/dev/null 2>&1 || true
    docker volume rm "$volume" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup_on_error EXIT

cd "$repo"
for name in "$init_name" "$dirty_name" "$reopen_name"; do
  ! docker container inspect "$name" >/dev/null 2>&1
done
! docker volume inspect "$volume" >/dev/null 2>&1
test ! -e "$events"
event "run-start"

git status --short > "$evidence/git-status-before.txt"
git show -s --format='%H %T %P %s' "$source_commit" > "$evidence/product-commit.txt"
git diff --exit-code "$source_commit" -- \
  Cargo.toml Cargo.lock crates containers tools > "$evidence/product-diff.patch"
git rev-parse HEAD HEAD^{tree} > "$evidence/repository-head-and-tree.txt"
uname -a > "$evidence/host-uname.txt"
docker version > "$evidence/docker-version.txt"
docker info > "$evidence/docker-info.txt"
docker image inspect "$image" > "$evidence/image-inspect.json"
docker image inspect "$image_id" > "$evidence/image-id-inspect.json"
shasum -a 256 containers/layerfs-fuse/fs-bench.sh > "$evidence/fs-bench.sha256"
event "binding-captured"

docker volume create "$volume" > "$evidence/volume-create.stdout"
docker volume inspect "$volume" > "$evidence/volume-inspect.json"
event "volume-created" "$volume"

docker run -d --name "$init_name" "${common[@]}" \
  > "$evidence/init-run.stdout" 2> "$evidence/init-run.stderr"
wait_ready "$init_name" "$evidence/init-wait.json"
docker inspect "$init_name" > "$evidence/init-running-inspect.json"
docker exec "$init_name" cat /proc/1/mountinfo > "$evidence/init-mountinfo.txt"
docker exec "$init_name" uname -a > "$evidence/container-uname.txt"
docker exec "$init_name" sha256sum /usr/local/bin/layerfs-fuse \
  > "$evidence/executable.sha256"
event "init-ready"
docker kill --signal TERM "$init_name" > "$evidence/init-kill.stdout"
docker wait "$init_name" > "$evidence/init-exit.txt"
docker logs "$init_name" > "$evidence/init-daemon.stdout" \
  2> "$evidence/init-daemon.stderr"
docker cp "$init_name:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/init-terminal.json"
docker inspect "$init_name" > "$evidence/init-stopped-inspect.json"
docker rm "$init_name" > "$evidence/init-rm.stdout"
event "init-complete"

docker run -d --name "$dirty_name" "${common[@]}" \
  > "$evidence/dirty-run.stdout" 2> "$evidence/dirty-run.stderr"
wait_ready "$dirty_name" "$evidence/dirty-wait.json"
docker inspect "$dirty_name" > "$evidence/dirty-running-inspect.json"
docker exec "$dirty_name" cat /proc/1/mountinfo > "$evidence/dirty-mountinfo.txt"
docker exec "$dirty_name" sh -c \
  'grep " /workspace " /proc/1/mountinfo' > "$evidence/dirty-mountinfo-workspace.txt"
event "dirty-ready"
docker exec "$dirty_name" python3 -c '
import hashlib
import json
import os
payload = bytes(range(256)) * 4096 + b"current-source-external-unmount"
path = "/workspace/dirty.bin"
with open(path, "wb", buffering=0) as target:
    offset = 0
    while offset < len(payload):
        written = target.write(payload[offset:offset + 64 * 1024])
        if not written:
            raise RuntimeError("short write")
        offset += written
with open("/var/tmp/layerfs-owned/expected.json", "x") as output:
    json.dump({
        "path": path,
        "size": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
        "explicit_fsync": False,
    }, output, indent=2, sort_keys=True)
    output.write("\n")
' > "$evidence/write.stdout" 2> "$evidence/write.stderr"
docker exec "$dirty_name" test -s /var/tmp/layerfs-owned/spool
docker exec "$dirty_name" stat -c '%n %s %f %i' \
  /workspace/dirty.bin /var/tmp/layerfs-owned/spool \
  > "$evidence/dirty-pre-unmount-stat.txt"
docker exec "$dirty_name" cat /proc/1/status > "$evidence/dirty-pre-unmount-proc-status.txt"
docker cp "$dirty_name:/var/tmp/layerfs-owned/expected.json" \
  "$evidence/expected.json"
event "dirty-write-released-without-fsync"

event "external-unmount-start"
docker exec "$dirty_name" /usr/bin/umount /workspace \
  > "$evidence/external-umount.stdout" 2> "$evidence/external-umount.stderr"
event "external-unmount-returned"
docker wait "$dirty_name" > "$evidence/dirty-exit.txt"
docker logs "$dirty_name" > "$evidence/dirty-daemon.stdout" \
  2> "$evidence/dirty-daemon.stderr"
docker cp "$dirty_name:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/dirty-terminal.json"
docker inspect "$dirty_name" > "$evidence/dirty-stopped-inspect.json"
docker rm "$dirty_name" > "$evidence/dirty-rm.stdout"
event "dirty-external-unmount-complete"

docker run -d --name "$reopen_name" "${common[@]}" \
  > "$evidence/reopen-run.stdout" 2> "$evidence/reopen-run.stderr"
wait_ready "$reopen_name" "$evidence/reopen-wait.json"
docker inspect "$reopen_name" > "$evidence/reopen-running-inspect.json"
docker exec "$reopen_name" cat /proc/1/mountinfo > "$evidence/reopen-mountinfo.txt"
docker exec "$reopen_name" sha256sum /workspace/dirty.bin \
  > "$evidence/reopen.sha256"
docker exec "$reopen_name" stat -c '%n %s %f %i' /workspace/dirty.bin \
  > "$evidence/reopen-stat.txt"
docker exec "$reopen_name" test ! -e /var/tmp/layerfs-owned/spool
event "verified-independent-reopen-exact-read-complete"
docker exec "$reopen_name" python3 -c '
import os
os.unlink("/workspace/dirty.bin")
directory = os.open("/workspace", os.O_RDONLY | os.O_DIRECTORY)
try:
    os.fsync(directory)
finally:
    os.close(directory)
' > "$evidence/cleanup-command.stdout" 2> "$evidence/cleanup-command.stderr"
docker exec "$reopen_name" test ! -e /workspace/dirty.bin
event "accepted-file-cleanup-complete"
docker kill --signal TERM "$reopen_name" > "$evidence/reopen-kill.stdout"
docker wait "$reopen_name" > "$evidence/reopen-exit.txt"
docker logs "$reopen_name" > "$evidence/reopen-daemon.stdout" \
  2> "$evidence/reopen-daemon.stderr"
docker cp "$reopen_name:/var/tmp/layerfs-owned/terminal.json" \
  "$evidence/reopen-terminal.json"
docker inspect "$reopen_name" > "$evidence/reopen-stopped-inspect.json"
docker rm "$reopen_name" > "$evidence/reopen-rm.stdout"
event "reopen-cleanup-complete"

docker volume rm "$volume" > "$evidence/volume-rm.stdout"
event "owned-volume-removed" "$volume"

python3 - "$evidence" "$source_commit" "$source_tree" "$image_id" \
  "$volume" "$init_name" "$dirty_name" "$reopen_name" <<'PY'
import json
from pathlib import Path
import platform
import subprocess
import sys

root = Path(sys.argv[1])
source_commit, source_tree, image_id, volume, *names = sys.argv[2:]

def load(name):
    return json.loads((root / name).read_text())

def exit_code(name):
    return int((root / name).read_text().strip())

init = load("init-terminal.json")
dirty = load("dirty-terminal.json")
reopen = load("reopen-terminal.json")
expected = load("expected.json")
actual_sha = (root / "reopen.sha256").read_text().split()[0]
image = load("image-inspect.json")[0]
dirty_inspect = load("dirty-running-inspect.json")[0]
dirty_stopped = load("dirty-stopped-inspect.json")[0]

def terminal_clean(receipt):
    mounted = receipt["mounted"]
    engine = receipt["engine"]
    return (
        mounted["operation_q_terminal_bytes"] == 0
        and mounted["spool_live_bytes"] == 0
        and mounted["spool_dead_bytes"] == 0
        and mounted["spool_physical_bytes"] == 0
        and engine["connections_terminal"] == 0
    )

def root_only(receipt):
    mounted = receipt["mounted"]
    return (
        mounted["lookup_refs"]
        == mounted["live_nodes"]
        == mounted["inode_mappings"]
        == 1
    )

mount_line = (root / "dirty-mountinfo-workspace.txt").read_text().strip()
labels = image["Config"]["Labels"]
checks = {
    "product_source_exact": labels["org.opencontainers.image.layerfs.source-commit"]
        == source_commit
        and labels["org.opencontainers.image.layerfs.source-tree"] == source_tree,
    "image_id_exact": image["Id"] == image_id,
    "native_arm64": image["Architecture"] == "arm64"
        and "aarch64" in (root / "container-uname.txt").read_text()
        and platform.machine() == "arm64",
    "real_fuse_mount": " - fuse layerfs " in mount_line
        and " /workspace " in mount_line,
    "exact_cpu_memory_network_envelope": dirty_inspect["HostConfig"]["NanoCpus"]
        == 1_000_000_000
        and dirty_inspect["HostConfig"]["Memory"] == 512 * 1024 * 1024
        and dirty_inspect["HostConfig"]["NetworkMode"] == "none",
    "fuse_device_and_capability": any(
        device.get("PathOnHost") == "/dev/fuse"
        for device in dirty_inspect["HostConfig"]["Devices"]
    ) and "CAP_SYS_ADMIN" in dirty_inspect["HostConfig"]["CapAdd"],
    "no_tmpfs": dirty_inspect["HostConfig"].get("Tmpfs") in ({}, None),
    "external_unmount_exit_zero": exit_code("dirty-exit.txt") == 0
        and (root / "external-umount.stderr").read_text() == "",
    "external_unmount_not_signal": dirty["signal"] == 0,
    "session_destroyed_once": dirty["session_terminated"]
        and dirty["terminal_snapshot_complete"]
        and dirty["callbacks"]["init"] == 1
        and dirty["callbacks"]["destroy"] == 1,
    "write_had_no_fsync_callback": not expected["explicit_fsync"]
        and dirty["callbacks"]["write"] > 0
        and dirty["callbacks"]["fsync"] == 0
        and dirty["callbacks"]["fsyncdir"] == 0,
    "dirty_spool_was_live": dirty["mounted"]["spool_live_high_water_bytes"]
        == expected["size"]
        and dirty["mounted"]["dirty_ranges_high_water"] > 0,
    "exactly_one_dirty_checkpoint": dirty["mounted"]["checkpoints"] == 1
        and dirty["mounted"]["no_op_checkpoints"] == 0,
    "exactly_one_dirty_publication": dirty["engine"]["transactions_started"] == 1
        and dirty["engine"]["transactions_committed"] == 1
        and dirty["engine"]["transactions_rolled_back"] == 0
        and dirty["engine"]["publication_commits"] == 1,
    "dirty_terminal_pass": dirty["status"] == "PASS"
        and dirty["error"] is None
        and dirty["kernel_cache_released"],
    "dirty_terminal_resources_zero": terminal_clean(dirty),
    "dirty_terminal_root_only": root_only(dirty),
    "dirty_logical_state_retained": dirty["mounted"]["logical_workspace_bytes"]
        == expected["size"],
    "verified_independent_reopen_exact": reopen["integrity"] == "Verified"
        and actual_sha == expected["sha256"],
    "generation_advanced_once": dirty["generation"] == init["generation"] + 1,
    "cleanup_checkpoint_exact": reopen["mounted"]["checkpoints"] == 1
        and reopen["engine"]["transactions_started"] == 1
        and reopen["engine"]["transactions_committed"] == 1
        and reopen["engine"]["transactions_rolled_back"] == 0
        and reopen["engine"]["publication_commits"] == 1,
    "cleanup_publication_advanced": reopen["generation"]
        == dirty["generation"] + 1
        and reopen["root"] != dirty["root"],
    "cleanup_terminal_pass": reopen["status"] == "PASS"
        and reopen["session_terminated"]
        and reopen["terminal_snapshot_complete"]
        and reopen["error"] is None,
    "cleanup_terminal_resources_zero": terminal_clean(reopen)
        and reopen["mounted"]["logical_workspace_bytes"] == 0,
    "cleanup_terminal_root_only": root_only(reopen),
    "all_container_exits_zero": all(
        exit_code(name) == 0
        for name in ("init-exit.txt", "dirty-exit.txt", "reopen-exit.txt")
    ) and dirty_stopped["State"]["ExitCode"] == 0,
    "all_terminal_sources_exact": all(
        receipt["source_commit"] == source_commit
        and receipt["source_tree"] == source_tree
        and receipt["integrity"] == "Verified"
        for receipt in (init, dirty, reopen)
    ),
}

container_results = {}
for name in names:
    result = subprocess.run(
        ["docker", "container", "inspect", name], capture_output=True, text=True
    )
    container_results[name] = {
        "absent": result.returncode != 0,
        "returncode": result.returncode,
        "stderr": result.stderr,
    }
volume_result = subprocess.run(
    ["docker", "volume", "inspect", volume], capture_output=True, text=True
)
cleanup = {
    "containers": container_results,
    "volume": {
        "name": volume,
        "absent": volume_result.returncode != 0,
        "returncode": volume_result.returncode,
        "stderr": volume_result.stderr,
    },
}
with (root / "cleanup.json").open("x") as output:
    json.dump(cleanup, output, indent=2, sort_keys=True)
    output.write("\n")
checks["owned_containers_removed"] = all(
    result["absent"] for result in container_results.values()
)
checks["owned_volume_removed"] = cleanup["volume"]["absent"]

binding = {
    "schema": "layerfs-stage2-focused-binding-v1",
    "product_source_commit": source_commit,
    "product_source_tree": source_tree,
    "repository_head": (root / "repository-head-and-tree.txt").read_text().splitlines()[0],
    "repository_tree": (root / "repository-head-and-tree.txt").read_text().splitlines()[1],
    "image": "layerfs-fuse:frozen-7e82abc",
    "image_id": image_id,
    "image_architecture": image["Architecture"],
    "image_os": image["Os"],
    "executable_sha256": (root / "executable.sha256").read_text().split()[0],
    "executable_blake3": dirty["executable_blake3"],
    "fs_bench_sha256": dirty["fs_bench_sha256"],
}
with (root / "binding.json").open("x") as output:
    json.dump(binding, output, indent=2, sort_keys=True)
    output.write("\n")

oracle = {
    "schema": "layerfs-stage2-focused-current-external-unmount-v1",
    "status": "PASS" if all(checks.values()) else "FAIL",
    "checks": checks,
    "expected": expected,
    "reopened_sha256": actual_sha,
    "dirty_generation": dirty["generation"],
    "cleanup_generation": reopen["generation"],
    "dirty_root": dirty["root"],
    "clean_root": reopen["root"],
}
with (root / "oracle.json").open("x") as output:
    json.dump(oracle, output, indent=2, sort_keys=True)
    output.write("\n")
if oracle["status"] != "PASS":
    first = next(name for name, passed in checks.items() if not passed)
    raise SystemExit(f"first failing equation: {first}")
PY
event "oracle-pass"
event "run-complete"
trap - EXIT
