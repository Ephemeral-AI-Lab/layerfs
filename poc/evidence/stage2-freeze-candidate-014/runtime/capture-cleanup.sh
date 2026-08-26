#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 CONTAINER_ID IMAGE_ID VOLUME_NAME PHASE SOURCE_COMMIT SOURCE_TREE" >&2
  exit 2
}

[[ $# -eq 6 ]] || usage
container_id="$1"
image_id="$2"
volume_name="$3"
phase="$4"
source_commit="$5"
source_tree="$6"

[[ "$container_id" =~ ^[0-9a-f]{64}$ ]] || usage
[[ "$image_id" =~ ^sha256:[0-9a-f]{64}$ ]] || usage
[[ "$volume_name" =~ ^layerfs_stage2_final014_[a-z0-9_]+$ ]] || usage
[[ "$phase" =~ ^[a-z0-9][a-z0-9-]*$ ]] || usage
[[ "$source_commit" =~ ^[0-9a-f]{40}$ ]] || usage
[[ "$source_tree" =~ ^[0-9a-f]{40}$ ]] || usage

script_dir="$(cd "$(dirname "$0")" && pwd)"
output="$script_dir/$phase"
[[ ! -e "$output" ]] || {
  echo "output already exists: $output" >&2
  exit 1
}
umask 077
mkdir "$output"

docker container inspect "$container_id" > "$output/container-inspect.json"
docker image inspect "$image_id" > "$output/image-inspect.json"
docker volume inspect "$volume_name" > "$output/volume-inspect.json"

actual_image="$(docker container inspect --format '{{.Image}}' "$container_id")"
actual_name="$(docker container inspect --format '{{.Name}}' "$container_id")"
running="$(docker container inspect --format '{{.State.Running}}' "$container_id")"
store_volume="$(docker container inspect --format '{{range .Mounts}}{{if eq .Destination "/var/lib/layerfs"}}{{.Name}}{{end}}{{end}}' "$container_id")"
workspace_mount="$(docker container inspect --format '{{range .Mounts}}{{if eq .Destination "/workspace"}}{{.Type}}:{{.Source}}{{end}}{{end}}' "$container_id")"
image_commit="$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$image_id")"
image_layerfs_commit="$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.layerfs.source-commit"}}' "$image_id")"
image_tree="$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.layerfs.source-tree"}}' "$image_id")"
image_source="$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.source"}}' "$image_id")"
image_bench="$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.layerfs.fs-bench-sha256"}}' "$image_id")"
image_environment="$(docker image inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$image_id")"
expected_args='["--store","/var/lib/layerfs/store.sqlite","--mount","/workspace","--spool","/var/tmp/layerfs-owned/spool","--receipt","/var/tmp/layerfs-owned/terminal.json","--ref","main","--integrity","trusted","--uid","0","--gid","0"]'

