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
[[ "$run_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || die "unsafe RUN_ID"
[[ "$iterations" =~ ^[1-9][0-9]*$ ]] || die "invalid iteration count"
[[ "$daemon_container_port" =~ ^[1-9][0-9]{0,4}$ ]] || die "invalid daemon container port"
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

cargo build --manifest-path "$repo/Cargo.toml" --release -p fs-benchmark-pro
binary="$repo/target/release/fs-benchmark-pro"
failed=0
for scenario in "${scenarios[@]}"; do
  scenario_dir="$run_dir/scenarios/$scenario"
  mkdir "$scenario_dir"
  fixture="$scenario_dir/fixture"
  set +e
  if [[ "$(uname -s)" == Darwin ]]; then
    /usr/bin/time -l -p "$binary" namespace-fixture "$fixture" "$scenario" \
      >"$scenario_dir/fixture-manifest.json" 2>"$scenario_dir/fixture-supervisor.txt"
  else
    /usr/bin/time -v "$binary" namespace-fixture "$fixture" "$scenario" \
      >"$scenario_dir/fixture-manifest.json" 2>"$scenario_dir/fixture-supervisor.txt"
  fi
  fixture_status=$?
  set -e
  printf '%s\n' "$fixture_status" >"$scenario_dir/fixture-exit-status.txt"
  [[ $fixture_status -eq 0 ]] || die "fixture generation failed; evidence retained at $scenario_dir"
  printf '%s  %s\n' "$(sha256_file "$scenario_dir/fixture-manifest.json")" \
    "$scenario_dir/fixture-manifest.json" >"$scenario_dir/fixture-manifest.sha256"

  for ((iteration = 1; iteration <= iterations; iteration++)); do
    ensure_daemon_running
    sample_dir=$(printf '%s/sample-%03d' "$scenario_dir" "$iteration")
    mkdir "$sample_dir" "$sample_dir/raw"
    printf '%s\n' "$daemon_endpoint" >"$sample_dir/daemon-endpoint.txt"
    benchmark_command=(
      env
      LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload
      LAYERFS_EXEC_TRANSPORT=daemon
      LAYERFS_FUSE_TRANSPORT=daemon
      LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint"
      LAYERFS_DAEMON_CAPABILITY="$daemon_capability"
      LAYERFS_DAEMON_CONTAINER_ID="$container_id"
      LAYERFS_FUSE_HOST=host.docker.internal
      "$binary" namespace "$sample_dir/work" "$fixture" "$container_id" "$scenario" "$iteration"
    )
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
      "$scenario_dir/fixture-manifest.json" "$sample_dir/result.json" \
      "$status" "$scenario" "$iteration" <<'PY'
import json
import re
import sys
from decimal import Decimal
from pathlib import Path

raw_path, supervisor_path, fixture_path, output_path = map(Path, sys.argv[1:5])
status, scenario, iteration = int(sys.argv[5]), sys.argv[6], int(sys.argv[7])
rows = [json.loads(line) for line in raw_path.read_text().splitlines() if line.strip()]
fixture_rows = [json.loads(line) for line in fixture_path.read_text().splitlines() if line.strip()]
if (len(fixture_rows) != 1
        or fixture_rows[0].get("schema") != "fs-bench-pro-namespace-fixture-v1"
        or fixture_rows[0].get("scenario") != scenario):
    raise SystemExit("fixture manifest cardinality")
fixture = fixture_rows[0]
if status == 0:
    if len(rows) != 1 or rows[0].get("schema") != "fs-bench-pro-namespace-v1":
        raise SystemExit("namespace row cardinality or schema")
    row = rows[0]
else:
    if len(rows) > 1 or (rows and rows[0].get("schema") != "fs-bench-pro-namespace-failure-v1"):
        raise SystemExit("namespace failure row cardinality or schema")
    row = rows[0] if rows else {
        "schema": "fs-bench-pro-namespace-supervised-failure-v1",
        "scenario": scenario,
        "iteration": iteration,
        "failed_phase": "before-structured-failure",
    }
    row["exit_status"] = status
if row.get("scenario") != scenario or row.get("iteration") != iteration:
    raise SystemExit("namespace scenario or iteration identity")
phase_names = [
    "layerstack_init_ns", "branch_fork_ns", "workspace_create_ns", "edit_ns",
    "commit_ns", "workspace_end_ns", "reopen_verify_ns",
]
integer_fields = phase_names + [
    "complete_product_ns", "regular_files", "data_directories", "logical_bytes",
    "scanned_files", "scanned_bytes", "candidate_objects", "candidate_bytes",
    "inserted_objects", "inserted_bytes", "reused_objects", "reused_bytes",
    "max_transaction_objects", "max_transaction_bytes",
]
if status == 0:
    if any(type(row.get(name)) is not int or row[name] < 0 for name in integer_fields):
        raise SystemExit("missing or invalid namespace integer field")
    if sum(row[name] for name in phase_names) != row["complete_product_ns"]:
        raise SystemExit("namespace phase equation")
    for name in ["regular_files", "data_directories", "logical_bytes", "fixture_digest"]:
        if row.get(name) != fixture.get(name):
            raise SystemExit(f"fixture mismatch: {name}")
    if row["scanned_files"] != row["regular_files"] or row["scanned_bytes"] != row["logical_bytes"]:
        raise SystemExit("initialization scan receipt mismatch")
    if row["candidate_objects"] != row["inserted_objects"] + row["reused_objects"]:
        raise SystemExit("candidate object equation")
    if row["candidate_bytes"] != row["inserted_bytes"] + row["reused_bytes"]:
        raise SystemExit("candidate byte equation")
    if row["max_transaction_objects"] >= 8192 or row["max_transaction_bytes"] >= 4 * 1024 * 1024:
        raise SystemExit("candidate transaction bound")
    for name in ["fixture_digest", "verified_digest"]:
        digest = row.get(name, "")
        if len(digest) != 64 or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise SystemExit(f"invalid digest: {name}")
    if row["fixture_digest"] == row["verified_digest"]:
        raise SystemExit("namespace edit did not change the tree digest")
else:
    for name in ["regular_files", "data_directories", "logical_bytes", "fixture_digest"]:
        if name in row and row[name] != fixture.get(name):
            raise SystemExit(f"failure fixture mismatch: {name}")
    if "scanned_files" in row and row["scanned_files"] != fixture.get("regular_files"):
        raise SystemExit("failure scan file mismatch")
    if "scanned_bytes" in row and row["scanned_bytes"] != fixture.get("logical_bytes"):
        raise SystemExit("failure scan byte mismatch")
supervisor = supervisor_path.read_text()
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
row["process_user_cpu_ns"] = int(user * 1_000_000_000)
row["process_system_cpu_ns"] = int(system * 1_000_000_000)
row["process_peak_rss_bytes"] = peak
row["metric_sources"] = {
    "phase_wall": "harness Instant boundaries",
    "process_resources": "raw OS /usr/bin/time supervisor output",
    "fixture": "deterministic fixture manifest",
    "initialization_scan": "LayerStack initialization receipt",
    "candidate_storage": "LayerFS initialization and Workspace Commit operation receipts",
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
import sys
from pathlib import Path

root = Path(sys.argv[1])
print("# LayerFS namespace campaign\n")
print("| Scenario | Sample | Status | Init ns | Commit ns | Complete ns | Peak RSS bytes |")
print("| --- | ---: | --- | ---: | ---: | ---: | ---: |")
for sample in sorted(root.glob("scenarios/namespace-*/sample-*")):
    status = sample.joinpath("exit-status.txt").read_text().strip()
    result = sample / "result.json"
    if result.is_file():
        row = json.loads(result.read_text())
        label = "pass" if status == "0" else f"fail ({status}, {row.get('failed_phase', 'unknown')})"
        print(f"| {row['scenario']} | {row['iteration']} | {label} | {row.get('layerstack_init_ns', '—')} | {row.get('commit_ns', '—')} | {row.get('complete_product_ns', '—')} | {row['process_peak_rss_bytes']} |")
    else:
        print(f"| {sample.parent.name} | {int(sample.name.split('-')[-1])} | fail ({status}) | — | — | — | — |")
PY
printf '%s\n' "$failed" >"$run_dir/campaign-failed.txt"
[[ $failed -eq 0 ]] || die "one or more namespace samples failed; evidence retained at $run_dir"
printf 'PASS %s\n' "$run_dir"
