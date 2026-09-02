#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root=${LAYERFS_NAMESPACE_RESULTS_ROOT:-"$repo/benchmark-results/fs-bench-pro/namespace"}
readonly namespace_100000_binding_init_ns=3235294118
readonly namespace_100000_binding_bytes_per_second=153000000
readonly namespace_100000_binding_files_per_second=30600

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
        root / "tools/test-fast.sh",
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
  [[ "$namespace_100000_binding_init_ns" == 3235294118 \
    && "$namespace_100000_binding_bytes_per_second" == 153000000 \
    && "$namespace_100000_binding_files_per_second" == 30600 ]] ||
    die "namespace-100000 binding threshold self-check"
  python3 - "$namespace_100000_binding_init_ns" \
    "$namespace_100000_binding_bytes_per_second" \
    "$namespace_100000_binding_files_per_second" <<'PY'
import sys

limit_ns, minimum_bytes, minimum_files = map(int, sys.argv[1:])
assert 3_019_172_334 <= limit_ns
assert 165_608_300 >= minimum_bytes
assert 33_121 >= minimum_files
assert not 3_235_294_119 <= limit_ns
assert not 152_999_999 >= minimum_bytes
assert not 30_599 >= minimum_files
PY
  cargo test --manifest-path "$repo/Cargo.toml" -p fs-benchmark-pro
  local temporary
  temporary=$(mktemp -d "${TMPDIR:-/tmp}/fs-bench-pro-namespace.XXXXXX")
  trap 'rm -rf -- "$temporary"' EXIT
  rustc --edition=2021 -C opt-level=3 "$here/workload.rs" -o "$temporary/fs-benchmark-workload"
  "$temporary/fs-benchmark-workload" self-check
  rm -rf -- "$temporary"
  trap - EXIT
}

if [[ "${1:-}" == "--self-check" ]]; then
  [[ -z "${LAYERFS_NAMESPACE_COMPOSITE_MANIFEST:-}" ]] ||
    die "external composite manifests are untrusted"
  self_check
  exit 0
fi
if [[ "${1:-}" == "--source-seal" ]]; then seal source; exit 0; fi

[[ $# -eq 4 ]] || die "usage: $0 RUN_ID CONTAINER_ID namespace-10000|all ITERATIONS"
run_id=$1
container=$2
selection=$3
iterations=$4
daemon_container_port=${LAYERFS_DAEMON_CONTAINER_PORT:-41273}
fixture_root=${LAYERFS_NAMESPACE_FIXTURE_ROOT:-}
run_composite=${LAYERFS_NAMESPACE_RUN_COMPOSITE:-0}
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
[[ -z "${LAYERFS_NAMESPACE_COMPOSITE_MANIFEST:-}" ]] ||
  die "external composite manifests are untrusted; use LAYERFS_NAMESPACE_RUN_COMPOSITE=1"
[[ "$run_composite" == 0 || "$run_composite" == 1 ]] ||
  die "LAYERFS_NAMESPACE_RUN_COMPOSITE must be 0 or 1"
if [[ "$run_composite" == 1 ]]; then
  [[ "$measurement_mode" == product && "$selection" == all && "$iterations" -ge 4 ]] ||
    die "composite proof requires product mode, selection all, and at least four samples"
fi
case "$selection" in
  all) scenarios=(namespace-100 namespace-1000 namespace-10000 namespace-100000) ;;
  namespace-100|namespace-1000|namespace-10000|namespace-100000) scenarios=("$selection") ;;
  *) die "unknown namespace scenario" ;;
esac

for command in cargo git python3 rustc sqlite3; do command -v "$command" >/dev/null || die "$command is required"; done
[[ -x /usr/bin/time ]] || die "/usr/bin/time is required"
current_seal=$(seal source)
container_id=not-applicable
daemon_endpoint=not-applicable
daemon_capability=not-applicable
if [[ "$measurement_mode" == product ]]; then
for command in docker nc; do command -v "$command" >/dev/null || die "$command is required in product mode"; done
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
fi

product_commit=$(git -C "$repo" rev-parse 'v0.1.0^{commit}')
product_tag=$(git -C "$repo" rev-parse v0.1.0)
[[ "$product_commit" == 243f98a3bf287d6a9f8168891452b7355d45529c ]] ||
  die "v0.1.0 product identity"
run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/scenarios"
printf '{"applicable":false,"status":"not-run"}\n' \
  >"$run_dir/environment/composite-proof.json"
printf '0\n' >"$run_dir/environment/composite-proof-passed.txt"
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
if [[ "$measurement_mode" == product ]]; then
  docker version >"$run_dir/environment/docker-version.txt"
  docker inspect "$container" >"$run_dir/environment/container-inspect.json"
  docker image inspect "$(docker inspect -f '{{.Image}}' "$container")" \
    >"$run_dir/environment/container-image-inspect.json"
else
  printf 'not applicable in init-only-diagnostic mode\n' >"$run_dir/environment/docker-version.txt"
  printf '{"applicable":false}\n' >"$run_dir/environment/container-inspect.json"
  printf '{"applicable":false}\n' >"$run_dir/environment/container-image-inspect.json"
