#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd "$here/../.." && pwd -P)
results_root=${LAYERFS_STORE_RESULTS_ROOT:-$repo/benchmark-results/fs-bench-pro/store-footprint}
prepared_root=${LAYERFS_STORE_PREPARED_ROOT:-${TMPDIR:-/tmp}/layerfs-fs-bench-pro-store-footprint}

die() { printf 'fs-bench-pro Store footprint: %s\n' "$*" >&2; exit 2; }

self_check() {
  local scratch started elapsed
  started=$(python3 -c 'import time; print(time.monotonic_ns())')
  bash -n "$0"
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-store-self-check.XXXXXX")
  trap 'rm -rf -- "$scratch"' EXIT
  printf 'mod family { include!(r#"%s"#); } fn main() { family::self_check().unwrap(); assert_eq!(family::CONTROLS.len(), 3); }\n' \
    "$here/families/store_footprint.rs" >"$scratch/check.rs"
  rustc --edition=2021 -Awarnings "$scratch/check.rs" -o "$scratch/check"
  "$scratch/check"
  elapsed=$(( $(python3 -c 'import time; print(time.monotonic_ns())') - started ))
  (( elapsed < 2000000000 )) || die "self-check exceeded two seconds"
  rm -rf -- "$scratch"
  trap - EXIT
  printf '{"schema":"fs-bench-pro-store-footprint-self-check-v1","elapsed_ns":%s,"container_started":false,"status":"pass"}\n' "$elapsed"
}

