#!/usr/bin/env bash
set -euo pipefail
if [[ ${LAYERFS_BENCH_ARCHIVAL:-0} != 1 ]]; then
  exec python3 "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)/shared/runner.py" --family store_footprint "$@"
fi
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root=${LAYERFS_STORE_RESULTS_ROOT:-$repo/benchmark-results/fs-bench-pro/store-footprint}
prepared_root=${LAYERFS_STORE_PREPARED_ROOT:-${TMPDIR:-/tmp}/layerfs-fs-bench-pro-store-footprint}
primary_control=store-footprint-unique-100000
evidence_version=v4

capture_failure() { failure_line=$1; failure_command=$2; }
die() { failure_line=${failure_line:-${BASH_LINENO[0]:-unknown}}; failure_reason=$*; printf 'fs-bench-pro Store footprint: %s\n' "$*" >&2; exit 2; }
trap 'capture_failure "$LINENO" "$BASH_COMMAND"' ERR

write_failure_context() {
  python3 - "$@" <<'PY'
import json,sys
path,case,seed,mode,status,timeout,line,shell_command,semantic_command,stderr=sys.argv[1:]
json.dump({'schema':'fs-bench-pro-store-failure-context-v3','case':case,'seed':seed,'sample_mode':mode,'exit_status':status,'timeout_seconds':timeout,'line':line,'shell_command':shell_command,'semantic_command':semantic_command,'summary_stderr':stderr},open(path,'w'),sort_keys=True,separators=(',',':'));open(path,'a').write('\n')
PY
}

self_check() {
  local scratch started elapsed
  started=$(python3 -c 'import time; print(time.monotonic_ns())')
  bash -n "$0"
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-store-self-check.XXXXXX")
  trap 'rm -rf -- "$scratch"' EXIT
  printf 'mod family { include!(r#"%s"#); } fn main() { family::self_check().unwrap(); assert_eq!(family::CONTROLS.iter().filter(|c| c.infra_tier == 100_000).count(), 3); }\n' \
    "$here/families/store_footprint/mod.rs" >"$scratch/check.rs"
  rustc --edition=2021 -Awarnings "$scratch/check.rs" -o "$scratch/check"
  "$scratch/check"
  write_failure_context "$scratch/failure.json" case 1 verify 23 90 250 \
    'env KEY=value command --flag value' 'store-footprint-verify case' 'supervisor.txt'
  python3 - "$scratch/failure.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]));assert r['shell_command']=='env KEY=value command --flag value' and r['exit_status']=='23' and r['line']=='250'
PY
  elapsed=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - started ))
  (( elapsed < 2000000000 )) || die "self-check exceeded two seconds"
  rm -rf -- "$scratch"
  trap - EXIT
  printf '{"schema":"fs-bench-pro-store-footprint-self-check-v1","elapsed_ns":%s,"container_started":false,"status":"pass"}\n' "$elapsed"
}