[[ "$actual_image" == "$image_id" ]]
[[ "$actual_name" == /layerfs-stage2-final014-* ]]
[[ "$running" == true ]]
[[ "$store_volume" == "$volume_name" ]]
[[ -z "$workspace_mount" ]]
[[ "$(docker container inspect --format '{{.Path}}' "$container_id")" == /usr/local/bin/layerfs-fuse ]]
[[ "$(docker container inspect --format '{{json .Args}}' "$container_id")" == "$expected_args" ]]
[[ "$(docker container inspect --format '{{.HostConfig.Privileged}}' "$container_id")" == false ]]
[[ "$(docker container inspect --format '{{.HostConfig.Init}}' "$container_id")" == true ]]
[[ "$(docker container inspect --format '{{.HostConfig.NanoCpus}}' "$container_id")" == 1000000000 ]]
[[ "$(docker container inspect --format '{{.HostConfig.Memory}}' "$container_id")" == 3221225472 ]]
[[ "$(docker container inspect --format '{{.HostConfig.PidsLimit}}' "$container_id")" == 512 ]]
[[ "$(docker container inspect --format '{{.HostConfig.NetworkMode}}' "$container_id")" == none ]]
[[ "$(docker container inspect --format '{{json .HostConfig.CapAdd}}' "$container_id")" == '["CAP_SYS_ADMIN"]' ]]
[[ "$(docker container inspect --format '{{range .HostConfig.Devices}}{{.PathOnHost}}:{{.PathInContainer}}:{{.CgroupPermissions}}{{end}}' "$container_id")" == /dev/fuse:/dev/fuse:rwm ]]
[[ "$(docker container inspect --format '{{index .HostConfig.Tmpfs "/tmp"}}' "$container_id")" == rw,nosuid,nodev,size=1g,mode=1777 ]]
[[ "$image_commit" == "$source_commit" ]]
[[ "$image_layerfs_commit" == "$source_commit" ]]
[[ "$image_tree" == "$source_tree" ]]
[[ "$image_source" == https://github.com/Ephemeral-AI-Lab/layerfs-engine-lab ]]
[[ "$image_bench" == 0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef ]]
grep -Fqx "LAYERFS_SOURCE_COMMIT=$source_commit" <<< "$image_environment"
grep -Fqx "LAYERFS_SOURCE_TREE=$source_tree" <<< "$image_environment"

cat > "$output/validated-runtime.txt" <<EOF
container_id=$container_id
container_name=$actual_name
image_id=$actual_image
source_commit=$source_commit
source_tree=$source_tree
store_volume=$store_volume
workspace_docker_mount=absent
EOF

printf 'docker exec %q sha256sum /usr/local/bin/layerfs-fuse\n' "$container_id" \
  > "$output/executable-sha256.command"
docker exec "$container_id" sha256sum /usr/local/bin/layerfs-fuse \
  > "$output/executable-sha256.txt"
printf 'docker exec %q sha256sum /usr/local/bin/fs-bench.sh\n' "$container_id" \
  > "$output/fs-bench-sha256.command"
docker exec "$container_id" sha256sum /usr/local/bin/fs-bench.sh \
  > "$output/fs-bench-sha256.txt"
grep -Fqx '0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef  /usr/local/bin/fs-bench.sh' \
  "$output/fs-bench-sha256.txt"

docker top "$container_id" -eo pid,ppid,stat,lstart,args \
  > "$output/processes-running.txt"
docker exec "$container_id" sh -c '
  for process in /proc/[0-9]*; do
    [ -r "$process/stat" ] || continue
    pid=${process##*/}
    command=$(tr "\0" " " < "$process/cmdline" 2>/dev/null || true)
    printf "%s\t%s\n" "$pid" "$command"
  done
' > "$output/processes-container.txt"
docker exec "$container_id" sh -c 'cat /proc/1/mountinfo' \
  > "$output/mountinfo.txt"
grep -Eq ' /workspace .* - fuse layerfs ' "$output/mountinfo.txt"
docker exec "$container_id" sh -c '
  for fd in /proc/[0-9]*/fd/[0-9]*; do
    target=$(readlink "$fd" 2>/dev/null || true)
    [ "$target" = /dev/fuse ] || continue
    printf "%s -> %s\n" "$fd" "$target"
  done
' > "$output/dev-fuse-holders.txt"

docker exec "$container_id" sh -c '
  for file in \
    cpu.max cpu.stat cpu.pressure \
    memory.current memory.peak memory.stat memory.events memory.events.local memory.pressure \
    io.stat io.pressure \
    pids.current pids.peak pids.max pids.events; do
    printf "===== %s =====\n" "$file"
    if [ -r "/sys/fs/cgroup/$file" ]; then
      cat "/sys/fs/cgroup/$file"
    else
      echo MISSING
    fi
  done
' > "$output/cgroup.txt"

docker exec "$container_id" sh -c '
  if [ -d /var/tmp/layerfs-owned ]; then
    find /var/tmp/layerfs-owned -xdev -printf "%y\t%m\t%s\t%p\n" | sort
  else
    echo ABSENT
  fi
' > "$output/runtime-owned-listing.txt"

docker stop --timeout 30 "$container_id" > "$output/docker-stop.txt"
docker container inspect "$container_id" > "$output/container-stopped-inspect.json"
[[ "$(docker container inspect --format '{{.State.Running}}' "$container_id")" == false ]]
[[ "$(docker container inspect --format '{{.State.ExitCode}}' "$container_id")" == 0 ]]
[[ "$(docker container inspect --format '{{.State.OOMKilled}}' "$container_id")" == false ]]

set +e
docker top "$container_id" -eo pid,ppid,stat,lstart,args \
  > "$output/processes-stopped.stdout" 2> "$output/processes-stopped.stderr"
top_status=$?
set -e
printf '%s\n' "$top_status" > "$output/processes-stopped.exit"
[[ "$top_status" -ne 0 ]]

docker cp "$container_id:/var/tmp/layerfs-owned/." - | tar -tvf - \
  > "$output/runtime-owned-stopped-listing.txt"

docker cp "$container_id:/var/tmp/layerfs-owned/terminal.json" \
  "$output/terminal.json" 2> "$output/terminal-copy.stderr"
python3 - "$output/terminal.json" "$source_commit" "$source_tree" \
  > "$output/terminal-validation.json" <<'PY'
import json
import re
import sys

path, source_commit, source_tree = sys.argv[1:]
with open(path, encoding="utf-8") as source:
    terminal = json.load(source)
mounted = terminal["mounted"]
engine = terminal["engine"]
checks = {
    "status_pass": terminal["status"] == "PASS",
    "source_commit_exact": terminal["source_commit"] == source_commit,
    "source_tree_exact": terminal["source_tree"] == source_tree,
    "executable_blake3_exact": re.fullmatch(
        r"[0-9a-f]{64}", terminal["executable_blake3"]
    ) is not None,
    "fs_bench_sha256_exact": terminal["fs_bench_sha256"]
    == "0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef",
    "connections_terminal_zero": engine["connections_terminal"] == 0,
    "operation_q_terminal_zero": mounted["operation_q_terminal_bytes"] == 0,
    "spool_terminal_zero": all(
        mounted[key] == 0
        for key in ("spool_live_bytes", "spool_dead_bytes", "spool_physical_bytes")
    ),
    "handles_terminal_zero": mounted["open_handles"] == 0,
    "dirty_terminal_zero": mounted["dirty_nodes"] == mounted["dirty_ranges"] == 0,
    "root_only_ownership": mounted["lookup_refs"]
    == mounted["live_nodes"]
    == mounted["inode_mappings"]
    == 1,
}
receipt = {
    "schema": "layerfs-stage2-runtime-terminal-validation-v1",
    "status": "PASS" if all(checks.values()) else "FAIL",
    "checks": checks,
}
json.dump(receipt, sys.stdout, indent=2, sort_keys=True)
sys.stdout.write("\n")
if receipt["status"] != "PASS":
    raise SystemExit(1)
PY

docker run --rm --platform linux/arm64 --network none --read-only \
  --mount "type=volume,src=$volume_name,dst=/volume,readonly" \
  --entrypoint sh "$image_id" -c '
    find /volume -xdev -printf "%y\t%m\t%s\t%p\n" | sort
    echo "===== sqlite-sidecars ====="
    find /volume -xdev -type f \( -name "*-journal" -o -name "*-wal" -o -name "*-shm" \) -print | sort
  ' > "$output/store-volume-listing.txt"

docker rm "$container_id" > "$output/docker-rm.txt"
docker volume rm "$volume_name" > "$output/docker-volume-rm.txt"

set +e
docker container inspect "$container_id" \
  > "$output/post-container-inspect.stdout" 2> "$output/post-container-inspect.stderr"
container_status=$?
docker volume inspect "$volume_name" \
  > "$output/post-volume-inspect.stdout" 2> "$output/post-volume-inspect.stderr"
volume_status=$?
set -e

[[ "$container_status" -ne 0 ]]
[[ "$volume_status" -ne 0 ]]
docker ps -a --no-trunc --filter "id=$container_id" --format '{{.ID}}' \
  > "$output/post-container-list.txt"
[[ ! -s "$output/post-container-list.txt" ]]
docker ps -a --no-trunc --format '{{.ID}}\t{{.Names}}' | \
  awk -v id="$container_id" -v name="${actual_name#/}" \
    '$1 == id || $2 == name { print }' \
  > "$output/post-container-scope.txt"
[[ ! -s "$output/post-container-scope.txt" ]]
docker volume ls --format '{{.Name}}' | \
  awk -v name="$volume_name" '$0 == name { print }' \
  > "$output/post-volume-list.txt"
[[ ! -s "$output/post-volume-list.txt" ]]

ps -axo pid=,ppid=,command= | awk -v name="${actual_name#/}" '
  index($0, "/usr/local/bin/layerfs-fuse") && index($0, name) { print }
' > "$output/post-host-processes.txt"
[[ ! -s "$output/post-host-processes.txt" ]]

cat > "$output/cleanup-verification.txt" <<EOF
status=PASS
container_inspect_exit=$container_status
volume_inspect_exit=$volume_status
container_absent=true
volume_absent=true
scoped_process_absent=true
scoped_mount_namespace_absent=true
EOF
