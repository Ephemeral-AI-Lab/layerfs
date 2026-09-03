#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root="$repo/benchmark-results/fs-bench-pro/runs"
history="$repo/benchmark-results/fs-bench-pro/optimization-history.md"

die() { printf 'fs-bench-pro: %s\n' "$*" >&2; exit 2; }

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  else shasum -a 256 "$1" | awk '{print $1}'; fi
}

source_seal() {
  python3 - "$repo" <<'PY'
import hashlib
import sys
from pathlib import Path
root = Path(sys.argv[1])
paths = []
for directory in [
    root / "crates/layerfs-content",
    root / "crates/layerfs-daemon",
    root / "crates/layerfs-layerstack-store",
    root / "crates/layerfs-sdk",
    root / "crates/layerfs-workspace",
    root / "crates/layerfs-fuse",
    root / "crates/layerfs-materialization",
    root / "crates/layerfs-monitor",
    root / "benchmark/fs-bench-pro/src",
]:
    paths.extend(path for path in directory.rglob("*") if path.is_file())
paths += [
    root / "Cargo.toml",
    root / "Cargo.lock",
    root / "docs/roadmap/0.1/benchmarking.md",
    root / "docs/research/history/v2-replacement/spec.md",
    root / "benchmark/fs-bench-pro/Cargo.toml",
    root / "benchmark/fs-bench-pro/Dockerfile.layerfs",
    root / "benchmark/fs-bench-pro/daemon-entrypoint.sh",
    root / "benchmark/fs-bench-pro/workload.rs",
]
digest = hashlib.sha256()
for path in sorted(set(paths)):
    if "target" in path.parts or "__pycache__" in path.parts:
        continue
    digest.update(str(path.relative_to(root)).encode())
    digest.update(b"\0")
    digest.update(path.read_bytes())
print(digest.hexdigest())
PY
}

self_check() {
  bash -n "$0"
  python3 "$here/compare.py" --self-check
  cargo test --manifest-path "$repo/Cargo.toml" -p fs-benchmark-pro \
    tests::lifecycle_equations_and_median_are_exact -- --exact
}

if [[ "${1:-}" == "--self-check" ]]; then self_check; exit 0; fi
if [[ "${1:-}" == "--source-seal" ]]; then source_seal; exit 0; fi

