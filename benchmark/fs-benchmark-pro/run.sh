#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root="$repo/benchmark-results/fs-bench-plus/runs"
compare="$here/compare.py"
workload_source="$here/workload.rs"
computer_commit=de87919a4fd37242e960e13b7b3ba802d1eef0a0
computer_tree=4fb409d7e1356e1098439293d77d2fdc2dbf2190
fixture_bytes=33554432
fixture_sha256=3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c

die() { printf 'fs-benchmark-pro: %s\n' "$*" >&2; exit 2; }

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

workload_source_sha256=$(sha256_file "$workload_source")

safe_id() {
  [[ "$1" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]]
}

source_seal() {
  python3 - "$repo" "${1:-}" <<'PY'
import hashlib
import sys
from pathlib import Path

root = Path(sys.argv[1])
output = Path(sys.argv[2]) if sys.argv[2] else None
paths = {root / "Cargo.toml", root / "Cargo.lock"}
for directory in (root / "crates", root / "tools", root / "benchmark/fs-benchmark-pro"):
    if directory.is_dir():
        paths.update(
            path
            for path in directory.rglob("*")
            if path.is_file()
            and "target" not in path.parts
            and "__pycache__" not in path.parts
            and path.name != ".DS_Store"
            and path.suffix != ".pyc"
        )
lines = []
for path in sorted(paths):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    lines.append(f"{digest}\t{path.relative_to(root)}\n")
payload = "".join(lines).encode()
if output is not None:
    with output.open("x", encoding="utf-8") as target:
        target.write(payload.decode())
print(hashlib.sha256(payload).hexdigest())
PY
}

generate_fixture() {
  local output=$1
  node - "$output" <<'JS'
const crypto = require("node:crypto");
const fs = require("node:fs");
const output = process.argv[2];
const bytes = 32 * 1024 * 1024;
const cipher = crypto.createCipheriv("aes-256-ctr", Buffer.alloc(32, 7), Buffer.alloc(16, 3));
const zeros = Buffer.alloc(1024 * 1024);
const fd = fs.openSync(output, "wx", 0o444);
let remaining = bytes;
try {
  while (remaining > 0) {
    const count = Math.min(remaining, zeros.length);
    fs.writeSync(fd, cipher.update(zeros.subarray(0, count)));
    remaining -= count;
  }
  const tail = cipher.final();
  if (tail.length) fs.writeSync(fd, tail);
  fs.fsyncSync(fd);
} finally {
  fs.closeSync(fd);
}
JS
  [[ "$(wc -c <"$output" | tr -d ' ')" == "$fixture_bytes" ]] || die "fixture size mismatch"
  [[ "$(sha256_file "$output")" == "$fixture_sha256" ]] || die "fixture SHA-256 mismatch"
}

self_check_dir=
active_layer_container=
cleanup_self_check() {
  case "$self_check_dir" in
    */fs-benchmark-pro-check.*) rm -rf -- "$self_check_dir" ;;
  esac
}

cleanup_layer_container() {
  if [[ -n "$active_layer_container" && "$active_layer_container" =~ ^layerfs-fs-pro-[A-Za-z0-9_.-]+$ ]]; then
    docker rm -f -- "$active_layer_container" >/dev/null 2>&1 || true
    active_layer_container=
  fi
}

self_check() {
  bash -n "$0"
  python3 "$compare" --self-check
  command -v node >/dev/null || die "Node.js is required for the neutral fixture"
  command -v rustc >/dev/null || die "rustc is required for the native workload self-check"
  self_check_dir=$(mktemp -d "${TMPDIR:-/tmp}/fs-benchmark-pro-check.XXXXXX")
  trap cleanup_self_check EXIT
  rustc --edition=2021 -C opt-level=3 -C strip=symbols \
    --remap-path-prefix="$here"=. "$workload_source" -o "$self_check_dir/fs-benchmark-workload"
  "$self_check_dir/fs-benchmark-workload" self-check
  generate_fixture "$self_check_dir/fixture.bin"
  "$self_check_dir/fs-benchmark-workload" digest "$self_check_dir/fixture.bin" |
    grep -Fx "$fixture_bytes	$fixture_sha256" >/dev/null || die "native workload fixture digest mismatch"
  cleanup_self_check
  trap - EXIT
  printf 'PASS fs-benchmark-pro shell, workload, fixture, and paired verifier checks\n'
}

