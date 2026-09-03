#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
family_kind=${LAYERFS_EDIT_FAMILY:-same-count}
[[ $family_kind == same-count || $family_kind == count-changing ]] || { printf 'unknown edit family: %s\n' "$family_kind" >&2; exit 2; }
if [[ $family_kind == same-count ]]; then
  results_root=${LAYERFS_SAME_COUNT_RESULTS_ROOT:-$repo/benchmark-results/fs-bench-pro/edit-same-count}
else
  results_root=${LAYERFS_COUNT_CHANGING_RESULTS_ROOT:-$repo/benchmark-results/fs-bench-pro/edit-count-changing}
fi
prepared_root=${LAYERFS_SAME_COUNT_PREPARED_ROOT:-${TMPDIR:-/tmp}/layerfs-fs-bench-pro-edit-same-count}
invocation_started=$(python3 -c 'import time; print(time.monotonic_ns())')
invocation_argv=("$@")
performance_external_wall_ns=0
verification_external_wall_ns=0
verification_failure=
control_external_wall_ns=0
count_changing_verifiers=(insert-middle-4k-on-8m-proof delete-middle-4k-on-8m-proof rewrite-full-grow-8m-to-12m-proof rewrite-full-shrink-8m-to-4m-proof)

die() { printf 'fs-bench-pro %s: %s\n' "$family_kind" "$*" >&2; exit 2; }

static_edit_proof() {
  python3 - "$repo/crates/layerfs-workspace/src/file_edit.rs" <<'PY'
import json, sys
text=open(sys.argv[1]).read()
start=text.index('pub(crate) struct PieceTree')
end=text.index('impl PieceTree', start)
shape=text[start:end]
for forbidden in ('BTreeMap', 'interval_map', 'materializ', 'offset_key', 'ForbiddenEditCounters'):
    assert forbidden not in text, forbidden
assert 'root: Link' in shape and 'serial: u64' in shape
print(json.dumps({'schema':'layerfs-edit-static-proof-v1','representation':'implicit-offset-persistent-piece-tree','absent_entry_points':['complete-interval-map-clone','full-interval-map-rescan','later-offset-rekey','complete-file-materialization'],'status':'pass'},sort_keys=True,separators=(',',':')))
PY
}

