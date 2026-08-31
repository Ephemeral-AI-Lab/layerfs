#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)

die() { printf 'fs-bench-pro computer: %s\n' "$*" >&2; exit 2; }

[[ $# -eq 5 ]] ||
  die "usage: $0 RUN_DIR HOST_FIXTURE CONTAINER_FIXTURE COMPUTER_ROOT COMPUTER_IMAGE"
run_dir=$1
host_fixture=$2
container_fixture=$3
computer_root=$4
computer_image=$5

[[ "$run_dir" = /* && "$host_fixture" = /* && "$container_fixture" = /* && "$computer_root" = /* ]] ||
  die "paths must be absolute"
[[ -f "$host_fixture" ]] || die "host fixture is missing"
[[ -f "$computer_root/packages/computer/dist/index.js" ]] || die "Computer dist is missing"
[[ -f "$computer_root/packages/dofs/dist/index.js" ]] || die "DOFS dist is missing"
[[ "$(git -C "$computer_root" rev-parse main)" == de87919a4fd37242e960e13b7b3ba802d1eef0a0 ]] ||
  die "Computer main commit mismatch"
git -C "$computer_root" diff --quiet de87919a4fd37242e960e13b7b3ba802d1eef0a0 -- packages package-lock.json ||
  die "Computer product files differ from the pinned commit"
docker image inspect "$computer_image" >/dev/null || die "Computer image is missing"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/containers"

containers=()
cleanup() {
  if [[ ${#containers[@]} -gt 0 ]]; then
    docker rm -f "${containers[@]}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

for label in cold edit prepend read; do
  name="layerfs-fair-computer-${label}-$$"
  container=$(docker create \
    --name "$name" \
    --device /dev/fuse \
    --cap-add SYS_ADMIN \
    --pids-limit 512 \
    --publish 127.0.0.1::45678 \
    --env FUSE_MOUNT=fuse \
    --env MOUNT_POINT=/workspace \
    --env PORT=45678 \
    --entrypoint node \
    "$computer_image" \
    /opt/cloudflare-computer/packages/computerd/dist/cli/computerd.cjs)
  containers+=("$container")
  docker start "$container" >/dev/null
  docker exec "$container" mkdir -p "$(dirname "$container_fixture")"
  docker cp "$host_fixture" "$container:$container_fixture" >/dev/null
  endpoint=$(docker port "$container" 45678/tcp | awk 'NR == 1 { print $1 }')
  [[ "$endpoint" =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] || die "$label endpoint"
  url="http://$endpoint"
  ready=false
  for _ in $(seq 1 300); do
    if curl -fsS "$url/__computerd/info" >"$run_dir/containers/$label-info.json" 2>/dev/null; then
      ready=true
      break
    fi
    sleep 0.1
  done
  [[ "$ready" == true ]] || die "$label computerd did not become ready"
  docker exec "$container" awk '$5 == "/workspace" && $0 ~ / - fuse/ { found=1 } END { exit !found }' \
    /proc/self/mountinfo || die "$label is not a real FUSE mount"
  docker inspect "$container" >"$run_dir/containers/$label-inspect.json"
  docker exec "$container" cat /proc/self/mountinfo >"$run_dir/containers/$label-mountinfo.txt"
  docker exec "$container" sha256sum /benchmark/fs-benchmark-workload \
    >"$run_dir/containers/$label-workload.sha256"
  printf '%s\n' "$url" >"$run_dir/containers/$label-url.txt"
  upper=$(printf '%s' "$label" | tr '[:lower:]' '[:upper:]')
  export "COMPUTERD_${upper}_URL=$url"
done

first_hash=$(awk '{print $1}' "$run_dir/containers/cold-workload.sha256")
for label in edit prepend read; do
  [[ "$(awk '{print $1}' "$run_dir/containers/$label-workload.sha256")" == "$first_hash" ]] ||
    die "Computer workload binaries differ"
done

git -C "$computer_root" log -1 --oneline --decorate >"$run_dir/environment/computer-head.txt"
git -C "$repo" log -1 --oneline --decorate >"$run_dir/environment/layerfs-head.txt"
node --version >"$run_dir/environment/node-version.txt"
docker version >"$run_dir/environment/docker-version.txt"
docker image inspect "$computer_image" >"$run_dir/environment/computer-image.json"
date -u +%Y-%m-%dT%H:%M:%SZ >"$run_dir/environment/started-utc.txt"

env \
  CLOUDFLARE_COMPUTER_ROOT="$computer_root" \
  COMPUTER_BENCH_WORKLOAD=/benchmark/fs-benchmark-workload \
  node --no-warnings "$here/computer.mjs" \
    --fixture "$host_fixture" \
    --container-fixture "$container_fixture" \
    --output "$run_dir/summary.json"

jq -e '.status == "PASS" and .schema == "fs-benchmark-pro-computer-v3"' \
  "$run_dir/summary.json" >/dev/null || die "Computer result did not pass"
printf 'PASS %s\n' "$run_dir"
