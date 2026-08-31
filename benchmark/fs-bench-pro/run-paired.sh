#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root=${LAYERFS_PAIRED_RESULTS_ROOT:-"$repo/benchmark-results/fs-bench-pro/paired"}
provenance_repo=${LAYERFS_PROVENANCE_REPO:-"$repo"}

die() { printf 'fs-bench-pro paired: %s\n' "$*" >&2; exit 2; }
sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  else shasum -a 256 "$1" | awk '{print $1}'; fi
}

[[ $# -ge 6 && $# -le 7 ]] ||
  die "usage: $0 RUN_ID LAYERFS_CONTAINER HOST_FIXTURE CONTAINER_FIXTURE COMPUTER_ROOT COMPUTER_IMAGE [PAIRS]"
run_id=$1
layerfs_container=$2
host_fixture=$3
container_fixture=$4
computer_root=$5
computer_image=$6
pairs=${7:-7}
active_layerfs=
cleanup() {
  if [[ -n "$active_layerfs" ]]; then
    docker rm -f "$active_layerfs" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

[[ "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || die "unsafe RUN_ID"
[[ "$pairs" =~ ^[1-9][0-9]*$ ]] || die "invalid pair count"
[[ -f "$host_fixture" ]] || die "host fixture is missing"
[[ "$(wc -c <"$host_fixture" | tr -d ' ')" == 33554432 ]] || die "fixture size"
docker inspect "$layerfs_container" | python3 -c '
import json, sys
mounts = json.load(sys.stdin)[0].get("Mounts", [])
if any(mount.get("Type") == "bind" for mount in mounts):
    raise SystemExit("LayerFS container has a host bind")
' || die "LayerFS container custody"

source_seal=$("$here/run.sh" --source-seal)
container_seal=$(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$layerfs_container")
[[ "$source_seal" == "$container_seal" ]] || die "LayerFS source seal mismatch"
layerfs_image=$(docker inspect -f '{{.Image}}' "$layerfs_container")
[[ "$layerfs_image" =~ ^sha256:[0-9a-f]{64}$ ]] || die "LayerFS image identity"

run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment"
git -C "$provenance_repo" status --short >"$run_dir/environment/git-status.txt"
git -C "$provenance_repo" diff --binary >"$run_dir/environment/working-tree.patch"
git -C "$provenance_repo" log -1 --oneline --decorate >"$run_dir/environment/layerfs-head.txt"
git -C "$computer_root" log -1 --oneline --decorate >"$run_dir/environment/computer-head.txt"
docker inspect "$layerfs_container" >"$run_dir/environment/layerfs-container.json"
docker image inspect "$computer_image" >"$run_dir/environment/computer-image.json"
printf '%s\n' "$source_seal" >"$run_dir/environment/layerfs-source-seal.sha256"
printf '%s  %s\n' "$(sha256_file "$host_fixture")" "$host_fixture" >"$run_dir/environment/fixture.sha256"
date -u +%Y-%m-%dT%H:%M:%SZ >"$run_dir/environment/started-utc.txt"
uname -a >"$run_dir/environment/uname.txt"
docker version >"$run_dir/environment/docker-version.txt"

python3 - "$run_id" "$pairs" >"$run_dir/schedule.tsv" <<'PY'
import hashlib, random, sys
run_id, count = sys.argv[1], int(sys.argv[2])
seed = int.from_bytes(hashlib.sha256(run_id.encode()).digest()[:8], "big")
randomizer = random.Random(seed)
print(f"# seed={seed}")
for pair in range(1, count + 1):
    order = ["layerfs", "computer"]
    randomizer.shuffle(order)
    print(f"{pair}\t{order[0]}\t{order[1]}")
PY

cargo build --manifest-path "$repo/Cargo.toml" --release -p fs-benchmark-pro
oracle_workload="$run_dir/environment/fs-benchmark-workload-host"
rustc --edition=2021 -C opt-level=3 -C strip=symbols \
  "$here/workload.rs" -o "$oracle_workload"

run_layerfs() {
  pair_dir=$1
  active_layerfs=$(docker create \
    --device /dev/fuse \
    --cap-add SYS_ADMIN \
    --pids-limit 512 \
    --publish 127.0.0.1::41273 \
    "$layerfs_image")
  docker start "$active_layerfs" >/dev/null
  docker exec "$active_layerfs" mkdir -p "$(dirname "$container_fixture")"
  docker cp "$host_fixture" "$active_layerfs:$container_fixture" >/dev/null
  daemon_endpoint=$(docker port "$active_layerfs" 41273/tcp | awk 'NR == 1 { print $1 }')
  [[ "$daemon_endpoint" =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] || die "LayerFS daemon endpoint"
  ready=false
  daemon_host=${daemon_endpoint%:*}
  daemon_port=${daemon_endpoint##*:}
  for _ in $(seq 1 300); do
    if docker exec "$active_layerfs" test -f /run/layerfs/capability 2>/dev/null \
      && nc -z "$daemon_host" "$daemon_port" 2>/dev/null; then
      ready=true
      break
    fi
    sleep 0.1
  done
  [[ "$ready" == true ]] || die "LayerFS daemon capability did not become ready"
  capability_file=$(mktemp "${TMPDIR:-/tmp}/layerfs-paired-capability.XXXXXX")
  docker cp "$active_layerfs:/run/layerfs/capability" "$capability_file" >/dev/null
  daemon_capability=$(od -An -tx1 -v "$capability_file" | tr -d ' \n')
  rm -f -- "$capability_file"
  [[ "$daemon_capability" =~ ^[0-9a-f]{64}$ ]] || die "LayerFS daemon capability"
  mkdir "$pair_dir/layerfs" "$pair_dir/layerfs/work"
  docker inspect "$active_layerfs" >"$pair_dir/layerfs/container.json"
  set +e
  env \
    LAYERFS_BENCH_SHELL=1 \
    LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
    LAYERFS_BENCH_ORACLE_WORKLOAD="$oracle_workload" \
    LAYERFS_BENCH_FIXTURE="$container_fixture" \
    LAYERFS_EXEC_TRANSPORT=daemon \
    LAYERFS_FUSE_TRANSPORT=daemon \
    LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" \
    LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
    LAYERFS_DAEMON_CONTAINER_ID="$active_layerfs" \
    LAYERFS_FUSE_HOST=host.docker.internal \
    "$repo/target/release/fs-benchmark-pro" run \
      "$pair_dir/layerfs/work" "$host_fixture" "$active_layerfs" 1 \
      >"$pair_dir/layerfs/raw.jsonl" 2>"$pair_dir/layerfs/stderr.txt"
  layerfs_status=$?
  set -e
  if [[ "$layerfs_status" == 0 ]]; then
    printf 'PASS\n' >"$pair_dir/layerfs/hard-gate-status.txt"
  elif [[ "$layerfs_status" == 1 ]] \
    && grep -Fqx 'fs-benchmark-pro: one or more hard performance gates failed' "$pair_dir/layerfs/stderr.txt" \
    && grep -Fq '"schema":"fs-bench-pro-v4-summary"' "$pair_dir/layerfs/raw.jsonl"; then
    printf 'FAIL\n' >"$pair_dir/layerfs/hard-gate-status.txt"
  else
    return "$layerfs_status"
  fi
  docker logs "$active_layerfs" >"$pair_dir/layerfs/container.log" 2>&1 || true
  docker rm -f "$active_layerfs" >/dev/null 2>&1 || true
  active_layerfs=
}

run_computer() {
  pair_dir=$1
  "$here/run-computer-host.sh" \
    "$pair_dir/computer" "$host_fixture" "$container_fixture" \
    "$computer_root" "$computer_image" \
    >"$pair_dir/computer.stdout.txt" 2>"$pair_dir/computer.stderr.txt"
}

while IFS=$'\t' read -r pair first second; do
  [[ "$pair" =~ ^[0-9]+$ ]] || continue
  pair_dir=$(printf '%s/pair-%03d' "$run_dir" "$pair")
  mkdir "$pair_dir"
  printf '%s\n%s\n' "$first" "$second" >"$pair_dir/order.txt"
  for candidate in "$first" "$second"; do
    case "$candidate" in
      layerfs) run_layerfs "$pair_dir" ;;
      computer) run_computer "$pair_dir" ;;
      *) die "invalid schedule candidate" ;;
    esac
  done
done <"$run_dir/schedule.tsv"

ending_seal=$("$here/run.sh" --source-seal)
printf '%s\n' "$ending_seal" >"$run_dir/environment/layerfs-ending-source-seal.sha256"
[[ "$ending_seal" == "$source_seal" ]] ||
  die "LayerFS source changed during the paired campaign; evidence retained as invalid"
python3 "$here/paired_compare.py" "$run_dir" --minimum-pairs "$pairs" \
  --output "$run_dir/report.md"
printf 'PASS %s\n' "$run_dir"
