#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root=${LAYERFS_SAME_COUNT_RESULTS_ROOT:-$repo/benchmark-results/fs-bench-pro/edit_same_count}

die() { printf 'fs-bench-pro same-count: %s\n' "$*" >&2; exit 2; }

self_check() {
  local scratch started elapsed
  started=$(python3 -c 'import time; print(time.monotonic_ns())')
  bash -n "$0"
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-same-count-self-check.XXXXXX")
  trap 'rm -rf -- "$scratch"' EXIT
  printf 'mod family { include!(r#"%s"#); } fn main() { family::self_check().unwrap(); assert_eq!(family::SCENARIOS.len(), 14); }\n' \
    "$here/families/edit_same_count.rs" >"$scratch/check.rs"
  rustc --edition=2021 -Awarnings "$scratch/check.rs" -o "$scratch/check"
  "$scratch/check"
  elapsed=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - started ))
  (( elapsed < 2000000000 )) || die "self-check exceeded two seconds"
  rm -rf -- "$scratch"
  trap - EXIT
  printf '{"schema":"fs-bench-pro-edit-same-count-self-check-v1","elapsed_ns":%s,"container_started":false,"status":"pass"}\n' "$elapsed"
}

if [[ ${1:-} == --self-check ]]; then
  [[ $# == 1 ]] || die "--self-check takes no arguments"
  self_check
  exit 0
fi

[[ $# -ge 2 ]] || die "usage: run-edit-same-count.sh RUN_ID CONTAINER_ID --case CASE --seed 1 --source baseline|candidate [--mode performance|verify] | RUN_ID CONTAINER_ID --all --source baseline|candidate --mode admission"
run_id=$1
container=$2
shift 2
[[ $run_id =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "invalid run id"

selection=
seed=
source_arm=
mode=performance
all=0
mode_set=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --case) [[ $# -ge 2 && -z $selection ]] || die "duplicate/missing --case"; selection=$2; shift 2 ;;
    --seed) [[ $# -ge 2 && -z $seed ]] || die "duplicate/missing --seed"; seed=$2; shift 2 ;;
    --source) [[ $# -ge 2 && -z $source_arm ]] || die "duplicate/missing --source"; source_arm=$2; shift 2 ;;
    --mode) [[ $# -ge 2 && $mode_set == 0 ]] || die "duplicate/missing --mode"; mode=$2; mode_set=1; shift 2 ;;
    --all) [[ $all == 0 ]] || die "duplicate --all"; all=1; shift ;;
    *) die "unknown argument: $1" ;;
  esac
done
[[ $source_arm == baseline || $source_arm == candidate ]] || die "explicit baseline/candidate source is required"
case "$mode" in
  performance)
    [[ $all == 0 && -n $selection && $seed =~ ^[123]$ ]] || die "performance requires one case and seed"
    ;;
  verify)
    [[ $all == 0 && -n $selection && ( -z $seed || $seed =~ ^[123]$ ) ]] || die "verify requires one case"
    seed=${seed:-1}
    ;;
  admission)
    [[ $all == 1 && -z $selection && -z $seed ]] || die "admission requires --all and no case/seed"
    ;;
  *) die "unknown mode: $mode" ;;
esac

for command in cargo docker nc python3 rustc shasum; do command -v "$command" >/dev/null || die "$command is required"; done
run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/performance" "$run_dir/verification" "$run_dir/scenarios" "$run_dir/oracles"

cargo build --release --manifest-path "$repo/Cargo.toml" -p fs-benchmark-pro >/dev/null
binary="$repo/target/release/fs-benchmark-pro"
oracle_workload="$run_dir/environment/fs-benchmark-workload"
rustc --edition=2021 -C opt-level=3 "$here/workload.rs" -o "$oracle_workload"
mapfile_path="$run_dir/environment/scenarios.tsv"
"$oracle_workload" same-count-list >"$mapfile_path"
[[ $(wc -l <"$mapfile_path" | tr -d ' ') == 14 ]] || die "family registry"
if [[ -n $selection && $selection != overwrite-fragmented-10b-ops-1000-proof ]]; then
  "$oracle_workload" same-count-resolve "$selection" >/dev/null || die "unknown case"
fi

fixture_256="$run_dir/fixture-256k"
"$binary" same-count-fixture "$fixture_256" >"$run_dir/environment/fixture-create.txt"
anchor_fixture=${LAYERFS_SAME_COUNT_ANCHOR_FIXTURE:-}
if [[ $mode == admission || $selection == small-edit || $selection == edit16 ]]; then
  [[ $anchor_fixture == /* && -f $anchor_fixture/payload.bin ]] || die "LAYERFS_SAME_COUNT_ANCHOR_FIXTURE must contain the registered payload.bin"
  [[ $(stat -f '%z' "$anchor_fixture/payload.bin" 2>/dev/null || stat -c '%s' "$anchor_fixture/payload.bin") == 33554432 ]] || die "registered anchor fixture length"
fi

current_seal=$("$here/run-namespace.sh" --source-seal)
container_seal=$(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container")
[[ $container_seal == "$current_seal" ]] || die "container/source seal mismatch"
docker inspect "$container" | python3 -c 'import json,sys; assert not any(x.get("Type") == "bind" for x in json.load(sys.stdin)[0].get("Mounts", []))' || die "product container has a bind mount"
docker exec "$container" test -c /dev/fuse 2>/dev/null || {
  [[ $(docker inspect -f '{{.State.Running}}' "$container") != true ]] || die "container lacks /dev/fuse"
}

git -C "$repo" status --short >"$run_dir/environment/git-status.txt"
git -C "$repo" log -1 --oneline --decorate >"$run_dir/environment/git-head.txt"
docker version >"$run_dir/environment/docker.txt"
docker inspect "$container" >"$run_dir/environment/container.json"
docker image inspect "$(docker inspect -f '{{.Image}}' "$container")" >"$run_dir/environment/image.json"
{ uname -a; sw_vers 2>/dev/null || true; } >"$run_dir/environment/host.txt"
printf '%s\n' "$current_seal" >"$run_dir/environment/source-seal.txt"
shasum -a 256 "$fixture_256/payload.bin" >"$run_dir/environment/fixtures.sha256"
if [[ -n $anchor_fixture ]]; then shasum -a 256 "$anchor_fixture/payload.bin" >>"$run_dir/environment/fixtures.sha256"; fi

daemon_endpoint=
daemon_capability=
container_id=
ensure_container() {
  if [[ $(docker inspect -f '{{.State.Running}}' "$container") != true ]]; then
    [[ $(docker inspect -f '{{.State.ExitCode}} {{.State.OOMKilled}}' "$container") == '0 false' ]] || die "container stopped abnormally"
    docker start "$container" >/dev/null
  fi
  container_id=$(docker inspect -f '{{.Id}}' "$container")
  for _ in $(seq 1 100); do
    daemon_endpoint=$(docker port "$container" 41273/tcp 2>/dev/null || true)
    if [[ $daemon_endpoint =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] \
      && docker exec "$container" test -s /run/layerfs/capability 2>/dev/null \
      && nc -z "${daemon_endpoint%:*}" "${daemon_endpoint##*:}" 2>/dev/null; then
      sleep 0.2
      daemon_capability=$(docker exec "$container" sh -c "od -An -tx1 -v /run/layerfs/capability | tr -d ' \\n'")
      [[ $daemon_capability =~ ^[0-9a-f]{64}$ ]] || die "daemon capability"
      return
    fi
    sleep 0.1
  done
  die "daemon readiness"
}

fixture_for() {
  case "$1" in small-edit|edit16) printf '%s\n' "$anchor_fixture" ;; *) printf '%s\n' "$fixture_256" ;; esac
}

run_performance() {
  local case_id=$1 sample_seed=$2 ordinal=$3 fixture sample_dir cache status
  fixture=$(fixture_for "$case_id")
  sample_dir="$run_dir/scenarios/$case_id/seed-$sample_seed"
  mkdir -p "$sample_dir"
  cache=$([[ $ordinal == 1 ]] && printf generated-first-sample-uncontrolled || printf generated-subsequent-sample-uncontrolled)
  ensure_container
  set +e
  perl -e 'alarm 6; exec @ARGV' env \
    LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
    LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
    LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
    LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
    "$binary" same-count-performance "$sample_dir/work" "$fixture" "$container_id" \
    "$case_id" "$sample_seed" "$source_arm" "$cache" >"$sample_dir/raw.jsonl" 2>"$sample_dir/supervisor.txt"
  status=$?
  set -e
  printf '%s\n' "$status" >"$sample_dir/exit-status.txt"
  [[ $status == 0 ]] || die "performance failed: $case_id seed $sample_seed"
  python3 - "$sample_dir/raw.jsonl" "$case_id" <<'PY'
import json, sys
rows=[json.loads(x) for x in open(sys.argv[1]) if x.startswith('{')]
rows=[x for x in rows if x.get('schema') == 'fs-bench-pro-edit-performance-v1']
assert len(rows)==1 and rows[0]['scenario_id']==sys.argv[2]
r=rows[0]
assert r['attempted_operations']==r['completed_operations']==r['operation_count']
assert r['initial_file_bytes']==r['final_file_bytes']
assert not r['oom'] and not r['timeout'] and r['swap_bytes']==0 and r['cleanup_status']=='pass'
assert r['process_peak_rss_bytes'] <= 128*1024*1024
if r['scenario_id']=='small-edit': assert r['commit_total_ns'] <= 6_000_000
elif r['scenario_id']=='edit16': assert r['complete_lifecycle_ns'] <= 200_000_000
else:
    floor=250 if r['position']=='distributed' else 500
    assert r['operations_per_second'] >= floor
    assert r['spool_allocated_bytes']==r['spool_live_bytes']+r['spool_superseded_bytes']
    assert r['piece_count'] and r['piece_height'] and r['piece_logical_charge_bytes'] and r['tree_visits']
PY
  grep '"schema":"fs-bench-pro-edit-performance-v1"' "$sample_dir/raw.jsonl" >>"$run_dir/performance/raw.jsonl"
}

oracle_digest() {
  local case_id=$1 sample_seed=$2 fixture oracle index
  fixture=$(fixture_for "$case_id")
  oracle="$run_dir/oracles/$case_id-seed-$sample_seed.bin"
  cp "$fixture/payload.bin" "$oracle"
  if [[ $case_id == small-edit ]]; then
    "$oracle_workload" edit "$oracle" 0 33554432
  elif [[ $case_id == edit16 ]]; then
    for index in $(seq 1 16); do "$oracle_workload" edit "$oracle" "$index" 33554432; done
  else
    "$oracle_workload" same-count-edit "$oracle" "$case_id" "$sample_seed" >/dev/null
  fi
  "$oracle_workload" digest "$oracle" | awk '{print $2}'
}

run_verify() {
  local case_id=$1 sample_seed=$2 fixture expected verify_dir status
  if [[ $case_id == overwrite-fragmented-10b-ops-1000-proof ]]; then
    ensure_container
    verify_dir="$run_dir/verification/fragmentation-seed-$sample_seed"
    mkdir -p "$verify_dir"
    set +e
    perl -e 'alarm 20; exec @ARGV' env LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
      LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
      LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
      LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
      "$binary" same-count-fragmentation-verify "$verify_dir/work" "$fixture_256" "$container_id" \
      "$source_arm" "$sample_seed" >"$verify_dir/raw.jsonl" 2>"$verify_dir/supervisor.txt"
    status=$?
  else
    fixture=$(fixture_for "$case_id")
    expected=$(oracle_digest "$case_id" "$sample_seed")
    ensure_container
    verify_dir="$run_dir/verification/$case_id-seed-$sample_seed"
    mkdir -p "$verify_dir"
    set +e
    perl -e 'alarm 20; exec @ARGV' env LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
      LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
      LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
      LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
      "$binary" same-count-verify "$verify_dir/work" "$fixture" "$container_id" "$case_id" \
      "$sample_seed" "$source_arm" "$expected" reused-first-sample-uncontrolled \
      >"$verify_dir/raw.jsonl" 2>"$verify_dir/supervisor.txt"
    status=$?
  fi
  set -e
  printf '%s\n' "$status" >"$verify_dir/exit-status.txt"
  [[ $status == 0 ]] || die "verification failed: $case_id seed $sample_seed"
}

if [[ $mode == performance ]]; then
  run_performance "$selection" "$seed" 1
elif [[ $mode == verify ]]; then
  run_verify "$selection" "$seed"
else
  ordinal=0
  while IFS=$'\t' read -r case_id _ <&3; do
    for sample_seed in 1 2 3; do ordinal=$((ordinal + 1)); run_performance "$case_id" "$sample_seed" "$ordinal"; done
  done 3<"$mapfile_path"
  while IFS=$'\t' read -r case_id _ <&3; do
    for sample_seed in 1 2 3; do run_verify "$case_id" "$sample_seed"; done
  done 3<"$mapfile_path"
  run_verify overwrite-fragmented-10b-ops-1000-proof 1
fi

python3 - "$run_dir" "$mode" "$source_arm" <<'PY'
import json, statistics, sys
from pathlib import Path
root, mode, source = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
rows=[]
raw=root/'performance/raw.jsonl'
if raw.exists(): rows=[json.loads(x) for x in raw.read_text().splitlines() if x]
summary={'schema':'fs-bench-pro-edit-same-count-summary-v1','mode':mode,'source_arm':source,'samples':len(rows),'status':'pass'}
if rows:
    grouped={}
    for row in rows: grouped.setdefault(row['scenario_id'],[]).append(row)
    summary['medians']={case:{'operations_per_second':int(statistics.median(x['operations_per_second'] for x in values)),'complete_lifecycle_ns':int(statistics.median(x['complete_lifecycle_ns'] for x in values)),'process_peak_rss_bytes':int(statistics.median(x['process_peak_rss_bytes'] for x in values))} for case,values in sorted(grouped.items())}
    summary['family_complete_lifecycle_ns']=sum(x['complete_lifecycle_ns'] for x in rows)
    if mode=='admission':
        assert len(rows)==42
        assert summary['family_complete_lifecycle_ns'] <= 6_000_000_000
        assert max(x['process_peak_rss_bytes'] for x in rows) <= 128*1024*1024
        assert max(x['process_peak_rss_bytes'] for x in summary['medians'].values()) <= 101_980_569
(root/'summary.json').write_text(json.dumps(summary,sort_keys=True,separators=(',',':'))+'\n')
PY

if [[ -n ${LAYERFS_SAME_COUNT_BASELINE_RUN:-} ]]; then
  [[ $mode == admission && $source_arm == candidate ]] || die "baseline comparison requires candidate admission"
  python3 - "$LAYERFS_SAME_COUNT_BASELINE_RUN" "$run_dir" <<'PY'
import json, statistics, sys
from pathlib import Path
baseline, candidate = map(Path, sys.argv[1:])
def load(root):
    rows=[json.loads(x) for x in (root/'performance/raw.jsonl').read_text().splitlines()]
    assert len(rows)==42
    return rows
def medians(rows):
    grouped={}
    for row in rows: grouped.setdefault(row['scenario_id'],[]).append(row['complete_lifecycle_ns'])
    return {case:statistics.median(values) for case,values in grouped.items()}
brows,crows=load(baseline),load(candidate)
bmed,cmed=medians(brows),medians(crows)
assert bmed.keys()==cmed.keys()
ratios={case:cmed[case]/bmed[case] for case in bmed}
assert max(ratios.values()) <= 1.05
paired=sum(x['complete_lifecycle_ns'] for x in brows+crows)
assert paired <= 12_000_000_000
report={'schema':'fs-bench-pro-edit-same-count-comparison-v1','baseline_run':str(baseline),'candidate_run':str(candidate),'complete_lifecycle_ratios':ratios,'maximum_ratio':max(ratios.values()),'paired_complete_lifecycle_ns':paired,'target_ratio':1.05,'disposition_required_ratio':1.10,'status':'pass'}
(candidate/'comparison.json').write_text(json.dumps(report,sort_keys=True,separators=(',',':'))+'\n')
PY
fi

if [[ $(docker inspect -f '{{.State.Running}}' "$container") == true ]]; then docker stop "$container" >/dev/null; fi
docker inspect "$container" >"$run_dir/environment/container-after.json"
printf '{"schema":"fs-bench-pro-edit-same-count-status-v1","mode":"%s","source_arm":"%s","status":"pass"}\n' "$mode" "$source_arm" >"$run_dir/run-status.json"
(cd "$run_dir" && find . -type f ! -name evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
printf 'PASS %s\n' "$run_dir"