fi
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
      sample_cache_profile="${fixture_mode}-first-sample-uncontrolled"
    else
      sample_cache_profile="${fixture_mode}-subsequent-sample-uncontrolled"
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
      docker exec "$container" sh -c '
        printf "memory_current=%s\n" "$(cat /sys/fs/cgroup/memory.current)"
        printf "memory_peak=%s\n" "$(cat /sys/fs/cgroup/memory.peak)"
        printf "swap_current=%s\n" "$(cat /sys/fs/cgroup/memory.swap.current)"
        printf "pids_current=%s\n" "$(cat /sys/fs/cgroup/pids.current)"
        grep "^oom " /sys/fs/cgroup/memory.events | tr " " "="
        grep "^oom_kill " /sys/fs/cgroup/memory.events | tr " " "="
      ' >"$sample_dir/raw/cgroup-before.txt"
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
    if [[ "$measurement_mode" == product ]]; then
      grep '^layerfs-container-cgroup-after-v1 ' "$sample_dir/raw/supervisor.txt" \
        >"$sample_dir/raw/cgroup-after.txt" || true
    else
      printf 'not applicable in init-only-diagnostic mode\n' >"$sample_dir/raw/cgroup-before.txt"
      printf 'not applicable in init-only-diagnostic mode\n' >"$sample_dir/raw/cgroup-after.txt"
    fi
    dbstat_status=0
    store_path="$sample_dir/work/store.sqlite"
    if [[ -f "$store_path" ]]; then
      printf '%s  %s\n' "$(sha256_file "$store_path")" "$store_path" >"$sample_dir/raw/store-before-dbstat.sha256"
      sqlite3 -readonly -header -column "$store_path" \
        'SELECT name, count(*) AS pages, sum(pgsize) AS allocated_bytes, sum(payload) AS payload_bytes, sum(unused) AS unused_bytes FROM dbstat GROUP BY name ORDER BY allocated_bytes DESC;' \
        >"$sample_dir/raw/sqlite-dbstat.txt" || dbstat_status=1
      sqlite3 -readonly "$store_path" \
        'EXPLAIN INSERT INTO objects(object_id, bytes) VALUES(zeroblob(32), zeroblob(1)) ON CONFLICT(object_id) DO NOTHING;' \
        >"$sample_dir/raw/sqlite-object-insert-explain.txt" || dbstat_status=1
      sqlite3 -readonly -json "$store_path" \
        "SELECT (SELECT count(*) FROM dbstat WHERE name='objects') AS sqlite_objects_table_pages, (SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name='objects') AS sqlite_objects_table_bytes, (SELECT count(*) FROM dbstat WHERE name='sqlite_autoindex_objects_1') AS sqlite_objects_primary_key_index_pages, (SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name='sqlite_autoindex_objects_1') AS sqlite_objects_primary_key_index_bytes, page_size AS sqlite_page_size_bytes, page_count AS sqlite_page_count, freelist_count AS sqlite_freelist_pages, (SELECT count(*) FROM objects) AS sqlite_object_rows, (SELECT coalesce(sum(length(bytes)),0) FROM objects) AS sqlite_canonical_object_bytes FROM pragma_page_size, pragma_page_count, pragma_freelist_count;" \
        >"$sample_dir/raw/sqlite-custody.json" || dbstat_status=1
      printf '%s  %s\n' "$(sha256_file "$store_path")" "$store_path" >"$sample_dir/raw/store-after-dbstat.sha256"
      cmp -s "$sample_dir/raw/store-before-dbstat.sha256" "$sample_dir/raw/store-after-dbstat.sha256" || dbstat_status=1
    elif [[ $status -eq 0 ]]; then
      dbstat_status=1
    fi
    cleanup_status=$dbstat_status
    if [[ "$measurement_mode" == product ]]; then
      docker logs "$container" >"$sample_dir/raw/container.log" 2>&1 || true
      docker inspect -f '{{json .State}}' "$container" >"$sample_dir/raw/container-state-after.json"
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
    else
      printf 'not applicable in init-only-diagnostic mode\n' >"$sample_dir/raw/container.log"
      printf '{"applicable":false}\n' >"$sample_dir/raw/container-state-after.json"
      printf 'not applicable in init-only-diagnostic mode\n' >"$sample_dir/raw/container-mounts-after.txt"
      printf 'not applicable in init-only-diagnostic mode\n' >"$sample_dir/raw/container-processes-after.txt"
      find "$sample_dir/work" -type f -print | LC_ALL=C sort >"$sample_dir/raw/host-workdir-files-after.txt"
      if [[ "$(wc -l <"$sample_dir/raw/host-workdir-files-after.txt" | tr -d ' ')" != 1 ]] \
        || ! grep -Fxq "$sample_dir/work/store.sqlite" "$sample_dir/raw/host-workdir-files-after.txt"; then
        cleanup_status=1
      fi
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
      "$sample_dir/raw/fixture-custody.json" "$cleanup_status" \
      "$sample_dir/raw/sqlite-custody.json" "$sample_dir/raw/cgroup-before.txt" \
      "$sample_dir/raw/cgroup-after.txt" "$namespace_100000_binding_init_ns" \
      "$namespace_100000_binding_bytes_per_second" \
      "$namespace_100000_binding_files_per_second" <<'PY'
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
dbstat_path = Path(sys.argv[13])
cgroup_before_path, cgroup_after_path = map(Path, sys.argv[14:16])
namespace_100000_limits = tuple(map(int, sys.argv[16:19]))
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
    "store_canonical_objects", "store_canonical_bytes", "process_t0_rss_bytes",
    "process_t1_rss_bytes", "process_t1_rss_growth_bytes",
    "process_t0_peak_rss_bytes", "process_t1_peak_rss_bytes",
    "process_initialization_incremental_peak_rss_bytes", "process_t0_swaps",
    "process_t1_swaps",
    "process_t0_physical_footprint_bytes", "process_t1_physical_footprint_bytes",
    "initialization_user_cpu_ns", "initialization_system_cpu_ns",
    "initialization_disk_read_bytes", "initialization_disk_write_bytes",
    "initialization_context_switches", "process_threads_before", "process_threads_after",
    "sqlite_t0_memory_used_bytes", "sqlite_t0_memory_peak_bytes",
    "sqlite_t0_page_cache_overflow_bytes", "sqlite_t0_page_cache_overflow_peak_bytes",
    "sqlite_t0_allocation_count", "sqlite_t0_allocation_peak_count",
    "sqlite_t0_connection_cache_used_bytes", "sqlite_connection_cache_target_bytes",
    "sqlite_t1_memory_used_bytes", "sqlite_t1_memory_peak_bytes",
    "sqlite_t1_page_cache_overflow_bytes", "sqlite_t1_page_cache_overflow_peak_bytes",
    "sqlite_t1_allocation_count", "sqlite_t1_allocation_peak_count",
    "sqlite_t1_connection_cache_used_bytes", "sqlite_t1_connection_cache_target_bytes",
]
if status == 0:
    expected_row_mode = "product-lifecycle" if expected_mode == "product" else expected_mode
    if row.get("measurement_mode") != expected_row_mode:
        raise SystemExit("namespace measurement mode")
    expected_profile = (
        "commit-head-exact-reopen-v2"
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
            "commit_ns", "workspace_end_ns", "reopen_verify_ns",
        ]
        integer_fields += phase_names[1:] + [
            "reconnect_ns", "reopen_workspace_create_ns", "reopen_content_verify_ns",
            "reopen_workspace_end_ns", "complete_product_ns", "product_lifecycle_ns",
            "edit_size", "max_transaction_objects",
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
            "commit_max_transaction_bytes", "maximum_verifier_buffer_bytes",
            "verifier_worker_count", "verifier_plan_bytes",
            "verifier_path_state_peak_bytes", "verifier_digest_state_peak_bytes",
            "maximum_product_read_ahead_bytes", "read_ahead_hits", "read_ahead_misses",
            "read_ahead_fetches", "read_ahead_requested_bytes", "read_ahead_fetched_bytes",
            "read_ahead_served_bytes", "read_ahead_unused_bytes",
            "workspace_read_local_calls", "workspace_read_local_ids",
            "workspace_read_local_rows", "workspace_read_local_bytes",
            "workspace_create_attach_ns", "workspace_create_non_attach_ns",
            "snapshot_database_calls", "snapshot_database_rows", "snapshot_database_bytes",
            "snapshot_cache_rows_at_create", "snapshot_cache_bytes_at_create",
            "snapshot_store_wide_scans", "small_file_prefetch_eligible",
            "small_file_prefetch_bytes", "anchor_prefetch_count",
            "commit_snapshot_database_calls", "commit_snapshot_database_rows",
            "commit_snapshot_database_bytes", "commit_payload_bytes_read",
            "commit_anchor_payload_reads",
            "process_t7_rss_bytes", "process_t7_peak_rss_bytes",
            "process_product_incremental_peak_rss_bytes", "process_t7_swaps",
            "process_t7_physical_footprint_bytes",
            "product_user_cpu_ns", "product_system_cpu_ns", "product_disk_read_bytes",
            "product_disk_write_bytes", "product_context_switches", "process_threads_at_t7",
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
    if expected_mode == "product" and (
            sum(row[name] for name in phase_names) != row["complete_product_ns"]
            or row["complete_product_ns"] != row["product_lifecycle_ns"]
            or row["reopen_verify_ns"] != row["reconnect_ns"]
                + row["reopen_workspace_create_ns"] + row["reopen_content_verify_ns"]):
        raise SystemExit("namespace phase equation")
    if (row.get("fixture_profile") != "synthetic-small-heavy-v2"
            or row.get("fixture_digest_profile") != "namespace-file-digest-tree-v2"
            or row.get("edit_contract") != "content-only-normalized-mtime-v1"
            or row.get("fixture_cache_profile") not in {
                "generated-first-sample-uncontrolled", "generated-subsequent-sample-uncontrolled",
                "reused-first-sample-uncontrolled", "reused-subsequent-sample-uncontrolled",
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
        if row.get("verified_digest") != fixture.get("edited_fixture_digest"):
            raise SystemExit("exact reopened namespace digest mismatch")
        if (row["maximum_verifier_buffer_bytes"] > 1024 * 1024
                or row["verifier_worker_count"] == 0
                or row["read_ahead_fetches"] == 0
                or row["read_ahead_fetches"] != row["read_ahead_misses"]
                or not 0 < row["maximum_product_read_ahead_bytes"] <= 8 * 1024 * 1024
                or row["read_ahead_fetched_bytes"] < row["read_ahead_served_bytes"]
                or row["read_ahead_fetched_bytes"]
                    != row["read_ahead_served_bytes"] + row["read_ahead_unused_bytes"]):
            raise SystemExit("exact reopened namespace resource equation")
        if (row["workspace_create_ns"]
                != row["workspace_create_attach_ns"] + row["workspace_create_non_attach_ns"]
                or row["workspace_create_non_attach_ns"] > 10_000_000
                or row["snapshot_store_wide_scans"] != 0
                or row["anchor_prefetch_count"] != 0
                or row["small_file_prefetch_eligible"] != 0
                or row["small_file_prefetch_bytes"] != 0
                or row["commit_anchor_payload_reads"] != 0
                or row["commit_payload_bytes_read"] > row["edit_size"]):
            raise SystemExit("Workspace Create snapshot resource equation")
        if (row["process_threads_at_t7"] == 0
                or row["process_t7_peak_rss_bytes"] < row["process_t1_peak_rss_bytes"]
                or row["process_t7_peak_rss_bytes"] < row["process_t7_rss_bytes"]
                or row["process_product_incremental_peak_rss_bytes"]
                    != row["process_t7_peak_rss_bytes"] - row["process_t0_rss_bytes"]
                or row.get("process_product_peak_status") != (
                    "exact-new-lifetime-high-water"
                    if row["process_t7_peak_rss_bytes"] > row["process_t0_peak_rss_bytes"]
                    else "unavailable-cumulative-high-water"
                )
                or row["process_t7_swaps"] != 0
                or row["product_user_cpu_ns"] < row["initialization_user_cpu_ns"]
                or row["product_system_cpu_ns"] < row["initialization_system_cpu_ns"]
                or row["product_disk_read_bytes"] < row["initialization_disk_read_bytes"]
                or row["product_disk_write_bytes"] < row["initialization_disk_write_bytes"]):
            raise SystemExit("complete product resource equation")
    if (not 0 < row["sqlite_connection_cache_target_bytes"] <= 64 * 1024 * 1024
            or row["sqlite_connection_cache_target_bytes"]
                != row["sqlite_t1_connection_cache_target_bytes"]):
        raise SystemExit("SQLite cache identity equation")
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
    if (row["process_t1_rss_growth_bytes"]
            != max(row["process_t1_rss_bytes"] - row["process_t0_rss_bytes"], 0)
            or row["process_t1_peak_rss_bytes"] < row["process_t0_peak_rss_bytes"]
            or row["process_t1_peak_rss_bytes"] < row["process_t1_rss_bytes"]
            or row["process_initialization_incremental_peak_rss_bytes"]
                != row["process_t1_peak_rss_bytes"] - row["process_t0_rss_bytes"]
            or row["process_t0_swaps"] != 0
            or row["process_t1_swaps"] != 0
            or row["process_threads_before"] == 0
            or row["process_threads_after"] == 0):
        raise SystemExit("namespace process resource equation")
    expected_peak_status = (
        "exact-new-lifetime-high-water"
        if row["process_t1_peak_rss_bytes"] > row["process_t0_peak_rss_bytes"]
        else "unavailable-cumulative-high-water"
    )
    if (row.get("process_initialization_peak_status") != expected_peak_status
            or row["sqlite_t0_memory_peak_bytes"] < row["sqlite_t0_memory_used_bytes"]
            or row["sqlite_t0_page_cache_overflow_peak_bytes"] < row["sqlite_t0_page_cache_overflow_bytes"]
            or row["sqlite_t0_allocation_peak_count"] < row["sqlite_t0_allocation_count"]
            or row["sqlite_t1_memory_peak_bytes"] < row["sqlite_t1_memory_used_bytes"]
            or row["sqlite_t1_page_cache_overflow_peak_bytes"] < row["sqlite_t1_page_cache_overflow_bytes"]
            or row["sqlite_t1_allocation_peak_count"] < row["sqlite_t1_allocation_count"]
            or row["sqlite_t0_connection_cache_used_bytes"] == 0
            or row["sqlite_t1_connection_cache_used_bytes"] == 0):
        raise SystemExit("namespace high-water evidence equation")
    for boundary in ("t0", "t1"):
        memory_status_available = any(row[f"sqlite_{boundary}_{name}"] > 0 for name in (
            "memory_used_bytes", "memory_peak_bytes", "allocation_count",
            "allocation_peak_count",
        ))
        row[f"sqlite_{boundary}_memory_status"] = (
            "available" if memory_status_available else "unavailable-disabled"
        )
    targets = {
        "namespace-100": (416_667_000, 300_000_000, 240, 15_000_000),
        "namespace-1000": (500_000_000, 400_000_000, 2_000, 18_000_000),
        "namespace-10000": (750_000_000, 400_000_000, 13_334, 22_000_000),
        "namespace-100000": (*namespace_100000_limits, 25_000_000),
    }[scenario]
    row["target_outcomes"] = {
        "init_absolute": row["layerstack_init_ns"] <= targets[0],
        "init_throughput": row["init_bytes_per_second"] >= targets[1],
        "init_file_rate": row["init_files_per_second"] >= targets[2],
    }
    row["preferred_target_outcomes"] = ({
        "init_absolute": row["layerstack_init_ns"] <= 2_500_000_000,
        "init_throughput": row["init_bytes_per_second"] >= 200_000_000,
    } if scenario == "namespace-100000" else {})
    row["stretch_target_outcomes"] = ({
        "init_absolute": row["layerstack_init_ns"] <= 2_000_000_000,
        "init_throughput": row["init_bytes_per_second"] >= 250_000_000,
    } if scenario == "namespace-100000" else {})
    if expected_mode == "product":
        row["target_outcomes"].update({
            "workspace_create": row["workspace_create_ns"] <= targets[3],
            "commit": row["commit_ns"] <= 10_000_000,
        })
        stretch = {
            "namespace-100": (600_000_000, 1_300_000_000),
            "namespace-1000": (1_000_000_000, 2_100_000_000),
            "namespace-10000": (1_800_000_000, 3_400_000_000),
            "namespace-100000": (7_000_000_000, 10_000_000_000),
        }[scenario]
        row["stretch_outcomes"] = {
            "reopen_verify": row["reopen_verify_ns"] <= stretch[0],
            "complete_product": row["complete_product_ns"] <= stretch[1],
        }
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
diagnostic_schema = "layerfs-initialization-diagnostic-v3"
diagnostic_names = [
    "nonce", "fast_path", "worker_count", "prepare_import_wall_ns",
    "source_file_open_calls", "source_file_read_calls", "source_file_read_bytes",
    "source_symlink_metadata_calls", "source_read_dir_calls", "single_chunk_files",
    "streaming_files", "cdc_scratch_peak_bytes", "metadata_cache_hits",
    "metadata_cache_misses", "metadata_cache_peak_entries", "explicit_buffer_peak_bytes", "canonical_frame_count",
    "explicit_slab_payload_limit_bytes", "explicit_slab_object_limit",
    "explicit_canonical_object_header_bytes", "explicit_pair_pending_limit_bytes",
    "canonical_payload_bytes", "canonical_payload_capacity_bytes",
    "canonical_payload_capacity_slack_bytes", "canonical_encode_calls", "canonical_hash_calls",
    "canonical_framing_bytes", "object_segment_write_calls",
    "object_segment_write_bytes", "object_segment_raw_read_calls",
    "object_segment_raw_read_bytes", "object_segment_passes", "slab_handoffs",
    "slab_sent_objects", "slab_sent_bytes", "slab_send_blocked_ns",
    "slab_partial_peak_objects", "slab_partial_peak_payload_bytes", "slab_queue_peak",
    "slab_queue_peak_bytes", "slab_consumer_idle_ns", "last_slab_receive_offset_ns",
    "direct_pipeline_wall_ns",
    "import_pipeline_thread_peak", "active_producers_after", "task_state_bytes",
    "completed_result_peak_bytes", "parent_final_state_peak_bytes",
    "candidate_copy_bytes", "structural_peak_bytes",
    "parent_payload_copy_bytes",
    "pair_segment_write_calls",
    "pair_segment_write_bytes", "pair_segment_raw_read_calls", "pair_segment_raw_read_bytes",
    "pair_segment_passes", "parent_merge_bytes", "pending_duplicate_objects",
    "pending_duplicate_bytes", "cross_batch_skipped_objects", "cross_batch_skipped_bytes",
    "collision_checks", "admission_batch_peak_objects", "admission_batch_peak_payload_bytes",
    "admission_batch_peak_vec_capacity", "pending_index_peak_entries",
    "pending_index_peak_bytes", "final_batch_peak_payload_bytes",
    "final_batch_peak_vec_capacity", "final_pending_index_peak_bytes", "sql_batch_count",
    "final_simultaneous_owned_peak_bytes",
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
    expected_workers = min(8, fixture["data_directories"])
    if diagnostic["fast_path"] == 1:
        if (diagnostic["explicit_slab_payload_limit_bytes"] != 256 * 1024
                or diagnostic["explicit_slab_object_limit"] != 512
                or diagnostic["explicit_canonical_object_header_bytes"] <= 0
                or diagnostic["explicit_pair_pending_limit_bytes"] != 256 * 1024):
            raise SystemExit("explicit buffer constant identity")
        header = diagnostic["explicit_canonical_object_header_bytes"]
        admission = (
            diagnostic["admission_batch_peak_payload_bytes"]
            + diagnostic["admission_batch_peak_vec_capacity"] * header
            + diagnostic["pending_index_peak_bytes"]
        )
        slab_headers = (
            diagnostic["worker_count"] + diagnostic["slab_queue_peak"] + 1
        ) * diagnostic["explicit_slab_object_limit"] * header
        pipeline_peak = (
            diagnostic["worker_count"] * diagnostic["explicit_slab_payload_limit_bytes"]
            + diagnostic["slab_queue_peak_bytes"]
            + diagnostic["explicit_slab_payload_limit_bytes"]
            + diagnostic["explicit_pair_pending_limit_bytes"]
            + diagnostic["cdc_scratch_peak_bytes"]
            + diagnostic["structural_peak_bytes"]
            + diagnostic["task_state_bytes"]
            + slab_headers
            + admission
        )
        completed_peak = (
            admission
            + diagnostic["task_state_bytes"]
            + diagnostic["completed_result_peak_bytes"]
            + diagnostic["explicit_pair_pending_limit_bytes"]
        )
        final_peak = (
            diagnostic["final_simultaneous_owned_peak_bytes"]
            + diagnostic["explicit_pair_pending_limit_bytes"]
        )
        recomputed_explicit_peak = max(pipeline_peak, completed_peak, final_peak)
        if diagnostic["explicit_buffer_peak_bytes"] != recomputed_explicit_peak:
            raise SystemExit("explicit buffer ownership equation")
        if (diagnostic["final_simultaneous_owned_peak_bytes"]
                < diagnostic["parent_final_state_peak_bytes"]
                or (diagnostic["insert_node_peak_capacity"] != "na"
                    and diagnostic["final_simultaneous_owned_peak_bytes"]
                        < diagnostic["insert_node_peak_capacity"])):
            raise SystemExit("final transient ownership equation")
        diagnostic["explicit_buffer_pipeline_peak_bytes"] = pipeline_peak
        diagnostic["explicit_buffer_completed_peak_bytes"] = completed_peak
        diagnostic["explicit_buffer_final_peak_bytes"] = final_peak
        diagnostic["explicit_buffer_recomputed_peak_bytes"] = recomputed_explicit_peak
    if status == 0 and diagnostic["fast_path"] != 1:
        raise SystemExit("successful namespace row did not use direct initialization")
    if diagnostic["fast_path"] == 1 and (
            diagnostic["worker_count"] != expected_workers
            or diagnostic["source_file_open_calls"] != fixture["regular_files"]
            or diagnostic["source_file_read_calls"] == 0
            or diagnostic["source_file_read_bytes"] != fixture["logical_bytes"]
            or diagnostic["source_symlink_metadata_calls"]
                != fixture["regular_files"] + fixture["data_directories"] + 1
            or diagnostic["source_read_dir_calls"] != fixture["data_directories"] + 1
            or diagnostic["metadata_cache_peak_entries"] > 8
            or diagnostic["explicit_buffer_peak_bytes"] > 10 * 1024 * 1024
            or diagnostic["object_segment_write_calls"] != 0
            or diagnostic["object_segment_write_bytes"] != 0
            or diagnostic["object_segment_raw_read_calls"] != 0
            or diagnostic["object_segment_raw_read_bytes"] != 0
            or diagnostic["object_segment_passes"] != 0
            or diagnostic["slab_partial_peak_objects"] > 512
            or diagnostic["slab_partial_peak_payload_bytes"] > 256 * 1024
            or diagnostic["slab_queue_peak"] > 4
            or diagnostic["slab_queue_peak_bytes"] > 4 * 256 * 1024
            or diagnostic["import_pipeline_thread_peak"] > expected_workers + 1
            or diagnostic["import_pipeline_thread_peak"] < 2
            or diagnostic["active_producers_after"] != 0
            or diagnostic["canonical_encode_calls"] != diagnostic["canonical_frame_count"]
            or diagnostic["canonical_hash_calls"] < diagnostic["canonical_encode_calls"]
            or diagnostic["canonical_payload_capacity_bytes"] < diagnostic["canonical_payload_bytes"]
            or diagnostic["canonical_payload_capacity_slack_bytes"]
                != diagnostic["canonical_payload_capacity_bytes"] - diagnostic["canonical_payload_bytes"]
            or diagnostic["slab_sent_objects"] != diagnostic["canonical_frame_count"]
            or diagnostic["slab_sent_bytes"] != diagnostic["canonical_payload_bytes"]
            or diagnostic["parent_payload_copy_bytes"] != 0
            or (scenario == "namespace-100000" and diagnostic["slab_handoffs"] > 2_200)):
        raise SystemExit("direct initialization resource contract")
    row["initialization_diagnostic_schema"] = diagnostic_schema
    row["initialization_diagnostic"] = diagnostic.copy()
    row.update(diagnostic)
    commit_schema = "layerfs-initialization-commits-v1"
    commit_names = {
        "nonce", "pipeline_count", "pipeline_ns", "pipeline_max_ns",
        "pipeline_max_ordinal", "final_build_count", "final_build_ns",
        "final_build_max_ns", "final_build_max_ordinal", "publication_ns",
        "publication_ordinal", "total_count", "total_ns",
    }
    commit_lines = [
        line for line in supervisor.splitlines()
        if line.startswith(commit_schema + " ")
    ]
    if len(commit_lines) != 1:
        raise SystemExit("initialization commit diagnostic cardinality")
    commits = {}
    for token in commit_lines[0].split()[1:]:
        if "=" not in token:
            raise SystemExit("malformed initialization commit diagnostic")
        name, value = token.split("=", 1)
        if name in commits:
            raise SystemExit(f"duplicate initialization commit field: {name}")
        commits[name] = value
    if set(commits) != commit_names or commits["nonce"] != diagnostic_nonce:
        raise SystemExit("initialization commit diagnostic shape")
    for name in commit_names - {"nonce"}:
        if not re.fullmatch(r"[0-9]+", commits[name]):
            raise SystemExit(f"invalid initialization commit value: {name}")
        commits[name] = int(commits[name])
    final_empty = commits["final_build_count"] == 0
    if (commits["pipeline_count"] + commits["final_build_count"] + 1
            != commits["total_count"]
            or commits["pipeline_ns"] + commits["final_build_ns"]
                + commits["publication_ns"] != commits["total_ns"]
            or commits["total_count"] != diagnostic["sql_batch_count"]
            or commits["total_ns"] != diagnostic["sql_commit_ns"]
            or commits["pipeline_count"] == 0
            or commits["pipeline_max_ns"] > commits["pipeline_ns"]
            or not 1 <= commits["pipeline_max_ordinal"] <= commits["pipeline_count"]
            or commits["publication_ordinal"] != commits["total_count"]
            or final_empty != (
                commits["final_build_ns"] == 0
                and commits["final_build_max_ns"] == 0
                and commits["final_build_max_ordinal"] == 0
            )
            or (not final_empty and (
                commits["final_build_max_ns"] > commits["final_build_ns"]
                or not commits["pipeline_count"] < commits["final_build_max_ordinal"]
                    < commits["publication_ordinal"]
            ))):
        raise SystemExit("initialization commit phase equation")
    row["initialization_commit_diagnostic_schema"] = commit_schema
    row["initialization_commit_diagnostic"] = commits.copy()
    for name in commit_names - {"nonce"}:
        row[f"initialization_commit_{name}"] = commits[name]
    producer_schema = "layerfs-initialization-producer-v1"
    producer_names = {
        "nonce", "producer", "wall_ns", "blocked_ns", "tasks", "files", "bytes",
        "completion_offset_ns",
    }
    producer_lines = [
        line for line in supervisor.splitlines()
        if line.startswith(producer_schema + " ")
    ]
    if diagnostic["fast_path"] == 1 and len(producer_lines) != expected_workers:
        raise SystemExit("initialization producer diagnostic cardinality")
    producers = []
    for line in producer_lines:
        producer = {}
        for token in line.split()[1:]:
            if "=" not in token:
                raise SystemExit("malformed initialization producer field")
            name, value = token.split("=", 1)
            if name in producer:
                raise SystemExit(f"duplicate initialization producer field: {name}")
            producer[name] = value
        if set(producer) != producer_names or producer["nonce"] != diagnostic_nonce:
            raise SystemExit("initialization producer diagnostic shape")
        for name in producer_names - {"nonce"}:
            if not re.fullmatch(r"[0-9]+", producer[name]) or int(producer[name]) > 2**64 - 1:
                raise SystemExit(f"invalid initialization producer u64: {name}")
            producer[name] = int(producer[name])
        producers.append(producer)
    producers.sort(key=lambda producer: producer["producer"])
    if status == 0 and producers and (
            [producer["producer"] for producer in producers] != list(range(expected_workers))
            or sum(producer["tasks"] for producer in producers) != fixture["data_directories"]
            or sum(producer["files"] for producer in producers) != fixture["regular_files"]
            or sum(producer["bytes"] for producer in producers) != fixture["logical_bytes"]
            or sum(producer["blocked_ns"] for producer in producers)
                != diagnostic["slab_send_blocked_ns"]
            or any(producer["blocked_ns"] > producer["wall_ns"]
                   or producer["wall_ns"] > producer["completion_offset_ns"]
                   or producer["files"] != producer["tasks"] * 100
                   for producer in producers)
            or max(producer["completion_offset_ns"] for producer in producers)
                > diagnostic["last_slab_receive_offset_ns"]
            or diagnostic["last_slab_receive_offset_ns"] > diagnostic["direct_pipeline_wall_ns"]
            or diagnostic["direct_pipeline_wall_ns"] > diagnostic["prepare_import_wall_ns"]
            or diagnostic["prepare_import_wall_ns"] > row["layerstack_init_ns"]):
        raise SystemExit("initialization producer diagnostic equation")
    row["initialization_producer_diagnostic_schema"] = producer_schema
    row["initialization_producers"] = producers
    if status == 0 and expected_mode == "product":
        row["target_outcomes"]["append_fast_path"] = (
            scenario == "namespace-100"
            or (diagnostic["fast_path"] == 1 and diagnostic["parent_merge_bytes"] == 0)
        )
        row["binding_targets_pass"] = all(row["target_outcomes"].values())
if status == 0:
    row["logical_path_movement_bytes"] = (
        row["source_file_read_bytes"]
        + row["object_segment_write_bytes"]
        + row["object_segment_raw_read_bytes"]
        + row["store_growth_bytes"]
    )
    row["logical_path_movement_ratio"] = (
        row["logical_path_movement_bytes"] / max(row["logical_bytes"], 1)
    )
normal_schema = "layerfs-normal-overwrite-v1"
normal_lines = [
    line for line in supervisor.splitlines()
    if line.startswith(normal_schema + " ")
]
expected_normal_lines = 1 if status == 0 and expected_mode == "product" else 0
if len(normal_lines) != expected_normal_lines:
    raise SystemExit("normal-overwrite diagnostic cardinality")
if normal_lines:
    normal = {}
    for token in normal_lines[0].split()[1:]:
        if "=" not in token:
            raise SystemExit("malformed normal-overwrite diagnostic")
        name, value = token.split("=", 1)
        if name in normal:
            raise SystemExit(f"duplicate normal-overwrite field: {name}")
        normal[name] = value
    if (set(normal) != {"nonce", "elapsed_ns", "mtime_seconds", "mtime_nanoseconds", "changed"}
            or normal["nonce"] != diagnostic_nonce):
        raise SystemExit("normal-overwrite diagnostic shape")
    for name in {"elapsed_ns", "mtime_seconds", "mtime_nanoseconds", "changed"}:
        if not re.fullmatch(r"[0-9]+", normal[name]):
            raise SystemExit(f"invalid normal-overwrite value: {name}")
        normal[name] = int(normal[name])
    if (normal["elapsed_ns"] == 0
            or normal["mtime_nanoseconds"] >= 1_000_000_000
            or normal["changed"] not in {0, 1}
            or normal["changed"] != int(
                (normal["mtime_seconds"], normal["mtime_nanoseconds"])
                != (fixture["mtime_seconds"], fixture["mtime_nanoseconds"])
            )):
        raise SystemExit("normal-overwrite mtime contract")
    row["normal_overwrite_diagnostic_schema"] = normal_schema
    row["normal_overwrite_diagnostic_ns"] = normal["elapsed_ns"]
    row["normal_overwrite_mtime_seconds"] = normal["mtime_seconds"]
    row["normal_overwrite_mtime_nanoseconds"] = normal["mtime_nanoseconds"]
    row["normal_overwrite_changed_mtime"] = bool(normal["changed"])
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
if status == 0:
    if peak < row["process_t0_rss_bytes"]:
        raise SystemExit("whole-process peak RSS below T0 baseline")
    row["whole_supervised_incremental_peak_rss_bytes"] = peak - row["process_t0_rss_bytes"]
whole_voluntary = int(match(
    r"^Voluntary context switches:\s*([0-9]+)\s*$",
    r"^\s*([0-9]+)\s+voluntary context switches\s*$",
))
whole_involuntary = int(match(
    r"^Involuntary context switches:\s*([0-9]+)\s*$",
    r"^\s*([0-9]+)\s+involuntary context switches\s*$",
))
whole_swaps = int(match(r"^Swaps:\s*([0-9]+)\s*$", r"^\s*([0-9]+)\s+swaps\s*$"))
row["whole_supervised_voluntary_context_switches"] = whole_voluntary
row["whole_supervised_involuntary_context_switches"] = whole_involuntary
row["whole_supervised_swaps"] = whole_swaps
container_state = json.loads(container_state_path.read_text())
oom_killed = container_state.get("OOMKilled")
if expected_mode == "product":
    if type(oom_killed) is not bool:
        raise SystemExit("container OOM state unavailable")
    row["container_oom_killed"] = oom_killed
else:
    if container_state != {"applicable": False}:
        raise SystemExit("init-only container state must be not applicable")
    row["container_oom_killed"] = None
row["fixture_custody"] = custody
if status == 0 and expected_mode == "product" and oom_killed:
    raise SystemExit("successful sample reported container OOM")
if expected_mode == "product":
    def cgroup(path, diagnostic=False):
        values = {}
        lines = path.read_text().splitlines()
        if diagnostic:
            if len(lines) != 1 or not lines[0].startswith("layerfs-container-cgroup-after-v1 "):
                raise SystemExit("missing cgroup-after diagnostic")
            tokens = lines[0].split()[1:]
        else:
            tokens = lines
        for token in tokens:
            name, value = token.split("=", 1)
            if diagnostic and name == "nonce":
                if value != diagnostic_nonce:
                    raise SystemExit("cgroup-after diagnostic nonce")
                continue
            if name in values or not re.fullmatch(r"[0-9]+", value):
                raise SystemExit("invalid cgroup evidence")
            values[name] = int(value)
        expected = {"memory_current", "memory_peak", "swap_current", "pids_current", "oom", "oom_kill"}
        if set(values) != expected:
            raise SystemExit("missing cgroup evidence")
        return values
    cgroup_before = cgroup(cgroup_before_path)
    cgroup_after = cgroup(cgroup_after_path, True) if status == 0 else None
    if (status == 0 and (
            cgroup_after["oom"] != cgroup_before["oom"]
            or cgroup_after["oom_kill"] != cgroup_before["oom_kill"]
            or cgroup_before["swap_current"] != 0
            or cgroup_after["swap_current"] != 0)):
        raise SystemExit("cgroup OOM or swap contract")
    row["cgroup_before"] = cgroup_before
    row["cgroup_after"] = cgroup_after
else:
    row["cgroup_before"] = None
    row["cgroup_after"] = None
if status == 0:
    dbstat_rows = json.loads(dbstat_path.read_text())
    if len(dbstat_rows) != 1:
        raise SystemExit("SQLite custody cardinality")
    dbstat = dbstat_rows[0]
    dbstat_names = {
        "sqlite_objects_table_pages", "sqlite_objects_table_bytes",
        "sqlite_objects_primary_key_index_pages", "sqlite_objects_primary_key_index_bytes",
        "sqlite_page_size_bytes", "sqlite_page_count", "sqlite_freelist_pages",
        "sqlite_object_rows", "sqlite_canonical_object_bytes",
    }
    if set(dbstat) != dbstat_names or any(type(dbstat[name]) is not int or dbstat[name] < 0 for name in dbstat_names):
        raise SystemExit("SQLite custody shape")
    if (dbstat["sqlite_page_size_bytes"] * dbstat["sqlite_page_count"] != row["store_database_bytes"]
            or dbstat["sqlite_object_rows"] != row["store_canonical_objects"]
            or dbstat["sqlite_canonical_object_bytes"] != row["store_canonical_bytes"]):
        raise SystemExit("SQLite custody Store equation")
    row.update(dbstat)
    row["sqlite_store_to_canonical_ratio"] = (
        row["store_database_bytes"] / max(row["sqlite_canonical_object_bytes"], 1)
    )
    row["sqlite_store_to_logical_ratio"] = row["store_database_bytes"] / max(row["logical_bytes"], 1)
filesystem_inputs = re.search(r"^File system inputs:\s*([0-9]+)\s*$", supervisor, re.MULTILINE)
filesystem_outputs = re.search(r"^File system outputs:\s*([0-9]+)\s*$", supervisor, re.MULTILINE)
if filesystem_inputs and filesystem_outputs:
    row["whole_supervised_filesystem_inputs"] = int(filesystem_inputs.group(1))
    row["whole_supervised_filesystem_outputs"] = int(filesystem_outputs.group(1))
process_resource_backend = (
    "macOS proc_pid_rusage RUSAGE_INFO_V2, PROC_PIDTASKINFO, and getrusage"
    if sys.platform == "darwin"
    else "Linux /proc/self/stat, /proc/self/status, /proc/self/io, and getrusage"
    if sys.platform.startswith("linux")
    else "unsupported"
)
row["process_resource_backend"] = process_resource_backend
row["metric_sources"] = {
    "phase_wall": "harness Instant boundaries",
    "whole_supervised_resources": "raw OS /usr/bin/time around the complete process; not product-only",
    "initialization_resources": process_resource_backend + " T0/T1 snapshots",
    "initialization_peak_rss": "getrusage high-water at T1 minus current RSS at T0; exact only when T1 establishes a new lifetime high-water",
    "sqlite_memory": "process-global sqlite3_status64 counters with explicit availability plus per-connection DBSTATUS_CACHE_USED and cache target at T0/T1",
    "whole_supervised_incremental_peak_rss": "whole-supervised OS peak minus native T0 current-RSS baseline; conservative and not used as the phase peak",
    "fixture": "sealed deterministic fixture manifest plus per-sample root metadata and manifest SHA; no content reread",
    "initialization_scan": "LayerStack initialization receipt",
    "candidate_storage": "LayerFS operation receipts present in the selected measurement mode",
    "initialization_diagnostic": "nonce-bound private LayerFS initialization stderr frame",
    "normal_overwrite": "nonce-bound real-FUSE overwrite and observed mtime after T7; discarded before cleanup",
    "store_growth": "LayerStackStore storage and canonical-storage snapshots",
    "sqlite_custody": "post-timestamp read-only SQLite dbstat and EXPLAIN bound by unchanged Store SHA-256",
    "container_oom": "Docker container state after sample",
    "cgroup": "container cgroup-v2 memory.current/peak/events, swap.current, and pids.current before/after product sample",
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

if [[ "$run_composite" == 1 ]]; then
  composite_dir="$run_dir/environment/composite"
  mkdir "$composite_dir"
  run_composite_command() {
    local name=$1 status
    shift
    mkdir "$composite_dir/$name"
    printf '%q ' "$@" >"$composite_dir/$name/command.txt"
    printf '\n' >>"$composite_dir/$name/command.txt"
    set +e
    "$@" >"$composite_dir/$name/output.txt" 2>&1
    status=$?
    set -e
    printf '%s\n' "$status" >"$composite_dir/$name/exit-status.txt"
    printf '%s  output.txt\n' "$(sha256_file "$composite_dir/$name/output.txt")" \
      >"$composite_dir/$name/output.sha256"
    if [[ $status -ne 0 ]]; then failed=1; fi
  }

  run_composite_command focused-clippy \
    cargo clippy --manifest-path "$repo/Cargo.toml" --workspace --all-targets \
      --all-features --locked -- -D warnings
  run_composite_command focused-fmt \
    cargo fmt --manifest-path "$repo/Cargo.toml" --all -- --check
  run_composite_command focused-diff \
    git -C "$repo" diff --check
  run_composite_command focused-tests \
    env LAYERFS_TEST_JOBS=4 "$repo/tools/test-fast.sh"
  printf '%s  %s\n' "$(sha256_file "$repo/tools/test-fast.sh")" \
    "$repo/tools/test-fast.sh" >"$composite_dir/test-fast.sha256"
  cargo --version >"$composite_dir/cargo-version.txt"
  rustc --version --verbose >"$composite_dir/rustc-version.txt"
  run_composite_command large-spill-reconnect \
    cargo test --manifest-path "$repo/Cargo.toml" --locked \
      -p layerfs-layerstack-store --lib \
      layerstack::tests::parallel_large_spill_matches_legacy_after_fresh_store_reopen \
      -- --ignored --exact --nocapture
  run_composite_command live-fuse \
    docker run --rm --device /dev/fuse --cap-add SYS_ADMIN \
      --security-opt apparmor=unconfined \
      -e LAYERFS_LIVE_FUSE=1 -e CARGO_TARGET_DIR=/tmp/layerfs-target \
      -v "$repo:/workspace:ro" -w /workspace \
      rust:1.85.1-bookworm@sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 \
      cargo test --locked -p layerfs-sdk --test live_fuse \
      namespace_10000_materialization_and_real_fuse_have_one_canonical_root \
      -- --exact --nocapture
  runtime_image=$(docker inspect -f '{{.Image}}' "$container")
  run_composite_command live-docker \
    env LAYERFS_LIVE_DOCKER=1 LAYERFS_LIVE_DOCKER_IMAGE="$runtime_image" \
      cargo test --manifest-path "$repo/Cargo.toml" --locked \
      -p layerfs-sdk --test live_docker \
      managed_container_lifecycle_and_disconnect_cleanup_are_exact \
      -- --exact --nocapture

  set +e
  python3 - "$composite_dir" "$run_dir/environment/composite-proof.json" \
    "$current_seal" "$runtime_image" <<'PY'
import hashlib
import json
import re
import sys
from pathlib import Path

root, target = map(Path, sys.argv[1:3])
seal, runtime_image = sys.argv[3:5]
patterns = {
    "focused-clippy": None,
    "focused-fmt": None,
    "focused-diff": None,
    "focused-tests": r"PASS full workspace native tests in [0-9]+s with 4 bounded jobs",
    "large-spill-reconnect": r"test layerstack::tests::parallel_large_spill_matches_legacy_after_fresh_store_reopen \.\.\. ok",
    "live-fuse": r"test namespace_10000_materialization_and_real_fuse_have_one_canonical_root \.\.\. ok",
    "live-docker": r"test managed_container_lifecycle_and_disconnect_cleanup_are_exact \.\.\. ok",
}
receipts = {}
for name, pattern in patterns.items():
    directory = root / name
    output = directory.joinpath("output.txt").read_text(errors="replace")
    digest = hashlib.sha256(output.encode()).hexdigest()
    recorded_digest = directory.joinpath("output.sha256").read_text().split()[0]
    status = int(directory.joinpath("exit-status.txt").read_text())
    command = directory.joinpath("command.txt").read_text().strip()
    if (status != 0 or not command or digest != recorded_digest
            or (pattern is not None and re.search(pattern, output) is None)):
        raise SystemExit(f"invalid runner-owned composite receipt: {name}")
    receipts[name] = {
        "command": command,
        "exit_status": status,
        "output": output,
        "output_sha256": digest,
    }
checks = {
    "focused_quality": [
        "focused-clippy", "focused-fmt", "focused-diff", "focused-tests"
    ],
    "large_spill_reconnect": ["large-spill-reconnect"],
    "materialization_fuse_equality": ["live-fuse"],
    "managed_docker_lifecycle": ["live-docker"],
    "post_mount_attachment_failure": ["live-docker"],
    "exact_reconnect": ["live-fuse"],
    "cleanup_census": ["live-fuse", "live-docker"],
}
proof = {
    "schema": "layerfs-namespace-runner-composite-proof-v2",
    "source_seal": seal,
    "runtime_image": runtime_image,
    "checks": {name: [receipts[receipt] for receipt in names]
               for name, names in checks.items()},
}
target.write_text(json.dumps(proof, sort_keys=True, separators=(",", ":")) + "\n")
PY
  composite_status=$?
  set -e
  printf '%s\n' "$([[ $composite_status -eq 0 ]] && printf 1 || printf 0)" \
    >"$run_dir/environment/composite-proof-passed.txt"
  if [[ $composite_status -ne 0 ]]; then failed=1; fi
fi

ending_seal=$(seal source)
printf '%s\n' "$ending_seal" >"$run_dir/environment/ending-source-seal.sha256"
if [[ "$ending_seal" != "$current_seal" ]]; then
  printf 'source changed during campaign\n' >"$run_dir/INVALID"
  failed=1
fi
python3 - "$run_dir" "$namespace_100000_binding_init_ns" \
  "$namespace_100000_binding_bytes_per_second" \
  "$namespace_100000_binding_files_per_second" >"$run_dir/report.md" <<'PY'
import json
import statistics
import sys
from pathlib import Path

root = Path(sys.argv[1])
namespace_100000_limits = tuple(map(int, sys.argv[2:5]))
print("# LayerFS namespace-v3 lifecycle campaign\n")
print("## Samples\n")
print("| Scenario | Sample | Mode | Fixture / digest profile | Cache profile | Valid | Binding status | Init ns | Init B/s | Files/s | 100k preferred / stretch | Create ns | Commit ns | Reopen ns | Product lifecycle ns | Lifecycle stretch | Whole-supervised peak RSS | Fast path | Parent merge | Object segment W/R | Admission peak | Fixture RO mount | Store growth |")
print("| --- | ---: | --- | --- | --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- | --- | --- | ---: |")
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
        stretch = row.get("stretch_outcomes")
        stretch_status = "—" if stretch is None else "pass" if all(stretch.values()) else "miss"
        preferred = row.get("preferred_target_outcomes", {})
        stretch_init = row.get("stretch_target_outcomes", {})
        init_goal_status = "—" if not preferred else f"{'pass' if all(preferred.values()) else 'miss'} / {'pass' if all(stretch_init.values()) else 'miss'}"
        print(f"| {row['scenario']} | {row['iteration']} | {row.get('measurement_mode', '—')} | {identity} | {row.get('fixture_cache_profile', '—')} | {'yes' if valid else 'no'} | {target if valid else '—'} | {row.get('layerstack_init_ns', '—')} | {row.get('init_bytes_per_second', '—')} | {row.get('init_files_per_second', '—')} | {init_goal_status} | {row.get('workspace_create_ns', '—')} | {row.get('commit_ns', '—')} | {row.get('reopen_verify_ns', '—')} | {row.get('product_lifecycle_ns', '—')} | {stretch_status} | {row['whole_supervised_peak_rss_bytes']} | {row.get('fast_path', '—')} | {row.get('parent_merge_bytes', '—')} | {segment_io} | {admission_peak} | {fixture_ro} | {row.get('store_growth_bytes', '—')} |")
    else:
        print(f"| {sample.parent.name} | {int(sample.name.split('-')[-1])} | — | — | — | no ({status}) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — |")

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
    "namespace-100": (416_667_000, 300_000_000, 240, 15_000_000),
    "namespace-1000": (500_000_000, 400_000_000, 2_000, 18_000_000),
    "namespace-10000": (750_000_000, 400_000_000, 13_334, 22_000_000),
    "namespace-100000": (*namespace_100000_limits, 25_000_000),
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
    absolute, throughput, file_rate, create = limits[scenario]
    passed = (
        len(group) >= 3
        and (
            scenario == "namespace-100"
            or all(row["fast_path"] == 1 and row["parent_merge_bytes"] == 0 for row in group)
        )
        and values["layerstack_init_ns"] <= absolute
        and values["init_bytes_per_second"] >= throughput
        and values["init_files_per_second"] >= file_rate
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
        ratios = [times[index + 1] / times[index] for index in range(2)]
        ratio_pass = (
            times[1] * 100 <= times[0] * 130
            and times[2] * 100 <= times[1] * 170
        )
        identity_pass = ratio_pass and all(medians[(scenario, *identity)][1] for scenario in order)
        matrix_pass = matrix_pass or identity_pass
        fixture_profile, digest_profile, edit_contract, cache_profile = identity
        print(f"\nAdjacent init ratios for {fixture_profile}/{digest_profile}/{edit_contract}/{cache_profile}: " + ", ".join(f"{ratio:.3f}x" for ratio in ratios) + f"; {'pass' if ratio_pass else 'miss'}.")
        adjacent_requirement = {
            "fixture_profile": fixture_profile,
            "fixture_digest_profile": digest_profile,
            "edit_contract": edit_contract,
            "fixture_cache_profile": cache_profile,
            "namespace_100_to_1000_pass": times[1] * 100 <= times[0] * 130,
            "namespace_1000_to_10000_pass": times[2] * 100 <= times[1] * 170,
        }
        adjacent_requirements.append(adjacent_requirement)
        print("The 100k result is evaluated only against its independent absolute/rate gates.")
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
verification_pass = (
    bool(product_rows)
    and all(row.get("result_profile") == "commit-head-exact-reopen-v2"
            and row.get("verified_digest")
                == json.loads((root / "scenarios" / row["scenario"] / "fixture-manifest.json").read_text())["edited_fixture_digest"]
            and row.get("maximum_verifier_buffer_bytes", 2**64) <= 1024 * 1024
            for row in product_rows)
) if product_rows else None
diagnostic_pass = correctness_pass and cleanup_pass if diagnostic_rows else None
sample_shape_groups = {}
for row in product_rows:
    cache_profile = row["fixture_cache_profile"]
    cache_origin = cache_profile.split("-", 1)[0]
    key = (
        row["scenario"], row["fixture_profile"], row["fixture_digest_profile"],
        row["edit_contract"], cache_origin,
    )
    state = "subsequent" if "-subsequent-sample-" in cache_profile else "first_sample"
    sample_shape_groups.setdefault(
        key, {"first_sample": [], "subsequent": []}
    )[state].append(row)
sample_shape_pass = (
    {key[0] for key in sample_shape_groups} == set(limits)
    and all(len(group["first_sample"]) == 1 and len(group["subsequent"]) >= 3
            for group in sample_shape_groups.values())
)
resource_rows = product_rows if product_rows else diagnostic_rows
absolute_resource_pass = bool(resource_rows) and all(
    row.get("initialization_user_cpu_ns", 2**64)
        + row.get("initialization_system_cpu_ns", 2**64) <= 14_070_000_000
    and row.get("process_initialization_peak_status") == "exact-new-lifetime-high-water"
    and row.get("process_initialization_incremental_peak_rss_bytes", 2**64) <= 128 * 1024 * 1024
    and row.get("explicit_buffer_peak_bytes", 2**64) <= 10 * 1024 * 1024
    and row.get("explicit_buffer_recomputed_peak_bytes") == row.get("explicit_buffer_peak_bytes")
    and row.get("sqlite_connection_cache_target_bytes", 2**64) <= 64 * 1024 * 1024
    and row.get("whole_supervised_swaps") == 0
    and row.get("process_threads_before", 0) == row.get("process_threads_after", -1)
    and row.get("active_producers_after") == 0
    and row.get("parent_payload_copy_bytes") == 0
    and row.get("object_segment_write_bytes") == 0
    and row.get("object_segment_raw_read_bytes") == 0
    and row.get("sqlite_freelist_pages") == 0
    and (row.get("measurement_mode") != "product-lifecycle"
         or (row.get("container_oom_killed") is False
             and row.get("process_product_peak_status") == "exact-new-lifetime-high-water"
             and row.get("process_product_incremental_peak_rss_bytes", 2**64)
                <= 256 * 1024 * 1024
             and row.get("cgroup_before", {}).get("swap_current") == 0
             and row.get("cgroup_after", {}).get("swap_current") == 0))
    for row in resource_rows
)
sqlite_memory_status_available = bool(resource_rows) and all(
    row.get("sqlite_t0_memory_status") == "available"
    and row.get("sqlite_t1_memory_status") == "available"
    for row in resource_rows
)
resource_pass = absolute_resource_pass
normal_overwrite_pass = bool(product_rows) and all(
    row.get("normal_overwrite_diagnostic_schema") == "layerfs-normal-overwrite-v1"
    and row.get("normal_overwrite_diagnostic_ns", 0) > 0
    and type(row.get("normal_overwrite_changed_mtime")) is bool
    for row in product_rows
)
composite_pass = root.joinpath(
    "environment/composite-proof-passed.txt"
).read_text().strip() == "1"
quality_pass = sample_shape_pass and composite_pass
unavailable = []
if not normal_overwrite_pass:
    unavailable.append("normal-overwrite mtime diagnostic for real-workspace extrapolation")
if not composite_pass:
    unavailable.append("retained composite FUSE/materialization/Docker/failure/quality proof manifest")
status = {
    "setup_pass": setup_pass,
    "product_pass": product_pass,
    "verification_pass": verification_pass,
    "diagnostic_pass": diagnostic_pass,
    "performance_pass": matrix_pass,
    "evidence_pass": False,
    "resource_pass": resource_pass,
    "absolute_resource_pass": absolute_resource_pass,
    "sqlite_memory_status_available": sqlite_memory_status_available,
    "correctness_pass": correctness_pass,
    "cleanup_pass": cleanup_pass,
    "quality_pass": quality_pass,
    "sample_shape_pass": sample_shape_pass,
    "normal_overwrite_pass": normal_overwrite_pass,
    "composite_pass": composite_pass,
    "unavailable_required_evidence": unavailable,
    "adjacent_ratio_requirements": adjacent_requirements,
}
status["evidence_pass"] = all(status[name] is True for name in (
    "setup_pass", "product_pass", "performance_pass", "resource_pass",
    "verification_pass", "correctness_pass", "cleanup_pass", "quality_pass"
)) and not unavailable
root.joinpath("run-status.json").write_text(
    json.dumps(status, sort_keys=True, separators=(",", ":")) + "\n"
)
for name in ("setup", "performance", "evidence", "correctness", "cleanup", "quality"):
    root.joinpath(f"{name}-pass.txt").write_text(
        ("1" if status[f"{name}_pass"] else "0") + "\n"
    )
root.joinpath("resource-pass.txt").write_text("1\n" if resource_pass else "0\n")
root.joinpath("normal-overwrite-pass.txt").write_text("1\n" if normal_overwrite_pass else "0\n")
root.joinpath("composite-pass.txt").write_text("1\n" if composite_pass else "0\n")
root.joinpath("product-pass.txt").write_text("not-run\n" if product_pass is None else ("1\n" if product_pass else "0\n"))
root.joinpath("verification-pass.txt").write_text("not-run\n" if verification_pass is None else ("1\n" if verification_pass else "0\n"))
root.joinpath("diagnostic-pass.txt").write_text("not-run\n" if diagnostic_pass is None else ("1\n" if diagnostic_pass else "0\n"))

print("\n## Gate status\n")
for name in ("setup", "product", "verification", "diagnostic", "performance", "evidence", "resource", "absolute_resource", "correctness", "cleanup", "quality", "sample_shape", "normal_overwrite", "composite"):
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