if [[ ${1:-} == --self-check ]]; then [[ $# == 1 ]] || die "--self-check takes no arguments"; self_check; exit 0; fi

[[ $# -ge 2 ]] || die "usage: $0 RUN_ID CONTAINER --case CONTROL --seed 1 --source baseline|candidate [--mode performance|verify] [--tier 100|1000|10000] | RUN_ID CONTAINER --all --source baseline|candidate --mode admission"
run_id=$1
container=$2
shift 2
selection= seed= source_arm= mode=performance tier=100 all=0 mode_set=0 tier_set=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --case) [[ $# -ge 2 && -z $selection ]] || die "duplicate/missing --case"; selection=$2; shift 2 ;;
    --seed) [[ $# -ge 2 && -z $seed ]] || die "duplicate/missing --seed"; seed=$2; shift 2 ;;
    --source) [[ $# -ge 2 && -z $source_arm ]] || die "duplicate/missing --source"; source_arm=$2; shift 2 ;;
    --mode) [[ $# -ge 2 && $mode_set == 0 ]] || die "duplicate/missing --mode"; mode=$2; mode_set=1; shift 2 ;;
    --tier) [[ $# -ge 2 && $tier_set == 0 ]] || die "duplicate/missing --tier"; tier=$2; tier_set=1; shift 2 ;;
    --all) [[ $all == 0 ]] || die "duplicate --all"; all=1; shift ;;
    *) die "unknown argument: $1" ;;
  esac
done
[[ $run_id =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] || die "unsafe RUN_ID"
[[ $source_arm == baseline || $source_arm == candidate ]] || die "explicit source arm is required"
case "$mode" in
  performance) [[ $all == 0 && -n $selection && $seed =~ ^[123]$ && $tier =~ ^(100|1000|10000)$ ]] || die "selected performance arguments" ;;
  verify) [[ $all == 0 && -n $selection && ${seed:-1} =~ ^[123]$ && $tier =~ ^(100|1000|10000|100000)$ ]] || die "selected verify arguments"; seed=${seed:-1} ;;
  admission) [[ $all == 1 && -z $selection && -z $seed && $tier_set == 0 ]] || die "admission requires --all and no case/seed/tier"; tier=100000 ;;
  *) die "unknown mode: $mode" ;;
esac

for command in cargo docker nc python3 rustc sqlite3; do command -v "$command" >/dev/null || die "$command is required"; done
source_seal=$("$here/run-namespace.sh" --source-seal)
[[ $(docker inspect -f '{{index .Config.Labels "dev.layerfs.source-seal"}}' "$container") == "$source_seal" ]] || die "container/source seal mismatch"
docker inspect "$container" | python3 -c 'import json,sys; assert not any(x.get("Type") == "bind" for x in json.load(sys.stdin)[0].get("Mounts", []))' || die "container has a bind mount"
container_id=$(docker inspect -f '{{.Id}}' "$container")
prepared="$prepared_root/$source_seal"
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
  fixture_dir="$prepared/fixtures/$tier/$control"
  if [[ ! -f $fixture_dir/manifest.json || ! -d $fixture_dir/fixture ]]; then
    mkdir -p "$prepared/fixtures/$tier"
    fixture_stage=$(mktemp -d "$prepared/fixtures/$tier/.fixture.XXXXXX")
    "$oracle_workload" store-footprint-fixture "$fixture_stage/fixture" "$control" "$tier" >"$fixture_stage/manifest.json"
    mv "$fixture_stage" "$fixture_dir"
  fi
  printf '%s\n' "$fixture_dir"
}

run_dir="$results_root/$run_id"
mkdir -p "$results_root"
mkdir "$run_dir" || die "refusing to overwrite $run_dir"
mkdir "$run_dir/environment" "$run_dir/performance" "$run_dir/verification" "$run_dir/scenarios"
printf '%q ' "$0" "$run_id" "$container" >"$run_dir/environment/command.txt"
printf '\n' >>"$run_dir/environment/command.txt"
printf '%s\n' "$source_seal" >"$run_dir/environment/source-seal.txt"
git -C "$repo" rev-parse HEAD >"$run_dir/environment/source-commit.txt"
docker inspect "$container" >"$run_dir/environment/container.json"
docker image inspect "$(docker inspect -f '{{.Image}}' "$container")" >"$run_dir/environment/image.json"
{ uname -a; sw_vers 2>/dev/null || true; } >"$run_dir/environment/host.txt"

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

run_sample() {
  local control=$1 sample_seed=$2 sample_mode=$3 fixture_dir manifest sample_dir status raw_target before_sha after_sha nonce
  fixture_dir=$(fixture_for "$control")
  manifest="$fixture_dir/manifest.json"
  sample_dir="$run_dir/scenarios/$control/$source_arm/seed-$sample_seed-$sample_mode"
  mkdir -p "$sample_dir"
  read -r expected_files expected_logical edit_path edit_size fixture_digest edited_digest < <(python3 - "$manifest" <<'PY'
import json,sys
r=json.load(open(sys.argv[1]));print(r['regular_files'],r['logical_bytes'],r['edit_path'],r['edit_size'],r['fixture_digest'],r['edited_fixture_digest'])
PY
  )
  ensure_daemon
  nonce=$(od -An -tx1 -N16 /dev/urandom | tr -d ' \n')
  set +e
  /usr/bin/time -l -p env LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE="$nonce" \
    LAYERFS_BENCH_WORKLOAD=/usr/local/bin/fs-benchmark-workload \
    LAYERFS_EXEC_TRANSPORT=daemon LAYERFS_FUSE_TRANSPORT=daemon \
    LAYERFS_DAEMON_TCP_ENDPOINT="$daemon_endpoint" LAYERFS_DAEMON_CAPABILITY="$daemon_capability" \
    LAYERFS_DAEMON_CONTAINER_ID="$container_id" LAYERFS_FUSE_HOST=host.docker.internal \
    "$binary" "store-footprint-$sample_mode" "$sample_dir/work" "$fixture_dir/fixture" "$container_id" \
    "$control" "$sample_seed" "$source_arm" "$expected_files" "$expected_logical" "$edit_path" "$edit_size" \
    "$fixture_digest" "$edited_digest" >"$sample_dir/raw.jsonl" 2>"$sample_dir/supervisor.txt"
  status=$?
  set -e
  printf '%s\n' "$status" >"$sample_dir/exit-status.txt"
  [[ $status == 0 ]] || die "$sample_mode failed: $control seed $sample_seed"
  await_daemon
  before_sha=$(shasum -a 256 "$sample_dir/work/store.sqlite" | awk '{print $1}')
  sqlite3 -readonly -json "$sample_dir/work/store.sqlite" "SELECT page_size AS sqlite_page_size_bytes,page_count AS sqlite_page_count,freelist_count AS sqlite_freelist_pages,(SELECT count(*) FROM objects) AS sqlite_object_rows,(SELECT coalesce(sum(length(bytes)),0) FROM objects) AS sqlite_canonical_object_bytes,(SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name='objects') AS sqlite_objects_table_bytes,(SELECT coalesce(sum(payload),0) FROM dbstat WHERE name='objects') AS sqlite_objects_table_payload_bytes,(SELECT coalesce(sum(unused),0) FROM dbstat WHERE name='objects') AS sqlite_objects_table_unused_bytes,(SELECT coalesce(sum(pgsize),0) FROM dbstat WHERE name='sqlite_autoindex_objects_1') AS sqlite_objects_index_bytes;" >"$sample_dir/sqlite.json"
  sqlite3 -readonly -header -column "$sample_dir/work/store.sqlite" 'SELECT name,count(*) AS pages,sum(pgsize) AS allocated_bytes,sum(payload) AS payload_bytes,sum(unused) AS unused_bytes FROM dbstat GROUP BY name ORDER BY allocated_bytes DESC;' >"$sample_dir/dbstat.txt"
  after_sha=$(shasum -a 256 "$sample_dir/work/store.sqlite" | awk '{print $1}')
  [[ $before_sha == "$after_sha" ]] || die "dbstat mutated Store"
  python3 - "$sample_dir/raw.jsonl" "$sample_dir/sqlite.json" "$sample_dir/supervisor.txt" "$sample_dir/result.json" "$tier" <<'PY'
import json,re,sys
raw,sqlite,supervisor,out,tier=sys.argv[1:]
rows=[json.loads(x) for x in open(raw) if x.startswith('{')]; assert len(rows)==1
r=rows[0]; q=json.load(open(sqlite)); assert len(q)==1; q=q[0]
text=open(supervisor).read(); lines=[x for x in text.splitlines() if x.startswith('layerfs-initialization-diagnostic-v3 ')]
assert len(lines)==1
metrics=dict(re.findall(r'([a-z0-9_]+)=([^ ]+)',lines[0]))
temporary_write=sum(int(metrics[x]) for x in ('object_segment_write_bytes','pair_segment_write_bytes'))
temporary_read=sum(int(metrics[x]) for x in ('object_segment_raw_read_bytes','pair_segment_raw_read_bytes'))
assert q['sqlite_page_size_bytes']*q['sqlite_page_count']==r['sqlite_database_bytes']
assert q['sqlite_object_rows']==r['canonical_objects'] and q['sqlite_canonical_object_bytes']==r['canonical_bytes']
r.update(q)
r.update({'tier':int(tier),'reportable':int(tier)==100000,'temporary_write_bytes':temporary_write,'temporary_read_bytes':temporary_read,'temporary_peak_upper_bound_bytes':temporary_write,'peak_disk_upper_bound_bytes':r['total_durable_store_bytes']+temporary_write,'dbstat_store_sha256_unchanged':True})
json.dump(r,open(out,'w'),sort_keys=True,separators=(',',':'));open(out,'a').write('\n')
PY
  if [[ $sample_mode == performance ]]; then raw_target="$run_dir/performance/raw.jsonl"; else raw_target="$run_dir/verification/raw.jsonl"; fi
  cat "$sample_dir/result.json" >>"$raw_target"
}

if [[ $mode == performance || $mode == verify ]]; then
  run_sample "$selection" "$seed" "$mode"
else
  while IFS=$'\t' read -r control _; do for sample_seed in 1 2 3; do run_sample "$control" "$sample_seed" performance; done; done <"$mapfile"
  while IFS=$'\t' read -r control _; do run_sample "$control" 1 verify; done <"$mapfile"
fi

python3 - "$run_dir" "$mode" "$source_arm" <<'PY'
import json,statistics,sys
from pathlib import Path
root,mode,source=Path(sys.argv[1]),sys.argv[2],sys.argv[3]
performance=[json.loads(x) for x in (root/'performance/raw.jsonl').read_text().splitlines()] if (root/'performance/raw.jsonl').exists() else []
verification=[json.loads(x) for x in (root/'verification/raw.jsonl').read_text().splitlines()] if (root/'verification/raw.jsonl').exists() else []
summary={'schema':'fs-bench-pro-store-footprint-summary-v1','mode':mode,'source_arm':source,'samples':len(performance),'verification_samples':len(verification)}
if mode=='admission':
 assert len(performance)==9 and len(verification)==3 and all(x['status']=='pass' for x in verification)
 medians={}
 for control in sorted({x['control_id'] for x in performance}):
  rows=[x for x in performance if x['control_id']==control]
  medians[control]={k:statistics.median(x[k] for x in rows) for k in ('total_durable_store_bytes','initialization_ns','commit_ns','reopen_ns','complete_ns','canonical_objects','canonical_bytes','process_peak_rss_bytes','temporary_peak_upper_bound_bytes','peak_disk_upper_bound_bytes')}
 summary['medians']=medians
 maximum=max(x['total_durable_store_bytes'] for x in medians.values())
 summary['maximum_total_durable_store_bytes']=maximum
 summary['status']='baseline-complete' if source=='baseline' else 'target-pass' if maximum<=600_000_000 else 'no-go'
 summary['admission_eligible']=source=='candidate' and maximum<=600_000_000
else:
 summary['status']='performance-complete-verification-not-run' if mode=='performance' else 'target-pass'
 summary['admission_eligible']=mode=='verify'
(root/'summary.json').write_text(json.dumps(summary,sort_keys=True,separators=(',',':'))+'\n')
(root/'run-status.json').write_text(json.dumps({'schema':'fs-bench-pro-store-footprint-status-v1','status':summary['status'],'admission_eligible':summary['admission_eligible']},sort_keys=True,separators=(',',':'))+'\n')
PY
(cd "$run_dir" && find . -type f ! -name evidence.sha256 -print0 | sort -z | xargs -0 shasum -a 256 >evidence.sha256)
status=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["status"])' "$run_dir/run-status.json")
[[ $status != no-go ]] || die "Store-footprint admission no-go"
printf 'PASS %s\n' "$run_dir"