if [[ "${1:-}" == "--self-check" ]]; then
  self_check
  exit 0
fi
if [[ "${1:-}" == "--source-seal" ]]; then
  source_seal
  exit 0
fi

[[ $# -ge 3 && $# -le 4 ]] ||
  die "usage: $0 focused|smoke|formal COMPUTER_IMAGE LAYERFS_IMAGE [RUN_ID]\n       $0 --self-check"
profile=$1
computer_image=$2
layerfs_image=$3
run_id=${4:-$(date -u +%Y%m%dT%H%M%SZ)-$(git -C "$repo" rev-parse --short=12 HEAD)}

case "$profile" in
  focused) pair_count=1 ;;
  smoke) pair_count=1 ;;
  formal) pair_count=30 ;;
  *) die "profile must be focused, smoke, or formal" ;;
esac
[[ "$profile" == focused ]] ||
  die "native+Bash baseline is scoped to focused EDIT16 until Computer scenario isolation is complete"
safe_id "$run_id" || die "RUN_ID must be 1-128 safe filename characters"
[[ "$computer_image" =~ ^[A-Za-z0-9][A-Za-z0-9_./:@-]*$ ]] || die "unsafe Computer image reference"
[[ "$layerfs_image" =~ ^[A-Za-z0-9][A-Za-z0-9_./:@-]*$ ]] || die "unsafe LayerFS image reference"

command -v docker >/dev/null || die "docker is required"
command -v node >/dev/null || die "Node.js is required"
python3 "$compare" --self-check >/dev/null
docker image inspect "$computer_image" >/dev/null || die "Computer image not found: $computer_image"
docker image inspect "$layerfs_image" >/dev/null || die "LayerFS image not found: $layerfs_image"

computer_labels=$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}} {{index .Config.Labels "org.opencontainers.image.source-tree"}}' "$computer_image")
[[ "$computer_labels" == "$computer_commit $computer_tree" ]] ||
  die "Computer image labels do not match pinned commit/tree: $computer_labels"
computer_build_mode=$(docker image inspect --format '{{index .Config.Labels "dev.layerfs.computer-build-mode"}}' "$computer_image")
case "$computer_build_mode" in
  sealed-source-build) ;;
  diagnostic-prebuilt-dist) [[ "$profile" != formal ]] || die "diagnostic Computer image is not formal evidence" ;;
  *) die "Computer image has unsupported build provenance: $computer_build_mode" ;;
esac
layerfs_commit=$(git -C "$repo" rev-parse HEAD)
layerfs_tree=$(git -C "$repo" rev-parse HEAD^{tree})
layerfs_dirty=false
[[ -z "$(git -C "$repo" status --porcelain)" ]] || layerfs_dirty=true
source_seal_sha256=$(source_seal)
layerfs_labels=$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}} {{index .Config.Labels "org.opencontainers.image.source-tree"}} {{index .Config.Labels "dev.layerfs.source-dirty"}} {{index .Config.Labels "dev.layerfs.source-seal"}} {{index .Config.Labels "dev.layerfs.workload-source-sha256"}} {{index .Config.Labels "dev.layerfs.workload-binary-sha256"}}' "$layerfs_image")
read -r admitted_commit admitted_tree admitted_dirty admitted_seal admitted_workload_source admitted_workload_binary <<<"$layerfs_labels"
[[ "$admitted_commit $admitted_tree $admitted_dirty $admitted_seal $admitted_workload_source" == "$layerfs_commit $layerfs_tree $layerfs_dirty $source_seal_sha256 $workload_source_sha256" ]] ||
  die "LayerFS image labels do not match current commit/tree/dirty state/source seal: $layerfs_labels"
[[ "$admitted_workload_binary" =~ ^[0-9a-f]{64}$ ]] || die "LayerFS image lacks admitted native helper binary SHA-256"
computer_arch=$(docker image inspect --format '{{.Architecture}}' "$computer_image")
layerfs_arch=$(docker image inspect --format '{{.Architecture}}' "$layerfs_image")
[[ "$computer_arch" == "$layerfs_arch" ]] || die "image architecture mismatch: Computer=$computer_arch LayerFS=$layerfs_arch"
run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite existing result: $run_dir"
mkdir "$run_dir/environment" "$run_dir/pairs"
fixture="$run_dir/fixture.bin"
generate_fixture "$fixture"

