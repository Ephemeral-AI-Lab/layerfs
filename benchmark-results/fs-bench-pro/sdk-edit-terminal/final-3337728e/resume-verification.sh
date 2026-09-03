#!/usr/bin/env bash
# Bounded recovery of six missing proofs; reuse unchanged frozen runner functions.
set -euo pipefail
export LC_ALL=C
source /tmp/layerfs-i20-perf-first-env.sh
repo=$(git rev-parse --show-toplevel)
here=$repo/benchmark/fs-bench-pro
family=edit_length_changing
mode=admission
all=1
run_dir=$repo/benchmark-results/fs-bench-pro/edit-length-changing/terminal-3337728e
[[ $(git rev-parse HEAD) == "$LAYERFS_SDK_EDIT_CANDIDATE_REVISION" ]]
git diff --exit-code HEAD -- benchmark/fs-bench-pro crates >/dev/null
baseline_bin=$LAYERFS_SDK_EDIT_BASELINE_BIN
candidate_bin=$LAYERFS_SDK_EDIT_CANDIDATE_BIN
candidate_build=$LAYERFS_SDK_EDIT_CANDIDATE_BUILD
prepared_root=$LAYERFS_SDK_EDIT_PREPARED_ROOT
die() { printf '%s\n' "$*" >&2; exit 2; }
attempt=$run_dir/verification/attempts/zero-extend-10mib-baseline-attempt1
python3 - "$run_dir" "$attempt" <<'PY'
import hashlib,json,pathlib,shutil,sys
root,attempt=map(pathlib.Path,sys.argv[1:]); raw=root/'verification/subproofs.jsonl'
rows=[json.loads(x) for x in raw.read_text().splitlines()]
assert len(rows)==58 and all(r['status']=='pass' for r in rows)
remaining={(f'zero-extend-tail-4k-on-{size}mib-ops-1',arm) for size in (10,100,500) for arm in ('baseline','candidate')}
assert not remaining & {(r['scenario_id'],r['source_arm']) for r in rows}
if attempt.exists():
    assert (attempt/'subproofs.jsonl').read_bytes()==raw.read_bytes()
    assert not (attempt/'retry-container.json').exists()
    print('Resuming setup-only attempt; no verifier has run.')
    raise SystemExit(0)
attempt.mkdir(parents=True)
for path in [raw,root/'evidence.sha256',root/'run-status.json',root/'environment/failed-container.json',root/'environment/failed-container.log',*sorted((root/'scenarios/zero-extend-tail-4k-on-10mib-ops-1').glob('verify-*-baseline.txt'))]:
    shutil.copy2(path,attempt/path.name)
(attempt/'recovery.json').write_text(json.dumps({'reason':'worker InvalidRequest; container exited 0 without OOM; cause unproven; retry missing verification only','successful_prefix_sha256':hashlib.sha256(raw.read_bytes()).hexdigest(),'performance_raw_sha256':hashlib.sha256((root/'performance/raw.jsonl').read_bytes()).hexdigest(),'remaining':sorted(remaining)},sort_keys=True)+'\n')
PY
scratch=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-issue20-six-proofs.XXXXXX")
active_container=
cleanup() {
  local status=$?
  if [[ -n ${active_container:-} ]]; then
    docker inspect "$active_container" >"$attempt/retry-container.json" 2>/dev/null || true
    docker logs "$active_container" >"$attempt/retry-container.log" 2>&1 || true
    docker rm -f "$active_container" >/dev/null 2>&1 || true
  fi
  [[ -d $scratch && ! -L $scratch && $scratch == "${TMPDIR:-/tmp}/layerfs-issue20-six-proofs."* ]] || exit 2
  find "$scratch" -type d -exec chmod u+w {} +
  rm -r -- "$scratch"
  exit "$status"
}
trap cleanup EXIT
eval "$(awk '/^clone_store\(\) \{/ {p=1} p && /^if \[\[ \$stage != verification/ {exit} p {print}' "$here/lib-edit-sdk-runner.sh")"
eval "$(awk '/^container_serial=0$/ {p=1} /^finalize_selected\(\) \{/ {exit} p {print}' "$here/lib-edit-sdk-runner.sh")"
declare -F clone_store run_verifier >/dev/null
for size in 10485760 104857600 524288000; do
  prepared=$(python3 "$here/sdk-edit-custody.py" prepare "$prepared_root" "$candidate_bin" "$size" "$scratch/cache-$size.json" "$candidate_build")
  expected=$(awk -F '\t' -v size="$size" '$1==size {print $3}' "$run_dir/environment/prepared-stores.tsv")
  [[ $(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["store_sha256"])' "$scratch/cache-$size.json") == "$expected" ]] || die 'prepared input changed'
  ln -s "$prepared" "$scratch/prepared-$size"
done
timed_manifest_sha256=$(shasum -a 256 "$run_dir/environment/timed-call-graph-manifest.json" | awk '{print $1}')
route_manifest_sha256=$(shasum -a 256 "$run_dir/environment/operation-route-manifest.json" | awk '{print $1}')
qualification_sha256=$(shasum -a 256 "$run_dir/environment/qualification.tsv" | awk '{print $1}')
conformance_sha256=$(shasum -a 256 "$run_dir/environment/edit-conformance-manifest.json" | awk '{print $1}')
baseline_image=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["baseline"]["image_id"])' "$run_dir/environment/source-identity.json")
candidate_image=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["candidate"]["image_id"])' "$run_dir/environment/source-identity.json")
for tier in 10 100 500; do
  for arm in baseline candidate; do
    run_verifier "$arm" "zero-extend-tail-4k-on-${tier}mib-ops-1"
    printf 'PASS recovered %s zero-extend %s MiB\n' "$arm" "$tier"
  done
done
python3 - "$run_dir" "$attempt" <<'PY'
import hashlib,json,pathlib,sys
root,attempt=map(pathlib.Path,sys.argv[1:]);prefix=(attempt/'subproofs.jsonl').read_bytes()
raw=(root/'verification/subproofs.jsonl').read_bytes();rows=[json.loads(x) for x in raw.splitlines()]
assert raw.startswith(prefix) and len(rows)==64 and all(r['status']=='pass' for r in rows)
assert len({(r['scenario_id'],r['source_arm']) for r in rows})==64
assert hashlib.sha256((root/'performance/raw.jsonl').read_bytes()).hexdigest()==json.loads((attempt/'recovery.json').read_text())['performance_raw_sha256']
print('All 64 LC proofs pass; original 58-proof prefix and all performance bytes unchanged.')
PY