[[ $# -ge 4 && $# -le 5 ]] ||
  die "usage: $0 RUN_ID CONTAINER_ID HOST_FIXTURE CONTAINER_FIXTURE [ITERATIONS]"
run_id=$1
container=$2
host_fixture=$3
container_fixture=$4
iterations=${5:-3}
daemon_container_port=${LAYERFS_DAEMON_CONTAINER_PORT:-41273}
[[ "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || die "unsafe RUN_ID"
[[ "$iterations" =~ ^[1-9][0-9]*$ ]] || die "invalid iteration count"
[[ "$daemon_container_port" =~ ^[1-9][0-9]{0,4}$ ]] || die "invalid daemon container port"
[[ -f "$host_fixture" ]] || die "host fixture is missing"
[[ "$(wc -c <"$host_fixture" | tr -d ' ')" == 33554432 ]] || die "fixture is not 32 MiB"
command -v docker >/dev/null || die "docker is required"
docker inspect -f '{{.State.Running}}' "$container" | grep -Fx true >/dev/null ||
  die "prepared container is not running"
container_id=$(docker inspect -f '{{.Id}}' "$container")
[[ "$container_id" =~ ^[0-9a-f]{64}$ ]] || die "prepared container identity"
docker exec "$container" test -c /dev/fuse || die "prepared container lacks /dev/fuse"
docker exec "$container" test -x /usr/local/bin/layerfs-daemon ||
  die "prepared container lacks layerfs-daemon"
docker exec "$container" test -x /usr/local/bin/layerfs-fuse ||
  die "prepared container lacks layerfs-fuse"
docker exec "$container" test -x /usr/local/bin/fs-benchmark-workload ||
  die "prepared container lacks fs-benchmark-workload"
docker exec "$container" test -f "$container_fixture" || die "container fixture is missing"
[[ "$(docker exec "$container" sha256sum "$container_fixture" | awk '{print $1}')" == "$(sha256_file "$host_fixture")" ]] ||
  die "host and prepared-container fixtures differ"

current_seal=$(source_seal)
[[ "$(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container")" == "$current_seal" ]] ||
  die "prepared container does not match the current source seal"
docker inspect "$container" | python3 -c '
import json, sys
mounts = json.load(sys.stdin)[0].get("Mounts", [])
if any(mount.get("Type") == "bind" for mount in mounts):
    raise SystemExit("prepared container has a forbidden host bind")
' || die "prepared container custody failed"

daemon_endpoint=$(docker port "$container" "$daemon_container_port/tcp" 2>/dev/null || true)
[[ "$daemon_endpoint" =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] ||
  die "daemon port must be published only on 127.0.0.1"
capability_file=$(mktemp "${TMPDIR:-/tmp}/layerfs-daemon-capability.XXXXXX")
trap 'rm -f -- "$capability_file"' EXIT
docker cp "$container:/run/layerfs/capability" "$capability_file" >/dev/null
[[ "$(wc -c <"$capability_file" | tr -d ' ')" == 32 ]] || die "daemon capability length"
daemon_capability=$(od -An -tx1 -v "$capability_file" | tr -d ' \n')
rm -f -- "$capability_file"
trap - EXIT
[[ "$daemon_capability" =~ ^[0-9a-f]{64}$ ]] || die "daemon capability encoding"

run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/raw" "$run_dir/work"

git -C "$repo" status --short >"$run_dir/environment/git-status.txt"
git -C "$repo" diff --binary >"$run_dir/environment/working-tree.patch"
git -C "$repo" diff --cached --binary >"$run_dir/environment/index.patch"
git -C "$repo" log -1 --oneline --decorate >"$run_dir/environment/git-head.txt"
printf '%s\n' "$current_seal" >"$run_dir/environment/source-seal.sha256"
date -u +%Y-%m-%dT%H:%M:%SZ >"$run_dir/environment/started-utc.txt"
uname -a >"$run_dir/environment/uname.txt"
docker version >"$run_dir/environment/docker-version.txt"
docker inspect "$container" >"$run_dir/environment/container-inspect.json"
printf '%s  %s\n' "$(sha256_file "$host_fixture")" "$host_fixture" >"$run_dir/environment/fixture.sha256"
printf '%s\n' "$daemon_endpoint" >"$run_dir/environment/daemon-endpoint.txt"

cargo build --manifest-path "$repo/Cargo.toml" --release -p fs-benchmark-pro
oracle_workload="$run_dir/environment/fs-benchmark-workload-host"
rustc --edition=2021 -C opt-level=3 -C strip=symbols \
  "$here/workload.rs" -o "$oracle_workload"
printf '%s  %s\n' "$(sha256_file "$oracle_workload")" "$oracle_workload" \
  >"$run_dir/environment/oracle-workload.sha256"
raw="$run_dir/raw/layerfs.jsonl"
stderr="$run_dir/raw/layerfs.stderr"
benchmark_command=(
  env
  LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload
  LAYERFS_BENCH_ORACLE_WORKLOAD="$oracle_workload"
  LAYERFS_BENCH_FIXTURE="$container_fixture"
  LAYERFS_EXEC_TRANSPORT=daemon
  LAYERFS_FUSE_TRANSPORT=daemon
  LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint"
  LAYERFS_DAEMON_CAPABILITY="$daemon_capability"
  LAYERFS_DAEMON_CONTAINER_ID="$container_id"
  LAYERFS_FUSE_HOST=host.docker.internal
  "$repo/target/release/fs-benchmark-pro" run "$run_dir/work" "$host_fixture" "$container_id" "$iterations"
)
set +e
if [[ "$(uname -s)" == Darwin ]]; then
  /usr/bin/time -l "${benchmark_command[@]}" >"$raw" 2>"$stderr"
else
  "${benchmark_command[@]}" >"$raw" 2>"$stderr"
fi
status=$?
set -e
if [[ "$(uname -s)" == Darwin ]]; then
  host_peak=$(awk '/maximum resident set size/{print $1}' "$stderr" | tail -n 1)
  host_swaps=$(awk '/^[[:space:]]*[0-9]+[[:space:]]+swaps$/{print $1}' "$stderr" | tail -n 1)
  [[ "$host_peak" =~ ^[0-9]+$ && "$host_swaps" =~ ^[0-9]+$ ]] || die "host memory evidence"
  printf 'host_peak_rss_bytes=%s\nhost_swaps=%s\n' "$host_peak" "$host_swaps" \
    >"$run_dir/environment/memory.txt"
  if (( host_peak > 128 * 1024 * 1024 || host_swaps != 0 )); then
    printf 'fs-bench-pro: host memory gate failed: peak=%s swaps=%s\n' "$host_peak" "$host_swaps" >>"$stderr"
    status=1
  fi
fi
set +e
python3 "$here/compare.py" "$raw" >"$run_dir/report.md"
compare_status=$?
set -e
if [[ $compare_status -ne 0 ]]; then
  printf '### One-Store fs-bench-pro campaign\n\n- Raw evidence: `%s`\n- Result: invalid or incomplete evidence; see `raw/layerfs.stderr`.\n' \
    "$raw" >"$run_dir/report.md"
  [[ $status -ne 0 ]] || status=$compare_status
fi
if [[ -f "$run_dir/environment/memory.txt" ]]; then
  printf '\n- Host memory: `%s`\n' "$(tr '\n' ' ' <"$run_dir/environment/memory.txt")" \
    >>"$run_dir/report.md"
fi
printf '\n## %s — one-Store public-SDK campaign\n\n' "$run_id" >>"$history"
cat "$run_dir/report.md" >>"$history"
printf '\n- Source seal: `%s`\n- Exit status: `%s`\n' "$(cat "$run_dir/environment/source-seal.sha256")" "$status" >>"$history"
[[ $status -eq 0 ]] || die "campaign missed a hard gate; evidence retained at $run_dir"
printf 'PASS %s\n' "$run_dir"