git -C "$repo" status --short >"$run_dir/environment/layerfs-git-status.txt"
git -C "$repo" diff --binary >"$run_dir/environment/layerfs-working-tree.patch"
git -C "$repo" diff --cached --binary >"$run_dir/environment/layerfs-index.patch"
git -C "$repo" ls-files --others --exclude-standard >"$run_dir/environment/layerfs-untracked-files.txt"
docker version >"$run_dir/environment/docker-version.txt"
docker info >"$run_dir/environment/docker-info.txt"
docker image inspect "$computer_image" >"$run_dir/environment/computer-image-inspect.json"
docker image inspect "$layerfs_image" >"$run_dir/environment/layerfs-image-inspect.json"
uname -a >"$run_dir/environment/uname.txt"
if command -v lscpu >/dev/null 2>&1; then lscpu >"$run_dir/environment/lscpu.txt"; fi

recorded_source_seal=$(source_seal "$run_dir/environment/layerfs-source-seal.tsv")
[[ "$recorded_source_seal" == "$source_seal_sha256" ]] || die "LayerFS source changed during admission"
schedule_seed=$(printf '%s\n' "$run_id" | sha256_file /dev/stdin)
started_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)

python3 - "$run_dir/manifest.json" "$run_id" "$profile" "$pair_count" "$schedule_seed" \
  "$computer_image" "$computer_build_mode" "$layerfs_image" "$layerfs_commit" "$layerfs_tree" "$layerfs_dirty" "$source_seal_sha256" "$started_utc" "$computer_arch" "$workload_source_sha256" "$admitted_workload_binary" <<'PY'
import json
import random
import sys

(
    output, run_id, profile, pair_count, seed, computer_image, computer_build_mode, layerfs_image,
    layerfs_commit, layerfs_tree, layerfs_dirty, source_seal, started_utc, architecture,
    helper_source_sha256, helper_binary_sha256,
) = sys.argv[1:]
rng = random.Random(int(seed, 16))
candidates = ["layerfs-reference"] if profile == "focused" else ["computer-upstream", "layerfs-reference"]
schedule = []
for index in range(1, int(pair_count) + 1):
    order = candidates.copy()
    rng.shuffle(order)
    schedule.append({"pair_id": f"{index:03d}", "order": order})
manifest = {
    "schema": "fs-benchmark-pro-run-v2",
    "run_id": run_id,
    "profile": profile,
    "claim_scope": "EDIT16 native+Bash baseline",
    "candidates": candidates,
    "pair_count": int(pair_count),
    "schedule_seed": seed,
    "schedule": schedule,
    "pins": {
        "computer-upstream": {
            "repository": "https://github.com/cloudflare/computer",
            "commit": "de87919a4fd37242e960e13b7b3ba802d1eef0a0",
            "tree": "4fb409d7e1356e1098439293d77d2fdc2dbf2190",
            "image": computer_image,
            "build_mode": computer_build_mode,
        },
        "layerfs-reference": {
            "repository": "https://github.com/Ephemeral-AI-Lab/layerfs",
            "commit": layerfs_commit,
            "tree": layerfs_tree,
            "dirty": layerfs_dirty == "true",
            "source_seal_sha256": source_seal,
            "image": layerfs_image,
        },
    },
    "fixture": {
        "algorithm": "AES-256-CTR(key=07*32,iv=03*16) over zero bytes",
        "bytes": 33_554_432,
        "sha256": "3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c",
        "path": "fixture.bin",
        "generated_outside_candidate_timers": True,
    },
    "helper": {
        "path": "fs-benchmark-workload",
        "source_path": "benchmark/fs-benchmark-pro/workload.rs",
        "source_sha256": helper_source_sha256,
        "sha256": helper_binary_sha256,
        "binary_identity_evidence": "environment/native-helper.sha256",
        "byte_identical_in_scheduled_candidates": True,
        "paired_identity_verified": profile != "focused",
    },
    "registered_edit_argv": {
        "layerfs-reference": ["/bin/bash", "-lc", "\"$@\"", "fs-bench-shell", "/usr/local/bin/fs-benchmark-workload", "edit", "<payload>", "<index>", "33554432"],
        "computer-upstream": ["/bin/bash", "-lc", "\"$@\"", "fs-bench-shell", "/benchmark/fs-benchmark-workload", "edit", "<payload>", "<index>", "33554432"],
    },
    "envelope": {
        "same_docker_daemon": True,
        "adjacent_pairs": True,
        "randomized_order": True,
        "cpus": 1,
        "memory_bytes": 1_073_741_824,
        "memory_swap_bytes": 1_073_741_824,
        "pids_limit": 512,
        "tmpfs": "/tmp:rw,nosuid,nodev,size=256m",
        "architecture": architecture,
        "os_page_cache": "uncontrolled",
        "layerfs_control_process_inside_envelope": True,
    },
    "started_utc": started_utc,
}
with open(output, "x", encoding="utf-8") as target:
    json.dump(manifest, target, indent=2, sort_keys=True)
    target.write("\n")