if [[ ${1:-} == --self-check ]]; then [[ $# == 1 ]] || die "--self-check takes no arguments"; self_check; exit 0; fi

[[ $# -ge 2 ]] || die "usage: $0 RUN_ID CONTAINER --case CONTROL --seed 1 --source baseline|candidate [--mode performance|verify] [--tier 100|1000|10000] | RUN_ID CONTAINER --all --source baseline|candidate --mode admission"
invocation_argv=("$@")
run_id=$1
container=$2
shift 2
selection= seed= source_arm= baseline_run= mode=performance tier=100 all=0 mode_set=0 tier_set=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --case) [[ $# -ge 2 && -z $selection ]] || die "duplicate/missing --case"; selection=$2; shift 2 ;;
    --seed) [[ $# -ge 2 && -z $seed ]] || die "duplicate/missing --seed"; seed=$2; shift 2 ;;
    --source) [[ $# -ge 2 && -z $source_arm ]] || die "duplicate/missing --source"; source_arm=$2; shift 2 ;;
    --baseline-run) [[ $# -ge 2 && -z $baseline_run ]] || die "duplicate/missing --baseline-run"; baseline_run=$2; shift 2 ;;
    --mode) [[ $# -ge 2 && $mode_set == 0 ]] || die "duplicate/missing --mode"; mode=$2; mode_set=1; shift 2 ;;
    --tier) [[ $# -ge 2 && $tier_set == 0 ]] || die "duplicate/missing --tier"; tier=$2; tier_set=1; shift 2 ;;
    --all) [[ $all == 0 ]] || die "duplicate --all"; all=1; shift ;;
    *) die "unknown argument: $1" ;;
  esac
done
[[ $run_id =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || die "unsafe RUN_ID"
[[ $source_arm == baseline || $source_arm == candidate ]] || die "explicit source arm is required"
case "$mode" in
  performance) [[ $all == 0 && -n $selection && $seed =~ ^[123]$ && $tier =~ ^(100|1000|10000)$ && -z $baseline_run ]] || die "selected performance arguments" ;;
  verify) [[ $all == 0 && -n $selection && ${seed:-1} =~ ^[123]$ && $tier =~ ^(100|1000|10000|100000)$ && -z $baseline_run ]] || die "selected verify arguments"; seed=${seed:-1} ;;
  admission) [[ $all == 1 && -z $selection && -z $seed && $tier_set == 0 ]] || die "admission requires --all and no case/seed/tier"; tier=100000 ;;
  collect|verify-all) [[ $all == 1 && -z $selection && -z $seed && $tier_set == 0 && -z $baseline_run ]] || die "full phase requires --all and no case/seed/tier/baseline"; tier=100000 ;;
  *) die "unknown mode: $mode" ;;
esac
if [[ $mode == admission && $source_arm == candidate ]]; then
  [[ -n $baseline_run && -f $baseline_run/evidence.sha256 ]] || die "candidate admission requires --baseline-run with sealed baseline evidence"
  baseline_run=$(cd "$baseline_run" && pwd -P)
  (cd "$baseline_run" && shasum -a 256 -c evidence.sha256 >/dev/null) || die "baseline evidence seal"
elif [[ -n $baseline_run ]]; then
  die "--baseline-run is only valid for candidate admission"
fi

for command in cargo docker nc python3 rustc sqlite3; do command -v "$command" >/dev/null || die "$command is required"; done
if [[ $mode == admission || $mode == collect || $mode == verify-all ]]; then
  git -C "$repo" diff-files --quiet || die "admission requires a clean tracked worktree"
  git -C "$repo" diff-index --cached --quiet HEAD -- || die "admission requires an index equal to HEAD"
  [[ $(git -C "$repo" write-tree) == $(git -C "$repo" rev-parse HEAD^{tree}) ]] || die "admission tree differs from HEAD"
  [[ -z $(git -C "$repo" ls-files --others --exclude-standard -- Cargo.toml Cargo.lock crates tools benchmark/fs-bench-pro) ]] || die "admission has untracked image/product/harness inputs"
  read -r image_revision image_tree image_dirty < <(docker inspect -f '{{index .Config.Labels "org.opencontainers.image.revision"}} {{index .Config.Labels "org.opencontainers.image.source-tree"}} {{index .Config.Labels "dev.layerfs.source-dirty"}}' "$container")
  [[ $image_revision == $(git -C "$repo" rev-parse HEAD) && $image_tree == $(git -C "$repo" rev-parse HEAD^{tree}) && $image_dirty == false ]] || die "admission candidate image is not clean HEAD"
fi
source_seal=$("$here/run-namespace.sh" --source-seal)
product_seal=$("$here/run-namespace.sh" --product-seal)
harness_seal=$("$here/run-namespace.sh" --harness-seal)
workload_sha=$(shasum -a 256 "$here/workload.rs" | awk '{print $1}')
cdc_identity=$(git -C "$repo" rev-parse HEAD:crates/layerfs-content/src/file/cdc)
[[ $(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container") == "$source_seal" ]] || die "container/source seal mismatch"
docker inspect "$container" | python3 -c 'import json,sys; assert not any(x.get("Type") == "bind" for x in json.load(sys.stdin)[0].get("Mounts", []))' || die "container has a bind mount"
container_id=$(docker inspect -f '{{.Id}}' "$container")
prepared="$prepared_root/$source_seal"
fixture_cache="$prepared_root/fixtures-fs-bench-pro-store-footprint-fixture-v1"
mkdir -p "$prepared_root"
if [[ ! -x $prepared/fs-benchmark-pro || ! -x $prepared/fs-benchmark-workload ]]; then
  stage=$(mktemp -d "$prepared_root/.prepare.XXXXXX")
  trap 'rm -rf -- "$stage"' EXIT
  cargo build --release --manifest-path "$repo/Cargo.toml" -p fs-benchmark-pro >/dev/null
  cp "$repo/target/release/fs-benchmark-pro" "$stage/fs-benchmark-pro"
  rustc --edition=2021 -C opt-level=3 "$here/workload.rs" -o "$stage/fs-benchmark-workload"
  "$stage/fs-benchmark-pro" self-check >"$stage/host-self-check.txt"
  "$stage/fs-benchmark-workload" store-footprint-self-check >"$stage/workload-self-check.txt"
  mv "$stage" "$prepared"
  trap - EXIT
fi
binary="$prepared/fs-benchmark-pro"
oracle_workload="$prepared/fs-benchmark-workload"
if [[ -n $selection ]]; then "$oracle_workload" store-footprint-resolve "$selection" >/dev/null || die "unknown control"; fi
if [[ $all == 1 ]]; then mapfile="$prepared/controls.tsv"; "$oracle_workload" store-footprint-list >"$mapfile"; else mapfile="$prepared/selected.tsv"; printf '%s\tselected\n' "$selection" >"$mapfile"; fi

fixture_for() {
  local control=$1 fixture_dir fixture_stage
  fixture_dir="$fixture_cache/$tier/$control"
  if [[ ! -f $fixture_dir/manifest.json || ! -d $fixture_dir/fixture ]]; then
    mkdir -p "$fixture_cache/$tier"
    fixture_stage=$(mktemp -d "$fixture_cache/$tier/.fixture.XXXXXX")
    "$oracle_workload" store-footprint-fixture "$fixture_stage/fixture" "$control" "$tier" >"$fixture_stage/manifest.json"
    mv "$fixture_stage" "$fixture_dir"
  fi
  printf '%s\n' "$fixture_dir"
}

run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/performance" "$run_dir/verification" "$run_dir/controls" "$run_dir/scenarios"
printf '%q ' "$0" "${invocation_argv[@]}" >"$run_dir/environment/runner-arguments.txt"
printf '\n' >>"$run_dir/environment/runner-arguments.txt"
cp "$run_dir/environment/runner-arguments.txt" "$run_dir/environment/command.txt"
printf '%s\n' "$source_seal" >"$run_dir/environment/source-seal.txt"
source_commit=$(git -C "$repo" rev-parse HEAD)
printf '%s\n' "$source_commit" >"$run_dir/environment/source-commit.txt"
printf '%s\n' "$product_seal" >"$run_dir/environment/product-seal.txt"
printf '%s\n' "$harness_seal" >"$run_dir/environment/harness-seal.txt"
printf '%s\n' "$workload_sha" >"$run_dir/environment/workload-sha256.txt"
printf '%s\n' "$cdc_identity" >"$run_dir/environment/cdc-identity.txt"
printf '{"schema":"fs-bench-pro-source-seal-v1","source_commit":"%s","source_seal":"%s"}\n' "$source_commit" "$source_seal" >"$run_dir/environment/source-seal.json"
docker inspect "$container" >"$run_dir/environment/container.json"
docker image inspect "$(docker inspect -f '{{.Image}}' "$container")" >"$run_dir/environment/image.json"
docker version --format '{{json .}}' >"$run_dir/environment/docker.json"
docker inspect -f '{{.Image}}' "$container" >"$run_dir/environment/image-digest.txt"
{ uname -a; sw_vers 2>/dev/null || true; system_profiler SPHardwareDataType 2>/dev/null || true; } >"$run_dir/environment/host.txt"
python3 - "$run_dir/environment/host.txt" "$run_dir/environment/host.json" <<'PY'
import json,platform,sys
json.dump({'schema':'fs-bench-pro-host-v1','architecture':platform.machine(),'platform':platform.platform(),'raw':open(sys.argv[1]).read()},open(sys.argv[2],'w'),sort_keys=True,separators=(',',':'));open(sys.argv[2],'a').write('\n')
PY
printf '%s\n' 'Store is host-resident; FUSE projection and workload are in the managed Docker Desktop Linux container. Initialization, Commit, Branch visibility, End, reconnect, and root validation are measured; full tree digest is verify-only.' >"$run_dir/environment/acknowledgement-boundary.txt"
printf '%s\n' 'sealed fixture cache keyed by fs-bench-pro-store-footprint-fixture-v1 and reused across harness/product-only source arms; every sample uses a fresh Store, Branch, Workspace, and workload process' >"$run_dir/environment/cache-profile.txt"
printf '%s\n' 'LayerStackStore::create(root/store.sqlite); host-resident regular file; SQLite schema v4; 64 KiB requested page size' >"$run_dir/environment/store-creation-profile.txt"
"$oracle_workload" store-footprint-list >"$run_dir/controls/registry.tsv"

if [[ -n $baseline_run ]]; then
  for file in harness-seal.txt workload-sha256.txt cdc-identity.txt cache-profile.txt store-creation-profile.txt docker.json host.json source-commit.txt; do
    [[ -f $baseline_run/environment/$file ]] || die "baseline custody missing $file"
  done
  cmp -s "$run_dir/environment/harness-seal.txt" "$baseline_run/environment/harness-seal.txt" || die "baseline/candidate harness identity"
  cmp -s "$run_dir/environment/workload-sha256.txt" "$baseline_run/environment/workload-sha256.txt" || die "baseline/candidate workload identity"
  cmp -s "$run_dir/environment/cdc-identity.txt" "$baseline_run/environment/cdc-identity.txt" || die "baseline/candidate CDC identity"
  cmp -s "$run_dir/environment/cache-profile.txt" "$baseline_run/environment/cache-profile.txt" || die "baseline/candidate cache profile"
  cmp -s "$run_dir/environment/store-creation-profile.txt" "$baseline_run/environment/store-creation-profile.txt" || die "baseline/candidate Store creation profile"
  cmp -s "$run_dir/environment/docker.json" "$baseline_run/environment/docker.json" || die "baseline/candidate Docker identity"
  python3 - "$baseline_run/environment/host.json" "$run_dir/environment/host.json" <<'PY' || die "baseline/candidate host identity"
import json,sys
left,right=map(lambda path:json.load(open(path)),sys.argv[1:])
assert (left['architecture'],left['platform'])==(right['architecture'],right['platform'])
PY
  python3 - "$baseline_run/environment/container.json" "$run_dir/environment/container.json" <<'PY' || die "baseline/candidate container resource identity"
import json,sys
left,right=map(lambda path:json.load(open(path))[0],sys.argv[1:])
keys=('Privileged','CapAdd','CapDrop','Devices','Memory','MemorySwap','MemoryReservation','NanoCpus','CpuQuota','CpuPeriod','CpusetCpus','PidsLimit','ReadonlyRootfs')
assert {k:left['HostConfig'].get(k) for k in keys}=={k:right['HostConfig'].get(k) for k in keys}
def ports(value): return {k:[x.get('HostIp') for x in v or []] for k,v in value['HostConfig'].get('PortBindings',{}).items()}
assert ports(left)==ports(right)=={'41273/tcp':['127.0.0.1']}
assert not left.get('Mounts') and not right.get('Mounts')
PY
  baseline_commit=$(<"$baseline_run/environment/source-commit.txt")
  changed=$(git -C "$repo" diff --name-only "$baseline_commit" "$source_commit" -- crates/layerfs-content crates/layerfs-daemon crates/layerfs-layerstack-store crates/layerfs-sdk crates/layerfs-workspace crates/layerfs-fuse crates/layerfs-materialization crates/layerfs-monitor)
  [[ $changed == crates/layerfs-layerstack-store/src/objects.rs ]] || die "candidate product diff is not the allowed bounded-order change"
  git -C "$repo" diff --unified=0 "$baseline_commit" "$source_commit" -- crates/layerfs-layerstack-store/src/objects.rs >"$run_dir/environment/allowed-candidate.patch"
  python3 - "$run_dir/environment/allowed-candidate.patch" <<'PY' || die "candidate product diff exceeds encoded-length ordering"
import sys
text=open(sys.argv[1]).read()
removed=[x[1:] for x in text.splitlines() if x.startswith('-') and not x.startswith('---')]
added=[x[1:] for x in text.splitlines() if x.startswith('+') and not x.startswith('+++')]
assert removed==['        batch.sort_unstable_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));']
assert added==['        batch.sort_unstable_by(|left, right| {','            left.bytes','                .len()','                .cmp(&right.bytes.len())','                .then_with(|| left.id.as_bytes().cmp(right.id.as_bytes()))','        });']
PY
fi

seal_failed_run() {
  local status=$?
  trap - EXIT
  [[ $status != 0 && ! -f $run_dir/evidence.sha256 ]] || exit "$status"
  set +e
  printf '%s\n' "${failure_reason:-unhandled runner failure}" >"$run_dir/environment/failure.txt"
  write_failure_context "$run_dir/environment/failure-context.json" \
    "${failure_case:-not-started}" "${failure_seed:-not-started}" "${failure_mode:-not-started}" \
    "${failure_status:-unknown}" "${failure_timeout:-unknown}" "${failure_line:-unknown}" \
    "${failure_command_exact:-${failure_command:-unknown}}" "${failure_command_safe:-not-started}" \
    "${failure_stderr:-not-started}"
  python3 - "$run_dir/run-status.json" "$evidence_version" "$mode" "$source_arm" "${failure_reason:-unhandled runner failure}" <<'PY'
import json,sys
json.dump({'schema':f'fs-bench-pro-store-footprint-status-{sys.argv[2]}','mode':sys.argv[3],'source_arm':sys.argv[4],'status':'hard-failure','admission_eligible':False,'reason':sys.argv[5]},open(sys.argv[1],'w'),sort_keys=True,separators=(',',':'));open(sys.argv[1],'a').write('\n')
PY
  docker stop "$container" >/dev/null 2>&1 || true
  docker inspect "$container" >"$run_dir/environment/container-after.json" 2>/dev/null || true
  (cd "$run_dir" && find . -type f ! -name evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
  exit "$status"
}
trap seal_failed_run EXIT

daemon_endpoint=
daemon_capability=
ensure_daemon() {
  if [[ $(docker inspect -f '{{.State.Running}}' "$container") != true ]]; then
    [[ $(docker inspect -f '{{.State.ExitCode}} {{.State.OOMKilled}}' "$container") == '0 false' ]] || die "container stopped abnormally"
    docker start "$container" >/dev/null
  fi
  for _ in $(seq 1 100); do
    daemon_endpoint=$(docker port "$container" 41273/tcp 2>/dev/null || true)
    if [[ $daemon_endpoint =~ ^127\.0\.0\.1:[1-9][0-9]{0,4}$ ]] && docker exec "$container" test -s /run/layerfs/capability 2>/dev/null && nc -z "${daemon_endpoint%:*}" "${daemon_endpoint##*:}" 2>/dev/null; then
      daemon_capability=$(docker exec "$container" sh -c "od -An -tx1 -v /run/layerfs/capability | tr -d ' \\n'")
      [[ $daemon_capability =~ ^[0-9a-f]{64}$ ]] || die "daemon capability"
      if [[ ! -f $run_dir/environment/container-kernel-fuse.txt ]]; then
        docker exec "$container" sh -c 'uname -a; stat -c "dev_fuse_type=%F dev_fuse_mode=%a" /dev/fuse; printf "capability_bytes="; wc -c </run/layerfs/capability; printf "fuse_filesystems="; grep -c fuse /proc/filesystems' >"$run_dir/environment/container-kernel-fuse.txt"
      fi
      return
    fi
    sleep 0.1
  done
  die "daemon readiness"
}

await_daemon() {
  local code
  code=$(perl -e 'alarm 5; exec @ARGV' docker wait "$container")
  [[ $code == 0 && $(docker inspect -f '{{.State.Status}} {{.State.OOMKilled}}' "$container") == 'exited false' ]] || die "daemon exit"
}

performance_external_wall_ns=0
verification_external_wall_ns=0

run_sample() {
  local control=$1 sample_seed=$2 sample_mode=$3 fixture_dir manifest sample_dir status raw_target before_sha after_sha nonce control_manifest timeout_seconds sample_started sample_wall_ns
  local -a sample_command
  fixture_dir=$(fixture_for "$control")
  manifest="$fixture_dir/manifest.json"
  control_manifest="$run_dir/controls/$control.json"
  if [[ -f $control_manifest ]]; then cmp -s "$manifest" "$control_manifest" || die "control fixture identity changed"; else cp "$manifest" "$control_manifest"; fi
  sample_dir="$run_dir/scenarios/$control/$source_arm/seed-$sample_seed-$sample_mode"
  mkdir -p "$sample_dir"
  read -r expected_files expected_logical edit_path edit_size fixture_digest edited_digest < <(python3 - "$manifest" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]));print(r['regular_files'],r['logical_bytes'],r['edit_path'],r['edit_size'],r['fixture_digest'],r['edited_fixture_digest'])
PY
  )
  ensure_daemon
  nonce=$(od -An -tx1 -N16 /dev/urandom | tr -d ' \n')
  if [[ $sample_mode == verify ]]; then timeout_seconds=90; elif [[ $mode == admission || $mode == collect ]]; then timeout_seconds=30; else timeout_seconds=5; fi
  failure_case=$control
  failure_seed=$sample_seed
  failure_mode=$sample_mode
  failure_timeout=$timeout_seconds
  failure_command_safe="fs-benchmark-pro store-footprint-$sample_mode CONTROL=$control SEED=$sample_seed SOURCE=$source_arm TIER=$tier"
  failure_stderr="$sample_dir/supervisor.txt"
  sample_started=$(python3 -c 'import time; print(time.monotonic_ns())')
  sample_command=(
    /usr/bin/time -l -p perl -e 'alarm shift; exec @ARGV' "$timeout_seconds" env
    LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE="$nonce"
    LAYERFS_BENCH_INITIALIZATION_SEED_HEX="$fixture_digest"
    LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload
    LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon
    LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability"
    LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal
    "$binary" "store-footprint-$sample_mode" "$sample_dir/work" "$fixture_dir/fixture" "$container_id"
    "$control" "$sample_seed" "$source_arm" "$expected_files" "$expected_logical" "$edit_path" "$edit_size"
    "$fixture_digest" "$edited_digest"
  )
  printf -v failure_command_exact '%q ' "${sample_command[@]}"
  set +e
  "${sample_command[@]}" >"$sample_dir/raw.jsonl" 2>"$sample_dir/supervisor.txt"
  status=$?
  failure_status=$status
  set -e
  printf '%s\n' "$status" >"$sample_dir/exit-status.txt"
  [[ $status == 0 ]] || die "$sample_mode failed: $control seed $sample_seed"
  await_daemon
  sample_wall_ns=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - sample_started ))
  printf '%s\n' "$sample_wall_ns" >"$sample_dir/external-wall-ns.txt"
  if [[ $sample_mode == performance ]]; then performance_external_wall_ns=$((performance_external_wall_ns + sample_wall_ns)); else verification_external_wall_ns=$((verification_external_wall_ns + sample_wall_ns)); fi
  python3 - "$sample_dir/work" "$sample_dir/durable-census.json" <<'PY'
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]); rows=[]
for path in sorted(x for x in root.rglob('*') if x.is_file()):
 h=hashlib.sha256()
 with path.open('rb') as f:
  while chunk:=f.read(1024*1024): h.update(chunk)
 rows.append({'path':str(path.relative_to(root)),'bytes':path.stat().st_size,'sha256':h.hexdigest()})
json.dump({'schema':'fs-bench-pro-store-durable-census-v1','files':rows,'file_count':len(rows),'total_bytes':sum(x['bytes'] for x in rows)},open(sys.argv[2],'w'),sort_keys=True,separators=(',',':'));open(sys.argv[2],'a').write('\n')
PY
  before_sha=$(shasum -a 256 "$sample_dir/work/store.sqlite" | awk '{print $1}')
  sqlite3 -readonly -json "$sample_dir/work/store.sqlite" "SELECT page_size AS sqlite_page_size_bytes,page_count AS sqlite_page_count,freelist_count AS sqlite_freelist_pages,(SELECT count(*) FROM objects) AS sqlite_object_rows,(SELECT coalesce(sum(length(bytes)),0) FROM objects) AS sqlite_canonical_object_bytes,(SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name='objects') AS sqlite_objects_table_bytes,(SELECT coalesce(sum(payload),0) FROM dbstat WHERE name='objects') AS sqlite_objects_table_payload_bytes,(SELECT coalesce(sum(unused),0) FROM dbstat WHERE name='objects') AS sqlite_objects_table_unused_bytes,(SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name='sqlite_autoindex_objects_1') AS sqlite_objects_index_bytes FROM pragma_page_size,pragma_page_count,pragma_freelist_count;" >"$sample_dir/sqlite.json"
  sqlite3 -readonly -header -column "$sample_dir/work/store.sqlite" 'SELECT name,count(*) AS pages,sum(pgsize) AS allocated_bytes,sum(payload) AS payload_bytes,sum(unused) AS unused_bytes FROM dbstat GROUP BY name ORDER BY allocated_bytes DESC;' >"$sample_dir/dbstat.txt"
  python3 - "$sample_dir/work/store.sqlite" "$sample_dir/object-set.json" <<'PY'
import hashlib,json,sqlite3,sys
db,out=sys.argv[1:]
connection=sqlite3.connect(f'file:{db}?mode=ro',uri=True)
page_size=connection.execute('PRAGMA page_size').fetchone()[0]
user_version=connection.execute('PRAGMA user_version').fetchone()[0]
schema=connection.execute("SELECT type,name,tbl_name,coalesce(sql,'') FROM sqlite_master ORDER BY type,name").fetchall()
schema_digest=hashlib.sha256(json.dumps([page_size,user_version,schema],separators=(',',':')).encode()).hexdigest()
digest=hashlib.sha256(b'layerfs/fs-bench-pro/object-set/v1\0'); rows=bytes_=0
for object_id,length in connection.execute('SELECT object_id,length(bytes) FROM objects ORDER BY object_id'):
 digest.update(len(object_id).to_bytes(8,'big')); digest.update(object_id); digest.update(length.to_bytes(8,'big')); rows+=1; bytes_+=length
shape=hashlib.sha256(b'layerfs/fs-bench-pro/object-shape/v1\0')
for tag,length,count in connection.execute('SELECT substr(bytes,1,1),length(bytes),count(*) FROM objects GROUP BY substr(bytes,1,1),length(bytes) ORDER BY substr(bytes,1,1),length(bytes)'):
 shape.update(len(tag).to_bytes(8,'big'));shape.update(tag);shape.update(length.to_bytes(8,'big'));shape.update(count.to_bytes(8,'big'))
connection.close()
json.dump({'schema':'fs-bench-pro-store-object-set-v2','object_set_digest':digest.hexdigest(),'object_shape_digest':shape.hexdigest(),'schema_digest':schema_digest,'page_size':page_size,'user_version':user_version,'canonical_objects':rows,'canonical_bytes':bytes_},open(out,'w'),sort_keys=True,separators=(',',':'));open(out,'a').write('\n')
PY
  after_sha=$(shasum -a 256 "$sample_dir/work/store.sqlite" | awk '{print $1}')
  [[ $before_sha == "$after_sha" ]] || die "dbstat mutated Store"
  python3 - "$sample_dir/raw.jsonl" "$sample_dir/sqlite.json" "$sample_dir/supervisor.txt" "$sample_dir/durable-census.json" "$sample_dir/object-set.json" "$sample_dir/result.json" "$tier" <<'PY'
import json,re,sys
raw,sqlite,supervisor,census,object_set,out,tier=sys.argv[1:]
rows=[json.loads(x) for x in open(raw) if x.startswith('{')]; assert len(rows)==1
r=rows[0]; q=json.load(open(sqlite)); assert len(q)==1; q=q[0]; c=json.load(open(census)); o=json.load(open(object_set))
text=open(supervisor).read(); lines=[x for x in text.splitlines() if x.startswith('layerfs-initialization-diagnostic-v3 ')]
assert len(lines)==1
metrics=dict(re.findall(r'([a-z0-9_]+)=([^ ]+)',lines[0]))
initialization_temporary_write=sum(int(metrics[x]) for x in ('object_segment_write_bytes','pair_segment_write_bytes'))
temporary_read=sum(int(metrics[x]) for x in ('object_segment_raw_read_bytes','pair_segment_raw_read_bytes'))
temporary_write=initialization_temporary_write+r['workspace_spool_write_bytes']
temporary_peak=initialization_temporary_write+r['workspace_spool_peak_bytes']
assert q['sqlite_page_size_bytes']*q['sqlite_page_count']==r['sqlite_database_bytes']
assert q['sqlite_object_rows']==r['canonical_objects'] and q['sqlite_canonical_object_bytes']==r['canonical_bytes']
assert c['file_count']==r['durable_store_files'] and c['total_bytes']==r['total_durable_store_bytes']
assert o['canonical_objects']==r['canonical_objects'] and o['canonical_bytes']==r['canonical_bytes'] and o['page_size']==q['sqlite_page_size_bytes']
if r['mode']=='verify': assert r['verification_ns']<=66_000_000_000 and r['initialization_ns']<=30_000_000_000
r.update(q);r.update({k:o[k] for k in ('object_set_digest','object_shape_digest','schema_digest','user_version')})
r.update({'tier':int(tier),'reportable':int(tier)==100000,'initialization_temporary_write_bytes':initialization_temporary_write,'temporary_write_bytes':temporary_write,'temporary_read_bytes':temporary_read,'temporary_peak_upper_bound_bytes':temporary_peak,'peak_disk_upper_bound_bytes':r['total_durable_store_bytes']+temporary_peak,'durable_census_status':'pass','dbstat_store_sha256_unchanged':True})
json.dump(r,open(out,'w'),sort_keys=True,separators=(',',':'));open(out,'a').write('\n')
PY
  if [[ $sample_mode == performance ]]; then raw_target="$run_dir/performance/raw.jsonl"; else raw_target="$run_dir/verification/raw.jsonl"; fi
  cat "$sample_dir/result.json" >>"$raw_target"
}

precondition_fixture() {
  local control=$1 fixture_dir manifest expected output
  fixture_dir=$(fixture_for "$control")
  manifest="$fixture_dir/manifest.json"
  expected=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["fixture_digest"])' "$manifest")
  mkdir -p "$run_dir/environment/preconditioning"
  output="$run_dir/environment/preconditioning/$control.txt"
  "$oracle_workload" store-footprint-digest "$fixture_dir/fixture" >"$output"
  grep -Fx "tree_digest=$expected" "$output" >/dev/null || die "fixture preconditioning digest: $control"
}

if [[ $mode == performance || $mode == verify ]]; then
  run_sample "$selection" "$seed" "$mode"
else
  while IFS=$'\t' read -r control _; do precondition_fixture "$control"; done <"$mapfile"
  if [[ $mode != verify-all ]]; then
    while IFS=$'\t' read -r control _; do for sample_seed in 1 2 3; do run_sample "$control" "$sample_seed" performance; done; done <"$mapfile"
  fi
  if [[ $mode != collect ]]; then
    while IFS=$'\t' read -r control _; do run_sample "$control" 1 verify; done <"$mapfile"
  fi
fi
printf '%s\n' "$performance_external_wall_ns" >"$run_dir/environment/performance-external-wall-ns.txt"
printf '%s\n' "$verification_external_wall_ns" >"$run_dir/environment/verification-external-wall-ns.txt"
(( verification_external_wall_ns <= 180000000000 )) || die "aggregate verification wall exceeded 180 seconds"
if [[ -n $baseline_run ]]; then cmp -s "$run_dir/environment/container-kernel-fuse.txt" "$baseline_run/environment/container-kernel-fuse.txt" || die "baseline/candidate kernel or FUSE identity"; fi

python3 - "$run_dir" "$mode" "$source_arm" "$baseline_run" "$evidence_version" "$primary_control" <<'PY'
import json,statistics,sys
from pathlib import Path
root,mode,source,baseline,version,primary=Path(sys.argv[1]),sys.argv[2],sys.argv[3],Path(sys.argv[4]),sys.argv[5],sys.argv[6]
performance=[json.loads(x) for x in (root/'performance/raw.jsonl').read_text().splitlines()] if (root/'performance/raw.jsonl').exists() else []
verification=[json.loads(x) for x in (root/'verification/raw.jsonl').read_text().splitlines()] if (root/'verification/raw.jsonl').exists() else []
summary={'schema':f'fs-bench-pro-store-footprint-summary-{version}','mode':mode,'source_arm':source,'samples':len(performance),'verification_samples':len(verification),'primary_control_id':primary}
metrics=('total_durable_store_bytes','initialization_ns','commit_ns','reopen_ns','complete_ns','canonical_objects','canonical_bytes','process_user_cpu_ns','process_system_cpu_ns','process_disk_read_bytes','process_disk_write_bytes','process_peak_rss_bytes','process_physical_footprint_bytes','container_memory_peak_bytes','temporary_write_bytes','temporary_read_bytes','temporary_peak_upper_bound_bytes','peak_disk_upper_bound_bytes')
def grouped_medians(rows):
 return {control:{k:statistics.median(x[k] for x in rows if x['control_id']==control) for k in metrics} for control in sorted({x['control_id'] for x in rows})}
performance_summary={'schema':f'fs-bench-pro-store-footprint-performance-summary-{version}','mode':mode,'source_arm':source,'samples':len(performance),'status':'no-performance-samples'}
verification_summary={'schema':f'fs-bench-pro-store-footprint-verification-summary-{version}','mode':mode,'source_arm':source,'samples':len(verification),'status':'not-run'}
if performance:
 performance_summary.update({'medians':grouped_medians(performance),'status':'complete'})
if verification:
 assert all(x['status']=='pass' and x['mode']=='verify' for x in verification)
 verifier_max=max(x['verification_ns'] for x in verification)
 verification_summary.update({'status':'target-pass' if verifier_max<=60_000_000_000 else 'tolerated-pass','maximum_verification_ns':verifier_max,'target_ns':60_000_000_000,'tolerated_ns':66_000_000_000,'resources':grouped_medians(verification)})
if mode=='admission':
 assert len(performance)==9 and len(verification)==3
 assert all(x['reportable'] and x['tier']==100000 and x['status']=='pass' and x['durable_census_status']=='pass' and x['dbstat_store_sha256_unchanged'] for x in performance+verification)
 assert {(x['control_id'],x['seed']) for x in performance}=={(c,s) for c in ('store-footprint-unique-100000','store-footprint-metadata-cardinality-100000','store-footprint-large-object-500m') for s in (1,2,3)}
 medians=grouped_medians(performance)
 summary['medians']=medians
 primary_bytes=medians[primary]['total_durable_store_bytes']
 summary['primary_total_durable_store_bytes']=primary_bytes
 summary['primary_storage_classification']='target-pass' if primary_bytes<=600_000_000 else 'tolerated-nonterminal-miss' if primary_bytes<=660_000_000 else 'no-go'
 summary['explanatory_control_bytes']={control:row['total_durable_store_bytes'] for control,row in medians.items() if control!=primary}
 if source=='baseline':
  summary.update({'status':'baseline-complete','admission_eligible':False,'resource_envelope_multiplier':1.10,'selected_performance_timeout_seconds':5,'admission_performance_timeout_seconds':30,'verifier_phase_target_seconds':60,'verifier_phase_tolerated_seconds':66,'verifier_process_timeout_seconds':90,'aggregate_verifier_timeout_seconds':180})
 else:
  baseline_summary=json.load(open(baseline/'summary.json'))
  baseline_rows=[json.loads(x) for x in (baseline/'performance/raw.jsonl').read_text().splitlines()]
  assert baseline_summary['mode']=='admission' and baseline_summary['source_arm']=='baseline' and baseline_summary['status']=='baseline-complete'
  assert len(baseline_rows)==9 and all(x['reportable'] and x['tier']==100000 for x in baseline_rows)
  baseline_verification=[json.loads(x) for x in (baseline/'verification/raw.jsonl').read_text().splitlines()]
  assert len(baseline_verification)==3
  identity=('fixture_digest','edited_fixture_digest','canonical_objects','canonical_bytes','object_shape_digest','schema_digest','sqlite_page_size_bytes','user_version')
  def exact(rows): return {(x['control_id'],x['seed']):tuple(x[k] for k in identity) for x in rows}
  assert exact(baseline_rows)==exact(performance) and exact(baseline_verification)==exact(verification)
  baseline_medians=grouped_medians(baseline_rows)
  comparisons=[]
  rank={'target-pass':0,'local-step-exception':0,'tolerated-pass':1,'no-go':2}
  def ratio(before,after): return after/before if before else (1.0 if after==0 else float('inf'))
  for control in medians:
   for metric in ('initialization_ns','commit_ns','reopen_ns','complete_ns'):
    before=baseline_medians[control][metric]; after=medians[control][metric]; value=ratio(before,after)
    status='local-step-exception' if metric!='complete_ns' and after<2_000_000 else 'target-pass' if value<=1.05 else 'tolerated-pass' if value<=1.10 else 'no-go'
    comparisons.append({'kind':'performance','control_id':control,'metric':metric,'baseline_median':before,'candidate_median':after,'ratio':value,'percent_change':(value-1)*100,'status':status})
   for metric in ('process_user_cpu_ns','process_system_cpu_ns','process_disk_read_bytes','process_disk_write_bytes','process_peak_rss_bytes','process_physical_footprint_bytes','container_memory_peak_bytes','temporary_write_bytes','temporary_read_bytes','temporary_peak_upper_bound_bytes','peak_disk_upper_bound_bytes'):
    before=baseline_medians[control][metric]; after=medians[control][metric]; value=ratio(before,after)
    status='target-pass' if value<=1.05 else 'tolerated-pass' if value<=1.10 else 'no-go'
    comparisons.append({'kind':'resource','control_id':control,'metric':metric,'baseline_median':before,'candidate_median':after,'ratio':value,'percent_change':(value-1)*100,'status':status})
  verifier_comparisons=[]
  base_verify={x['control_id']:x for x in baseline_verification}; candidate_verify={x['control_id']:x for x in verification}
  for control in sorted(candidate_verify):
   for metric in ('process_user_cpu_ns','process_system_cpu_ns','process_disk_read_bytes','process_disk_write_bytes','process_peak_rss_bytes','process_physical_footprint_bytes','container_memory_peak_bytes','temporary_write_bytes','temporary_read_bytes','temporary_peak_upper_bound_bytes','peak_disk_upper_bound_bytes'):
    before=base_verify[control][metric];after=candidate_verify[control][metric];value=ratio(before,after)
    status='target-pass' if value<=1.05 else 'tolerated-pass' if value<=1.10 else 'no-go'
    verifier_comparisons.append({'kind':'verification-resource','control_id':control,'metric':metric,'baseline':before,'candidate':after,'ratio':value,'percent_change':(value-1)*100,'status':status})
  performance_status=max((x['status'] for x in comparisons),key=rank.get)
  verifier_status=max((x['status'] for x in verifier_comparisons),key=rank.get)
  sibling_storage={control:{'baseline':baseline_medians[control]['total_durable_store_bytes'],'candidate':medians[control]['total_durable_store_bytes'],'ratio':ratio(baseline_medians[control]['total_durable_store_bytes'],medians[control]['total_durable_store_bytes']),'status':'target-pass' if medians[control]['total_durable_store_bytes']<=baseline_medians[control]['total_durable_store_bytes'] else 'no-go'} for control in medians if control!=primary}
  sibling_status='target-pass' if all(x['status']=='target-pass' for x in sibling_storage.values()) else 'no-go'
  summary.update({'baseline_run':str(baseline),'baseline_medians':baseline_medians,'performance_resource_comparisons':comparisons,'performance_resource_status':performance_status,'verification_resource_comparisons':verifier_comparisons,'verification_resource_status':verifier_status,'explanatory_control_storage_comparisons':sibling_storage,'explanatory_control_storage_status':sibling_status})
  eligible=primary_bytes<=600_000_000 and performance_status!='no-go' and verifier_status!='no-go' and sibling_status!='no-go' and verification_summary['status'] in ('target-pass','tolerated-pass')
  accepted_status=max((performance_status,verifier_status,verification_summary['status']),key=rank.get)
  summary.update({'status':('target-pass' if accepted_status in ('target-pass','local-step-exception') else 'tolerated-pass') if eligible else 'no-go','admission_eligible':eligible})
else:
 summary['status']='performance-complete-verification-not-run' if mode in ('performance','collect') else verification_summary['status']
 if mode=='collect':
  assert len(performance)==9 and not verification
  assert {(x['control_id'],x['seed']) for x in performance}=={(c,s) for c in ('store-footprint-unique-100000','store-footprint-metadata-cardinality-100000','store-footprint-large-object-500m') for s in (1,2,3)}
 if mode=='verify-all': assert len(verification)==3 and not performance
 summary['admission_eligible']=False
(root/'performance/summary.json').write_text(json.dumps(performance_summary,sort_keys=True,separators=(',',':'))+'\n')
(root/'verification/summary.json').write_text(json.dumps(verification_summary,sort_keys=True,separators=(',',':'))+'\n')
(root/'summary.json').write_text(json.dumps(summary,sort_keys=True,separators=(',',':'))+'\n')
(root/'run-status.json').write_text(json.dumps({'schema':f'fs-bench-pro-store-footprint-status-{version}','mode':mode,'source_arm':source,'status':summary['status'],'admission_eligible':summary['admission_eligible']},sort_keys=True,separators=(',',':'))+'\n')
lines=['# Store-footprint benchmark report','',f'- Mode: `{mode}`',f'- Source arm: `{source}`',f'- Status: `{summary["status"]}`',f'- Performance samples: {len(performance)}',f'- Verification samples: {len(verification)}','', '## Results','']
if performance:
 lines += ['| Control | Durable bytes | Canonical bytes | Ratio | Init ms | Commit ms | Reopen ms |','|---|---:|---:|---:|---:|---:|---:|']
 for control,row in grouped_medians(performance).items(): lines.append(f'| `{control}` | {row["total_durable_store_bytes"]:,.0f} | {row["canonical_bytes"]:,.0f} | {row["total_durable_store_bytes"]/row["canonical_bytes"]:.4f} | {row["initialization_ns"]/1e6:.3f} | {row["commit_ns"]/1e6:.3f} | {row["reopen_ns"]/1e6:.3f} |')
if mode=='admission':
 lines += ['',f'- Primary `600,000,000`-byte gate: `{primary}` = {summary["primary_total_durable_store_bytes"]:,.0f} bytes ({summary["primary_storage_classification"]}).','- Metadata-cardinality and large-object rows are explanatory controls with full accounting and baseline-relative non-regression gates; they do not inherit the primary control’s 600 MB threshold.']
if source=='candidate' and mode=='admission':
 lines += ['','## Baseline comparison','', '| Kind | Control | Metric | Baseline | Candidate | Ratio | Disposition |','|---|---|---|---:|---:|---:|---|']
 for row in summary['performance_resource_comparisons']: lines.append(f'| {row["kind"]} | `{row["control_id"]}` | {row["metric"]} | {row["baseline_median"]:.0f} | {row["candidate_median"]:.0f} | {row["ratio"]:.4f} | {row["status"]} |')
 for row in summary['verification_resource_comparisons']: lines.append(f'| {row["kind"]} | `{row["control_id"]}` | {row["metric"]} | {row["baseline"]:.0f} | {row["candidate"]:.0f} | {row["ratio"]:.4f} | {row["status"]} |')
lines += ['','## Accounting','', '- Every Store-owned file is listed with size and SHA-256 in each sample’s `durable-census.json`.','- SQLite table/index/payload/slack are retained in `sqlite.json` and `dbstat.txt`; pre/post census hashes prove read-only inspection.','- Each fresh Store authenticates its own canonical root/object-set digest; cross-arm equality uses semantic fixture/tree identity plus the canonical tag/encoded-length/count shape digest because public initialization intentionally allocates a fresh LayerStackId.','- Schema digest, page size, CDC identity, cache, Store creation, host, Docker, container limits, and fixture digests are exact custody gates.','- Initialization segment reads/writes and Workspace spool bytes are counted; peak disk is conservatively bounded as durable bytes plus simultaneous temporary upper bounds.','- Full tree digest and metadata verification runs only in separate verification samples and streams bounded directory state.','']
(root/'report.md').write_text('\n'.join(lines))
PY
docker inspect "$container" >"$run_dir/environment/container-after.json"
(cd "$run_dir" && find . -type f ! -name evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
trap - EXIT
status=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["status"])' "$run_dir/run-status.json")
[[ $status != no-go ]] || die "Store-footprint admission no-go"
printf 'PASS %s\n' "$run_dir"
