#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
family=${LAYERFS_SDK_EDIT_FAMILY:?missing SDK edit family}
case "$family" in
  edit_length_preserving) result_family=edit-length-preserving ;;
  edit_length_changing) result_family=edit-length-changing ;;
  edit_canonical_chunk_count) result_family=edit-canonical-chunk-count ;;
  *) printf 'unknown SDK edit family: %s\n' "$family" >&2; return 2 2>/dev/null || exit 2 ;;
esac

results_root=${LAYERFS_SDK_EDIT_RESULTS_ROOT:-$repo/benchmark-results/fs-bench-pro/$result_family}
prepared_root=${LAYERFS_SDK_EDIT_PREPARED_ROOT:-${TMPDIR:-/tmp}/layerfs-sdk-edit-prepared-v1}
default_bin=$repo/target/release/fs-benchmark-pro

die() { printf 'fs-bench-pro %s: %s\n' "$family" "$*" >&2; exit 2; }

self_check() {
  local bin=${LAYERFS_SDK_EDIT_BIN:-$default_bin}
  [[ -x $bin ]] || die "build the release fs-benchmark-pro binary first"
  bash -n "$0" "$here/lib-edit-sdk-runner.sh"
  "$bin" sdk-edit-self-check "$family"
  python3 - "$here/src/sdk_file_edit.rs" "$here/generate-sdk-edit-report.py" <<'PY'
import ast, pathlib, sys
timed=pathlib.Path(sys.argv[1]).read_text()
for forbidden in ('std::fs', 'File::create', 'OpenOptions', 'set_len', 'rename(', 'copy(', 'remove_', 'Command', 'edit_workspace_file_ranges', 'exec_workspace', 'shell_workspace'):
    assert forbidden not in timed, forbidden
for required in ('edit_workspace_file_range(edit)', 'commit_workspace_session(workspace_id)', 'end_workspace_session', 'client.query'):
    assert required in timed, required
ast.parse(pathlib.Path(sys.argv[2]).read_text())
PY
  python3 "$here/generate-sdk-edit-report.py" --self-check
}