PY

run_computer() {
  local arm_dir=$1
  docker run --rm --privileged --device /dev/fuse:rwm --cap-add SYS_ADMIN \
    --security-opt apparmor=unconfined --security-opt seccomp=unconfined \
    --network none --cpus 1 --memory 1g --memory-swap 1g --pids-limit 512 \
    --tmpfs /tmp:rw,nosuid,nodev,size=256m \
    --mount "type=bind,src=$fixture,dst=/fixture/payload.bin,readonly" \
    --mount "type=bind,src=$arm_dir,dst=/results" \
    "$computer_image" --fixture /fixture/payload.bin --output /results/summary.json \
    >"$arm_dir/stdout.txt" 2>"$arm_dir/stderr.txt"
}

run_layerfs() {
  local arm_dir=$1
  local pair_id=$2
  active_layer_container="layerfs-fs-pro-${run_id:0:48}-$pair_id-$$"
  docker run -d --rm --name "$active_layer_container" --privileged --device /dev/fuse:rwm \
    --cap-add SYS_ADMIN --security-opt apparmor=unconfined --security-opt seccomp=unconfined \
    --network none --cpus 1 --memory 1g --memory-swap 1g --pids-limit 512 \
    --tmpfs /tmp:rw,nosuid,nodev,size=256m \
    --mount type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock \
    --mount "type=bind,src=$fixture,dst=/fixture/payload.bin,readonly" \
    --mount "type=bind,src=$arm_dir,dst=/results" \
    -e LAYERFS_BENCH_SELF_TARGET=1 -e LAYERFS_BENCH_REQUIRE_DIRECT_ENGINE=1 \
    -e LAYERFS_BENCH_FIXTURE=/fixture/payload.bin \
    --entrypoint sleep "$layerfs_image" infinity >"$arm_dir/docker-run.stdout.txt" 2>"$arm_dir/docker-run.stderr.txt"
  docker inspect --type container "$active_layer_container" >"$arm_dir/measure-container-inspect.json"
  docker exec "$active_layer_container" /usr/local/bin/fs-benchmark-pro \
    measure "$active_layer_container" /results 32 16 \
    >"$arm_dir/measure.stdout.txt" 2>"$arm_dir/measure.stderr.txt"
  docker stop "$active_layer_container" >"$arm_dir/measure-container-stop.stdout.txt" 2>"$arm_dir/measure-container-stop.stderr.txt"
  active_layer_container=
  for _ in {1..100}; do
    if ! docker inspect --type container "layerfs-fs-pro-${run_id:0:48}-$pair_id-$$" >/dev/null 2>&1; then break; fi
    sleep 0.1
  done
  if docker inspect --type container "layerfs-fs-pro-${run_id:0:48}-$pair_id-$$" >/dev/null 2>&1; then
    die "timed-out waiting for the measured LayerFS container to be removed"
  fi
  active_layer_container="layerfs-fs-pro-${run_id:0:48}-$pair_id-$$"
  docker run -d --rm --name "$active_layer_container" --privileged --device /dev/fuse:rwm \
    --cap-add SYS_ADMIN --security-opt apparmor=unconfined --security-opt seccomp=unconfined \
    --network none --cpus 1 --memory 1g --memory-swap 1g --pids-limit 512 \
    --tmpfs /tmp:rw,nosuid,nodev,size=256m \
    --mount type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock \
    --mount "type=bind,src=$arm_dir,dst=/results" \
    -e LAYERFS_BENCH_SELF_TARGET=1 -e LAYERFS_BENCH_REQUIRE_DIRECT_ENGINE=1 \
    --entrypoint sleep "$layerfs_image" infinity >"$arm_dir/docker-reopen.stdout.txt" 2>"$arm_dir/docker-reopen.stderr.txt"
  docker inspect --type container "$active_layer_container" >"$arm_dir/verify-container-inspect.json"
  docker exec "$active_layer_container" /usr/local/bin/fs-benchmark-pro verify \
    /results/layerfs-reference-state.tsv /results/summary.json \
    >"$arm_dir/verify.stdout.txt" 2>"$arm_dir/verify.stderr.txt"
  docker stop "$active_layer_container" >"$arm_dir/docker-stop.stdout.txt" 2>"$arm_dir/docker-stop.stderr.txt"
  active_layer_container=

  for diagnostic in true bash helper edit; do
    for sample_number in {1..5}; do
      sample=$(printf '%03d' "$sample_number")
      active_layer_container="layerfs-fs-pro-${run_id:0:32}-$pair_id-$diagnostic-$sample-$$"
      docker run -d --rm --name "$active_layer_container" --privileged --device /dev/fuse:rwm \
        --cap-add SYS_ADMIN --security-opt apparmor=unconfined --security-opt seccomp=unconfined \
        --network none --cpus 1 --memory 1g --memory-swap 1g --pids-limit 512 \
        --tmpfs /tmp:rw,nosuid,nodev,size=256m \
        --mount type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock \
        --mount "type=bind,src=$fixture,dst=/fixture/payload.bin,readonly" \
        --mount "type=bind,src=$arm_dir,dst=/results" \
        -e LAYERFS_BENCH_SELF_TARGET=1 -e LAYERFS_BENCH_REQUIRE_DIRECT_ENGINE=1 \
        -e LAYERFS_BENCH_FIXTURE=/fixture/payload.bin \
        --entrypoint sleep "$layerfs_image" infinity >"$arm_dir/docker-diagnostic-$diagnostic-$sample.stdout.txt" 2>"$arm_dir/docker-diagnostic-$diagnostic-$sample.stderr.txt"
      docker inspect --type container "$active_layer_container" >"$arm_dir/diagnostic-$diagnostic-$sample-container-inspect.json"
      docker exec "$active_layer_container" /usr/local/bin/fs-benchmark-pro diagnose \
        "$active_layer_container" /results "$diagnostic" "$sample" \
        >"$arm_dir/diagnostic-$diagnostic-$sample.stdout.txt" 2>"$arm_dir/diagnostic-$diagnostic-$sample.stderr.txt"
      docker stop "$active_layer_container" >"$arm_dir/diagnostic-$diagnostic-$sample-stop.stdout.txt" 2>"$arm_dir/diagnostic-$diagnostic-$sample-stop.stderr.txt"
      active_layer_container=
    done
  done
  python3 - "$arm_dir" <<'PY'
import json
import statistics
import sys
from pathlib import Path

root = Path(sys.argv[1])
summary = {
    "schema": "fs-benchmark-pro-execution-diagnostics-summary-v1",
    "headline": False,
    "diagnostic_prewarm": False,
    "os_page_cache": "uncontrolled",
    "operations": {},
}
for name in ("true", "bash", "helper", "edit"):
    rows = [json.loads(path.read_text()) for path in sorted(root.glob(f"execution-diagnostic-{name}-*.json"))]
    values = sorted(row["sdk_exec_to_terminal_ns"] for row in rows)
    if len(values) != 5:
        raise SystemExit(f"{name}: expected five isolated diagnostics")
    summary["operations"][name] = {
        "exact_argv": rows[0]["exact_argv"],
        "raw_public_ns": values,
        "median_ns": statistics.median(values),
        "q1_ns": statistics.median(values[:2]),
        "q3_ns": statistics.median(values[3:]),
    }
(root / "execution-diagnostics-summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
PY
}

trap cleanup_layer_container EXIT
trap 'cleanup_layer_container; exit 130' INT TERM
for ((pair_index = 1; pair_index <= pair_count; pair_index++)); do
  pair_id=$(printf '%03d' "$pair_index")
  pair_dir="$run_dir/pairs/$pair_id"
  mkdir "$pair_dir"
  order=$(python3 - "$run_dir/manifest.json" "$pair_index" <<'PY'
import json
import sys
manifest = json.load(open(sys.argv[1], encoding="utf-8"))
print(*manifest["schedule"][int(sys.argv[2]) - 1]["order"])
PY
  )
  for candidate in $order; do
    arm_dir="$pair_dir/$candidate"
    mkdir "$arm_dir"
    printf '%s\n' "$candidate" >"$arm_dir/candidate.txt"
    printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$arm_dir/started-utc.txt"
    case "$candidate" in
      computer-upstream) run_computer "$arm_dir" ;;
      layerfs-reference) run_layerfs "$arm_dir" "$pair_id" ;;
      *) die "internal schedule error: $candidate" ;;
    esac
    printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$arm_dir/finished-utc.txt"
  done
