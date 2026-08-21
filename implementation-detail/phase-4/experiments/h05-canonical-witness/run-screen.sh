#!/bin/zsh
set -euo pipefail

repo=${0:A:h}/../../../..
runner_path=${0:A}
artifact=${repo}/target/phase4-h05-canonical-witness-screen-20260821-v1
out=${artifact}/screen-results-v7
control=${artifact}/control/phase4_create_edit_benchmark-cp0009
candidate=${artifact}/candidate/phase4_create_edit_benchmark-h05
control_source=${artifact}/control/phase4_create_edit_benchmark-cp0009.rs
candidate_source=${artifact}/candidate/phase4_create_edit_benchmark-h05.rs
fixture=${repo}/target/wp4m-f2-construction-proof-k64-20260819-v3/S1-100.source
manifest=${artifact}/PRE-MEASUREMENT-MANIFEST.tsv
lock=${artifact}/H05_SCREEN.lock
size=104857600
fixture_sha=63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4
control_sha=9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7
candidate_sha=15a668739e96de064a5a7dff1c0b1278406fa077f089687da210e83451e257dd
control_source_sha=3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a
candidate_source_sha=e675d2fc7646745eaf709f61703ff84098949ce4319cb4e6882b96698d95d031
expected_plan=$'pair 0  warmup   AB\npair 1  measured AB\npair 2  measured BA\npair 3  measured AB'
started=0
lock_held=false
timed_out=false

finish() {
    local code=$?
    setopt LOCAL_OPTIONS APPEND_CREATE CLOBBER
    if ${lock_held}; then
        /usr/bin/find ${lock} -depth -delete
        lock_held=false
        print -- "lock_released_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)" >>${lock_record}
    fi
    if [[ ! -e ${run_status} ]]; then
        local timeout=false
        ${timed_out} && timeout=true
        print -- "status=FAIL setup_or_screen_failure=true timeout=${timeout} exit=${code} wall_seconds=$(( $(/bin/date +%s) - started ))" >${run_status}
    fi
}

supervise() {
    local ceiling=$1
    shift
    exec /usr/bin/perl -e '
        $ceiling = shift;
        $pid = fork();
        die "fork failed: $!" unless defined $pid;
        if ($pid == 0) {
            setpgrp(0, 0);
            exec @ARGV or die "exec failed: $!";
        }
        sub stop_group {
            my ($signal, $exit_code) = @_;
            kill $signal, -$pid;
            for (1 .. 20) {
                my $done = waitpid($pid, 1);
                exit $exit_code if $done == $pid;
                select undef, undef, undef, 0.025;
            }
            kill "KILL", -$pid;
            waitpid($pid, 0);
            exit $exit_code;
        }
        $SIG{ALRM} = sub { stop_group("USR1", 124) };
        $SIG{INT} = sub { stop_group("INT", 130) };
        $SIG{TERM} = sub { stop_group("TERM", 143) };
        $SIG{HUP} = sub { stop_group("HUP", 129) };
        alarm($ceiling > 1 ? $ceiling - 1 : $ceiling);
        waitpid($pid, 0);
        alarm 0;
        exit(($? & 127) ? 128 + ($? & 127) : $? >> 8);
    ' ${ceiling} "$@"
}