if [[ ${1:-} == --self-check ]]; then
  [[ $# == 1 ]] || die "--self-check takes no arguments"
  self_check
  exit 0
fi

invocation_argv=("$@")
[[ $# -ge 2 ]] || die "usage: RUN_ID CONTAINER --all --mode admission | RUN_ID CONTAINER --case ID --repetition N --mode performance --source ARM | RUN_ID CONTAINER --case ID --mode verify --source ARM"
run_id=$1
container_input=$2
shift 2
[[ $run_id =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "invalid run id"
mode=
selection=
repetition=
source_arm=
all=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --all) (( all == 0 )) || die "duplicate --all"; all=1; shift ;;
    --case) [[ $# -ge 2 && -z $selection ]] || die "duplicate/missing --case"; selection=$2; shift 2 ;;
    --repetition) [[ $# -ge 2 && -z $repetition ]] || die "duplicate/missing --repetition"; repetition=$2; shift 2 ;;
    --mode) [[ $# -ge 2 && -z $mode ]] || die "duplicate/missing --mode"; mode=$2; shift 2 ;;
    --source) [[ $# -ge 2 && -z $source_arm ]] || die "duplicate/missing --source"; source_arm=$2; shift 2 ;;
    *) die "unknown argument: $1" ;;
  esac
done

baseline_bin=${LAYERFS_SDK_EDIT_BASELINE_BIN:-}
candidate_bin=${LAYERFS_SDK_EDIT_CANDIDATE_BIN:-$default_bin}
baseline_image=${LAYERFS_SDK_EDIT_BASELINE_IMAGE:-}
candidate_image=${LAYERFS_SDK_EDIT_CANDIDATE_IMAGE:-$container_input}
baseline_revision=${LAYERFS_SDK_EDIT_BASELINE_REVISION:-}
candidate_revision=${LAYERFS_SDK_EDIT_CANDIDATE_REVISION:-}
baseline_build=${LAYERFS_SDK_EDIT_BASELINE_BUILD:-}
candidate_build=${LAYERFS_SDK_EDIT_CANDIDATE_BUILD:-}
for command in docker python3 shasum; do command -v "$command" >/dev/null || die "$command is required"; done
[[ -x $candidate_bin ]] || die "candidate binary is required"

run_dir=$results_root/$run_id
[[ ! -e $run_dir ]] || die "run directory already exists"
mkdir -p "$run_dir/environment" "$run_dir/performance" "$run_dir/verification" "$run_dir/scenarios"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-issue20-${family}.XXXXXX")
active_container=
cleanup() {
  local status=${1:-0}
  if [[ -n ${active_container:-} ]]; then
    docker inspect "$active_container" >"$run_dir/environment/failed-container.json" 2>/dev/null || true
    docker logs "$active_container" >"$run_dir/environment/failed-container.log" 2>&1 || true
    docker rm -f "$active_container" >/dev/null 2>&1 || true
  fi
  [[ $scratch == "${TMPDIR:-/tmp}/layerfs-issue20-${family}."* ]] && rm -rf -- "$scratch"
  if (( status != 0 )) && [[ ! -e $run_dir/run-status.json ]]; then
    printf '{"schema":"fs-bench-pro-sdk-edit-status-v1","family_id":"%s","admission_eligible":false,"status":"fail-incomplete"}\n' "$family" >"$run_dir/run-status.json"
  fi
  if (( status != 0 )); then
    printf '%s\n' "$status" >"$run_dir/exit-status.txt"
    (cd "$run_dir" && find . -type f ! -path ./evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
  fi
}
trap 'cleanup $?' EXIT

{
  printf '%q ' "$0"
  printf '%q ' "${invocation_argv[@]}"
  printf '\n'
} >"$run_dir/environment/command.txt"
{
  printf 'LAYERFS_SDK_EDIT_BASELINE_BIN=%q\n' "$baseline_bin"
  printf 'LAYERFS_SDK_EDIT_CANDIDATE_BIN=%q\n' "$candidate_bin"
  printf 'LAYERFS_SDK_EDIT_BASELINE_IMAGE=%q\n' "$baseline_image"
  printf 'LAYERFS_SDK_EDIT_CANDIDATE_IMAGE=%q\n' "$candidate_image"
  printf 'LAYERFS_SDK_EDIT_BASELINE_REVISION=%q\n' "$baseline_revision"
  printf 'LAYERFS_SDK_EDIT_CANDIDATE_REVISION=%q\n' "$candidate_revision"
  printf 'LAYERFS_SDK_EDIT_BASELINE_BUILD=%q\n' "$baseline_build"
  printf 'LAYERFS_SDK_EDIT_CANDIDATE_BUILD=%q\n' "$candidate_build"
  printf 'LAYERFS_SDK_EDIT_RESULTS_ROOT=%q\n' "$results_root"
  printf 'LAYERFS_SDK_EDIT_PREPARED_ROOT=%q\n' "$prepared_root"
} >"$run_dir/environment/behavior.env"
"$candidate_bin" sdk-edit-registry "$family" >"$scratch/candidate-registry.txt"
head -n 1 "$scratch/candidate-registry.txt" >"$run_dir/environment/registry-meta.json"
tail -n +2 "$scratch/candidate-registry.txt" >"$run_dir/environment/scenario-registry.tsv"
expected_ids=$(($(wc -l <"$run_dir/environment/scenario-registry.tsv") - 1))
case "$family" in
  edit_length_preserving|edit_canonical_chunk_count) [[ $expected_ids == 12 ]] || die "registry cardinality" ;;
  edit_length_changing) [[ $expected_ids == 32 ]] || die "registry cardinality" ;;
  *) die "unknown family" ;;
esac

python3 - "$run_dir/environment/registry-meta.json" "$run_dir/environment/scenario-registry.tsv" <<'PY'
import hashlib,json,sys
meta=json.load(open(sys.argv[1])); actual=hashlib.sha256(open(sys.argv[2],'rb').read()).hexdigest()
assert meta['registry_manifest_sha256']==actual
assert meta['combined_registry_sha256']=='1773c7b82f739eaf1c2b8a2877f56baaa7e72b26ac8980802bdb82c80e270af6'
PY

python3 - "$here/src/sdk_file_edit.rs" "$run_dir/environment/timed-call-graph-manifest.json" "$run_dir/environment/operation-route-manifest.json" "$repo/crates/layerfs-workspace/tests/file_edit.rs" "$run_dir/environment/edit-conformance-manifest.json" "$here/src/sdk_edit_verify.rs" <<'PY'
import hashlib,json,pathlib,sys
timed_path,timed_out,route_out,conformance_path,conformance_out,verifier_path=sys.argv[1:]
text=pathlib.Path(timed_path).read_text()
required={'edit_workspace_file_range(edit)?':1,'commit_workspace_session(workspace_id)':1,'end_workspace_session(workspace_id, EndWorkspaceMode::Clean)':1,'client.query(Query::new(QueryKind::Branches).limit(512))':1}
for needle,count in required.items(): assert text.count(needle)==count,(needle,text.count(needle))
for needle in ('edit_workspace_file_ranges','std::fs','Command','exec_workspace','shell_workspace','File::create','OpenOptions','set_len','rename(','copy(','remove_'): assert needle not in text,needle
sha=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
json.dump({'schema':'fs-bench-pro-sdk-edit-timed-call-graph-v1','source':'benchmark/fs-bench-pro/src/sdk_file_edit.rs','source_sha256':sha(timed_path),'exact_occurrences':required,'forbidden_occurrences':0,'status':'pass'},open(timed_out,'w'),sort_keys=True,separators=(',',':'));open(timed_out,'a').write('\n')
verifier=pathlib.Path(verifier_path).read_text()
assert verifier.count('execute(')==2 and verifier.count('client.edit_workspace_file_range(edit)')==1
assert 'filesystem::replace_range' not in verifier and 'execute_workload(' not in verifier
json.dump({'schema':'fs-bench-pro-sdk-edit-operation-route-v1','operations':['workspace.create','workspace.file_range_edit','workspace.commit','workspace.end','query'],'counts':[1,1,1,1,1],'workspace_execution_count':0,'projection':'fuse','mount_transport':'authenticated-daemon','allowed_projection_lifecycles':[['attach','end'],['attach','end','attach','end']],'docker_calls':0,'verifier_source_sha256':sha(verifier_path),'read_only_verifier_executions':['stat-inode','digest-inode'],'status':'pass'},open(route_out,'w'),sort_keys=True,separators=(',',':'));open(route_out,'a').write('\n')
conformance=pathlib.Path(conformance_path).read_text()
tests=['group_4_invalid_type_range_overflow_and_limits_are_atomic','group_5_commit_publication_is_exactly_once_and_retry_is_up_to_date']
for test in tests: assert test in conformance
json.dump({'schema':'fs-bench-pro-sdk-edit-conformance-v1','source':'crates/layerfs-workspace/tests/file_edit.rs','source_sha256':sha(conformance_path),'tests':tests,'status':'pass'},open(conformance_out,'w'),sort_keys=True,separators=(',',':'));open(conformance_out,'a').write('\n')
PY
timed_manifest_sha256=$(shasum -a 256 "$run_dir/environment/timed-call-graph-manifest.json" | awk '{print $1}')
route_manifest_sha256=$(shasum -a 256 "$run_dir/environment/operation-route-manifest.json" | awk '{print $1}')
conformance_sha256=$(shasum -a 256 "$run_dir/environment/edit-conformance-manifest.json" | awk '{print $1}')

printf 'fixture_bytes\tstore_bytes\tstore_sha256\n' >"$run_dir/environment/prepared-stores.tsv"
if [[ $mode == admission ]]; then
  [[ $all == 1 && -z $selection && -z $repetition && -z $source_arm ]] || die "admission requires --all only"
  [[ -x $baseline_bin && -n $baseline_image && -n $candidate_image ]] || die "both source binaries and images are required"
  [[ -d $baseline_build && -d $candidate_build ]] || die "both sealed build evidence directories are required"
  [[ $baseline_revision =~ ^[0-9a-f]{40}$ && $candidate_revision =~ ^[0-9a-f]{40}$ && $baseline_revision != "$candidate_revision" ]] || die "distinct source revisions are required"
  git -C "$repo" diff-files --quiet || die "tracked worktree must be clean"
  git -C "$repo" diff-index --cached --quiet HEAD -- || die "index must equal HEAD"
  [[ $(git -C "$repo" rev-parse HEAD) == "$candidate_revision" ]] || die "candidate revision must be HEAD"
  git -C "$repo" diff --quiet "$baseline_revision" "$candidate_revision" -- benchmark/fs-bench-pro || die "benchmark harness changed between source arms"
  [[ $(docker image inspect -f '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$baseline_image") == "$baseline_revision" ]] || die "baseline image revision"
  [[ $(docker image inspect -f '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$candidate_image") == "$candidate_revision" ]] || die "candidate image revision"
  "$baseline_bin" sdk-edit-registry "$family" | tail -n +2 >"$scratch/baseline-registry.tsv"
  cmp -s "$scratch/baseline-registry.tsv" "$run_dir/environment/scenario-registry.tsv" || die "source-arm registries differ"
else
  [[ $all == 0 && -n $selection && $source_arm =~ ^(baseline|candidate)$ ]] || die "selected mode requires case and source"
  if [[ $mode == performance ]]; then [[ $repetition =~ ^[1-5]$ ]] || die "selected performance requires repetition 1-5"; elif [[ $mode == verify ]]; then [[ -z $repetition ]] || die "selected verify takes no repetition"; else die "unknown selected mode"; fi
  baseline_bin=${baseline_bin:-$candidate_bin}
  baseline_image=${baseline_image:-$candidate_image}
fi

python3 - "$run_dir/environment/scenario-registry.tsv" "$run_dir/environment/sample-order.tsv" "$run_dir/environment/registry-meta.json" <<'PY'
import csv,json,sys
source,target,meta_path=sys.argv[1:]
rows=list(csv.DictReader(open(source),delimiter='\t'))
meta=json.load(open(meta_path));offsets=meta['rotations']
with open(target,'w') as out:
    out.write('ordinal\trepetition\tscenario_id\tfirst_arm\tsecond_arm\n')
    ordinal=0
    for repetition,offset in enumerate(offsets,1):
        ordered=rows[offset:]+rows[:offset]
        arms=('baseline','candidate') if repetition%2 else ('candidate','baseline')
        for row in ordered:
            ordinal+=1
            out.write(f"{ordinal}\t{repetition}\t{row['scenario_id']}\t{arms[0]}\t{arms[1]}\n")
PY

prepare_size() {
  local size=$1 prepared receipt=$run_dir/environment/prepared-cache-$1.json
  prepared=$(python3 "$here/sdk-edit-custody.py" prepare "$prepared_root" "$candidate_bin" "$size" "$receipt" "$candidate_build")
  ln -s "$prepared" "$scratch/prepared-$size"
  python3 - "$receipt" "$scratch/fixture-$size.json" "$run_dir/environment/prepared-stores.tsv" <<'PY'
import json,sys
receipt=json.load(open(sys.argv[1]))
with open(sys.argv[2],'w') as output: json.dump(receipt['fixture'],output,sort_keys=True,separators=(',',':'));output.write('\n')
with open(sys.argv[3],'a') as output: output.write(f"{receipt['fixture']['fixture_bytes']}\t{receipt['store_bytes']}\t{receipt['store_sha256']}\n")
PY
}

clone_store() {
  local size=$1 expected
  cloned_store=$(mktemp -d "$scratch/sample.XXXXXX")
  expected=$(awk -F '\t' -v size="$size" '$1==size {print $3}' "$run_dir/environment/prepared-stores.tsv")
  python3 "$here/sdk-edit-custody.py" clone "$scratch/prepared-$size/store.sqlite" "$cloned_store/store.sqlite" "$expected" >"$cloned_store/clone.json"
  read -r clone_method clone_wall_ns clone_sha256 < <(python3 - "$cloned_store/clone.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]));print(r['clone_method'],r['clone_wall_ns'],r['clone_store_sha256'])
PY
)
}
if [[ $mode == admission ]]; then
  fixture_sizes=(1048576 10485760 104857600 524288000)
else
  selected_size=$(awk -F '\t' -v id="$selection" '$2==id {print $4}' "$run_dir/environment/scenario-registry.tsv")
  [[ -n $selected_size ]] || die "unknown selected scenario"
  fixture_sizes=("$selected_size")
fi
for size in "${fixture_sizes[@]}"; do prepare_size "$size"; done
python3 - "$run_dir/environment/fixture-manifest.json" "$scratch" <<'PY'
import glob,json,sys
rows=[json.load(open(path)) for path in sorted(glob.glob(sys.argv[2]+'/fixture-*.json'),key=lambda p:json.load(open(p))['fixture_bytes'])]
json.dump({'schema':'fs-bench-pro-sdk-edit-fixture-v1','fixtures':rows},open(sys.argv[1],'w'),sort_keys=True,separators=(',',':'))
open(sys.argv[1],'a').write('\n')
PY
printf 'family_id\tscenario_id\tplan_sha256\tinitial_branch_root\texpected_branch_root\texpected_file_root\texpected_mapping_root\tinitial_extent_count\texpected_extent_count\texpected_sha256\n' >"$run_dir/environment/qualification.tsv"
printf 'scenario_id\tqualification_wall_ns\tclone_wall_ns\tclone_method\tclone_store_sha256\n' >"$run_dir/environment/qualification-timing.tsv"
while IFS=$'\t' read -r _ scenario _ size _; do
  [[ $scenario == scenario_id ]] && continue
  [[ $mode == admission || $scenario == "$selection" ]] || continue
  branch=$(<"$scratch/prepared-$size/branch-id")
  clone_store "$size"
  python3 - "$baseline_bin" "$cloned_store" "$branch" "$family" "$scenario" "$run_dir/environment/qualification.tsv" "$run_dir/environment/qualification-timing.tsv" "$clone_wall_ns" "$clone_method" "$clone_sha256" <<'PY'
import subprocess,sys,time
binary,store,branch,family,scenario,output,timing,clone_wall,clone_method,clone_sha=sys.argv[1:]
started=time.monotonic_ns()
with open(output,'ab') as stdout:
    result=subprocess.run([binary,'sdk-edit-qualify',store,branch,family,scenario],stdout=stdout,timeout=30)
with open(timing,'a') as target: target.write(f'{scenario}\t{time.monotonic_ns()-started}\t{clone_wall}\t{clone_method}\t{clone_sha}\n')
raise SystemExit(result.returncode)
PY
  rm -rf -- "$cloned_store"
done <"$run_dir/environment/scenario-registry.tsv"
qualification_sha256=$(shasum -a 256 "$run_dir/environment/qualification.tsv" | awk '{print $1}')
docker image inspect "$baseline_image" "$candidate_image" >"$run_dir/environment/image.json"
python3 "$here/sdk-edit-custody.py" capture "$run_dir" "$baseline_bin" "$candidate_bin" "$baseline_revision" "$candidate_revision" "$baseline_build" "$candidate_build" "$mode"
python3 - "$run_dir/environment/edit-conformance-manifest.json" "$run_dir/environment/source-identity.json" "$mode" <<'PY'
import json,pathlib,sys
path=pathlib.Path(sys.argv[1]);manifest=json.loads(path.read_text());source=json.load(open(sys.argv[2]))
if sys.argv[3]=='admission':
    manifest['source_arm_build_manifests']={arm:source[f'{arm}_build_manifest_sha256'] for arm in ('baseline','candidate')}
    manifest['status']='pass-tested'
else: manifest['status']='source-only-selected'
path.write_text(json.dumps(manifest,sort_keys=True,separators=(',',':'))+'\n')
PY
conformance_sha256=$(shasum -a 256 "$run_dir/environment/edit-conformance-manifest.json" | awk '{print $1}')
baseline_image=$(docker image inspect -f '{{.Id}}' "$baseline_image")
candidate_image=$(docker image inspect -f '{{.Id}}' "$candidate_image")
source_identity_sha256=$(shasum -a 256 "$run_dir/environment/source-identity.json" | awk '{print $1}')
(cd "$run_dir" && find environment -type f ! -name pre-run.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >environment/pre-run.sha256)
(cd "$run_dir" && shasum -a 256 -c environment/pre-run.sha256 >/dev/null)

container_serial=0
performance_serial=0
run_in_fresh_container() {
  local arm=$1 command=$2 store_root=$3 branch=$4 scenario=$5 repetition=$6 row_ids=$7 output_path=$8 error_path=$9
  local bin image name container_id port capability running exit_code oom_killed
  if [[ $arm == baseline ]]; then bin=$baseline_bin; image=$baseline_image; else bin=$candidate_bin; image=$candidate_image; fi
  [[ -x $bin && -n $image ]] || die "$arm source assets"
  container_serial=$((container_serial + 1))
  name="layerfs-i20-$$-${container_serial}"
  active_container=$name
  container_start_started=$(python3 -c 'import time; print(time.monotonic_ns())')
  docker run -d --name "$name" --device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined -p 127.0.0.1::41273 "$image" >/dev/null
  container_id=$(docker inspect -f '{{.Id}}' "$name")
  port=$(docker inspect -f '{{(index (index .NetworkSettings.Ports "41273/tcp") 0).HostPort}}' "$name")
  ready=false
  for _ in $(seq 1 50); do
    if docker exec "$name" test -s /run/layerfs/capability >/dev/null 2>&1; then ready=true; break; fi
    sleep 0.05
  done
  [[ $ready == true ]] || die "daemon readiness timeout"
  capability=$(docker exec "$name" sh -c 'od -An -tx1 -v /run/layerfs/capability | tr -d " \n"')
  container_start_ns=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - container_start_started ))
  if [[ $command == performance ]]; then
    timeout_seconds=10
    command_args=(env LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon LAYERFS_DAEMON_TCP_ENDPOINT="127.0.0.1:$port" LAYERFS_DAEMON_CAPABILITY="$capability" LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal LAYERFS_SDK_EDIT_ADMISSION=$all LAYERFS_SDK_EDIT_TIMED_MANIFEST_SHA256="$timed_manifest_sha256" LAYERFS_SDK_EDIT_ROUTE_MANIFEST_SHA256="$route_manifest_sha256" "$bin" sdk-edit-run "$store_root" "$branch" "$family" "$scenario" "$arm" "$repetition" "$container_id")
  else
    timeout_seconds=30
    command_args=(env LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon LAYERFS_DAEMON_TCP_ENDPOINT="127.0.0.1:$port" LAYERFS_DAEMON_CAPABILITY="$capability" LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload LAYERFS_SDK_EDIT_CONFORMANCE_SHA256="$conformance_sha256" LAYERFS_SDK_EDIT_QUALIFICATION_FILE="$run_dir/environment/qualification.tsv" LAYERFS_SDK_EDIT_QUALIFICATION_SHA256="$qualification_sha256" LAYERFS_SDK_EDIT_TIMED_MANIFEST_SHA256="$timed_manifest_sha256" LAYERFS_SDK_EDIT_ROUTE_MANIFEST_SHA256="$route_manifest_sha256" "$bin" sdk-edit-verify "$store_root" "$branch" "$family" "$scenario" "$arm" "$container_id" "$row_ids")
  fi
  python3 - "$timeout_seconds" "$output_path" "$error_path" "${command_args[@]}" <<'PY'
import os,signal,subprocess,sys
seconds,stdout_path,stderr_path,*command=sys.argv[1:]
with open(stdout_path,'wb') as stdout,open(stderr_path,'wb') as stderr:
    process=subprocess.Popen(command,stdout=stdout,stderr=stderr,start_new_session=True)
    try:
        status=process.wait(timeout=int(seconds))
    except subprocess.TimeoutExpired:
        try: os.killpg(process.pid,signal.SIGKILL)
        except ProcessLookupError: pass
        process.wait()
        stderr.write(b'outer worker watchdog expired\n')
        raise SystemExit(124)
    raise SystemExit(status)
PY
  for _ in $(seq 1 100); do
    running=$(docker inspect -f '{{.State.Running}}' "$name")
    [[ $running == false ]] && break
    sleep 0.05
  done
  [[ $running == false ]] || die "container did not exit naturally"
  read -r exit_code oom_killed < <(docker inspect -f '{{.State.ExitCode}} {{.State.OOMKilled}}' "$name")
  [[ $exit_code == 0 && $oom_killed == false ]] || die "container exit/OOM"
  python3 - "$output_path" "$timed_manifest_sha256" "$route_manifest_sha256" "$run_dir/environment/source-identity.json" "$cloned_store/clone.json" "$arm" "$container_start_ns" "$performance_serial" <<'PY'
import hashlib,json,pathlib,sys
path=pathlib.Path(sys.argv[1]);row=json.loads(path.read_text());identity_path=pathlib.Path(sys.argv[4]);identity=json.loads(identity_path.read_text());arm=sys.argv[6]
row.update(container_cleanup_status='pass',container_exit_code=0,container_oom_killed=False,timed_call_graph_manifest_sha256=sys.argv[2],operation_route_manifest_sha256=sys.argv[3])
clone=json.load(open(sys.argv[5]));assert clone['status']=='pass' and clone['hard_link'] is False
row.update({key:clone[key] for key in ('clone_method','clone_wall_ns','clone_store_sha256','prepared_store_sha256','clone_bytes')})
cache=json.loads((identity_path.parent/f"prepared-cache-{row['initial_file_bytes']}.json").read_text())
row.update(cache_profile=cache['cache_profile'],cache_key=cache['key'],cache_manifest_sha256=cache['cache_manifest_sha256'],container_start_ns=int(sys.argv[7]))
row.update(source_identity_sha256=hashlib.sha256(identity_path.read_bytes()).hexdigest(),source_revision=identity[arm]['revision'],contract_commit=identity['contract_commit'],scenario_version=1,product_identity=identity[arm].get('product_seal'),harness_identity=identity[arm].get('harness_seal'),workload_identity=identity['workload_sha256'],report_generator_identity=identity['report_generator_sha256'],treatment=identity.get('treatment_sha256','selected-unbound'),operation_entrypoint='Client::edit_workspace_file_range',orchestration_executor='shell-and-native-supervisor',operation_contract_id='sdk-single-range-edit-v1',timing_boundary_id='sdk-edit-commit-return-v1')
if row.get('mode')=='performance': row['sample_ordinal']=int(sys.argv[8])
path.write_text(json.dumps(row,sort_keys=True,separators=(',',':'))+'\n')
PY
  docker rm "$name" >/dev/null
  active_container=
}

scenario_size() { awk -F '\t' -v id="$1" '$2==id {print $4}' "$run_dir/environment/scenario-registry.tsv"; }

run_performance() {
  local arm=$1 scenario=$2 repetition=$3 size branch output output_path error_path scenario_dir
  size=$(scenario_size "$scenario")
  branch=$(<"$scratch/prepared-$size/branch-id")
  clone_store "$size"
  performance_serial=$((performance_serial + 1))
  scenario_dir=$run_dir/scenarios/$scenario/$repetition
  mkdir -p "$scenario_dir"
  output_path=$scenario_dir/stdout-$arm.txt
  error_path=$scenario_dir/stderr-$arm.txt
  run_in_fresh_container "$arm" performance "$cloned_store" "$branch" "$scenario" "$repetition" - "$output_path" "$error_path"
  output=$(<"$output_path")
  printf '%s\n' "$output" >>"$run_dir/performance/raw.jsonl"
  printf '%s\n' "$output" >>"$scenario_dir/raw.jsonl"
  printf '%s sdk-edit-run %s repetition=%s clone_method=%s clone_wall_ns=%s clone_sha256=pass container_exit=0 oom_killed=false\n' "$arm" "$scenario" "$repetition" "$clone_method" "$clone_wall_ns" >>"$scenario_dir/supervisor.txt"
  printf '%s\t0\n' "$arm" >>"$scenario_dir/exit-status.txt"
  rm -rf -- "$cloned_store"
}

run_verifier() {
  local arm=$1 scenario=$2 size branch row_ids output output_path error_path
  size=$(scenario_size "$scenario")
  branch=$(<"$scratch/prepared-$size/branch-id")
  clone_store "$size"
  row_ids=
  if [[ $mode == admission ]]; then for repetition in 1 2 3 4 5; do row_ids+="${row_ids:+,}${family}:${scenario}:r${repetition}:${arm}"; done; else row_ids=-; fi
  output_path=$run_dir/scenarios/$scenario/verify-stdout-$arm.txt
  error_path=$run_dir/scenarios/$scenario/verify-stderr-$arm.txt
  mkdir -p "$run_dir/scenarios/$scenario"
  run_in_fresh_container "$arm" verify "$cloned_store" "$branch" "$scenario" 0 "$row_ids" "$output_path" "$error_path"
  output=$(<"$output_path")
  printf '%s\n' "$output" >>"$run_dir/verification/subproofs.jsonl"
  rm -rf -- "$cloned_store"
}

finalize_selected() {
  python3 - "$run_dir" "$family" "$mode" "$here/generate-sdk-edit-report.py" <<'PY'
import importlib.util,json,pathlib,sys
root=pathlib.Path(sys.argv[1]);family=sys.argv[2];mode=sys.argv[3]
selected_status='pass-selected-non-admission'
if mode=='performance':
    spec=importlib.util.spec_from_file_location('sdk_edit_report',sys.argv[4]);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    _,_,rows,failures,summary=module.performance_validation(root,selected=True)
    selected_status=summary['status']
else:
    rows=[json.loads(x) for x in (root/'verification/subproofs.jsonl').read_text().splitlines() if x]
    assert len(rows)==1 and rows[0]['performance_binding_status']=='unbound-selected-mode'
    receipt={'schema':'fs-bench-pro-sdk-edit-verification-v1','receipt_kind':'selected-verifier','family_id':family,'scenario_id':rows[0]['scenario_id'],'source_subproofs':{rows[0]['source_arm']:rows[0]},'performance_binding_status':'unbound-selected-mode','admission_eligible':False,'status':'pass-selected-non-admission'}
    (root/'verification/raw.jsonl').write_text(json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n')
    (root/'verification/summary.json').write_text(json.dumps({'schema':'fs-bench-pro-sdk-edit-summary-v1','family_id':family,'receipt_count':1,'status':'pass-selected-non-admission'},sort_keys=True,separators=(',',':'))+'\n')
status={'schema':'fs-bench-pro-sdk-edit-status-v1','family_id':family,'mode':mode,'admission_eligible':False,'status':selected_status}
(root/'run-status.json').write_text(json.dumps(status,sort_keys=True,separators=(',',':'))+'\n')
(root/'report.md').write_text(f'# {family} selected {mode}\n\nStatus: **{selected_status}**\n\nRaw evidence is retained under `{mode}/`.\n')
PY
  (cd "$run_dir" && shasum -a 256 -c environment/pre-run.sha256 >/dev/null)
  (cd "$run_dir" && find . -type f ! -path ./evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
  (cd "$run_dir" && shasum -a 256 -c evidence.sha256 >/dev/null)
  python3 - "$run_dir/run-status.json" <<'PY'
import json,sys
raise SystemExit(0 if json.load(open(sys.argv[1]))['status']=='pass-selected-non-admission' else 1)
PY
}

if [[ $mode == performance ]]; then
  run_performance "$source_arm" "$selection" "$repetition"
  finalize_selected
  exit 0
fi
if [[ $mode == verify ]]; then
  run_verifier "$source_arm" "$selection"
  finalize_selected
  exit 0
fi

while IFS=$'\t' read -r _ repetition scenario first second; do
  run_performance "$first" "$scenario" "$repetition"
  run_performance "$second" "$scenario" "$repetition"
done < <(tail -n +2 "$run_dir/environment/sample-order.tsv")

python3 "$here/generate-sdk-edit-report.py" "$run_dir" --performance-only

while IFS=$'\t' read -r _ scenario _; do
  [[ $scenario == scenario_id ]] && continue
  run_verifier baseline "$scenario"
  run_verifier candidate "$scenario"
done <"$run_dir/environment/scenario-registry.tsv"

python3 "$here/sdk-edit-custody.py" finalize "$run_dir"
python3 "$here/generate-sdk-edit-report.py" "$run_dir"
(cd "$run_dir" && shasum -a 256 -c environment/pre-run.sha256 >/dev/null)
(cd "$run_dir" && find . -type f ! -path ./evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
(cd "$run_dir" && shasum -a 256 -c evidence.sha256 >/dev/null)
trap - EXIT
cleanup 0
printf 'PASS %s %s\n' "$family" "$run_dir"