done
trap - EXIT INT TERM

layerfs_helper_sha256=$(docker run --rm --network none --entrypoint sha256sum "$layerfs_image" /usr/local/bin/fs-benchmark-workload | awk '{print $1}')
printf 'layerfs-reference\t%s\n' "$layerfs_helper_sha256" >"$run_dir/environment/native-helper.sha256"
if [[ "$profile" != focused ]]; then
  computer_workload_labels=$(docker image inspect --format '{{index .Config.Labels "dev.layerfs.workload-source-sha256"}} {{index .Config.Labels "dev.layerfs.workload-binary-sha256"}}' "$computer_image")
  [[ "$computer_workload_labels" == "$workload_source_sha256 $admitted_workload_binary" ]] ||
    die "Computer native helper labels do not match LayerFS: $computer_workload_labels"
  computer_helper_sha256=$(docker run --rm --network none --entrypoint sha256sum "$computer_image" /benchmark/fs-benchmark-workload | awk '{print $1}')
  printf 'computer-upstream\t%s\n' "$computer_helper_sha256" >>"$run_dir/environment/native-helper.sha256"
  [[ "$computer_helper_sha256" == "$layerfs_helper_sha256" ]] ||
    die "candidate native helper SHA-256 mismatch: Computer=$computer_helper_sha256 LayerFS=$layerfs_helper_sha256"
