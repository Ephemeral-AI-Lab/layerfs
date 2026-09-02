#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root=${LAYERFS_NAMESPACE_RESULTS_ROOT:-"$repo/benchmark-results/fs-bench-pro/namespace"}

die() { printf 'fs-bench-pro namespace: %s\n' "$*" >&2; exit 2; }

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  else shasum -a 256 "$1" | awk '{print $1}'; fi
}

seal() {
  python3 - "$repo" "$1" <<'PY'
import hashlib
import sys
from pathlib import Path

root, kind = Path(sys.argv[1]), sys.argv[2]
product = [
    root / "crates/layerfs-content",
    root / "crates/layerfs-daemon",
    root / "crates/layerfs-layerstack-store",
    root / "crates/layerfs-sdk",
    root / "crates/layerfs-workspace",
    root / "crates/layerfs-fuse",
    root / "crates/layerfs-materialization",
    root / "crates/layerfs-monitor",
]
harness = [root / "benchmark/fs-bench-pro"]
directories = product + harness if kind == "source" else product if kind == "product" else harness
paths = []
for directory in directories:
    paths.extend(path for path in directory.rglob("*") if path.is_file())
if kind == "source":
    paths += [root / "Cargo.toml", root / "Cargo.lock"]
if kind in {"source", "harness"}:
    paths += [
        root / "docs/roadmap/0.1/benchmarking.md",
        root / "docs/roadmap/0.1/0.1.1/README.md",
        root / "docs/roadmap/0.1/0.1.1/namespace-optimization-spec.md",
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
  cargo test --manifest-path "$repo/Cargo.toml" -p fs-benchmark-pro
  local temporary
  temporary=$(mktemp -d "${TMPDIR:-/tmp}/fs-bench-pro-namespace.XXXXXX")
  trap 'rm -rf -- "$temporary"' EXIT
  rustc --edition=2021 -C opt-level=3 "$here/workload.rs" -o "$temporary/fs-benchmark-workload"
  "$temporary/fs-benchmark-workload" self-check
  rm -rf -- "$temporary"
  trap - EXIT
}

if [[ "${1:-}" == "--self-check" ]]; then self_check; exit 0; fi
if [[ "${1:-}" == "--source-seal" ]]; then seal source; exit 0; fi

[[ $# -eq 4 ]] || die "usage: $0 RUN_ID CONTAINER_ID namespace-10000|all ITERATIONS"
run_id=$1
container=$2
selection=$3
iterations=$4
daemon_container_port=${LAYERFS_DAEMON_CONTAINER_PORT:-41273}
fixture_root=${LAYERFS_NAMESPACE_FIXTURE_ROOT:-}
measurement_mode=${LAYERFS_NAMESPACE_MODE:-product}
[[ "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || die "unsafe RUN_ID"
[[ "$iterations" =~ ^[1-9][0-9]*$ ]] || die "invalid iteration count"
[[ "$daemon_container_port" =~ ^[1-9][0-9]{0,4}$ ]] || die "invalid daemon container port"
case "$measurement_mode" in
  product|init-only-diagnostic) ;;
  *) die "LAYERFS_NAMESPACE_MODE must be product or init-only-diagnostic" ;;
esac
if [[ -n "$fixture_root" ]]; then
  [[ "$fixture_root" == /* && -d "$fixture_root" ]] || die "LAYERFS_NAMESPACE_FIXTURE_ROOT must be an existing absolute directory"
  fixture_root=$(cd "$fixture_root" && pwd -P)
fi
case "$selection" in
  all) scenarios=(namespace-100 namespace-1000 namespace-10000 namespace-100000) ;;
  namespace-100|namespace-1000|namespace-10000|namespace-100000) scenarios=("$selection") ;;
  *) die "unknown namespace scenario" ;;
esac

for command in cargo docker git nc python3 rustc; do command -v "$command" >/dev/null || die "$command is required"; done
[[ -x /usr/bin/time ]] || die "/usr/bin/time is required"
docker inspect -f '{{.State.Running}}' "$container" | grep -Fx true >/dev/null ||
  die "prepared container is not running"
container_id=$(docker inspect -f '{{.Id}}' "$container")
[[ "$container_id" =~ ^[0-9a-f]{64}$ ]] || die "prepared container identity"
docker exec "$container" test -c /dev/fuse || die "prepared container lacks /dev/fuse"
docker exec "$container" test -x /usr/local/bin/layerfs-daemon || die "prepared container lacks layerfs-daemon"
docker exec "$container" test -x /usr/local/bin/layerfs-fuse || die "prepared container lacks layerfs-fuse"
docker exec "$container" test -x /usr/local/bin/fs-benchmark-workload ||
  die "prepared container lacks fs-benchmark-workload"
docker inspect "$container" | python3 -c '
import json, sys
if any(mount.get("Type") == "bind" for mount in json.load(sys.stdin)[0].get("Mounts", [])):
    raise SystemExit(1)
' || die "prepared container has a forbidden host bind"

current_seal=$(seal source)
container_seal=$(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container")
[[ "$container_seal" == "$current_seal" ]] || die "prepared container does not match the namespace source seal"
daemon_endpoint=$(docker port "$container" "$daemon_container_port/tcp" 2>/dev/null || true)
[[ "$daemon_endpoint" =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] ||
  die "daemon port must be published only on 127.0.0.1"
ensure_daemon_running() {
  if [[ "$(docker inspect -f '{{.State.Running}}' "$container")" != true ]]; then
    [[ "$(docker inspect -f '{{.State.ExitCode}} {{.State.OOMKilled}}' "$container")" == "0 false" ]] ||
      die "prepared daemon container stopped abnormally"
    docker start "$container" >/dev/null
  fi
  daemon_endpoint=$(docker port "$container" "$daemon_container_port/tcp" 2>/dev/null || true)
  [[ "$daemon_endpoint" =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] ||
    die "restarted daemon port must be published only on 127.0.0.1"
  local ready=false host port
  host=${daemon_endpoint%:*}
  port=${daemon_endpoint##*:}
  for _ in $(seq 1 300); do
    if docker exec "$container" test -f /run/layerfs/capability 2>/dev/null \
      && nc -z "$host" "$port" 2>/dev/null; then
      ready=true
      break
    fi
    sleep 0.1
  done
  [[ "$ready" == true ]] || die "prepared daemon did not become ready"
}
capability_file=$(mktemp "${TMPDIR:-/tmp}/layerfs-namespace-capability.XXXXXX")
trap 'rm -f -- "$capability_file"' EXIT
docker cp "$container:/run/layerfs/capability" "$capability_file" >/dev/null
[[ "$(wc -c <"$capability_file" | tr -d ' ')" == 32 ]] || die "daemon capability length"
daemon_capability=$(od -An -tx1 -v "$capability_file" | tr -d ' \n')
rm -f -- "$capability_file"
trap - EXIT
[[ "$daemon_capability" =~ ^[0-9a-f]{64}$ ]] || die "daemon capability encoding"

product_commit=$(git -C "$repo" rev-parse 'v0.1.0^{commit}')
product_tag=$(git -C "$repo" rev-parse v0.1.0)
[[ "$product_commit" == 243f98a3bf287d6a9f8168891452b7355d45529c ]] ||
  die "v0.1.0 product identity"
run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/scenarios"
nonce_registry="$run_dir/environment/initialization-diagnostic-nonces.txt"
: >"$nonce_registry"

git -C "$repo" status --short >"$run_dir/environment/git-status.txt"
git -C "$repo" diff --binary >"$run_dir/environment/working-tree.patch"
git -C "$repo" diff --cached --binary >"$run_dir/environment/index.patch"
git -C "$repo" log -1 --oneline --decorate >"$run_dir/environment/git-head.txt"
git -C "$repo" diff --binary "$product_commit" -- crates \
  >"$run_dir/environment/product-v0.1.0.diff"
git -C "$repo" diff --binary "$product_commit" -- benchmark/fs-bench-pro docs/roadmap/0.1 \
  >"$run_dir/environment/harness-v0.1.0.diff"
git -C "$repo" ls-files --others --exclude-standard >"$run_dir/environment/untracked-files.txt"
while IFS= read -r path; do
  [[ -n "$path" ]] && printf '%s  %s\n' "$(sha256_file "$repo/$path")" "$path"
done <"$run_dir/environment/untracked-files.txt" >"$run_dir/environment/untracked-files.sha256"
printf '%s\n' "$product_commit" >"$run_dir/environment/product-v0.1.0.commit"
printf '%s\n' "$product_tag" >"$run_dir/environment/product-v0.1.0.tag-object"
git -C "$repo" rev-parse HEAD >"$run_dir/environment/harness-head.commit"
printf '%s\n' "$(seal product)" >"$run_dir/environment/product-source-seal.sha256"
printf '%s\n' "$(seal harness)" >"$run_dir/environment/harness-source-seal.sha256"
printf '%s\n' "$current_seal" >"$run_dir/environment/source-seal.sha256"
printf '%s  %s\n' "$(sha256_file "$here/run-namespace.sh")" "$here/run-namespace.sh" \
  >"$run_dir/environment/namespace-runner.sha256"
date -u +%Y-%m-%dT%H:%M:%SZ >"$run_dir/environment/started-utc.txt"
uname -a >"$run_dir/environment/uname.txt"
docker version >"$run_dir/environment/docker-version.txt"
docker inspect "$container" >"$run_dir/environment/container-inspect.json"
docker image inspect "$(docker inspect -f '{{.Image}}' "$container")" \
  >"$run_dir/environment/container-image-inspect.json"
printf '%s\n' "$daemon_endpoint" >"$run_dir/environment/daemon-endpoint.txt"
printf '%s\n' "${fixture_root:-generated-per-scenario}" >"$run_dir/environment/fixture-source-root.txt"
printf '%s\n' "$measurement_mode" >"$run_dir/environment/measurement-mode.txt"

cargo build --manifest-path "$repo/Cargo.toml" --release -p fs-benchmark-pro
binary="$repo/target/release/fs-benchmark-pro"
workload_dir=$(mktemp -d "${TMPDIR:-/tmp}/fs-bench-pro-namespace-workload.XXXXXX")
trap 'rm -rf -- "$workload_dir"' EXIT
rustc --edition=2021 -C opt-level=3 "$here/workload.rs" -o "$workload_dir/fs-benchmark-workload"
"$binary" self-check >"$run_dir/environment/harness-self-check.txt"
"$workload_dir/fs-benchmark-workload" self-check >"$run_dir/environment/workload-self-check.txt"
printf '1\n' >"$run_dir/environment/self-check-passed.txt"
failed=0
for scenario in "${scenarios[@]}"; do
  scenario_dir="$run_dir/scenarios/$scenario"
  mkdir "$scenario_dir"
  if [[ -n "$fixture_root" ]]; then
    fixture="$fixture_root/$scenario/fixture"
    fixture_manifest_input="$fixture_root/$scenario/fixture-manifest.json"
    [[ -d "$fixture" && -f "$fixture_manifest_input" ]] ||
      die "missing reusable sealed fixture or manifest for $scenario"
    printf 'reused sealed fixture: %s\n' "$fixture" >"$scenario_dir/fixture-supervisor.txt"
    printf '0\n' >"$scenario_dir/fixture-exit-status.txt"
    printf '%s\n' "$fixture" >"$scenario_dir/fixture-source.txt"
    printf '%s  %s\n' "$(sha256_file "$fixture_manifest_input")" "$fixture_manifest_input" \
      >"$scenario_dir/fixture-source-manifest.sha256"
    fixture_mode=reused
  else
    fixture="$scenario_dir/fixture"
    fixture_manifest_input="$scenario_dir/fixture-manifest.raw.json"
    set +e
    if [[ "$(uname -s)" == Darwin ]]; then
      /usr/bin/time -l -p "$binary" namespace-fixture "$fixture" "$scenario" \
        >"$fixture_manifest_input" 2>"$scenario_dir/fixture-supervisor.txt"
    else
      /usr/bin/time -v "$binary" namespace-fixture "$fixture" "$scenario" \
        >"$fixture_manifest_input" 2>"$scenario_dir/fixture-supervisor.txt"
    fi
    fixture_status=$?
    set -e
    printf '%s\n' "$fixture_status" >"$scenario_dir/fixture-exit-status.txt"
    [[ $fixture_status -eq 0 ]] || die "fixture generation failed; evidence retained at $scenario_dir"
    fixture_mode=generated
  fi
  python3 - "$fixture_manifest_input" "$scenario_dir/fixture-supervisor.txt" \
    "$scenario_dir/fixture-manifest.json" "$scenario" "$fixture_mode" <<'PY'
import json
import re
import sys
from decimal import Decimal
from pathlib import Path

input_path, supervisor_path, output_path = map(Path, sys.argv[1:4])
scenario, mode = sys.argv[4:6]
rows = [json.loads(line) for line in input_path.read_text().splitlines() if line.strip()]
if len(rows) != 1:
    raise SystemExit("fixture manifest cardinality")
row = rows[0]
expected = {
    "namespace-100": (100, 1, 125_000_000, 1, 78, 15, 5, 1),
    "namespace-1000": (1_000, 10, 200_000_000, 10, 789, 150, 50, 1),
    "namespace-10000": (10_000, 100, 300_000_000, 100, 7_899, 1_500, 500, 1),
    "namespace-100000": (100_000, 1_000, 500_000_000, 1_000, 78_998, 15_000, 5_000, 2),
}[scenario]
names = (
    "regular_files", "data_directories", "logical_bytes", "empty_files",
    "tiny_files", "small_files", "medium_files", "anchor_files",
)
if (row.get("schema") != "fs-bench-pro-namespace-fixture-v2"
        or row.get("scenario") != scenario
        or row.get("fixture_profile") != "synthetic-small-heavy-v2"
        or row.get("fixture_digest_profile") != "namespace-file-digest-tree-v2"
        or row.get("edit_contract") != "content-only-normalized-mtime-v1"
        or tuple(row.get(name) for name in names) != expected
        or row.get("anchor_bytes") != expected[-1] * 100_000_000
        or (row.get("file_mode"), row.get("directory_mode"),
            row.get("mtime_seconds"), row.get("mtime_nanoseconds"))
        != (0o640, 0o750, 1_700_000_000, 0)):
    raise SystemExit("fixture manifest identity or equation")
for name in ("fixture_digest", "edited_fixture_digest"):
    if not re.fullmatch(r"[0-9a-f]{64}", row.get(name, "")):
        raise SystemExit(f"invalid fixture digest: {name}")
if row["fixture_digest"] == row["edited_fixture_digest"]:
    raise SystemExit("fixture edit digest did not change")
integer_fields = [
    "file_mode", "directory_mode", "mtime_seconds", "mtime_nanoseconds",
    "edit_size", "fixture_plan_ns", "fixture_generate_ns", "fixture_manifest_ns",
    "fixture_files_per_second", "fixture_bytes_per_second", "fixture_worker_count",
    "maximum_fixture_write_buffer_bytes", "fixture_plan_bytes",
    "fixture_path_state_bytes", "fixture_digest_record_bytes", "fixture_open_calls",
    "fixture_write_calls", "fixture_content_bytes_generated",
    "fixture_content_bytes_written", "fixture_content_hash_input_bytes",
    "post_generation_content_rereads", "complete_file_vec_allocations", "per_file_fsyncs",
]
if any(type(row.get(name)) is not int or row[name] < 0 for name in integer_fields):
    raise SystemExit("missing or invalid fixture integer field")
if (not isinstance(row.get("edit_path"), str)
        or row["edit_path"].startswith("/") or ".." in row["edit_path"]
        or row["edit_size"] <= 10
        or row["fixture_worker_count"] != 1
        or row["fixture_cache_profile"] != "generated-warm-uncontrolled"
        or row["maximum_fixture_write_buffer_bytes"] > 1024 * 1024
        or row["fixture_open_calls"] != row["regular_files"]
        or row["fixture_content_bytes_generated"] != row["logical_bytes"]
        or row["fixture_content_bytes_written"] != row["logical_bytes"]
        or row["fixture_content_hash_input_bytes"] != row["logical_bytes"] + row["edit_size"]
        or row["post_generation_content_rereads"] != 0
        or row["complete_file_vec_allocations"] != 0
        or row["per_file_fsyncs"] != 0
        or row.get("atomic_publish") is not True):
    raise SystemExit("fixture streaming or ownership contract")
expected_file_rate = row["regular_files"] * 1_000_000_000 // max(row["fixture_generate_ns"], 1)
expected_byte_rate = row["logical_bytes"] * 1_000_000_000 // max(row["fixture_generate_ns"], 1)
if (row["fixture_files_per_second"] != expected_file_rate
        or row["fixture_bytes_per_second"] != expected_byte_rate):
    raise SystemExit("fixture throughput equation")

if mode == "generated":
    supervisor = supervisor_path.read_text()
    def match(*patterns):
        for pattern in patterns:
            found = re.search(pattern, supervisor, re.MULTILINE)
            if found:
                return found.group(1)
        raise SystemExit(f"missing fixture supervisor metric: {patterns[0]}")
    user = Decimal(match(r"^User time \(seconds\):\s*([0-9.]+)\s*$", r"^user\s+([0-9.]+)\s*$", r"^\s*([0-9.]+)\s+user\s*$"))
    system = Decimal(match(r"^System time \(seconds\):\s*([0-9.]+)\s*$", r"^sys\s+([0-9.]+)\s*$", r"^\s*([0-9.]+)\s+sys\s*$"))
    linux_peak = re.search(r"^Maximum resident set size \(kbytes\):\s*([0-9]+)\s*$", supervisor, re.MULTILINE)
    peak = int(linux_peak.group(1)) * 1024 if linux_peak else int(match(r"^\s*([0-9]+)\s+maximum resident set size\s*$"))
    row["fixture_user_cpu_ns"] = int(user * 1_000_000_000)
    row["fixture_system_cpu_ns"] = int(system * 1_000_000_000)
    row["fixture_peak_rss_bytes"] = peak
else:
    for name in ("fixture_user_cpu_ns", "fixture_system_cpu_ns", "fixture_peak_rss_bytes"):
        if type(row.get(name)) is not int or row[name] < 0:
            raise SystemExit(f"reused manifest lacks {name}")
row["metric_sources"] = {
    "fixture_wall_and_ownership": "fixture generator counters",
    "fixture_process_resources": "raw OS /usr/bin/time supervisor output",
    "fixture_zero_contracts": "sealed generator has no post-generation content reads, duplicate file Vec, or per-file fsync",
}
output_path.write_text(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
PY
  printf '%s  %s\n' "$(sha256_file "$scenario_dir/fixture-manifest.json")" \
    "$scenario_dir/fixture-manifest.json" >"$scenario_dir/fixture-manifest.sha256"

  IFS=$'\t' read -r fixture_digest edited_fixture_digest edit_path edit_size < <(
    python3 - "$scenario_dir/fixture-manifest.json" <<'PY'
import json, sys
row = json.load(open(sys.argv[1]))
print(row["fixture_digest"], row["edited_fixture_digest"], row["edit_path"], row["edit_size"], sep="\t")
PY
  )

  for ((iteration = 1; iteration <= iterations; iteration++)); do
    if [[ "$measurement_mode" == product ]]; then ensure_daemon_running; fi
    if [[ $iteration -eq 1 ]]; then
      sample_cache_profile="${fixture_mode}-first-use-uncontrolled"
    else
      sample_cache_profile="${fixture_mode}-post-first-use-uncontrolled"
    fi
    sample_dir=$(printf '%s/sample-%03d' "$scenario_dir" "$iteration")
    mkdir "$sample_dir" "$sample_dir/raw"
    printf '%s\n' "$daemon_endpoint" >"$sample_dir/daemon-endpoint.txt"
    diagnostic_nonce=$(od -An -tx1 -N16 /dev/urandom | tr -d ' \n')
    [[ "$diagnostic_nonce" =~ ^[0-9a-f]+$ ]] || die "diagnostic nonce generation failed"
    ! grep -Fxq "$diagnostic_nonce" "$nonce_registry" || die "duplicate diagnostic nonce"
    printf '%s\n' "$diagnostic_nonce" >>"$nonce_registry"
    printf '%s\n' "$diagnostic_nonce" >"$sample_dir/diagnostic-nonce.txt"
    manifest_sha=$(sha256_file "$scenario_dir/fixture-manifest.json")
    [[ "$manifest_sha" == "$(awk '{print $1}' "$scenario_dir/fixture-manifest.sha256")" ]] ||
      die "fixture manifest changed before sample"
    python3 - "$fixture" "$scenario_dir/fixture-manifest.json" \
      "$sample_dir/raw/fixture-custody.json" "$manifest_sha" <<'PY'
import json
import os
import stat
import sys
from pathlib import Path

fixture, manifest_path, output_path = map(Path, sys.argv[1:4])
manifest_sha = sys.argv[4]
manifest = json.loads(manifest_path.read_text())
metadata = os.stat(fixture, follow_symlinks=False)
if (not stat.S_ISDIR(metadata.st_mode)
        or stat.S_IMODE(metadata.st_mode) != manifest["directory_mode"]
        or metadata.st_mtime_ns != manifest["mtime_seconds"] * 1_000_000_000 + manifest["mtime_nanoseconds"]):
    raise SystemExit("fixture root custody metadata mismatch")
output_path.write_text(json.dumps({
    "fixture_manifest_sha256": manifest_sha,
    "fixture_profile": manifest["fixture_profile"],
    "fixture_digest_profile": manifest["fixture_digest_profile"],
    "fixture_root_mode": stat.S_IMODE(metadata.st_mode),
    "fixture_root_mtime_ns": metadata.st_mtime_ns,
    "host_fixture_os_read_only_mount": False,
    "host_fixture_owner_writable": bool(metadata.st_mode & stat.S_IWUSR),
    "full_content_verification": False,
}, sort_keys=True, separators=(",", ":")) + "\n")
PY
    if [[ "$measurement_mode" == product ]]; then
      benchmark_command=(
        env
        LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload
        LAYERFS_EXEC_TRANSPORT=daemon
        LAYERFS_FUSE_TRANSPORT=daemon
        LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint"
        LAYERFS_DAEMON_CAPABILITY="$daemon_capability"
        LAYERFS_DAEMON_CONTAINER_ID="$container_id"
        LAYERFS_FUSE_HOST=host.docker.internal
        LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE="$diagnostic_nonce"
        "$binary" namespace "$sample_dir/work" "$fixture" "$container_id" "$scenario" "$iteration" \
          "$fixture_digest" "$edited_fixture_digest" "$edit_path" "$edit_size" "$sample_cache_profile"
      )
    else
      benchmark_command=(
        env
        LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE="$diagnostic_nonce"
        "$binary" namespace-init-diagnostic "$sample_dir/work" "$fixture" "$scenario" "$iteration" \
          "$fixture_digest" "$sample_cache_profile"
      )
    fi
    set +e
    if [[ "$(uname -s)" == Darwin ]]; then
      /usr/bin/time -l -p "${benchmark_command[@]}" \
        >"$sample_dir/raw/namespace.jsonl" 2>"$sample_dir/raw/supervisor.txt"
    else
      /usr/bin/time -v "${benchmark_command[@]}" \
        >"$sample_dir/raw/namespace.jsonl" 2>"$sample_dir/raw/supervisor.txt"
    fi
    status=$?
    set -e
    printf '%s\n' "$status" >"$sample_dir/exit-status.txt"
    docker logs "$container" >"$sample_dir/raw/container.log" 2>&1 || true
    docker inspect -f '{{json .State}}' "$container" >"$sample_dir/raw/container-state-after.json"
    cleanup_status=0
    if [[ "$(docker inspect -f '{{.State.Running}}' "$container")" == true ]]; then
      docker exec "$container" cat /proc/mounts >"$sample_dir/raw/container-mounts-after.txt" 2>&1 || cleanup_status=1
      docker exec "$container" ps -ef >"$sample_dir/raw/container-processes-after.txt" 2>&1 || cleanup_status=1
      if grep -Fq "/workspace/layerfs-$scenario-$iteration-" "$sample_dir/raw/container-mounts-after.txt" \
        || grep -Fq "/usr/local/bin/layerfs-fuse " "$sample_dir/raw/container-processes-after.txt"; then
        cleanup_status=1
      fi
    else
      printf 'container stopped; no mount namespace remains\n' >"$sample_dir/raw/container-mounts-after.txt"
      printf 'container stopped; no process namespace remains\n' >"$sample_dir/raw/container-processes-after.txt"
      [[ "$(docker inspect -f '{{.State.ExitCode}} {{.State.OOMKilled}}' "$container")" == "0 false" ]] || cleanup_status=1
    fi
    printf '%s\n' "$cleanup_status" >"$sample_dir/cleanup-exit-status.txt"
    if [[ $cleanup_status -ne 0 ]]; then
      printf 'container mount or helper cleanup failed\n' >"$sample_dir/CLEANUP_FAILED"
      failed=1
    fi
    if [[ $status -ne 0 ]]; then
      failed=1
    fi
    set +e
    python3 - "$sample_dir/raw/namespace.jsonl" "$sample_dir/raw/supervisor.txt" \
      "$scenario_dir/fixture-manifest.json" "$sample_dir/raw/container-state-after.json" \
      "$sample_dir/result.json" \
      "$status" "$scenario" "$iteration" "$diagnostic_nonce" "$measurement_mode" \
      "$sample_dir/raw/fixture-custody.json" "$cleanup_status" <<'PY'
import json
import re
import sys
from decimal import Decimal
from pathlib import Path

raw_path, supervisor_path, fixture_path, container_state_path, output_path = map(Path, sys.argv[1:6])
status, scenario, iteration, diagnostic_nonce, expected_mode = (
    int(sys.argv[6]), sys.argv[7], int(sys.argv[8]), sys.argv[9], sys.argv[10]
)
custody = json.loads(Path(sys.argv[11]).read_text())
cleanup_status = int(sys.argv[12])
if cleanup_status not in {0, 1}:
    raise SystemExit("invalid cleanup status")
rows = [json.loads(line) for line in raw_path.read_text().splitlines() if line.strip()]
fixture_rows = [json.loads(line) for line in fixture_path.read_text().splitlines() if line.strip()]
if (len(fixture_rows) != 1
        or fixture_rows[0].get("schema") != "fs-bench-pro-namespace-fixture-v2"
        or fixture_rows[0].get("scenario") != scenario):
    raise SystemExit("fixture manifest cardinality")
fixture = fixture_rows[0]
if status == 0:
    if len(rows) != 1 or rows[0].get("schema") != "fs-bench-pro-namespace-v3":
        raise SystemExit("namespace row cardinality or schema")
    row = rows[0]
else:
    if len(rows) > 1 or (rows and rows[0].get("schema") != "fs-bench-pro-namespace-failure-v3"):
        raise SystemExit("namespace failure row cardinality or schema")
    row = rows[0] if rows else {
        "schema": "fs-bench-pro-namespace-supervised-failure-v3",
        "scenario": scenario,
        "iteration": iteration,
        "measurement_mode": expected_mode,
        "failed_phase": "before-structured-failure",
    }
    row["exit_status"] = status
if row.get("scenario") != scenario or row.get("iteration") != iteration:
    raise SystemExit("namespace scenario or iteration identity")
row["cleanup_status"] = "pass" if cleanup_status == 0 else "fail"
common_integer_fields = [
    "setup_ns", "layerstack_init_ns", "init_bytes_per_second", "init_files_per_second",
    "regular_files", "data_directories", "logical_bytes", "empty_files", "tiny_files",
    "small_files", "medium_files", "anchor_files", "anchor_bytes", "file_mode",
    "directory_mode", "mtime_seconds", "mtime_nanoseconds",
    "scanned_files", "scanned_bytes", "candidate_objects", "candidate_bytes",
    "inserted_objects", "inserted_bytes", "reused_objects", "reused_bytes",
    "store_baseline_bytes", "store_database_bytes", "store_growth_bytes",
    "store_canonical_objects", "store_canonical_bytes",
]
if status == 0:
    expected_row_mode = "product-lifecycle" if expected_mode == "product" else expected_mode
    if row.get("measurement_mode") != expected_row_mode:
        raise SystemExit("namespace measurement mode")
    expected_profile = (
        "commit-head-reopen-ready-v1"
        if expected_mode == "product"
        else "initialization-only-diagnostic-v1"
    )
    if row.get("result_profile") != expected_profile:
        raise SystemExit("namespace result profile")
    if expected_mode == "init-only-diagnostic" and row.get("nonterminal") is not True:
        raise SystemExit("init-only result must be nonterminal")
    integer_fields = common_integer_fields.copy()
    if expected_mode == "product":
        phase_names = [
            "layerstack_init_ns", "branch_fork_ns", "workspace_create_ns", "edit_ns",
            "commit_ns", "workspace_end_ns", "reconnect_ns", "reopen_workspace_create_ns",
            "reopen_workspace_end_ns",
        ]
        integer_fields += phase_names[1:] + [
            "product_lifecycle_ns", "edit_size", "max_transaction_objects",
            "max_transaction_bytes", "initialize_candidate_objects",
            "initialize_candidate_bytes", "initialize_inserted_objects",
            "initialize_inserted_bytes", "initialize_reused_objects",
            "initialize_reused_bytes", "initialize_batch_inserted_objects",
            "initialize_batch_inserted_bytes", "initialize_final_inserted_objects",
            "initialize_final_inserted_bytes", "initialize_preexisting_reused_objects",
            "initialize_preexisting_reused_bytes", "initialize_admission_transactions",
            "initialize_max_transaction_objects", "initialize_max_transaction_bytes",
            "commit_candidate_objects", "commit_candidate_bytes", "commit_inserted_objects",
            "commit_inserted_bytes", "commit_reused_objects", "commit_reused_bytes",
            "commit_admission_transactions", "commit_max_transaction_objects",
            "commit_max_transaction_bytes",
        ]
    else:
        integer_fields += [
            "teardown_ns", "initialize_batch_inserted_objects",
            "initialize_batch_inserted_bytes", "initialize_final_inserted_objects",
            "initialize_final_inserted_bytes", "initialize_preexisting_reused_objects",
            "initialize_preexisting_reused_bytes", "initialize_admission_transactions",
            "initialize_max_transaction_objects", "initialize_max_transaction_bytes",
        ]
    if any(type(row.get(name)) is not int or row[name] < 0 for name in integer_fields):
        raise SystemExit("missing or invalid namespace integer field")
    if expected_mode == "product" and sum(row[name] for name in phase_names) != row["product_lifecycle_ns"]:
        raise SystemExit("namespace phase equation")
    if (row.get("fixture_profile") != "synthetic-small-heavy-v2"
            or row.get("fixture_digest_profile") != "namespace-file-digest-tree-v2"
            or row.get("edit_contract") != "content-only-normalized-mtime-v1"
            or row.get("fixture_cache_profile") not in {
                "generated-first-use-uncontrolled", "generated-post-first-use-uncontrolled",
                "reused-first-use-uncontrolled", "reused-post-first-use-uncontrolled",
            }):
        raise SystemExit("namespace profile identity")
    for name in [
        "regular_files", "data_directories", "logical_bytes", "empty_files", "tiny_files",
        "small_files", "medium_files", "anchor_files", "anchor_bytes",
        "file_mode", "directory_mode", "mtime_seconds", "mtime_nanoseconds",
        "fixture_digest", "edit_contract",
    ]:
        if row.get(name) != fixture.get(name):
            raise SystemExit(f"fixture mismatch: {name}")
    if expected_mode == "product":
        for name in ("edit_path", "edit_size"):
            if row.get(name) != fixture.get(name):
                raise SystemExit(f"fixture mismatch: {name}")
    if row["scanned_files"] != row["regular_files"] or row["scanned_bytes"] != row["logical_bytes"]:
        raise SystemExit("initialization scan receipt mismatch")
    if row["candidate_objects"] != row["inserted_objects"] + row["reused_objects"]:
        raise SystemExit("candidate object equation")
    if row["candidate_bytes"] != row["inserted_bytes"] + row["reused_bytes"]:
        raise SystemExit("candidate byte equation")
    if expected_mode == "product":
        for prefix in ("initialize", "commit"):
            if row[f"{prefix}_candidate_objects"] != row[f"{prefix}_inserted_objects"] + row[f"{prefix}_reused_objects"]:
                raise SystemExit(f"{prefix} candidate object equation")
            if row[f"{prefix}_candidate_bytes"] != row[f"{prefix}_inserted_bytes"] + row[f"{prefix}_reused_bytes"]:
                raise SystemExit(f"{prefix} candidate byte equation")
        if (row["candidate_objects"] != row["initialize_candidate_objects"] + row["commit_candidate_objects"]
                or row["candidate_bytes"] != row["initialize_candidate_bytes"] + row["commit_candidate_bytes"]
                or row["inserted_objects"] != row["initialize_inserted_objects"] + row["commit_inserted_objects"]
                or row["inserted_bytes"] != row["initialize_inserted_bytes"] + row["commit_inserted_bytes"]
                or row["reused_objects"] != row["initialize_reused_objects"] + row["commit_reused_objects"]
                or row["reused_bytes"] != row["initialize_reused_bytes"] + row["commit_reused_bytes"]):
            raise SystemExit("combined candidate equation")
        initialize_inserted_objects = row["initialize_inserted_objects"]
        initialize_inserted_bytes = row["initialize_inserted_bytes"]
        initialize_reused_objects = row["initialize_reused_objects"]
        initialize_reused_bytes = row["initialize_reused_bytes"]
    else:
        initialize_inserted_objects = row["inserted_objects"]
        initialize_inserted_bytes = row["inserted_bytes"]
        initialize_reused_objects = row["reused_objects"]
        initialize_reused_bytes = row["reused_bytes"]
    if (initialize_inserted_objects
            != row["initialize_batch_inserted_objects"] + row["initialize_final_inserted_objects"]
            or initialize_inserted_bytes
            != row["initialize_batch_inserted_bytes"] + row["initialize_final_inserted_bytes"]
            or initialize_reused_objects != row["initialize_preexisting_reused_objects"]
            or initialize_reused_bytes != row["initialize_preexisting_reused_bytes"]):
        raise SystemExit("initialization candidate detail equation")
    if (row["initialize_max_transaction_objects"] >= 8192
            or row["initialize_max_transaction_bytes"] >= 4 * 1024 * 1024):
        raise SystemExit("phase candidate transaction bound")
    if expected_mode == "product" and (
            row["max_transaction_objects"] >= 8192
            or row["max_transaction_bytes"] >= 4 * 1024 * 1024
            or row["commit_max_transaction_objects"] >= 128
            or row["commit_max_transaction_bytes"] >= 4 * 1024 * 1024):
        raise SystemExit("candidate transaction bound")
    if not re.fullmatch(r"[0-9a-f]{64}", row["fixture_digest"]):
        raise SystemExit("invalid fixture digest")
    if (row["init_bytes_per_second"]
            != row["logical_bytes"] * 1_000_000_000 // max(row["layerstack_init_ns"], 1)
            or row["init_files_per_second"]
            != row["regular_files"] * 1_000_000_000 // max(row["layerstack_init_ns"], 1)):
        raise SystemExit("namespace initialization rate equation")
    if (row["store_database_bytes"] < row["store_baseline_bytes"]
            or row["store_growth_bytes"] != row["store_database_bytes"] - row["store_baseline_bytes"]):
        raise SystemExit("namespace Store growth equation")
    targets = {
        "namespace-100": (625_000_000, 15_000_000),
        "namespace-1000": (1_000_000_000, 18_000_000),
        "namespace-10000": (1_500_000_000, 22_000_000),
        "namespace-100000": (2_500_000_000, 25_000_000),
    }[scenario]
    row["target_outcomes"] = {
        "init_absolute": row["layerstack_init_ns"] <= targets[0],
        "init_throughput": row["init_bytes_per_second"] >= 200_000_000,
        "init_file_rate": scenario != "namespace-100000" or row["init_files_per_second"] >= 40_000,
    }
    if expected_mode == "product":
        row["target_outcomes"].update({
            "workspace_create": row["workspace_create_ns"] <= targets[1],
            "commit": row["commit_ns"] <= 10_000_000,
        })
        row["binding_targets_pass"] = all(row["target_outcomes"].values())
    else:
        row["binding_targets_pass"] = False
        row["diagnostic_nonterminal"] = True
else:
    for name in [
        "regular_files", "data_directories", "logical_bytes", "empty_files", "tiny_files",
        "small_files", "medium_files", "anchor_files", "anchor_bytes", "file_mode",
        "directory_mode", "mtime_seconds", "mtime_nanoseconds", "fixture_digest",
        "edit_contract",
    ]:
        if name in row and row[name] != fixture.get(name):
            raise SystemExit(f"failure fixture mismatch: {name}")
    if "scanned_files" in row and row["scanned_files"] != fixture.get("regular_files"):
        raise SystemExit("failure scan file mismatch")
    if "scanned_bytes" in row and row["scanned_bytes"] != fixture.get("logical_bytes"):
        raise SystemExit("failure scan byte mismatch")
supervisor = supervisor_path.read_text()
diagnostic_schema = "layerfs-initialization-diagnostic-v1"
diagnostic_names = [
    "nonce", "fast_path", "worker_count", "prepare_import_wall_ns",
    "source_file_open_calls", "source_file_read_calls", "source_file_read_bytes",
    "source_symlink_metadata_calls", "source_read_dir_calls", "single_chunk_files",
    "streaming_files", "cdc_scratch_peak_bytes", "explicit_buffer_peak_bytes", "canonical_frame_count",
    "canonical_payload_bytes", "canonical_framing_bytes", "object_segment_write_calls",
    "object_segment_write_bytes", "object_segment_raw_read_calls",
    "object_segment_raw_read_bytes", "object_segment_passes", "pair_segment_write_calls",
    "pair_segment_write_bytes", "pair_segment_raw_read_calls", "pair_segment_raw_read_bytes",
    "pair_segment_passes", "parent_merge_bytes", "pending_duplicate_objects",
    "pending_duplicate_bytes", "cross_batch_skipped_objects", "cross_batch_skipped_bytes",
    "collision_checks", "admission_batch_peak_objects", "admission_batch_peak_payload_bytes",
    "admission_batch_peak_vec_capacity", "pending_index_peak_entries", "sql_batch_count",
    "sql_row_count_shape_count", "sql_submitted_rows", "sql_returned_ids", "sql_skipped_ids",
    "sql_string_build_ns", "sql_prepare_ns", "sql_bind_step_returning_ns",
    "conflict_read_calls", "conflict_read_rows", "conflict_read_bytes", "conflict_read_ns",
    "sql_begin_ns", "sql_commit_ns", "final_root_inode_table_wall_ns",
    "insert_node_peak_len", "insert_node_peak_capacity",
]
diagnostic_lines = [
    line for line in supervisor.splitlines()
    if line == diagnostic_schema or line.startswith(diagnostic_schema + " ")
]
if len(diagnostic_lines) > 1 or (status == 0 and len(diagnostic_lines) != 1):
    raise SystemExit("initialization diagnostic cardinality")
if diagnostic_lines:
    diagnostic = {}
    for token in diagnostic_lines[0].split()[1:]:
        if "=" not in token:
            raise SystemExit("malformed initialization diagnostic field")
        name, value = token.split("=", 1)
        if name in diagnostic:
            raise SystemExit(f"duplicate initialization diagnostic field: {name}")
        diagnostic[name] = value
    if set(diagnostic) != set(diagnostic_names):
        missing = sorted(set(diagnostic_names) - set(diagnostic))
        unknown = sorted(set(diagnostic) - set(diagnostic_names))
        raise SystemExit(f"initialization diagnostic fields missing={missing} unknown={unknown}")
    if diagnostic["nonce"] != diagnostic_nonce or not re.fullmatch(r"[0-9a-fA-F]+", diagnostic["nonce"]):
        raise SystemExit("initialization diagnostic nonce")
    optional_na = {"parent_merge_bytes", "insert_node_peak_len", "insert_node_peak_capacity"}
    for name in diagnostic_names:
        if name == "nonce" or (name in optional_na and diagnostic[name] == "na"):
            continue
        if not re.fullmatch(r"[0-9]+", diagnostic[name]) or int(diagnostic[name]) > 2**64 - 1:
            raise SystemExit(f"invalid initialization diagnostic u64: {name}")
        diagnostic[name] = int(diagnostic[name])
    if diagnostic["fast_path"] not in {0, 1}:
        raise SystemExit("initialization diagnostic fast_path")
    if diagnostic["fast_path"] == 1 and diagnostic["parent_merge_bytes"] != 0:
        raise SystemExit("fast-path parent merge must be zero")
    row["initialization_diagnostic_schema"] = diagnostic_schema
    row["initialization_diagnostic"] = diagnostic.copy()
    row.update(diagnostic)
    if status == 0 and expected_mode == "product":
        row["target_outcomes"]["append_fast_path"] = (
            scenario == "namespace-100"
            or (diagnostic["fast_path"] == 1 and diagnostic["parent_merge_bytes"] == 0)
        )
        row["binding_targets_pass"] = all(row["target_outcomes"].values())
def match(*patterns):
    for pattern in patterns:
        found = re.search(pattern, supervisor, re.MULTILINE)
        if found:
            return found.group(1)
    raise SystemExit(f"missing supervisor metric: {patterns[0]}")
user = Decimal(match(r"^User time \(seconds\):\s*([0-9.]+)\s*$", r"^user\s+([0-9.]+)\s*$", r"^\s*([0-9.]+)\s+user\s*$"))
system = Decimal(match(r"^System time \(seconds\):\s*([0-9.]+)\s*$", r"^sys\s+([0-9.]+)\s*$", r"^\s*([0-9.]+)\s+sys\s*$"))
linux_peak = re.search(r"^Maximum resident set size \(kbytes\):\s*([0-9]+)\s*$", supervisor, re.MULTILINE)
peak = int(linux_peak.group(1)) * 1024 if linux_peak else int(match(r"^\s*([0-9]+)\s+maximum resident set size\s*$"))
row["whole_supervised_user_cpu_ns"] = int(user * 1_000_000_000)
row["whole_supervised_system_cpu_ns"] = int(system * 1_000_000_000)
row["whole_supervised_peak_rss_bytes"] = peak
container_state = json.loads(container_state_path.read_text())
oom_killed = container_state.get("OOMKilled")
if type(oom_killed) is not bool:
    raise SystemExit("container OOM state unavailable")
row["container_oom_killed"] = oom_killed
row["fixture_custody"] = custody
if status == 0 and oom_killed:
    raise SystemExit("successful sample reported container OOM")
filesystem_inputs = re.search(r"^File system inputs:\s*([0-9]+)\s*$", supervisor, re.MULTILINE)
filesystem_outputs = re.search(r"^File system outputs:\s*([0-9]+)\s*$", supervisor, re.MULTILINE)
if filesystem_inputs and filesystem_outputs:
    row["whole_supervised_filesystem_inputs"] = int(filesystem_inputs.group(1))
    row["whole_supervised_filesystem_outputs"] = int(filesystem_outputs.group(1))
row["metric_sources"] = {
    "phase_wall": "harness Instant boundaries",
    "whole_supervised_resources": "raw OS /usr/bin/time around the complete process; not product-only",
    "product_only_resources": "unavailable",
    "fixture": "sealed deterministic fixture manifest plus per-sample root metadata and manifest SHA; no content reread",
    "initialization_scan": "LayerStack initialization receipt",
    "candidate_storage": "LayerFS operation receipts present in the selected measurement mode",
    "initialization_diagnostic": "nonce-bound private LayerFS initialization stderr frame",
    "store_growth": "LayerStackStore storage and canonical-storage snapshots",
    "container_oom": "Docker container state after sample",
}
output_path.write_text(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
PY
    validation_status=$?
    set -e
    if [[ $validation_status -ne 0 ]]; then
      printf '%s\n' "$validation_status" >"$sample_dir/validation-exit-status.txt"
      failed=1
    else
      printf '0\n' >"$sample_dir/validation-exit-status.txt"
    fi
  done
done

ending_seal=$(seal source)
printf '%s\n' "$ending_seal" >"$run_dir/environment/ending-source-seal.sha256"
if [[ "$ending_seal" != "$current_seal" ]]; then
  printf 'source changed during campaign\n' >"$run_dir/INVALID"
  failed=1
fi
python3 - "$run_dir" >"$run_dir/report.md" <<'PY'
import json
import statistics
import sys
from pathlib import Path

root = Path(sys.argv[1])
print("# LayerFS namespace-v3 lifecycle campaign\n")
print("## Samples\n")
print("| Scenario | Sample | Mode | Fixture / digest profile | Cache profile | Valid | Binding status | Init ns | Init B/s | Files/s | Create ns | Commit ns | Product lifecycle ns | Whole-supervised peak RSS | Fast path | Parent merge | Object segment W/R | Admission peak | Fixture RO mount | Store growth |")
print("| --- | ---: | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | ---: |")
rows = []
samples = sorted(root.glob("scenarios/namespace-*/sample-*"))
for sample in samples:
    status = sample.joinpath("exit-status.txt").read_text().strip()
    result = sample / "result.json"
    if result.is_file():
        row = json.loads(result.read_text())
        valid = (
            status == "0"
            and sample.joinpath("validation-exit-status.txt").read_text().strip() == "0"
            and sample.joinpath("cleanup-exit-status.txt").read_text().strip() == "0"
        )
        if valid:
            rows.append(row)
        target = "nonterminal" if row.get("measurement_mode") == "init-only-diagnostic" else "pass" if row.get("binding_targets_pass") else "miss"
        identity = f"{row.get('fixture_profile', '—')} / {row.get('fixture_digest_profile', '—')}"
        segment_io = f"{row.get('object_segment_write_bytes', '—')} / {row.get('object_segment_raw_read_bytes', '—')}"
        admission_peak = f"{row.get('admission_batch_peak_objects', '—')} / {row.get('admission_batch_peak_payload_bytes', '—')} B"
        fixture_ro = row.get("fixture_custody", {}).get("host_fixture_os_read_only_mount", "—")
        print(f"| {row['scenario']} | {row['iteration']} | {row.get('measurement_mode', '—')} | {identity} | {row.get('fixture_cache_profile', '—')} | {'yes' if valid else 'no'} | {target if valid else '—'} | {row.get('layerstack_init_ns', '—')} | {row.get('init_bytes_per_second', '—')} | {row.get('init_files_per_second', '—')} | {row.get('workspace_create_ns', '—')} | {row.get('commit_ns', '—')} | {row.get('product_lifecycle_ns', '—')} | {row['whole_supervised_peak_rss_bytes']} | {row.get('fast_path', '—')} | {row.get('parent_merge_bytes', '—')} | {segment_io} | {admission_peak} | {fixture_ro} | {row.get('store_growth_bytes', '—')} |")
    else:
        print(f"| {sample.parent.name} | {int(sample.name.split('-')[-1])} | — | — | — | no ({status}) | — | — | — | — | — | — | — | — | — | — | — | — | — | — |")

product_rows = [row for row in rows if row.get("measurement_mode") == "product-lifecycle"]
diagnostic_rows = [row for row in rows if row.get("measurement_mode") == "init-only-diagnostic"]

print("\n## Fixture preparation\n")
print("| Scenario | Fixture profile | Digest profile | Edit contract | Generation cache profile | Plan ns | Generate ns | Manifest ns | Files/s | Bytes/s | Peak RSS | Worker count | Write buffer | Plan bytes | Path bytes | Digest bytes |")
print("| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
for manifest in sorted(root.glob("scenarios/namespace-*/fixture-manifest.json")):
    row = json.loads(manifest.read_text())
    print(f"| {row['scenario']} | {row['fixture_profile']} | {row['fixture_digest_profile']} | {row['edit_contract']} | {row['fixture_cache_profile']} | {row['fixture_plan_ns']} | {row['fixture_generate_ns']} | {row['fixture_manifest_ns']} | {row['fixture_files_per_second']} | {row['fixture_bytes_per_second']} | {row['fixture_peak_rss_bytes']} | {row['fixture_worker_count']} | {row['maximum_fixture_write_buffer_bytes']} | {row['fixture_plan_bytes']} | {row['fixture_path_state_bytes']} | {row['fixture_digest_record_bytes']} |")

print("\n## Initialization diagnostics\n")
for row in rows:
    print(f"### {row['scenario']} sample {row['iteration']}\n")
    print("```json")
    print(json.dumps({
        "schema": row["initialization_diagnostic_schema"],
        **row["initialization_diagnostic"],
    }, sort_keys=True, separators=(",", ":")))
    print("```\n")
if diagnostic_rows:
    print(f"{len(diagnostic_rows)} init-only diagnostic rows are nonterminal and excluded from every binding median and PASS decision.\n")

print("\n## Medians (never pooled across cache profiles)\n")
print("| Scenario | Fixture profile | Digest profile | Edit contract | Cache profile | Samples | Init ns | Init B/s | Files/s | Create ns | Commit ns | Product lifecycle ns | Whole-supervised peak RSS | Binding medians |")
print("| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |")
groups = {}
for row in product_rows:
    groups.setdefault((
        row["scenario"], row["fixture_profile"], row["fixture_digest_profile"],
        row["edit_contract"], row["fixture_cache_profile"],
    ), []).append(row)
medians = {}
limits = {
    "namespace-100": (625_000_000, 15_000_000),
    "namespace-1000": (1_000_000_000, 18_000_000),
    "namespace-10000": (1_500_000_000, 22_000_000),
    "namespace-100000": (2_500_000_000, 25_000_000),
}
for key, group in sorted(groups.items()):
    def median(name):
        return int(statistics.median_high(sorted(row[name] for row in group)))
    values = {name: median(name) for name in (
        "layerstack_init_ns", "init_bytes_per_second", "init_files_per_second",
        "workspace_create_ns", "commit_ns", "product_lifecycle_ns",
        "whole_supervised_peak_rss_bytes",
    )}
    scenario, fixture_profile, digest_profile, edit_contract, cache_profile = key
    absolute, create = limits[scenario]
    passed = (
        len(group) >= 3
        and (
            scenario == "namespace-100"
            or all(row["fast_path"] == 1 and row["parent_merge_bytes"] == 0 for row in group)
        )
        and values["layerstack_init_ns"] <= absolute
        and values["init_bytes_per_second"] >= 200_000_000
        and (scenario != "namespace-100000" or values["init_files_per_second"] >= 40_000)
        and values["workspace_create_ns"] <= create
        and values["commit_ns"] <= 10_000_000
    )
    medians[key] = (values, passed)
    print(f"| {scenario} | {fixture_profile} | {digest_profile} | {edit_contract} | {cache_profile} | {len(group)} | {values['layerstack_init_ns']} | {values['init_bytes_per_second']} | {values['init_files_per_second']} | {values['workspace_create_ns']} | {values['commit_ns']} | {values['product_lifecycle_ns']} | {values['whole_supervised_peak_rss_bytes']} | {'pass' if passed else 'miss/incomplete'} |")

profiles = {tuple(identity) for _, *identity in medians}
matrix_pass = False
adjacent_requirements = []
for identity in sorted(profiles):
    order = ["namespace-100", "namespace-1000", "namespace-10000", "namespace-100000"]
    if all((scenario, *identity) in medians for scenario in order):
        times = [medians[(scenario, *identity)][0]["layerstack_init_ns"] for scenario in order]
        ratios = [times[index + 1] / times[index] for index in range(3)]
        ratio_pass = all(ratio <= 2.0 for ratio in ratios)
        identity_pass = ratio_pass and all(medians[(scenario, *identity)][1] for scenario in order)
        matrix_pass = matrix_pass or identity_pass
        fixture_profile, digest_profile, edit_contract, cache_profile = identity
        print(f"\nAdjacent init ratios for {fixture_profile}/{digest_profile}/{edit_contract}/{cache_profile}: " + ", ".join(f"{ratio:.3f}x" for ratio in ratios) + f"; {'pass' if ratio_pass else 'miss'}.")
        adjacent_requirement = {
            "fixture_profile": fixture_profile,
            "fixture_digest_profile": digest_profile,
            "edit_contract": edit_contract,
            "fixture_cache_profile": cache_profile,
            "namespace_10000_median_ns": times[2],
            "namespace_100000_max_ns": times[2] * 2,
            "namespace_100000_median_ns": times[3],
            "pass": times[3] <= times[2] * 2,
        }
        adjacent_requirements.append(adjacent_requirement)
        print(
            "Derived 100k adjacent requirement: "
            f"actual 10k median {times[2]} ns -> 100k must be <= {times[2] * 2} ns; "
            f"actual 100k median {times[3]} ns; "
            f"{'pass' if adjacent_requirement['pass'] else 'miss'}. No tier is delayed."
        )
root.joinpath("binding-targets-passed.txt").write_text("1\n" if matrix_pass else "0\n")

correctness_pass = bool(samples) and root.joinpath(
    "environment/self-check-passed.txt"
).read_text().strip() == "1" and not root.joinpath("INVALID").exists() and all(
    sample.joinpath("exit-status.txt").read_text().strip() == "0"
    and sample.joinpath("validation-exit-status.txt").read_text().strip() == "0"
    for sample in samples
)
setup_pass = root.joinpath("environment/self-check-passed.txt").read_text().strip() == "1" and all(
    scenario.joinpath("fixture-exit-status.txt").read_text().strip() == "0"
    and scenario.joinpath("fixture-manifest.json").is_file()
    for scenario in root.glob("scenarios/namespace-*")
) and all(sample.joinpath("raw/fixture-custody.json").is_file() for sample in samples)
cleanup_pass = bool(samples) and all(
    sample.joinpath("cleanup-exit-status.txt").read_text().strip() == "0"
    for sample in samples
)
product_pass = correctness_pass and cleanup_pass if product_rows else None
verification_pass = None
diagnostic_pass = correctness_pass and cleanup_pass if diagnostic_rows else None
quality_groups = {}
for row in product_rows:
    cache_profile = row["fixture_cache_profile"]
    cache_origin = cache_profile.split("-", 1)[0]
    key = (
        row["scenario"], row["fixture_profile"], row["fixture_digest_profile"],
        row["edit_contract"], cache_origin,
    )
    state = "post" if "-post-first-use-" in cache_profile else "first"
    quality_groups.setdefault(key, {"first": [], "post": []})[state].append(row)
quality_pass = (
    {key[0] for key in quality_groups} == set(limits)
    and all(len(group["first"]) == 1 and len(group["post"]) >= 3
            for group in quality_groups.values())
)
unavailable = [
    "T0 baseline and incremental RSS",
    "phase CPU and <=5% baseline regression",
    "canonical hash and copy-ownership counters",
    "SQLite page/write-amplification and Store physical-I/O counters",
    "phase physical I/O and page-cache/cgroup attribution",
    "cgroup memory.current/peak/events and swap",
    "normal-overwrite mtime diagnostic for real-workspace extrapolation",
    "product-only CPU, RSS, and physical I/O",
]
status = {
    "setup_pass": setup_pass,
    "product_pass": product_pass,
    "verification_pass": verification_pass,
    "diagnostic_pass": diagnostic_pass,
    "performance_pass": matrix_pass,
    "evidence_pass": False,
    "resource_pass": None,
    "correctness_pass": correctness_pass,
    "cleanup_pass": cleanup_pass,
    "quality_pass": quality_pass,
    "unavailable_required_evidence": unavailable,
    "adjacent_ratio_requirements": adjacent_requirements,
}
status["evidence_pass"] = all(status[name] is True for name in (
    "setup_pass", "product_pass", "performance_pass", "resource_pass",
    "correctness_pass", "cleanup_pass", "quality_pass"
)) and not unavailable
root.joinpath("run-status.json").write_text(
    json.dumps(status, sort_keys=True, separators=(",", ":")) + "\n"
)
for name in ("setup", "performance", "evidence", "correctness", "cleanup", "quality"):
    root.joinpath(f"{name}-pass.txt").write_text(
        ("1" if status[f"{name}_pass"] else "0") + "\n"
    )
root.joinpath("resource-pass.txt").write_text("unavailable\n")
root.joinpath("product-pass.txt").write_text("not-run\n" if product_pass is None else ("1\n" if product_pass else "0\n"))
root.joinpath("verification-pass.txt").write_text("not-run\n")
root.joinpath("diagnostic-pass.txt").write_text("not-run\n" if diagnostic_pass is None else ("1\n" if diagnostic_pass else "0\n"))

print("\n## Gate status\n")
for name in ("setup", "product", "verification", "diagnostic", "performance", "evidence", "resource", "correctness", "cleanup", "quality"):
    value = status[f"{name}_pass"]
    print(f"- {name}: {'unavailable' if value is None else 'pass' if value else 'fail'}")

print("\n## Evidence limits\n")
print("Required evidence unavailable: " + "; ".join(unavailable) + ".")
PY
printf '%s\n' "$failed" >"$run_dir/campaign-failed.txt"
[[ $failed -eq 0 ]] || die "one or more namespace samples failed; evidence retained at $run_dir"
if [[ "$measurement_mode" == init-only-diagnostic ]]; then
  printf 'INIT-ONLY DIAGNOSTIC COMPLETE; NONTERMINAL %s\n' "$run_dir"
elif [[ "$(cat "$run_dir/evidence-pass.txt")" == 1 ]]; then
  printf 'ALL REQUIRED EVIDENCE GATES PASSED %s\n' "$run_dir"
elif [[ "$(cat "$run_dir/performance-pass.txt")" == 1 ]]; then
  printf 'PERFORMANCE TARGETS MET; REQUIRED EVIDENCE INCOMPLETE %s\n' "$run_dir"
else
  printf 'PERFORMANCE TARGETS NOT PROVED; REQUIRED EVIDENCE INCOMPLETE %s\n' "$run_dir"
fi
