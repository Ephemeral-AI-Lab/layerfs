#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root=${LAYERFS_SAME_COUNT_RESULTS_ROOT:-$repo/benchmark-results/fs-bench-pro/edit-same-count}
prepared_root=${LAYERFS_SAME_COUNT_PREPARED_ROOT:-${TMPDIR:-/tmp}/layerfs-fs-bench-pro-edit-same-count}
invocation_started=$(python3 -c 'import time; print(time.monotonic_ns())')

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

prepare_assets() {
  local container=$1 source_seal product_seal harness_seal prepared stage workload_sha custody
  source_seal=$("$here/run-namespace.sh" --source-seal)
  product_seal=$("$here/run-namespace.sh" --product-seal)
  harness_seal=$("$here/run-namespace.sh" --harness-seal)
  [[ $(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container") == "$source_seal" ]] || die "container/source seal mismatch"
  prepared="$prepared_root/$source_seal"
  if [[ -x $prepared/fs-benchmark-pro && -x $prepared/fs-benchmark-workload && -f $prepared/fixture-256k/payload.bin && -d $prepared/issue14-r003-custody ]]; then
    printf 'PASS prepared %s\n' "$prepared"
    return
  fi
  mkdir -p "$prepared_root"
  stage=$(mktemp -d "$prepared_root/.prepare.XXXXXX")
  trap 'rm -rf -- "$stage"' EXIT
  cargo build --release --manifest-path "$repo/Cargo.toml" -p fs-benchmark-pro >/dev/null
  cp "$repo/target/release/fs-benchmark-pro" "$stage/fs-benchmark-pro"
  rustc --edition=2021 -C opt-level=3 "$here/workload.rs" -o "$stage/fs-benchmark-workload"
  "$stage/fs-benchmark-pro" same-count-fixture "$stage/fixture-256k" >"$stage/fixture-create.txt"
  workload_sha=$(shasum -a 256 "$here/workload.rs" | awk '{print $1}')
  {
    printf 'source_commit=%s\n' "$(git -C "$repo" rev-parse HEAD)"
    printf 'source_tree=%s\n' "$(git -C "$repo" rev-parse HEAD^{tree})"
    printf 'source_seal=%s\n' "$source_seal"
    printf 'product_seal=%s\n' "$product_seal"
    printf 'harness_seal=%s\n' "$harness_seal"
    printf 'workload_sha256=%s\n' "$workload_sha"
    printf 'image_id=%s\n' "$(docker inspect -f '{{.Image}}' "$container")"
    printf 'image_revision=%s\n' "$(docker inspect -f '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$container")"
  } >"$stage/identity.txt"
  docker inspect "$container" >"$stage/container.json"
  docker image inspect "$(docker inspect -f '{{.Image}}' "$container")" >"$stage/image.json"
  docker version >"$stage/docker.txt"
  { uname -a; sw_vers 2>/dev/null || true; } >"$stage/host.txt"
  custody="$repo/benchmark-results/fs-bench-pro/edit-engine-acceptance/issue14-terminal-r003-20260903/environment"
  [[ -d $custody ]] || die "issue #14 r003 custody reference is required"
  cp -R "$custody" "$stage/issue14-r003-custody"
  mv "$stage" "$prepared"
  trap - EXIT
  printf 'PASS prepared %s\n' "$prepared"
}

if [[ ${1:-} == --prepare ]]; then
  [[ $# == 2 ]] || die "usage: run-edit-same-count.sh --prepare CONTAINER_ID"
  for command in cargo docker rustc shasum; do command -v "$command" >/dev/null || die "$command is required"; done
  prepare_assets "$2"
  exit 0
fi

[[ $# -ge 2 ]] || die "usage: run-edit-same-count.sh --prepare CONTAINER_ID | RUN_ID CONTAINER_ID --case CASE --seed 1 --source ARM [--mode performance|verify] | RUN_ID CONTAINER_A --all --source a-a-repeatability --mode admission --paired-container CONTAINER_B"
run_id=$1
container=$2
shift 2
[[ $run_id =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "invalid run id"

selection=
seed=
source_arm=
paired_container=
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
    --paired-container) [[ $# -ge 2 && -z $paired_container ]] || die "duplicate/missing --paired-container"; paired_container=$2; shift 2 ;;
    *) die "unknown argument: $1" ;;
  esac
done
[[ $source_arm == baseline || $source_arm == candidate || $source_arm == repeat-a || $source_arm == repeat-b || $source_arm == a-a-repeatability ]] || die "explicit source arm is required"
case "$mode" in
  performance)
    [[ $all == 0 && -n $selection && $seed =~ ^[123]$ && -z $paired_container && $source_arm != a-a-repeatability ]] || die "performance requires one case, seed, and one concrete source arm"
    ;;
  verify)
    [[ $all == 0 && -n $selection && ( -z $seed || $seed =~ ^[123]$ ) && -z $paired_container && $source_arm != a-a-repeatability ]] || die "verify requires one case and one concrete source arm"
    seed=${seed:-1}
    ;;
  admission)
    [[ $all == 1 && -z $selection && -z $seed && -n $paired_container && $source_arm == a-a-repeatability ]] || die "admission requires --all --source a-a-repeatability --paired-container and no case/seed"
    ;;
  *) die "unknown mode: $mode" ;;
esac

for command in docker nc python3 shasum; do command -v "$command" >/dev/null || die "$command is required"; done
current_seal=$("$here/run-namespace.sh" --source-seal)
prepared="$prepared_root/$current_seal"
[[ -x $prepared/fs-benchmark-pro && -x $prepared/fs-benchmark-workload && -f $prepared/fixture-256k/payload.bin ]] || die "run --prepare for this source/container identity first"
binary="$prepared/fs-benchmark-pro"
oracle_workload="$prepared/fs-benchmark-workload"
fixture_256="$prepared/fixture-256k"
run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/performance" "$run_dir/verification" "$run_dir/scenarios" "$run_dir/oracles"

mapfile_path="$run_dir/environment/scenarios.tsv"
"$oracle_workload" same-count-list >"$mapfile_path"
[[ $(wc -l <"$mapfile_path" | tr -d ' ') == 14 ]] || die "family registry"
if [[ -n $selection && $selection != overwrite-fragmented-10b-ops-1000-proof ]]; then
  "$oracle_workload" same-count-resolve "$selection" >/dev/null || die "unknown case"
fi

anchor_fixture=${LAYERFS_SAME_COUNT_ANCHOR_FIXTURE:-}
if [[ $mode == admission || $selection == small-edit || $selection == edit16 ]]; then
  [[ $anchor_fixture == /* && -f $anchor_fixture/payload.bin ]] || die "LAYERFS_SAME_COUNT_ANCHOR_FIXTURE must contain the registered payload.bin"
  [[ $(stat -f '%z' "$anchor_fixture/payload.bin" 2>/dev/null || stat -c '%s' "$anchor_fixture/payload.bin") == 33554432 ]] || die "registered anchor fixture length"
fi

container_seal=$(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container")
[[ $container_seal == "$current_seal" ]] || die "container/source seal mismatch"
docker inspect "$container" | python3 -c 'import json,sys; assert not any(x.get("Type") == "bind" for x in json.load(sys.stdin)[0].get("Mounts", []))' || die "product container has a bind mount"
docker exec "$container" test -c /dev/fuse 2>/dev/null || {
  [[ $(docker inspect -f '{{.State.Running}}' "$container") != true ]] || die "container lacks /dev/fuse"
}
if [[ -n $paired_container ]]; then
  [[ $(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$paired_container") == "$current_seal" ]] || die "paired container/source seal mismatch"
  docker inspect "$paired_container" | python3 -c 'import json,sys; assert not any(x.get("Type") == "bind" for x in json.load(sys.stdin)[0].get("Mounts", []))' || die "paired product container has a bind mount"
  [[ $(docker inspect -f '{{.Image}}' "$paired_container") == $(docker inspect -f '{{.Image}}' "$container") ]] || die "A/A repeatability requires identical image identity"
fi

cp "$prepared/identity.txt" "$run_dir/environment/prepared-identity.txt"
cp "$prepared/docker.txt" "$run_dir/environment/docker.txt"
cp "$prepared/host.txt" "$run_dir/environment/host.txt"
cp "$prepared/container.json" "$run_dir/environment/prepared-container.json"
cp "$prepared/image.json" "$run_dir/environment/image.json"
cp -R "$prepared/issue14-r003-custody" "$run_dir/environment/issue14-r003-custody"
docker inspect "$container" >"$run_dir/environment/container.json"
if [[ -n $paired_container ]]; then docker inspect "$paired_container" >"$run_dir/environment/paired-container.json"; fi
printf '%s\n' "$current_seal" >"$run_dir/environment/source-seal.txt"
shasum -a 256 "$fixture_256/payload.bin" >"$run_dir/environment/fixtures.sha256"
if [[ -n $anchor_fixture ]]; then shasum -a 256 "$anchor_fixture/payload.bin" >>"$run_dir/environment/fixtures.sha256"; fi

daemon_endpoint=
daemon_capability=
container_id=
ensure_container() {
  local active_container=$1 stopped
  if [[ $(docker inspect -f '{{.State.Running}}' "$active_container") != true ]]; then
    stopped=$(docker inspect -f '{{.State.ExitCode}} {{.State.OOMKilled}}' "$active_container")
    [[ $stopped == '0 false' || $stopped == '143 false' ]] || die "container stopped abnormally"
    docker start "$active_container" >/dev/null
  fi
  container_id=$(docker inspect -f '{{.Id}}' "$active_container")
  for _ in $(seq 1 100); do
    daemon_endpoint=$(docker port "$active_container" 41273/tcp 2>/dev/null || true)
    if [[ $daemon_endpoint =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] \
      && docker exec "$active_container" test -s /run/layerfs/capability 2>/dev/null \
      && nc -z "${daemon_endpoint%:*}" "${daemon_endpoint##*:}" 2>/dev/null; then
      sleep 0.2
      daemon_capability=$(docker exec "$active_container" sh -c "od -An -tx1 -v /run/layerfs/capability | tr -d ' \\n'")
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
  local case_id=$1 sample_seed=$2 ordinal=$3 active_container=$4 active_arm=$5 fixture sample_dir cache status
  fixture=$(fixture_for "$case_id")
  sample_dir="$run_dir/scenarios/$case_id/$active_arm/seed-$sample_seed"
  mkdir -p "$sample_dir"
  cache=$([[ $ordinal == 1 ]] && printf generated-first-sample-uncontrolled || printf generated-subsequent-sample-uncontrolled)
  ensure_container "$active_container"
  set +e
  perl -e 'alarm 6; exec @ARGV' env \
    LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
    LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
    LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
    LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
    "$binary" same-count-performance "$sample_dir/work" "$fixture" "$container_id" \
    "$case_id" "$sample_seed" "$active_arm" "$cache" >"$sample_dir/raw.jsonl" 2>"$sample_dir/supervisor.txt"
  status=$?
  set -e
  printf '%s\n' "$status" >"$sample_dir/exit-status.txt"
  [[ $status == 0 ]] || die "performance failed: $case_id seed $sample_seed"
  python3 - "$sample_dir/raw.jsonl" "$case_id" "$sample_dir/classification.json" <<'PY'
import json, sys
rows=[json.loads(x) for x in open(sys.argv[1]) if x.startswith('{')]
rows=[x for x in rows if x.get('schema') == 'fs-bench-pro-edit-performance-v1']
assert len(rows)==1 and rows[0]['scenario_id']==sys.argv[2]
r=rows[0]
assert r['attempted_operations']==r['completed_operations']==r['operation_count']
assert r['initial_file_bytes']==r['final_file_bytes']
assert not r['oom'] and not r['timeout'] and r['swap_bytes']==0 and r['cleanup_status']=='pass'
states=[]
def upper(value,target,tolerated,hard,name):
    state='target-pass' if value<=target else 'tolerated-pass' if value<=tolerated else 'no-go' if value<=hard else 'hard-failure'
    states.append((state,name,value,target,tolerated,hard))
def lower(value,target,tolerated,name):
    state='target-pass' if value>=target else 'tolerated-pass' if value>=tolerated else 'no-go'
    states.append((state,name,value,target,tolerated,None))
upper(r['process_peak_rss_bytes'],97_124_352,101_980_569,128*1024*1024,'rss')
if r['scenario_id']=='small-edit': upper(r['commit_total_ns'],4_503_000,4_953_300,6_000_000,'small_edit_commit')
elif r['scenario_id']=='edit16': upper(r['complete_lifecycle_ns'],156_446_000,172_090_600,200_000_000,'edit16_complete')
else:
    target,tolerated=(250,225) if r['position']=='distributed' else (500,450)
    lower(r['operations_per_second'],target,tolerated,'operations_per_second')
    assert r['spool_allocated_bytes']==r['spool_live_bytes']+r['spool_superseded_bytes']
    assert r['piece_count'] and r['piece_height'] and r['piece_logical_charge_bytes'] and r['tree_visits']
rank={'target-pass':0,'tolerated-pass':1,'no-go':2,'hard-failure':3}
overall=max(states,key=lambda x:rank[x[0]])[0]
json.dump({'schema':'fs-bench-pro-edit-same-count-classification-v1','scenario_id':r['scenario_id'],'seed':r['seed'],'metrics':[{'status':s,'metric':n,'value':v,'target':t,'tolerated':q,'hard':h} for s,n,v,t,q,h in states],'status':overall},open(sys.argv[3],'w'),sort_keys=True,separators=(',',':'))
open(sys.argv[3],'a').write('\n')
assert overall in ('target-pass','tolerated-pass')
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
  local case_id=$1 sample_seed=$2 active_container=$3 active_arm=$4 fixture expected verify_dir status
  if [[ $case_id == overwrite-fragmented-10b-ops-1000-proof ]]; then
    fragment_oracle="$run_dir/oracles/fragmentation-seed-$sample_seed"
    python3 - "$fixture_256/payload.bin" "$fragment_oracle" "$sample_seed" <<'PY'
from pathlib import Path
import sys
fixture, root, seed = Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3])
base=fixture.read_bytes()
assert len(base)==256*1024 and seed in (1,2,3)
for cohort in ('increasing','descending','hotspot'):
  for count in (100,1000):
    data=bytearray(base); covered=[False]*len(data)
    for operation in range(count):
      if cohort=='increasing': offset=operation*20
      elif cohort=='descending': offset=(999-operation)*20
      else: offset=((operation*7919+seed*101)%(64*1024-10))+96*1024
      replacement=bytes(((((offset+i)*37+(operation+1)*101+seed*53)%251)^0x5a) for i in range(10))
      data[offset:offset+10]=replacement
      covered[offset:offset+10]=[True]*10
    case=root/f'{cohort}-{count}'; case.mkdir(parents=True,exist_ok=True)
    (case/'payload.bin').write_bytes(data)
    ranges=[]; i=0
    while i<len(covered):
      if not covered[i]: i+=1; continue
      end=i+1
      while end<len(covered) and covered[end]: end+=1
      ranges.append(f'{i} {end}')
      i=end
    (case/'ranges.txt').write_text('\n'.join(ranges)+'\n')
PY
    ensure_container "$active_container"
    verify_dir="$run_dir/verification/$active_arm-fragmentation-seed-$sample_seed"
    mkdir -p "$verify_dir"
    set +e
    perl -e 'alarm 20; exec @ARGV' env LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload LAYERFS_BENCH_ORACLE_WORKLOAD="$oracle_workload" \
      LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
      LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
      LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
      "$binary" same-count-fragmentation-verify "$verify_dir/work" "$fixture_256" "$fragment_oracle" "$container_id" \
      "$active_arm" "$sample_seed" >"$verify_dir/raw.jsonl" 2>"$verify_dir/supervisor.txt"
    status=$?
  else
    fixture=$(fixture_for "$case_id")
    expected=$(oracle_digest "$case_id" "$sample_seed")
    ensure_container "$active_container"
    verify_dir="$run_dir/verification/$active_arm-$case_id-seed-$sample_seed"
    mkdir -p "$verify_dir"
    set +e
    perl -e 'alarm 20; exec @ARGV' env LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
      LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
      LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
      LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
      "$binary" same-count-verify "$verify_dir/work" "$fixture" "$container_id" "$case_id" \
      "$sample_seed" "$active_arm" "$expected" reused-first-sample-uncontrolled \
      >"$verify_dir/raw.jsonl" 2>"$verify_dir/supervisor.txt"
    status=$?
  fi
  set -e
  printf '%s\n' "$status" >"$verify_dir/exit-status.txt"
  [[ $status == 0 ]] || die "verification failed: $case_id seed $sample_seed"
}

if [[ $mode == performance ]]; then
  run_performance "$selection" "$seed" 1 "$container" "$source_arm"
elif [[ $mode == verify ]]; then
  run_verify "$selection" "$seed" "$container" "$source_arm"
else
  ordinal=0
  while IFS=$'\t' read -r case_id _ <&3; do
    for sample_seed in 1 2 3; do
      if [[ $sample_seed == 2 ]]; then
        order=("$paired_container repeat-b" "$container repeat-a")
      else
        order=("$container repeat-a" "$paired_container repeat-b")
      fi
      for pair in "${order[@]}"; do
        ordinal=$((ordinal + 1))
        active=${pair% *}
        inactive=$([[ $active == "$container" ]] && printf '%s' "$paired_container" || printf '%s' "$container")
        if [[ $(docker inspect -f '{{.State.Running}}' "$inactive") == true ]]; then docker stop "$inactive" >/dev/null; fi
        run_performance "$case_id" "$sample_seed" "$ordinal" "$active" "${pair##* }"
      done
    done
  done 3<"$mapfile_path"
  run_verify overwrite-fragmented-10b-ops-1000-proof 1 "$container" repeat-a
  run_verify overwrite-fragmented-10b-ops-1000-proof 1 "$paired_container" repeat-b
fi

python3 - "$run_dir" "$mode" "$source_arm" <<'PY'
import json, statistics, sys
from pathlib import Path
root, mode, source = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
rows=[]
raw=root/'performance/raw.jsonl'
if raw.exists(): rows=[json.loads(x) for x in raw.read_text().splitlines() if x]
summary={'schema':'fs-bench-pro-edit-same-count-summary-v2','mode':mode,'source_identity':source,'samples':len(rows),'status':'target-pass'}
phases=['workspace_create_ns','execution_ns','commit_api_ns','layerstack_visible_ns','workspace_end_ns','complete_lifecycle_ns']
counters=['fuse_kernel_write_requests','fuse_kernel_write_bytes','spool_write_bytes','candidate_objects','candidate_bytes','inserted_objects','reused_objects','commit_payload_bytes_read','commit_cdc_bytes_scanned','tree_visits','metric_nodes_scanned']
def med(values,field): return float(statistics.median(x[field] for x in values))
def ratio(left,right): return right/left if left else (1.0 if right==0 else float('inf'))
if mode=='admission':
    assert len(rows)==84
    arms={arm:[x for x in rows if x['source_arm']==arm] for arm in ('repeat-a','repeat-b')}
    assert all(len(values)==42 for values in arms.values())
    medians={}
    for arm,values in arms.items():
        medians[arm]={}
        for case in sorted({x['scenario_id'] for x in values}):
            selected=[x for x in values if x['scenario_id']==case]
            medians[arm][case]={field:med(selected,field) for field in phases+counters+['operations_per_second','process_peak_rss_bytes']}
    walls={arm:sum(x['complete_lifecycle_ns'] for x in values) for arm,values in arms.items()}
    paired=sum(walls.values())
    wall_status='target-pass' if max(walls.values())<=3_000_000_000 and paired<=6_000_000_000 else 'tolerated-pass' if max(walls.values())<=3_300_000_000 and paired<=6_600_000_000 else 'no-go' if max(walls.values())<=6_000_000_000 and paired<=12_000_000_000 else 'hard-failure'
    ratios={case:ratio(medians['repeat-a'][case]['complete_lifecycle_ns'],medians['repeat-b'][case]['complete_lifecycle_ns']) for case in medians['repeat-a']}
    ratio_status='target-pass' if max(ratios.values())<=1.05 else 'tolerated-pass' if max(ratios.values())<=1.10 else 'no-go'
    dispositions={}
    for case,value in ratios.items():
        if value<=1.05: continue
        phase_rows={field:{'ratio':ratio(medians['repeat-a'][case][field],medians['repeat-b'][case][field]),'under_2ms_exception':field!='complete_lifecycle_ns' and medians['repeat-a'][case][field]<2_000_000} for field in phases}
        counter_rows={field:ratio(medians['repeat-a'][case][field],medians['repeat-b'][case][field]) for field in counters}
        dispositions[case]={'reason':'identical-source A/A scheduling variance; no improvement claim','complete_ratio':value,'phase_ratios':phase_rows,'counter_ratios':counter_rows}
    summary.update({'comparison_type':'A/A repeatability','medians':medians,'arm_complete_lifecycle_ns':walls,'paired_complete_lifecycle_ns':paired,'family_wall_status':wall_status,'complete_lifecycle_ratios':ratios,'ratio_status':ratio_status,'phase_counter_dispositions':dispositions})
    order={'target-pass':0,'tolerated-pass':1,'no-go':2,'hard-failure':3}
    summary['status']=max((wall_status,ratio_status),key=order.get)
else:
    grouped={}
    for row in rows: grouped.setdefault(row['scenario_id'],[]).append(row)
    summary['medians']={case:{field:med(values,field) for field in phases+counters+['operations_per_second','process_peak_rss_bytes']} for case,values in sorted(grouped.items())}
(root/'summary.json').write_text(json.dumps(summary,sort_keys=True,separators=(',',':'))+'\n')
if summary['status'] not in ('target-pass','tolerated-pass'): raise SystemExit(f"same-count admission {summary['status']}")
PY

overall_status=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["status"])' "$run_dir/summary.json")
if [[ $(docker inspect -f '{{.State.Running}}' "$container") == true ]]; then docker stop "$container" >/dev/null; fi
if [[ -n $paired_container && $(docker inspect -f '{{.State.Running}}' "$paired_container") == true ]]; then docker stop "$paired_container" >/dev/null; fi
docker inspect "$container" >"$run_dir/environment/container-after.json"
if [[ -n $paired_container ]]; then docker inspect "$paired_container" >"$run_dir/environment/paired-container-after.json"; fi
printf '{"schema":"fs-bench-pro-edit-same-count-status-v2","mode":"%s","source_identity":"%s","status":"%s","admission_eligible":true}\n' "$mode" "$source_arm" "$overall_status" >"$run_dir/run-status.json"
(invocation_elapsed=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - invocation_started )); printf '%s\n' "$invocation_elapsed" >"$run_dir/environment/external-wall-ns.txt"; if [[ $mode == performance && $invocation_elapsed -gt 2000000000 ]]; then printf '{"schema":"fs-bench-pro-edit-same-count-status-v2","mode":"%s","source_identity":"%s","status":"no-go","admission_eligible":false,"reason":"selected external wall exceeded 2 seconds","external_wall_ns":%s}\n' "$mode" "$source_arm" "$invocation_elapsed" >"$run_dir/run-status.json"; fi)
(cd "$run_dir" && find . -type f ! -name evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
[[ $(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["admission_eligible"])' "$run_dir/run-status.json") == True ]] || die "selected external wall gate"
printf 'PASS %s\n' "$run_dir"