fi
[[ "$layerfs_helper_sha256" == "$admitted_workload_binary" ]] ||
  die "LayerFS native helper bytes do not match admitted label"
docker run --rm --network none --entrypoint /bin/bash "$layerfs_image" -c \
  'printf "version\t"; /bin/bash --version | head -1; sha256sum /bin/bash /usr/local/bin/fs-benchmark-workload' \
  >"$run_dir/environment/layerfs-bash-helper-provenance.txt"
if [[ "$profile" != focused ]]; then
  docker run --rm --network none --entrypoint /bin/bash "$computer_image" -c \
    'printf "version\t"; /bin/bash --version | head -1; sha256sum /bin/bash /benchmark/fs-benchmark-workload' \
    >"$run_dir/environment/computer-bash-helper-provenance.txt"
fi

python3 - "$run_dir/terminal.json" <<'PY'
import json
import sys
from datetime import datetime, timezone
path = sys.argv[1]
terminal = {
    "schema": "fs-benchmark-pro-terminal-v1",
    "status": "complete",
    "finished_utc": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}
with open(path, "x", encoding="utf-8") as target:
    json.dump(terminal, target, indent=2, sort_keys=True)
    target.write("\n")
PY

if [[ "$profile" != focused ]]; then
  python3 "$compare" "$run_id"
fi

(
  cd "$run_dir"
  find . -type f ! -name raw-inventory.sha256 -print0 |
    sort -z |
    xargs -0 shasum -a 256 >raw-inventory.sha256
)