self_check() {
  local scratch started elapsed family_file expected
  started=$(python3 -c 'import time; print(time.monotonic_ns())')
  bash -n "$0"
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-same-count-self-check.XXXXXX")
  trap 'rm -rf -- "$scratch"' EXIT
  if [[ $family_kind == same-count ]]; then family_file=edit_same_count.rs; expected=14; else family_file=edit_count_changing.rs; expected=25; fi
  printf 'mod family { include!(r#"%s"#); } fn main() { family::self_check().unwrap(); assert_eq!(family::SCENARIOS.len(), %s); }\n' \
    "$here/families/$family_file" "$expected" >"$scratch/check.rs"
  rustc --edition=2021 -Awarnings "$scratch/check.rs" -o "$scratch/check"
  "$scratch/check"
  static_edit_proof >/dev/null
  python3 - <<'PY'
import statistics
def symmetric_ratio(left, right):
    return max(left / right, right / left) if left and right else (1.0 if left == right else float('inf'))
assert symmetric_ratio(31_459_250, 27_816_750) > 1.10
assert symmetric_ratio(100, 105) == 1.05
assert statistics.median([5_648_958, 4_100_000, 4_200_000]) == 4_200_000
PY
  elapsed=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - started ))
  (( elapsed < 2000000000 )) || die "self-check exceeded two seconds"
  rm -rf -- "$scratch"
  trap - EXIT
  printf '{"schema":"fs-bench-pro-edit-%s-self-check-v1","elapsed_ns":%s,"container_started":false,"status":"pass"}\n' "$family_kind" "$elapsed"
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
  workload_sha=$(shasum -a 256 "$here/workload.rs" | awk '{print $1}')
  [[ $(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container") == "$source_seal" ]] || die "container/source seal mismatch"
  prepared="$prepared_root/$source_seal"
  if [[ -x $prepared/fs-benchmark-pro && -x $prepared/fs-benchmark-workload && -f $prepared/fixture-256k/payload.bin && -f $prepared/fixture-8m/payload.bin && -f $prepared/issue14-r005-custody/evidence.sha256 ]] \
    && grep -Fx "source_seal=$source_seal" "$prepared/identity.txt" >/dev/null \
    && grep -Fx "product_seal=$product_seal" "$prepared/identity.txt" >/dev/null \
    && grep -Fx "harness_seal=$harness_seal" "$prepared/identity.txt" >/dev/null \
    && grep -Fx "workload_sha256=$workload_sha" "$prepared/identity.txt" >/dev/null; then
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
  mkdir "$stage/fixture-8m"
  python3 - "$stage/fixture-8m/payload.bin" <<'PY'
import sys
with open(sys.argv[1],'wb') as output:
    for base in range(0,8*1024*1024,64*1024):
        output.write(bytes((((base+i)*29+(base+i)//7)%251) for i in range(64*1024)))
PY
  static_edit_proof >"$stage/static-edit-proof.json"
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
  custody="$repo/benchmark-results/fs-bench-pro/edit-engine-acceptance/issue14-terminal-r005-20260903"
  [[ -f $custody/evidence.sha256 ]] || die "authoritative issue #14 r005 custody is required"
  mkdir "$stage/issue14-r005-custody"
  cp "$custody/evidence.sha256" "$stage/issue14-r005-custody/evidence.sha256"
  cp -R "$custody/environment" "$stage/issue14-r005-custody/environment"
  printf 'issue14_r005_evidence_sha256=%s\n' "$(shasum -a 256 "$custody/evidence.sha256" | awk '{print $1}')" >>"$stage/identity.txt"
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

[[ $# -ge 2 ]] || die "usage: run-edit-same-count.sh --prepare CONTAINER_ID | RUN_ID CONTAINER_ID --case CASE --seed 1 --source ARM [--mode performance|verify] | RUN_ID CONTAINER_A --case CASE --source a-a-repeatability --mode repeatability --paired-container CONTAINER_B | RUN_ID CONTAINER_A --all --source a-a-repeatability --mode admission --paired-container CONTAINER_B"
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
[[ $source_arm == baseline || $source_arm == candidate || $source_arm == repeat-a || $source_arm == repeat-b || $source_arm == a-a-repeatability || $source_arm == baseline-candidate ]] || die "explicit source arm is required"
case "$mode" in
  performance)
    [[ $all == 0 && -n $selection && $seed =~ ^[123]$ && -z $paired_container && $source_arm != a-a-repeatability ]] || die "performance requires one case, seed, and one concrete source arm"
    ;;
  verify)
    [[ $all == 0 && -n $selection && ( -z $seed || $seed =~ ^[123]$ ) && -z $paired_container && $source_arm != a-a-repeatability ]] || die "verify requires one case and one concrete source arm"
    seed=${seed:-1}
    ;;
  admission)
    [[ $all == 1 && -z $selection && -z $seed && -n $paired_container && ( $source_arm == a-a-repeatability || ( $family_kind == count-changing && $source_arm == baseline-candidate ) ) ]] || die "admission requires --all, a supported paired source, --paired-container, and no case/seed"
    ;;
  repeatability)
    [[ $all == 0 && -n $selection && -z $seed && -n $paired_container && $source_arm == a-a-repeatability ]] || die "repeatability requires one case, no seed, --source a-a-repeatability, and --paired-container"
    ;;
  *) die "unknown mode: $mode" ;;
esac

for command in docker nc python3 shasum; do command -v "$command" >/dev/null || die "$command is required"; done
current_seal=$("$here/run-namespace.sh" --source-seal)
prepared="$prepared_root/$current_seal"
[[ -x $prepared/fs-benchmark-pro && -x $prepared/fs-benchmark-workload && -f $prepared/fixture-256k/payload.bin && -f $prepared/fixture-8m/payload.bin ]] || die "run --prepare for this source/container identity first"
binary="$prepared/fs-benchmark-pro"
oracle_workload="$prepared/fs-benchmark-workload"
fixture_256="$prepared/fixture-256k"
fixture_8m="$prepared/fixture-8m"
run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/performance" "$run_dir/controls" "$run_dir/verification" "$run_dir/scenarios" "$run_dir/oracles"

mapfile_path="$run_dir/environment/scenarios.tsv"
if [[ $family_kind == same-count ]]; then
  "$oracle_workload" same-count-list >"$mapfile_path"
  [[ $(wc -l <"$mapfile_path" | tr -d ' ') == 14 ]] || die "family registry"
  if [[ -n $selection && $selection != overwrite-fragmented-10b-ops-1000-proof ]]; then "$oracle_workload" same-count-resolve "$selection" >/dev/null || die "unknown case"; fi
  anchor_fixture=${LAYERFS_SAME_COUNT_ANCHOR_FIXTURE:-}
  needs_anchor=$([[ $mode == admission || $selection == small-edit || $selection == edit16 ]] && printf true || printf false)
else
  "$oracle_workload" count-changing-list >"$mapfile_path"
  [[ $(wc -l <"$mapfile_path" | tr -d ' ') == 25 ]] || die "family registry"
  if [[ -n $selection ]] && ! printf '%s\n' "${count_changing_verifiers[@]}" | grep -Fx "$selection" >/dev/null; then "$oracle_workload" count-changing-resolve "$selection" >/dev/null || die "unknown case"; fi
  anchor_fixture=${LAYERFS_COUNT_CHANGING_ANCHOR_FIXTURE:-${LAYERFS_SAME_COUNT_ANCHOR_FIXTURE:-}}
  needs_anchor=$([[ $mode == admission || $selection == prepend-temp-copy-rename ]] && printf true || printf false)
fi
if [[ $needs_anchor == true ]]; then
  [[ $anchor_fixture == /* && -f $anchor_fixture/payload.bin ]] || die "anchor fixture must contain the registered payload.bin"
  [[ $(stat -f '%z' "$anchor_fixture/payload.bin" 2>/dev/null || stat -c '%s' "$anchor_fixture/payload.bin") == 33554432 ]] || die "registered anchor fixture length"
fi

container_seal=$(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container")
[[ $container_seal == "$current_seal" ]] || die "container/source seal mismatch"
docker inspect "$container" | python3 -c 'import json,sys; assert not any(x.get("Type") == "bind" for x in json.load(sys.stdin)[0].get("Mounts", []))' || die "product container has a bind mount"
docker exec "$container" test -c /dev/fuse 2>/dev/null || {
  [[ $(docker inspect -f '{{.State.Running}}' "$container") != true ]] || die "container lacks /dev/fuse"
}
if [[ -n $paired_container ]]; then
  paired_seal=$(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$paired_container")
  if [[ $source_arm == a-a-repeatability ]]; then [[ $paired_seal == "$current_seal" ]] || die "paired container/source seal mismatch"; else [[ $paired_seal != "$current_seal" ]] || die "baseline/candidate containers must have distinct source seals"; fi
  docker inspect "$paired_container" | python3 -c 'import json,sys; assert not any(x.get("Type") == "bind" for x in json.load(sys.stdin)[0].get("Mounts", []))' || die "paired product container has a bind mount"
  if [[ $source_arm == a-a-repeatability ]]; then [[ $(docker inspect -f '{{.Image}}' "$paired_container") == $(docker inspect -f '{{.Image}}' "$container") ]] || die "A/A repeatability requires identical image identity"; fi
fi

cp "$prepared/identity.txt" "$run_dir/environment/prepared-identity.txt"
cp "$prepared/docker.txt" "$run_dir/environment/docker.txt"
cp "$prepared/host.txt" "$run_dir/environment/host.txt"
cp "$prepared/container.json" "$run_dir/environment/prepared-container.json"
cp "$prepared/image.json" "$run_dir/environment/image.json"
cp "$prepared/static-edit-proof.json" "$run_dir/environment/static-edit-proof.json"
cp -R "$prepared/issue14-r005-custody" "$run_dir/environment/issue14-r005-custody"
docker inspect "$container" >"$run_dir/environment/container.json"
if [[ -n $paired_container ]]; then docker inspect "$paired_container" >"$run_dir/environment/paired-container.json"; fi
docker ps --no-trunc >"$run_dir/environment/pre-run-competing-containers.txt"
{
  if [[ $family_kind == count-changing ]]; then entrypoint="$here/run-edit-count-changing.sh"; else entrypoint="$0"; fi
  printf '%q ' "$entrypoint" "${invocation_argv[@]}"
  printf '\n'
} >"$run_dir/environment/command.txt"
printf '%s\n' 'complete_lifecycle_ns begins immediately before public CreateWorkspaceSession and ends after public EndWorkspaceSession(Clean); layerstack initialization and Branch fork are excluded; Commit includes public Commit return plus explicit visible Branch-head acknowledgement.' >"$run_dir/environment/acknowledgement-boundary.txt"
printf '%s\n' "$current_seal" >"$run_dir/environment/source-seal.txt"
shasum -a 256 "$fixture_256/payload.bin" >"$run_dir/environment/fixtures.sha256"
if [[ $family_kind == count-changing ]]; then shasum -a 256 "$fixture_8m/payload.bin" >>"$run_dir/environment/fixtures.sha256"; fi
if [[ -n $anchor_fixture ]]; then shasum -a 256 "$anchor_fixture/payload.bin" >>"$run_dir/environment/fixtures.sha256"; fi

daemon_endpoint=
daemon_capability=
container_id=
pending_stop_root="$run_dir/environment/pending-runner-stops"
mkdir "$pending_stop_root"
ensure_container() {
  local active_container=$1 stopped active_id marker
  active_id=$(docker inspect -f '{{.Id}}' "$active_container")
  marker="$pending_stop_root/$active_id"
  if [[ $(docker inspect -f '{{.State.Running}}' "$active_container") != true ]]; then
    stopped=$(docker inspect -f '{{.State.ExitCode}} {{.State.OOMKilled}}' "$active_container")
    [[ $stopped == '0 false' || $stopped == '143 false' || ( $stopped == '137 false' && -f $marker ) ]] || die "container stopped abnormally"
    rm -f "$marker"
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
      if [[ ! -f $run_dir/environment/container-kernel-fuse.txt ]]; then
        docker exec "$active_container" sh -c 'uname -a; stat -c "dev_fuse_type=%F dev_fuse_mode=%a" /dev/fuse; printf "capability_bytes="; wc -c </run/layerfs/capability; printf "fuse_filesystems="; grep -c fuse /proc/filesystems' >"$run_dir/environment/container-kernel-fuse.txt"
      fi
      return
    fi
    sleep 0.1
  done
  die "daemon readiness"
}

stop_container() {
  local target=$1 target_id
  if [[ $(docker inspect -f '{{.State.Running}}' "$target") == true ]]; then
    target_id=$(docker inspect -f '{{.Id}}' "$target")
    : >"$pending_stop_root/$target_id"
    docker stop "$target" >/dev/null
  fi
}

fixture_for() {
  case "$1" in small-edit|edit16|prepend-temp-copy-rename) printf '%s\n' "$anchor_fixture" ;; *) printf '%s\n' "$fixture_256" ;; esac
}

run_performance() {
  local case_id=$1 sample_seed=$2 ordinal=$3 active_container=$4 active_arm=$5 fixture sample_dir cache status wall_started benchmark_command
  wall_started=$(python3 -c 'import time; print(time.monotonic_ns())')
  fixture=$(fixture_for "$case_id")
  sample_dir="$run_dir/scenarios/$case_id/$active_arm/seed-$sample_seed"
  mkdir -p "$sample_dir"
  cache=$([[ $ordinal == 1 ]] && printf generated-first-sample-uncontrolled || printf generated-subsequent-sample-uncontrolled)
  ensure_container "$active_container"
  benchmark_command=$([[ $family_kind == same-count ]] && printf same-count-performance || printf count-changing-performance)
  set +e
  perl -e 'alarm 5; exec @ARGV' env \
    LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
    LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
    LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
    LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
    "$binary" "$benchmark_command" "$sample_dir/work" "$fixture" "$container_id" \
    "$case_id" "$sample_seed" "$active_arm" "$cache" >"$sample_dir/raw.jsonl" 2>"$sample_dir/supervisor.txt"
  status=$?
  set -e
  printf '%s\n' "$status" >"$sample_dir/exit-status.txt"
  [[ $status == 0 ]] || die "performance failed: $case_id seed $sample_seed"
  python3 - "$sample_dir/raw.jsonl" "$case_id" "$sample_dir/classification.json" "$mode" "$family_kind" <<'PY'
import json, sys
rows=[json.loads(x) for x in open(sys.argv[1]) if x.startswith('{')]
rows=[x for x in rows if x.get('schema') == 'fs-bench-pro-edit-performance-v1']
assert len(rows)==1 and rows[0]['scenario_id']==sys.argv[2]
r=rows[0]
assert r['attempted_operations']==r['completed_operations']==r['operation_count']
if sys.argv[5]=='same-count': assert r['initial_file_bytes']==r['final_file_bytes']
else:
    assert r['initial_file_bytes']!=r['final_file_bytes'] and r['paired_same_count_control_id']
    if r['operation']=='sparse-write':
        assert r['logical_zero_bytes']>0 and r['spool_write_bytes']<=r['supplied_bytes'] and r['physical_spool_high_water_bytes']<=r['supplied_bytes']
assert not r['oom'] and not r['timeout'] and r['swap_bytes']==0 and r['cleanup_status']=='pass'
assert r['container_memory_peak_bytes']<=128*1024*1024
assert r['physical_spool_high_water_bytes']<=128*1024*1024
states=[]
def upper(value,target,tolerated,hard,name):
    state='target-pass' if value<=target else 'tolerated-pass' if value<=tolerated else 'no-go' if value<=hard else 'hard-failure'
    states.append((state,name,value,target,tolerated,hard))
def lower(value,target,tolerated,name):
    state='target-pass' if value>=target else 'tolerated-pass' if value>=tolerated else 'no-go'
    states.append((state,name,value,target,tolerated,None))
def lower_hard(value,target,tolerated,hard,name):
    state='target-pass' if value>=target else 'tolerated-pass' if value>=tolerated else 'no-go' if value>=hard else 'hard-failure'
    states.append((state,name,value,target,tolerated,hard))
upper(r['process_peak_rss_bytes'],101_980_569,112_178_626,128*1024*1024,'rss')
if sys.argv[5]=='count-changing':
    if r['scenario_id']=='prepend-temp-copy-rename': upper(r['complete_lifecycle_ns'],223_763_000,246_139_300,250_000_000,'prepend_complete')
    elif r['implementation']=='direct-posix': lower_hard(r['operations_per_second'],250,225,100,'operations_per_second')
    else:
        if r['operation_count']==1: target,tolerated,hard=50,45,30
        elif r['operation_count']==10: target,tolerated,hard=75,67.5,40
        elif r['operation']=='delete' or 'shrink' in r['scenario_id']: target,tolerated,hard=55,49.5,40
        elif 'grow-2k' in r['scenario_id']: target,tolerated,hard=110,99,80
        else: target,tolerated,hard=135,121.5,100
        lower_hard(r['copied_payload_bytes_per_second'],target*1024*1024,tolerated*1024*1024,hard*1024*1024,'copied_payload_bytes_per_second')
elif r['scenario_id']=='small-edit': upper(r['commit_total_ns'],4_503_000,4_953_300,6_000_000,'small_edit_commit')
elif r['scenario_id']=='edit16': upper(r['complete_lifecycle_ns'],156_446_000,172_090_600,200_000_000,'edit16_complete')
else:
    target,tolerated=(250,225) if r['position']=='distributed' else (500,450)
    lower(r['operations_per_second'],target,tolerated,'operations_per_second')
    assert r['spool_allocated_bytes']==r['spool_live_bytes']+r['spool_superseded_bytes']
    assert r['piece_count'] and r['piece_height'] and r['piece_logical_charge_bytes'] and r['tree_visits']
rank={'target-pass':0,'tolerated-pass':1,'no-go':2,'hard-failure':3}
overall=max(states,key=lambda x:rank[x[0]])[0]
json.dump({'schema':f'fs-bench-pro-edit-{sys.argv[5]}-classification-v1','scenario_id':r['scenario_id'],'seed':r['seed'],'metrics':[{'status':s,'metric':n,'value':v,'target':t,'tolerated':q,'hard':h} for s,n,v,t,q,h in states],'status':overall},open(sys.argv[3],'w'),sort_keys=True,separators=(',',':'))
open(sys.argv[3],'a').write('\n')
if sys.argv[4] in ('admission','repeatability'):
    assert r['process_peak_rss_bytes']<=128*1024*1024
    if r['scenario_id'] in ('small-edit','edit16','prepend-temp-copy-rename'):
        assert overall != 'hard-failure'
else:
    assert overall in ('target-pass','tolerated-pass')
PY
  grep '"schema":"fs-bench-pro-edit-performance-v1"' "$sample_dir/raw.jsonl" >>"$run_dir/performance/raw.jsonl"
  performance_external_wall_ns=$(( performance_external_wall_ns + $(python3 -c 'import time; print(time.monotonic_ns())') - wall_started ))
}

run_control() {
  local control_id=$1 sample_seed=$2 ordinal=$3 active_container=$4 sample_dir status wall_started cache
  wall_started=$(python3 -c 'import time; print(time.monotonic_ns())')
  sample_dir="$run_dir/controls/$control_id/seed-$sample_seed"
  mkdir -p "$sample_dir"
  cache=$([[ $ordinal == 1 ]] && printf generated-first-sample-uncontrolled || printf generated-subsequent-sample-uncontrolled)
  ensure_container "$active_container"
  set +e
  perl -e 'alarm 5; exec @ARGV' env \
    LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
    LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
    LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
    LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
    "$binary" same-count-performance "$sample_dir/work" "$fixture_256" "$container_id" \
    "$control_id" "$sample_seed" repeat-a "$cache" >"$sample_dir/raw.jsonl" 2>"$sample_dir/supervisor.txt"
  status=$?
  set -e
  printf '%s\n' "$status" >"$sample_dir/exit-status.txt"
  [[ $status == 0 ]] || die "pair control failed: $control_id seed $sample_seed"
  python3 - "$sample_dir/raw.jsonl" "$control_id" <<'PY'
import json,sys
rows=[json.loads(x) for x in open(sys.argv[1]) if x.startswith('{')]
rows=[x for x in rows if x.get('schema')=='fs-bench-pro-edit-performance-v1']
assert len(rows)==1 and rows[0]['scenario_id']==sys.argv[2]
r=rows[0]
assert r['initial_file_bytes']==r['final_file_bytes']==262144
assert r['attempted_operations']==r['completed_operations']==r['operation_count']
assert r['spool_allocated_bytes']==r['spool_live_bytes']+r['spool_superseded_bytes']
assert r['physical_spool_high_water_bytes']>=r['spool_allocated_bytes']
assert r['process_peak_rss_bytes']<=128*1024*1024 and r['container_memory_peak_bytes']<=128*1024*1024
assert r['swap_bytes']==0 and not r['oom'] and not r['timeout'] and r['cleanup_status']=='pass'
PY
  grep '"schema":"fs-bench-pro-edit-performance-v1"' "$sample_dir/raw.jsonl" >>"$run_dir/controls/raw.jsonl"
  control_external_wall_ns=$(( control_external_wall_ns + $(python3 -c 'import time; print(time.monotonic_ns())') - wall_started ))
}

oracle_digest() {
  local case_id=$1 sample_seed=$2 fixture oracle index
  fixture=$(fixture_for "$case_id")
  oracle="$run_dir/oracles/$case_id-seed-$sample_seed.bin"
  cp "$fixture/payload.bin" "$oracle"
  if [[ $family_kind == count-changing ]]; then
    if [[ $case_id == prepend-temp-copy-rename ]]; then "$oracle_workload" prepend "$oracle" >/dev/null; else "$oracle_workload" count-changing-edit "$oracle" "$case_id" "$sample_seed" >/dev/null; fi
  elif [[ $case_id == small-edit ]]; then
    "$oracle_workload" edit "$oracle" 0 33554432
  elif [[ $case_id == edit16 ]]; then
    for index in $(seq 1 16); do "$oracle_workload" edit "$oracle" "$index" 33554432; done
  else
    "$oracle_workload" same-count-edit "$oracle" "$case_id" "$sample_seed" >/dev/null
  fi
  "$oracle_workload" digest "$oracle"
}

run_verify() {
  local case_id=$1 sample_seed=$2 active_container=$3 active_arm=$4 fixture expected expected_size verify_dir status wall_started structural_oracle
  wall_started=$(python3 -c 'import time; print(time.monotonic_ns())')
  if [[ $family_kind == count-changing ]] && printf '%s\n' "${count_changing_verifiers[@]}" | grep -Fx "$case_id" >/dev/null; then
    structural_oracle="$run_dir/oracles/$case_id.bin"
    python3 - "$fixture_8m/payload.bin" "$structural_oracle" "$case_id" <<'PY'
from pathlib import Path
import sys
source,target,case=Path(sys.argv[1]),Path(sys.argv[2]),sys.argv[3]
data=source.read_bytes(); assert len(data)==8*1024*1024
if case=='insert-middle-4k-on-8m-proof':
  payload=bytes(((i*17+3)%251) for i in range(4096)); data=data[:4*1024*1024]+payload+data[4*1024*1024:]
elif case=='delete-middle-4k-on-8m-proof':
  start=4*1024*1024-2048; data=data[:start]+data[start+4096:]
elif case in ('rewrite-full-grow-8m-to-12m-proof','rewrite-full-shrink-8m-to-4m-proof'):
  size=(12 if 'grow' in case else 4)*1024*1024
  data=bytes(((i*31+size//(1024*1024))%251) for i in range(size))
else: raise AssertionError(case)
target.write_bytes(data)
PY
    read -r expected_size expected < <("$oracle_workload" digest "$structural_oracle")
    ensure_container "$active_container"
    verify_dir="$run_dir/verification/$active_arm-$case_id"
    mkdir -p "$verify_dir"
    set +e
    perl -e 'alarm 40; exec @ARGV' env LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload LAYERFS_BENCH_ORACLE_FILE="$structural_oracle" \
      LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
      LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
      LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
      "$binary" count-changing-structural-verify "$verify_dir/work" "$fixture_8m" "$container_id" \
      "$case_id" "$active_arm" "$expected" "$expected_size" >"$verify_dir/raw.jsonl" 2>"$verify_dir/supervisor.txt"
    status=$?
  elif [[ $case_id == overwrite-fragmented-10b-ops-1000-proof ]]; then
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
    read -r expected_size expected < <(oracle_digest "$case_id" "$sample_seed")
    ensure_container "$active_container"
    verify_dir="$run_dir/verification/$active_arm-$case_id-seed-$sample_seed"
    mkdir -p "$verify_dir"
    set +e
    if [[ $family_kind == same-count ]]; then
      perl -e 'alarm 20; exec @ARGV' env LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
        LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
        LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
        LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
        "$binary" same-count-verify "$verify_dir/work" "$fixture" "$container_id" "$case_id" \
        "$sample_seed" "$active_arm" "$expected" reused-first-sample-uncontrolled \
        >"$verify_dir/raw.jsonl" 2>"$verify_dir/supervisor.txt"
    else
      perl -e 'alarm 40; exec @ARGV' env LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload LAYERFS_BENCH_ORACLE_FILE="$run_dir/oracles/$case_id-seed-$sample_seed.bin" \
        LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
        LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
        LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
        "$binary" count-changing-verify "$verify_dir/work" "$fixture" "$container_id" "$case_id" \
        "$sample_seed" "$active_arm" "$expected" "$expected_size" reused-first-sample-uncontrolled \
        >"$verify_dir/raw.jsonl" 2>"$verify_dir/supervisor.txt"
    fi
    status=$?
  fi
  set -e
  printf '%s\n' "$status" >"$verify_dir/exit-status.txt"
  verification_external_wall_ns=$(( verification_external_wall_ns + $(python3 -c 'import time; print(time.monotonic_ns())') - wall_started ))
  if [[ $status != 0 ]]; then verification_failure="$case_id seed $sample_seed"; return 1; fi
}

if [[ $mode == performance ]]; then
  run_performance "$selection" "$seed" 1 "$container" "$source_arm"
elif [[ $mode == verify ]]; then
  run_verify "$selection" "$seed" "$container" "$source_arm"
else
  ordinal=0
  if [[ $mode == repeatability ]]; then printf '%s\tselected\n' "$selection" >"$run_dir/environment/repeatability-selection.tsv"; family_map="$run_dir/environment/repeatability-selection.tsv"; else family_map="$mapfile_path"; fi
  while IFS=$'\t' read -r case_id _ <&3; do
    for sample_seed in 1 2 3; do
      if [[ $source_arm == baseline-candidate && $sample_seed == 2 ]]; then
        order=("$paired_container baseline" "$container candidate")
      elif [[ $source_arm == baseline-candidate ]]; then
        order=("$container candidate" "$paired_container baseline")
      elif [[ $sample_seed == 2 ]]; then
        order=("$paired_container repeat-b" "$container repeat-a")
      else
        order=("$container repeat-a" "$paired_container repeat-b")
      fi
      for pair in "${order[@]}"; do
        ordinal=$((ordinal + 1))
        active=${pair% *}
        run_performance "$case_id" "$sample_seed" "$ordinal" "$active" "${pair##* }"
      done
    done
  done 3<"$family_map"
  if [[ $mode == admission && $family_kind == count-changing ]]; then
    cut -f3 "$mapfile_path" | grep -v '^not-applicable-' | sort -u >"$run_dir/environment/pair-controls.txt"
    control_ordinal=0
    while IFS= read -r control_id; do
      for sample_seed in 1 2 3; do
        control_ordinal=$((control_ordinal + 1))
        run_control "$control_id" "$sample_seed" "$control_ordinal" "$container"
      done
    done <"$run_dir/environment/pair-controls.txt"
  fi
fi

python3 - "$run_dir" "$mode" "$source_arm" "$family_kind" <<'PY'
import json, statistics, sys
from pathlib import Path
root, mode, source, family = Path(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
rows=[]
raw=root/'performance/raw.jsonl'
if raw.exists(): rows=[json.loads(x) for x in raw.read_text().splitlines() if x]
summary={'schema':f'fs-bench-pro-edit-{family}-summary-v2','mode':mode,'source_identity':source,'samples':len(rows),'status':'target-pass'}
if family=='count-changing' and mode=='admission':
    controls=[json.loads(x) for x in (root/'controls/raw.jsonl').read_text().splitlines() if x]
    assert len(controls)==45
    by_id={(x['scenario_id'],x['seed']):x for x in controls}
    validations={}
    for row in rows:
        control_id=row['paired_same_count_control_id']
        if control_id.startswith('not-applicable-'):
            continue
        control=by_id[(control_id,row['seed'])]
        quantity=row['deleted_bytes'] if row['pair_byte_quantity_basis']=='deleted_bytes' else row['supplied_bytes']
        assert row['pair_byte_quantity_match'] is None
        assert row['pair_fixture_bytes']==row['initial_file_bytes']==control['initial_file_bytes']==control['final_file_bytes']==262144
        assert row['position']==control['position'] and row['operation_count']==control['operation_count']
        assert quantity==control['supplied_bytes']
        assert control['layerstack_visible_ns']==control['workspace_create_ns']+control['execution_ns']+control['commit_api_ns']
        assert control['complete_lifecycle_ns']==control['layerstack_visible_ns']+control['workspace_end_ns']
        validations[control_id]={'fixture_bytes':262144,'position':control['position'],'operation_count':control['operation_count'],'byte_quantity':control['supplied_bytes'],'source_identity':'current-sealed-control-cohort','timing_boundary':'Create-through-Commit-visibility-through-End','status':'pass'}
    assert len(validations)==15
    summary.update({'pair_validation_status':'target-pass','pair_control_samples':len(controls),'pair_controls':validations})
phases=['workspace_create_ns','execution_ns','commit_api_ns','layerstack_visible_ns','workspace_end_ns','complete_lifecycle_ns']
counters=['fuse_kernel_write_requests','fuse_kernel_write_bytes','spool_write_bytes','candidate_objects','candidate_bytes','inserted_objects','reused_objects','commit_payload_bytes_read','commit_cdc_bytes_scanned','tree_visits','metric_nodes_scanned']
def med(values,field): return float(statistics.median(x[field] for x in values))
def ratio(left,right): return right/left if left else (1.0 if right==0 else float('inf'))
def symmetric_ratio(left,right): return max(ratio(left,right),ratio(right,left))
if mode in ('admission','repeatability'):
    expected=(84 if family=='same-count' else 150) if mode=='admission' else 6
    assert len(rows)==expected
    arm_names=('baseline','candidate') if source=='baseline-candidate' else ('repeat-a','repeat-b')
    arms={arm:[x for x in rows if x['source_arm']==arm] for arm in arm_names}
    assert all(len(values)==expected//2 for values in arms.values())
    medians={}
    for arm,values in arms.items():
        medians[arm]={}
        for case in sorted({x['scenario_id'] for x in values}):
            selected=[x for x in values if x['scenario_id']==case]
            metrics=phases+counters+['operations_per_second','process_peak_rss_bytes','commit_total_ns']+([] if family=='same-count' else ['copied_payload_bytes_per_second'])
            medians[arm][case]={field:med(selected,field) for field in metrics}
    walls={arm:sum(x['complete_lifecycle_ns'] for x in values) for arm,values in arms.items()}
    paired=sum(walls.values())
    if family=='same-count': arm_target,arm_tolerated,arm_hard,paired_target,paired_tolerated,paired_hard=3_000_000_000,3_300_000_000,6_000_000_000,6_000_000_000,6_600_000_000,12_000_000_000
    else: arm_target,arm_tolerated,arm_hard,paired_target,paired_tolerated,paired_hard=10_000_000_000,11_000_000_000,20_000_000_000,20_000_000_000,22_000_000_000,40_000_000_000
    wall_status='target-pass' if max(walls.values())<=arm_target and paired<=paired_target else 'tolerated-pass' if max(walls.values())<=arm_tolerated and paired<=paired_tolerated else 'no-go' if max(walls.values())<=arm_hard and paired<=paired_hard else 'hard-failure'
    if source=='baseline-candidate':
        ratios={case:ratio(medians['baseline'][case]['complete_lifecycle_ns'],medians['candidate'][case]['complete_lifecycle_ns']) for case in medians['baseline']}
    else:
        ratios={case:symmetric_ratio(medians['repeat-a'][case]['complete_lifecycle_ns'],medians['repeat-b'][case]['complete_lifecycle_ns']) for case in medians['repeat-a']}
    ratio_status='target-pass' if max(ratios.values())<=1.05 else 'tolerated-pass' if max(ratios.values())<=1.10 else 'no-go'
    rank={'target-pass':0,'tolerated-pass':1,'no-go':2,'hard-failure':3}
    absolute=[]
    def upper(arm,case,metric,value,target,tolerated,hard):
        state='target-pass' if value<=target else 'tolerated-pass' if value<=tolerated else 'no-go' if value<=hard else 'hard-failure'
        absolute.append({'arm':arm,'scenario_id':case,'metric':metric,'value':value,'target':target,'tolerated':tolerated,'hard':hard,'status':state})
    def lower(arm,case,metric,value,target,tolerated):
        state='target-pass' if value>=target else 'tolerated-pass' if value>=tolerated else 'no-go'
        absolute.append({'arm':arm,'scenario_id':case,'metric':metric,'value':value,'target':target,'tolerated':tolerated,'hard':None,'status':state})
    def lower_hard(arm,case,metric,value,target,tolerated,hard):
        state='target-pass' if value>=target else 'tolerated-pass' if value>=tolerated else 'no-go' if value>=hard else 'hard-failure'
        absolute.append({'arm':arm,'scenario_id':case,'metric':metric,'value':value,'target':target,'tolerated':tolerated,'hard':hard,'status':state})
    absolute_arms={'candidate':arms['candidate']} if source=='baseline-candidate' else arms
    for arm,values in absolute_arms.items():
        for case in medians[arm]:
            selected=[x for x in values if x['scenario_id']==case]
            upper(arm,case,'rss_max',max(x['process_peak_rss_bytes'] for x in selected),101_980_569,112_178_626,128*1024*1024)
            if family=='count-changing':
                if case=='prepend-temp-copy-rename': upper(arm,case,'prepend_complete_median',medians[arm][case]['complete_lifecycle_ns'],223_763_000,246_139_300,250_000_000)
                elif selected[0]['implementation']=='direct-posix': lower_hard(arm,case,'operations_per_second_median',medians[arm][case]['operations_per_second'],250,225,100)
                else:
                    row=selected[0]
                    if row['operation_count']==1: target,tolerated,hard=50,45,30
                    elif row['operation_count']==10: target,tolerated,hard=75,67.5,40
                    elif row['operation']=='delete' or 'shrink' in case: target,tolerated,hard=55,49.5,40
                    elif 'grow-2k' in case: target,tolerated,hard=110,99,80
                    else: target,tolerated,hard=135,121.5,100
                    lower_hard(arm,case,'copied_payload_bytes_per_second_median',medians[arm][case]['copied_payload_bytes_per_second'],target*1024*1024,tolerated*1024*1024,hard*1024*1024)
            elif case=='small-edit': upper(arm,case,'small_edit_commit_median',medians[arm][case]['commit_total_ns'],4_503_000,4_953_300,6_000_000)
            elif case=='edit16': upper(arm,case,'edit16_complete_median',medians[arm][case]['complete_lifecycle_ns'],156_446_000,172_090_600,200_000_000)
            else:
                target,tolerated=(250,225) if selected[0]['position']=='distributed' else (500,450)
                lower(arm,case,'operations_per_second_median',medians[arm][case]['operations_per_second'],target,tolerated)
    absolute_status=max((x['status'] for x in absolute),key=rank.get)
    dispositions={}
    for case,value in ratios.items():
        if value<=1.05: continue
        if source=='baseline-candidate':
            phase_rows={field:{'ratio':ratio(medians['baseline'][case][field],medians['candidate'][case][field]),'candidate_ns':medians['candidate'][case][field],'under_2ms_exception':field in ('workspace_create_ns','workspace_end_ns') and medians['candidate'][case][field]<2_000_000} for field in phases}
            counter_rows={field:ratio(medians['baseline'][case][field],medians['candidate'][case][field]) for field in counters}
            reason='directional candidate/baseline comparison; retained phase and counter disposition'
        else:
            phase_rows={field:{'ratio':symmetric_ratio(medians['repeat-a'][case][field],medians['repeat-b'][case][field]),'under_2ms_exception':False,'exception_reason':'A/A repeatability has no candidate arm; local-step exception is inapplicable'} for field in phases}
            counter_rows={field:symmetric_ratio(medians['repeat-a'][case][field],medians['repeat-b'][case][field]) for field in counters}
            reason='identical-source A/A scheduling variance; no improvement claim'
        dispositions[case]={'reason':reason,'complete_ratio':value,'phase_ratios':phase_rows,'counter_ratios':counter_rows}
    summary.update({'comparison_type':'directional baseline/candidate' if source=='baseline-candidate' else 'A/A repeatability','medians':medians,'arm_complete_lifecycle_ns':walls,'paired_complete_lifecycle_ns':paired,'family_wall_status':wall_status,'complete_lifecycle_ratios':ratios,'ratio_status':ratio_status,'absolute_classification':absolute,'absolute_status':absolute_status,'phase_counter_dispositions':dispositions})
    summary['status']=max((wall_status,ratio_status,absolute_status),key=rank.get)
else:
    grouped={}
    for row in rows: grouped.setdefault(row['scenario_id'],[]).append(row)
    metrics=phases+counters+['operations_per_second','process_peak_rss_bytes']+([] if family=='same-count' else ['copied_payload_bytes_per_second'])
    summary['medians']={case:{field:med(values,field) for field in metrics} for case,values in sorted(grouped.items())}
(root/'summary.json').write_text(json.dumps(summary,sort_keys=True,separators=(',',':'))+'\n')
PY

overall_status=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["status"])' "$run_dir/summary.json")
if [[ $overall_status == target-pass || $overall_status == tolerated-pass ]]; then admission_eligible=true; else admission_eligible=false; fi
if [[ $admission_eligible == true && $mode == admission ]]; then
  if [[ $family_kind == same-count ]]; then
    run_verify overwrite-fragmented-10b-ops-1000-proof 1 "$container" repeat-a || true
  else
    verification_cases=(prepend-temp-copy-rename sparse-write-past-eof-gap-60k-payload-4k-ops-100 "${count_changing_verifiers[@]}")
    for verifier in "${verification_cases[@]}"; do run_verify "$verifier" 1 "$container" repeat-a || break; done
  fi
fi
if [[ -n $verification_failure ]]; then
  overall_status=hard-failure
  admission_eligible=false
  printf '%s\n' "$verification_failure" >"$run_dir/verification/failure.txt"
  python3 - "$run_dir/summary.json" "$verification_failure" <<'PY'
import json,sys
p=sys.argv[1]; data=json.load(open(p)); data['performance_status']=data['status']; data['verification_status']='hard-failure'; data['verification_failure']=sys.argv[2]; data['status']='hard-failure'; open(p,'w').write(json.dumps(data,sort_keys=True,separators=(',',':'))+'\n')
PY
fi
printf '%s\n' "$performance_external_wall_ns" >"$run_dir/environment/performance-external-wall-ns.txt"
printf '%s\n' "$control_external_wall_ns" >"$run_dir/environment/control-external-wall-ns.txt"
printf '%s\n' "$verification_external_wall_ns" >"$run_dir/environment/verification-external-wall-ns.txt"
stop_container "$container"
if [[ -n $paired_container ]]; then stop_container "$paired_container"; fi
docker inspect "$container" >"$run_dir/environment/container-after.json"
if [[ -n $paired_container ]]; then docker inspect "$paired_container" >"$run_dir/environment/paired-container-after.json"; fi
printf '{"schema":"fs-bench-pro-edit-%s-status-v2","mode":"%s","source_identity":"%s","status":"%s","admission_eligible":%s}\n' "$family_kind" "$mode" "$source_arm" "$overall_status" "$admission_eligible" >"$run_dir/run-status.json"
(invocation_elapsed=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - invocation_started )); printf '%s\n' "$invocation_elapsed" >"$run_dir/environment/total-external-wall-ns.txt"; if [[ $mode == performance && $performance_external_wall_ns -gt 2000000000 ]]; then printf '{"schema":"fs-bench-pro-edit-%s-status-v2","mode":"%s","source_identity":"%s","status":"no-go","admission_eligible":false,"reason":"selected performance external wall exceeded 2 seconds","performance_external_wall_ns":%s,"total_external_wall_ns":%s}\n' "$family_kind" "$mode" "$source_arm" "$performance_external_wall_ns" "$invocation_elapsed" >"$run_dir/run-status.json"; fi)
(cd "$run_dir" && find . -type f ! -name evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
[[ $(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["admission_eligible"])' "$run_dir/run-status.json") == True ]] || die "admission or selected external wall gate"
printf 'PASS %s\n' "$run_dir"