sha() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
construct_plan() {
    print -- 'pair 0  warmup   AB'
    print -- 'pair 1  measured AB'
    print -- 'pair 2  measured BA'
    print -- 'pair 3  measured AB'
}
assert_plan() {
    local constructed=$(construct_plan)
    print -- 'constructed plan:'
    print -- ${constructed}
    print -- 'expected plan:'
    print -- ${expected_plan}
    [[ ${constructed} == ${expected_plan} ]] || return 1
    print -- 'schedule assertion: PASS'
    print -- 'row sequence: A B | A B | B A | A B'
}
verify_file() {
    local path=$1 expected=$2 label=$3 actual
    [[ -f ${path} ]] || { print -u2 -- "missing ${label}: ${path}"; return 1; }
    actual=$(sha ${path})
    [[ ${actual} == ${expected} ]] || {
        print -u2 -- "${label} SHA-256 mismatch: expected=${expected} actual=${actual}"
        return 1
    }
    print -- "${label}\t${actual}\t${path}"
}
verify_manifest() {
    /usr/bin/python3 - ${manifest} <<'PY'
import hashlib, pathlib, sys
manifest = pathlib.Path(sys.argv[1])
for line in manifest.read_text().splitlines()[1:]:
    kind, name, expected = line.split("\t")
    path = pathlib.Path(name)
    actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else "MISSING"
    if actual != expected:
        raise SystemExit(f"manifest mismatch: {kind} {name} expected={expected} actual={actual}")
    print(f"manifest {kind}\tPASS\t{actual}\t{name}")
PY
}
control_diff_from_head() {
    /usr/bin/python3 - ${repo:A} ${artifact:A} ${control_source:A} <<'PY'
import hashlib, pathlib, subprocess, sys, tempfile
repo, artifact, control = map(pathlib.Path, sys.argv[1:])
name = "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs"
with tempfile.TemporaryDirectory(dir=artifact, prefix=".control-diff-") as directory:
    head = pathlib.Path(directory) / name
    head.parent.mkdir(parents=True)
    head.write_bytes(subprocess.check_output(["git", "-C", repo, "show", f"HEAD:{name}"]))
    result = subprocess.run(["git", "-C", repo, "diff", "--no-index", "--no-ext-diff", head, control],
                            stdout=subprocess.PIPE, check=False)
    if result.returncode != 1:
        raise SystemExit("unexpected control diff status")
    lines = result.stdout.splitlines(keepends=True)
    lines[0] = f"diff --git a/{name} b/{name}\n".encode()
    lines[2] = f"--- a/{name}\n".encode()
    lines[3] = f"+++ b/{name}\n".encode()
    print(hashlib.sha256(b"".join(lines)).hexdigest())
PY
}
custody() {
    [[ ${repo:A} == /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty ]]
    [[ $(git -C ${repo} branch --show-current) == codex/empty-worktree ]]
    [[ $(git -C ${repo} rev-parse HEAD) == febc20f046bba84ccdce1256363d77799eabf2db ]]
    verify_file ${control} ${control_sha} control-executable
    verify_file ${candidate} ${candidate_sha} candidate-executable
    verify_file ${control_source} ${control_source_sha} control-source
    verify_file ${candidate_source} ${candidate_source_sha} candidate-source
    verify_file ${fixture} ${fixture_sha} retained-fixture
    [[ $(/usr/bin/stat -f %z ${fixture}) == ${size} ]]
    local control_diff=$(control_diff_from_head)
    local candidate_diff=$(git -C ${repo} diff --no-ext-diff HEAD -- crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')
    local h05_delta=$(git -C ${repo} diff --no-index --no-ext-diff target/phase4-h05-canonical-witness-screen-20260821-v1/control/phase4_create_edit_benchmark-cp0009.rs target/phase4-h05-canonical-witness-screen-20260821-v1/candidate/phase4_create_edit_benchmark-h05.rs | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}')
    [[ ${control_diff} == b073a7e04c7a7a2b17671f80c42aee598cc5d8039e4ba83d63b7cac89d150f84 ]]
    [[ ${candidate_diff} == c365686b476a8072e919f6ef0328e1b0534b43f28fbea2bceb254477451fd052 ]]
    [[ ${h05_delta} == e68864f2f1e3bac7bd3fb5158c0bba11f224e20aaee7ce894ecb42569cb98070 ]]
    print -- "control-diff-from-HEAD\t${control_diff}"
    print -- "candidate-diff-from-HEAD\t${candidate_diff}"
    print -- "H05-delta-from-control\t${h05_delta}"
    verify_manifest
}
dry_run() {
    print -- 'mode=dry-run; no fixture preparation or executable invocation is permitted'
    assert_plan
    custody
    print -- "required fixture size=${size} sha256=${fixture_sha}"
    print -- "required control executable=${control_sha} source=${control_source_sha} expectations=LFS-WP4M-EXPECTATIONS-3"
    print -- "required candidate executable=${candidate_sha} source=${candidate_source_sha} expectations=LFS-H05-EXPECTATIONS-1"
    print -- 'dry-run status=PASS execution_started=false rows=0'
}

record_environment() {
    {
        print -- "pwd=${repo:A}"
        print -- "branch=$(git branch --show-current)"
        print -- "head=$(git rev-parse HEAD)"
        print -- "started_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)"
        /usr/bin/uname -a
        /usr/bin/sw_vers
        "$(command -v rustc)" --version
        /usr/bin/python3 --version
        /usr/bin/sqlite3 --version
        /usr/sbin/sysctl -n hw.model
        /usr/bin/pmset -g batt
        print -- 'cache_scope=OS/filesystem cache warm-or-unknown'
        print -- 'physical_io=Unavailable(reason=no supported VFS/syscall/filesystem/media observer in frozen row)'
        print -- 'phase_local_cpu=Unavailable(reason=/usr/bin/time observes whole child only)'
        git status --short
    } >${out}/ENVIRONMENT-v1.txt 2>&1
}

check_quiescence() {
    /bin/ps -axo pid=,ppid=,command= >${out}/QUIESCENCE-PROCESSES-v1.txt
    /usr/bin/python3 - ${out}/QUIESCENCE-PROCESSES-v1.txt ${out}/QUIESCENCE-CONFLICTS-v1.txt $$ <<'PY'
import pathlib, re, sys
source, destination, own_pid = sys.argv[1], sys.argv[2], int(sys.argv[3])
pattern = re.compile(r"(?:^|[ /])(?:cargo|rustc|sqlite3|zstd|gzip|bzip2|xz|pigz|lz4|perf|dtrace|fs_usage|iostat|fio|dd|rsync|tar|cp|ditto|git)(?: |$)|phase4_create_edit_benchmark")
conflicts = []
for line in pathlib.Path(source).read_text().splitlines():
    fields = line.strip().split(None, 2)
    if len(fields) != 3:
        continue
    pid, ppid, command = int(fields[0]), int(fields[1]), fields[2]
    if pid != own_pid and ppid != own_pid and pattern.search(command):
        conflicts.append(line)
pathlib.Path(destination).write_text("".join(line + "\n" for line in conflicts))
PY
    [[ ! -s ${out}/QUIESCENCE-CONFLICTS-v1.txt ]] || {
        print -u2 -- 'host is not quiescent'
        return 1
    }
    print -- 'quiescence=PASS no prohibited named build/benchmark/SQLite/compression/profiler/copy/filesystem command matched; complete process snapshot retained' >${out}/QUIESCENCE-v1.txt
}

run_execution_prefix() {
    assert_plan >${out}/SCHEDULE-ASSERTION-EXECUTION-v1.txt
    custody >${out}/EXECUTION-CUSTODY-RECHECK-v1.txt
    record_environment
    print -- "$0 --execute" >${out}/COMMAND-v1.txt
    check_quiescence
}

if [[ ${1:-} == --dry-run && $# == 1 ]]; then
    cd ${repo}
    dry_run
    exit 0
fi
if [[ ${1:-} == --check-prefix && $# == 2 ]]; then
    out=$2
    [[ -d ${out} && ! -L ${out} && ! -e ${out}/SCREEN-ATTEMPT-v1.txt &&
       ! -e ${out}/SCREEN-RAW-v1.jsonl && ! -e ${out}/PROTECTED-SMOKE-v1.jsonl ]] || exit 2
    [[ -z $(/usr/bin/find ${out} -mindepth 1 -maxdepth 1 -print -quit) ]] || exit 2
    setopt NO_CLOBBER
    cd ${repo}
    run_execution_prefix
    print -- 'execution-prefix=PASS lock_acquired=false rows=0'
    exit 0
fi
if [[ ${1:-} == --timeout-self-test && $# == 2 ]]; then
    [[ -d $2 && ! -L $2 ]] || exit 2
    [[ -z $(/usr/bin/find $2 -mindepth 1 -maxdepth 1 -print -quit) ]] || exit 2
    supervise 1 ${0:A} --timeout-self-test-worker $2
    exit $?
fi
if [[ ${1:-} == --zerr-self-test && $# == 2 ]]; then
    [[ -d $2 && ! -L $2 ]] || exit 2
    [[ -z $(/usr/bin/find $2 -mindepth 1 -maxdepth 1 -print -quit) ]] || exit 2
    supervise 5 ${0:A} --timeout-self-test-worker $2 zerr
    exit $?
fi
if [[ ${1:-} == --timeout-self-test-worker && ( $# == 2 || $# == 3 ) ]]; then
    out=$2
    mode=${3:-timeout}
    lock=${out}/H05_SCREEN.timeout-test.lock
    run_status=${out}/TIMEOUT-SELF-TEST-STATUS-v1.txt
    lock_record=${out}/TIMEOUT-SELF-TEST-LOCK-v1.txt
    started=$(/bin/date +%s)
    trap finish EXIT
    trap finish ZERR
    trap 'timed_out=true; exit 124' USR1
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
    /bin/mkdir ${lock}
    lock_held=true
    print -- "BENCHMARK_LOCK=H05_SCREEN_TEST\nlock_path=${lock}\nlock_acquired_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)" >${lock_record}
    if [[ ${mode} == zerr ]]; then
        fail_inside_function() { false; }
        fail_inside_function
    fi
    /usr/bin/perl -e 'sleep 5'
    exit 99
fi
if [[ ${1:-} == --execute && $# == 1 ]]; then
    supervise 120 ${0:A} --execute-worker
    exit $?
fi
[[ ${1:-} == --execute-worker && $# == 1 ]] || {
    print -u2 -- "usage: $0 --dry-run | --check-prefix DIR | --timeout-self-test DIR | --zerr-self-test DIR | --execute"
    exit 2
}

[[ -d ${out} && ! -L ${out} ]] || { print -u2 -- "missing validation-created output directory: ${out}"; exit 2; }
attempt=${out}/SCREEN-ATTEMPT-v1.txt
raw=${out}/SCREEN-RAW-v1.jsonl
smoke_raw=${out}/PROTECTED-SMOKE-v1.jsonl
run_status=${out}/RUN-STATUS-v1.txt
work=${out}/work-v1
rows_dir=${out}/rows-v1
custody_tsv=${out}/SCREEN-INPUT-CUSTODY-v1.tsv
lock_record=${out}/LOCK-TIMEOUT-v1.txt
[[ ! -e ${attempt} && ! -e ${raw} && ! -e ${smoke_raw} && ! -e ${work} ]] || {
    print -u2 -- 'screen attempt or row evidence already exists; refusing a rerun'
    exit 2
}
setopt LOCAL_OPTIONS NO_CLOBBER APPEND_CREATE
print -- "attempt=1 started_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ) command=$0 --execute" >${attempt}
/bin/mkdir ${work} ${rows_dir}
print -- $'scope\tpair\tarm\toperation\tsource_sha256\tbase_database_sha256\tbase_authority_sha256\tbase_expectations_sha256\texpectations_version\tcanonical_commitment' >${custody_tsv}
started=$(/bin/date +%s)

trap finish EXIT
trap finish ZERR
trap 'timed_out=true; exit 124' USR1
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

remaining() { print -- $((120 - ($(/bin/date +%s) - started))); }
run_capped() {
    local seconds=$(remaining)
    if (( seconds <= 0 )); then
        timed_out=true
        return 124
    fi
    (( seconds > 60 )) && seconds=60
    local code=0
    /usr/bin/perl -e '$seconds=shift; alarm $seconds; exec @ARGV or die $!' ${seconds} "$@" || code=$?
    if (( code == 142 || $(remaining) <= 0 )); then
        timed_out=true
    fi
    return ${code}
}
run_row_capped() {
    local seconds=$(remaining)
    if (( seconds <= 0 )); then
        timed_out=true
        return 124
    fi
    (( seconds > 60 )) && seconds=60
    local code=0
    /usr/bin/time -l /usr/bin/perl -e '$seconds=shift; alarm $seconds; exec @ARGV or die $!' \
        ${seconds} "$@" || code=$?
    if (( code == 142 || $(remaining) <= 0 )); then
        timed_out=true
    fi
    return ${code}
}

parse_time() {
    local stderr=$1 field=$2
    case ${field} in
        user) /usr/bin/awk '/ real .* user .* sys$/ {print $3; exit}' ${stderr} ;;
        system) /usr/bin/awk '/ real .* user .* sys$/ {print $5; exit}' ${stderr} ;;
        rss) /usr/bin/awk '/maximum resident set size/ {print $1; exit}' ${stderr} ;;
        peak) /usr/bin/awk '/peak memory footprint/ {print $1; exit}' ${stderr} ;;
    esac
}

expectation_value() {
    local path=$1 key=$2
    /usr/bin/awk -F= -v key=${key} '$1 == key {print $2; exit}' ${path}
}
lower_hex_64() {
    /usr/bin/python3 -c 'import re,sys; raise SystemExit(re.fullmatch(r"[0-9a-f]{64}", sys.argv[1]) is None)' "$1"
}

enrich() {
    local stdout=$1 output=$2 arm=$3 pair=$4 kind=$5 order=$6 operation=$7
    local executable_sha=$8 source_sha=$9 source_size=${10} database_sha=${11} authority_sha=${12}
    local expectations_sha=${13} version=${14} commitment=${15} post_database_sha=${16}
    local post_authority_sha=${17} stderr=${18} database=${19}
    local screen_runner_sha=${20}
    local user=$(parse_time ${stderr} user) system=$(parse_time ${stderr} system)
    local rss=$(parse_time ${stderr} rss) peak=$(parse_time ${stderr} peak)
    [[ -n ${user} && -n ${system} && -n ${rss} && -n ${peak} ]]
    /usr/bin/python3 - ${stdout} ${output} ${arm} ${pair} ${kind} ${order} ${operation} \
        ${executable_sha} ${source_sha} ${source_size} ${database_sha} ${authority_sha} \
        ${expectations_sha} ${version} ${commitment} ${post_database_sha} ${post_authority_sha} \
        ${user} ${system} ${rss} ${peak} ${database} ${control_source_sha} ${candidate_source_sha} \
        ${screen_runner_sha} <<'PY'
import json, pathlib, sys
(source, output, arm, pair, kind, order, operation, executable_sha, source_sha,
 source_size, database_sha, authority_sha, expectations_sha, version, commitment,
 post_database_sha, post_authority_sha, user, system, rss, peak, database,
 control_source_sha, candidate_source_sha, screen_runner_sha) = sys.argv[1:]
lines = [line for line in pathlib.Path(source).read_text().splitlines() if line.strip()]
if len(lines) != 1:
    raise SystemExit(f"expected one JSON row, got {len(lines)}")
row = json.loads(lines[0])
residue = [suffix for suffix in ("-journal", "-wal", "-shm") if pathlib.Path(database + suffix).exists()]
boundaries = {
    "-": "durable-submit", "same-middle": "same-open-durable-edit",
    "plus1-early": "same-open-durable-edit", "plus1-middle": "same-open-durable-edit",
    "materialize-warm": "logical-materialization-warm",
    "materialize-fresh": "fresh-process-logical-materialization",
    "read-range-1m": "authenticated-sequential-1m-range",
    "reopen": "fresh-process-head-ready",
}
row.update({
    "schema": "phase4-current-baseline-v1", "acceptance_scope": "baseline",
    "candidate_comparison": False, "measurement_boundary": boundaries[operation],
    "runner_sha256": screen_runner_sha, "runner_wall_ceiling_seconds": 120,
    "runner_command_ceiling_seconds": 60,
    "cpu_scope": "whole-child-process; phase-local CPU unavailable",
    "cache_scope": "fresh LayerFS process/connection where declared; OS/filesystem cache warm-or-unknown",
    "screen_schema": "h05-private-screen-row-v1", "screen_arm": arm,
    "screen_pair": int(pair), "screen_sample_kind": kind, "screen_order": order,
    "screen_smoke_operation": None if operation == "-" else operation,
    "screen_executable_sha256": executable_sha,
    "screen_executable_source_sha256": control_source_sha if arm == "A" else candidate_source_sha,
    "screen_source_sha256": source_sha, "screen_source_size": int(source_size),
    "screen_base_database_sha256": database_sha,
    "screen_base_authority_sha256": authority_sha,
    "screen_base_expectations_sha256": expectations_sha,
    "screen_expectations_version": version,
    "screen_canonical_commitment": None if commitment == "-" else commitment,
    "screen_post_database_sha256": post_database_sha,
    "screen_post_authority_sha256": post_authority_sha,
    "screen_residue": residue,
    "external_time": {"user_seconds": float(user), "system_seconds": float(system),
        "maximum_resident_set_bytes": int(rss), "peak_memory_footprint_bytes": int(peak)},
})
pathlib.Path(output).write_text(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

run_row() {
    local scope=$1 arm=$2 pair=$3 kind=$4 order=$5 operation=$6 public_operation=$7
    local root=$8 iteration=$9 executable=${10} cli=${11}
    local database=${root}/db-K64-F64-${size}-${operation}-${iteration}.sqlite
    local expectations=${database}.expectations authority=${database}.authority
    local database_sha=$(sha ${database}) authority_sha=$(sha ${authority}) expectations_sha=$(sha ${expectations})
    local version=$(/usr/bin/head -1 ${expectations}) commitment=$(expectation_value ${expectations} canonical_commitment)
    [[ ${arm} == A && ${version} == LFS-WP4M-EXPECTATIONS-3 ]] && commitment=-
    [[ ${arm} == B && ${version} == LFS-H05-EXPECTATIONS-1 ]] && lower_hex_64 ${commitment} || [[ ${arm} == A ]]
    local label=${scope}-p${pair}-${arm}-${operation}
    local stdout=${rows_dir}/${label}.stdout.json
    local stderr=${rows_dir}/${label}.stderr.txt
    print -- "row_started_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ) scope=${scope} pair=${pair} arm=${arm} operation=${operation}" >>${out}/ROW-STARTS-v1.txt
    run_row_capped /usr/bin/env \
        LAYERFS_FAST_LANE=1 WP4M_EXECUTABLE_SHA256=$(sha ${executable}) \
        WP4M_BASE_COPY_METHOD=physical-byte-copy-identical-database-authority-expectations \
        WP4M_BASE_DATABASE_SHA256=${database_sha} WP4M_BASE_AUTHORITY_SHA256=${authority_sha} \
        WP4M_BASE_EXPECTATIONS_SHA256=${expectations_sha} \
        ${executable} ${cli} ${root} ${size} ${public_operation} ${iteration} \
        $([[ ${kind} == warmup ]] && print true || print false) \
        $([[ ${scope} == smoke ]] && print complete-roundtrip || print capture-only) \
        >${stdout} 2>${stderr}
    local post_database_sha=$(sha ${database}) post_authority_sha=$(sha ${authority})
    local enriched=${rows_dir}/${label}.enriched.json
    enrich ${stdout} ${enriched} ${arm} ${pair} ${kind} ${order} \
        $([[ ${scope} == smoke ]] && print -- ${operation} || print -- '-') \
        $(sha ${executable}) ${fixture_sha} ${size} ${database_sha} ${authority_sha} \
        ${expectations_sha} ${version} ${commitment} ${post_database_sha} ${post_authority_sha} \
        ${stderr} ${database} $(sha ${runner_path})
    if [[ ${scope} == smoke ]]; then
        /bin/cat ${enriched} >>${smoke_raw}
        /usr/bin/python3 - ${enriched} ${operation} <<'PY'
import json, sys
r=json.load(open(sys.argv[1])); op=sys.argv[2]
mutation=op in ("same-middle","plus1-early","plus1-middle")
ok=r["status"]=="PASS" and r["error"] is None and r["q_current"]==0 and r["screen_residue"]==[]
ok=ok and ((r["transactions"],r["commits"]) == ((1,1) if mutation else (0,0)))
raise SystemExit(not ok)
PY
    else
        /bin/cat ${enriched} >>${raw}
    fi
    print -- "row_completed_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ) scope=${scope} pair=${pair} arm=${arm} operation=${operation}" >>${out}/ROW-STARTS-v1.txt
}

cd ${repo}
run_execution_prefix

export BENCHMARK_LOCK=H05_SCREEN
/bin/mkdir ${lock} 2>/dev/null || { print -u2 -- "benchmark lock already held: ${lock}"; exit 1; }
lock_held=true
{
    print -- "BENCHMARK_LOCK=${BENCHMARK_LOCK}"
    print -- "lock_path=${lock}"
    print -- "lock_acquired_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)"
    print -- 'complete_screen_wall_ceiling_seconds=120'
} >${lock_record}

# One protected smoke gate composed only of the seven required frozen-candidate operations.
typeset -a smoke_public=(edit-same edit-plus1-early edit-plus1-middle materialize-warm materialize-fresh read-range-1m reopen)
typeset -a smoke_internal=(same-middle plus1-early plus1-middle materialize-warm materialize-fresh read-range-1m reopen)
for index in {1..7}; do
    public=${smoke_public[${index}]} internal=${smoke_internal[${index}]}
    root=${work}/smoke-${index}-${internal}
    /bin/mkdir ${root}
    /bin/cp ${fixture} ${root}/S1-100.source
    [[ $(sha ${root}/S1-100.source) == ${fixture_sha} ]]
    iteration=$((910000 + index))
    if [[ ${internal} == plus1-* ]]; then
        run_capped ${candidate} --count-change-scale-prepare ${root} ${size} ${public} ${iteration}
        cli=--count-change-scale-row
    else
        run_capped ${candidate} --fast-prepare ${root} ${size} ${public} ${iteration}
        cli=--fast-row
    fi
    database=${root}/db-K64-F64-${size}-${internal}-${iteration}.sqlite
    print -- "smoke\t-1\tB\t${internal}\t${fixture_sha}\t$(sha ${database})\t$(sha ${database}.authority)\t$(sha ${database}.expectations)\t$(/usr/bin/head -1 ${database}.expectations)\t$(expectation_value ${database}.expectations canonical_commitment)" >>${custody_tsv}
    run_row smoke B -1 smoke NA ${internal} ${public} ${root} ${iteration} ${candidate} ${cli}
done
[[ $(/usr/bin/wc -l <${smoke_raw} | /usr/bin/tr -d ' ') == 7 ]]
print -- 'protected_smoke=PASS operations=7 gate=correctness/resource/non-controlling' >${out}/PROTECTED-SMOKE-RESULT-v1.txt

prepare_pair() {
    local pair=$1
    local iteration=$((920000 + pair))
    local base=${work}/pair-${pair}
    local prep_a=${base}/prep-A prep_b=${base}/prep-B row_a=${base}/row-A row_b=${base}/row-B
    /bin/mkdir -p ${prep_a} ${prep_b} ${row_a} ${row_b}
    for root in ${prep_a} ${prep_b} ${row_a} ${row_b}; do
        /bin/cp ${fixture} ${root}/S1-100.source
        [[ $(sha ${root}/S1-100.source) == ${fixture_sha} ]]
    done
    run_capped ${control} --fast-prepare ${prep_a} ${size} write ${iteration}
    run_capped ${candidate} --fast-prepare ${prep_b} ${size} write ${iteration}
    local master_a=${prep_a}/db-K64-F64-${size}-full-${iteration}.sqlite
    local master_b=${prep_b}/db-K64-F64-${size}-full-${iteration}.sqlite
    [[ $(/usr/bin/head -1 ${master_a}.expectations) == LFS-WP4M-EXPECTATIONS-3 ]]
    [[ $(/usr/bin/head -1 ${master_b}.expectations) == LFS-H05-EXPECTATIONS-1 ]]
    /usr/bin/python3 - ${master_a}.expectations ${master_b}.expectations <<'PY'
import pathlib, re, sys
a = pathlib.Path(sys.argv[1]).read_text().splitlines()
b = pathlib.Path(sys.argv[2]).read_text().splitlines()
hex64 = re.compile(r"[0-9a-f]{64}").fullmatch
assert a[0] == "LFS-WP4M-EXPECTATIONS-3" and b[0] == "LFS-H05-EXPECTATIONS-1"
assert not any(line.startswith("canonical_commitment=") for line in a)
commitments = [line.removeprefix("canonical_commitment=") for line in b
               if line.startswith("canonical_commitment=")]
assert len(commitments) == 1 and hex64(commitments[0])
assert a[-1].startswith("manifest_blake3=") and hex64(a[-1].removeprefix("manifest_blake3="))
assert b[-1].startswith("manifest_blake3=") and hex64(b[-1].removeprefix("manifest_blake3="))
normalize = lambda lines: [line for line in lines[1:-1]
                           if not line.startswith("canonical_commitment=")]
assert normalize(a) == normalize(b)
PY
    local db_a=${row_a}/db-K64-F64-${size}-full-${iteration}.sqlite
    local db_b=${row_b}/db-K64-F64-${size}-full-${iteration}.sqlite
    /bin/cp ${master_a} ${db_a}; /bin/cp ${master_a}.authority ${db_a}.authority; /bin/cp ${master_a}.expectations ${db_a}.expectations
    /bin/cp ${master_a} ${db_b}; /bin/cp ${master_a}.authority ${db_b}.authority; /bin/cp ${master_b}.expectations ${db_b}.expectations
    [[ $(sha ${db_a}) == $(sha ${db_b}) && $(sha ${db_a}.authority) == $(sha ${db_b}.authority) ]]
    print -- "screen\t${pair}\tA\tfull\t${fixture_sha}\t$(sha ${db_a})\t$(sha ${db_a}.authority)\t$(sha ${db_a}.expectations)\tLFS-WP4M-EXPECTATIONS-3\t-" >>${custody_tsv}
    print -- "screen\t${pair}\tB\tfull\t${fixture_sha}\t$(sha ${db_b})\t$(sha ${db_b}.authority)\t$(sha ${db_b}.expectations)\tLFS-H05-EXPECTATIONS-1\t$(expectation_value ${db_b}.expectations canonical_commitment)" >>${custody_tsv}
}

for pair in 0 1 2 3; do
    kind=measured; order=AB
    (( pair == 0 )) && kind=warmup
    (( pair == 2 )) && order=BA
    prepare_pair ${pair}
    typeset -a arms
    [[ ${order} == AB ]] && arms=(A B) || arms=(B A)
    for arm in ${arms}; do
        root=${work}/pair-${pair}/row-${arm}
        executable=${control}; [[ ${arm} == B ]] && executable=${candidate}
        run_row screen ${arm} ${pair} ${kind} ${order} full write ${root} $((920000 + pair)) ${executable} --fast-row
    done
done
[[ $(/usr/bin/wc -l <${raw} | /usr/bin/tr -d ' ') == 8 ]]
/usr/bin/find ${work} -depth -delete
(( $(/bin/date +%s) - started <= 120 )) || { timed_out=true; exit 1; }
/usr/bin/find ${lock} -depth -delete
lock_held=false
print -- "lock_released_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)" >>${lock_record}
print -- "screen runner PASS rows=8 smoke=7 wall_seconds=$(( $(/bin/date +%s) - started ))"
/bin/chmod 0444 ${attempt} ${raw} ${smoke_raw} ${custody_tsv} ${out}/*.txt ${rows_dir}/*
status_tmp=${run_status}.tmp
print -- "status=PASS timeout=false screen_executed_exactly_once=true warmup_rows=2 measured_rows=6 total_rows=8 protected_smoke_rows=7 wall_seconds=$(( $(/bin/date +%s) - started ))" >${status_tmp}
/bin/chmod 0444 ${status_tmp}
/bin/mv ${status_tmp} ${run_status}
