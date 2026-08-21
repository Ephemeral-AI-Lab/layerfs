#!/bin/zsh
set -euo pipefail

runner_path=${0:A}
repo=${0:A:h}/../../../..
repo=${repo:A}
artifact=${repo}/target/phase4-h05c-aa100-attribution-20260821-v1
out=${artifact}/aa-results-v2
control=${repo}/target/phase4-h05-canonical-witness-screen-20260821-v1/control/phase4_create_edit_benchmark-cp0009
control_source=${repo}/target/phase4-h05-canonical-witness-screen-20260821-v1/control/phase4_create_edit_benchmark-cp0009.rs
fixture=${repo}/target/wp4m-f2-construction-proof-k64-20260819-v3/S1-100.source
premeasurement_manifest=${repo}/target/phase4-h05-canonical-witness-screen-20260821-v1/PRE-MEASUREMENT-MANIFEST.tsv
original_preregistration=${artifact}/PROSPECTIVE-AA100-PREREGISTRATION-v1.md
preregistration=${artifact}/PROSPECTIVE-AA100-REPAIR-v2.md
methodology_custody=${artifact}/PROSPECTIVE-METHODOLOGY-CUSTODY-v2.tsv
phase1_v1_manifest=${artifact}/AA100-PHASE1-v1-MANIFEST.tsv
historical_manifest=${artifact}/HISTORICAL-H05-H05B-MANIFEST-v1.tsv
historical_pre=${artifact}/HISTORICAL-H05-H05B-VERIFICATION-PRE-v1.txt
candidate_pre=${artifact}/CANDIDATE-NONINVOCATION-CUSTODY-PRE-v1.txt
lock=${artifact}/H05C_AA100.lock
size=104857600
fixture_sha=63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4
control_sha=9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7
control_source_sha=3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a
historical_manifest_sha=90595c15c0fb3992ef19110f197555d978f323f06bab0b1469b7517973a528ba
expected_plan=$'pair 0  AB\npair 1  BA\npair 2  AB\npair 3  BA\npair 4  AB\npair 5  BA'
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
    if [[ -n ${run_status:-} && ! -e ${run_status} ]]; then
        local timeout=false
        ${timed_out} && timeout=true
        print -- "status=FAIL setup_or_study_failure=true timeout=${timeout} exit=${code} wall_seconds=$(( $(/bin/date +%s) - started ))" >${run_status}
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
    print -- 'pair 0  AB'
    print -- 'pair 1  BA'
    print -- 'pair 2  AB'
    print -- 'pair 3  BA'
    print -- 'pair 4  AB'
    print -- 'pair 5  BA'
}
assert_plan() {
    local constructed=$(construct_plan)
    print -- 'constructed plan:'
    print -- ${constructed}
    print -- 'expected plan:'
    print -- ${expected_plan}
    [[ ${constructed} == ${expected_plan} ]]
    print -- 'schedule assertion: PASS'
    print -- 'row sequence: A B | B A | A B | B A | A B | B A'
}
verify_file() {
    local path=$1 expected=$2 label=$3 actual
    [[ -f ${path} ]]
    actual=$(sha ${path})
    [[ ${actual} == ${expected} ]]
    print -- "${label}\t${actual}\t${path}"
}
verify_manifest() {
    /usr/bin/python3 - ${premeasurement_manifest} <<'PY'
import hashlib, pathlib, sys
manifest = pathlib.Path(sys.argv[1])
for line in manifest.read_text().splitlines()[1:]:
    kind, name, expected = line.split("\t")
    path = pathlib.Path(name)
    actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else "MISSING"
    if actual != expected:
        raise SystemExit(f"manifest mismatch: {kind} {name} expected={expected} actual={actual}")
print("premeasurement_manifest=PASS")
PY
}
custody() {
    [[ ${repo:A} == /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty ]]
    [[ $(git -C ${repo} branch --show-current) == codex/empty-worktree ]]
    [[ $(git -C ${repo} rev-parse HEAD) == febc20f046bba84ccdce1256363d77799eabf2db ]]
    verify_file ${control} ${control_sha} control-executable
    verify_file ${control_source} ${control_source_sha} control-source
    verify_file ${fixture} ${fixture_sha} retained-fixture
    [[ $(/usr/bin/stat -f %z ${fixture}) == ${size} ]]
    [[ -f ${preregistration} ]]
    [[ -n ${H05C_METHOD_CUSTODY_SHA256:-} && $(sha ${methodology_custody}) == ${H05C_METHOD_CUSTODY_SHA256} ]]
    /usr/bin/python3 - ${methodology_custody} <<'PY'
import csv, hashlib, pathlib, sys
rows = list(csv.DictReader(pathlib.Path(sys.argv[1]).open(), delimiter="\t"))
if [row["label"] for row in rows] != ["runner", "runner-test", "analyzer", "analyzer-helper", "original-preregistration", "repair-preregistration", "historical-manifest", "historical-verification-pre", "candidate-custody-pre", "phase1-v1-manifest"]:
    raise SystemExit("methodology custody labels/order mismatch")
for row in rows:
    path = pathlib.Path(row["path"])
    actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else "MISSING"
    if actual != row["sha256"]:
        raise SystemExit(f"methodology custody mismatch: {row['label']} expected={row['sha256']} actual={actual}")
print("methodology_custody=PASS")
PY
    [[ $(sha ${historical_manifest}) == ${historical_manifest_sha} ]]
    [[ -f ${original_preregistration} && -f ${historical_pre} && -f ${candidate_pre} ]]
    /usr/bin/python3 - ${phase1_v1_manifest} ${repo} <<'PY'
import csv, hashlib, pathlib, sys
manifest, repo = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
for row in csv.DictReader(manifest.open(), delimiter="\t"):
    path = repo / row["path"]
    actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else "MISSING"
    if actual != row["sha256"] or path.stat().st_size != int(row["size_bytes"]):
        raise SystemExit(f"phase1 v1 custody mismatch: {path}")
print("phase1_v1_custody=PASS")
PY
    verify_manifest
}
record_post_custody() {
    /usr/bin/python3 - ${candidate_pre} ${out}/CANDIDATE-NONINVOCATION-CUSTODY-POST-v1.txt <<'PY'
import hashlib, pathlib, sys
source, destination = map(pathlib.Path, sys.argv[1:])
pre = dict(line.split("=", 1) for line in source.read_text().splitlines() if "=" in line)
observed = {"classification": "READ_ONLY_NONINVOCATION_CUSTODY_POST"}
for prefix in ("candidate_executable", "candidate_source"):
    path = pathlib.Path(pre[f"{prefix}_path"])
    stat = path.stat()
    observed.update({f"{prefix}_path": str(path),
                     f"{prefix}_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                     f"{prefix}_size": str(stat.st_size), f"{prefix}_mtime": str(int(stat.st_mtime))})
    for field in ("path", "sha256", "size", "mtime"):
        if observed[f"{prefix}_{field}"] != pre[f"{prefix}_{field}"]:
            raise SystemExit(f"candidate non-modification custody mismatch: {prefix}_{field}")
order = ["classification", "candidate_executable_path", "candidate_executable_sha256",
         "candidate_executable_size", "candidate_executable_mtime", "candidate_source_path",
         "candidate_source_sha256", "candidate_source_size", "candidate_source_mtime"]
destination.write_text("".join(f"{key}={observed[key]}\n" for key in order))
PY
    /usr/bin/python3 - ${historical_manifest} ${out}/HISTORICAL-H05-H05B-VERIFICATION-POST-v1.txt ${repo} <<'PY'
import csv, hashlib, pathlib, sys
manifest, destination, repo = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
mismatches = []
rows = list(csv.DictReader(manifest.open(), delimiter="\t"))
for row in rows:
    path = pathlib.Path(row["path"])
    if not path.is_absolute():
        path = repo / path
    actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else "MISSING"
    size = path.stat().st_size if path.is_file() else -1
    if actual != row["sha256"] or size != int(row["size_bytes"]):
        mismatches.append(str(path))
if mismatches:
    raise SystemExit("historical custody mismatch: " + ",".join(mismatches))
destination.write_text(
    f"status=PASS\nentries={len(rows)}\nmismatches=0\n"
    f"manifest_sha256={hashlib.sha256(manifest.read_bytes()).hexdigest()}\n"
    "H05_v7=H05 MEASURED NO-GO / REVERT\nH05b=H05B_NOT_JUSTIFIED / STOP\nreopened=false\n")
PY
}
dry_run() {
    print -- 'mode=dry-run; no fixture preparation or executable invocation is permitted'
    assert_plan
    custody
    print -- "operand_count=1 control=${control_sha}"
    print -- 'candidate_invocations=0'
    print -- 'rows=12 snapshots=36 optimization_claim=false'
    print -- 'dry-run status=PASS execution_started=false'
}
record_environment() {
    {
        print -- "pwd=${repo:A}"
        print -- "branch=$(git branch --show-current)"
        print -- "head=$(git rev-parse HEAD)"
        print -- "started_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)"
        print -- "methodology_custody_sha256=${H05C_METHOD_CUSTODY_SHA256}"
        /usr/bin/uname -a
        /usr/bin/sw_vers
        /usr/bin/sqlite3 --version
        /bin/df ${artifact}
        /sbin/mount | /usr/bin/grep ' /System/Volumes/Data '
        print -- 'cache_scope=OS/filesystem cache warm-or-unknown'
        print -- 'physical_io=Unavailable(reason=no supported privileged/VFS observer)'
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
    [[ ! -s ${out}/QUIESCENCE-CONFLICTS-v1.txt ]]
    print -- 'quiescence=PASS no prohibited build/benchmark/SQLite/compression/profiler/filesystem task matched' >${out}/QUIESCENCE-v1.txt
}
run_execution_prefix() {
    assert_plan >${out}/SCHEDULE-ASSERTION-v1.txt
    custody >${out}/EXECUTION-CUSTODY-v1.txt
    record_environment
    print -- "/usr/bin/env H05C_METHOD_CUSTODY_SHA256=${H05C_METHOD_CUSTODY_SHA256} ${runner_path} --execute" >${out}/COMMAND-v1.txt
    check_quiescence
}

if [[ ${1:-} == --dry-run && $# == 1 ]]; then
    cd ${repo}
    dry_run
    exit 0
fi
if [[ ${1:-} == --check-prefix && $# == 2 ]]; then
    out=$2
    [[ -d ${out} && ! -L ${out} && -z $(/usr/bin/find ${out} -mindepth 1 -print -quit) ]]
    setopt NO_CLOBBER APPEND_CREATE
    cd ${repo}
    run_execution_prefix
    print -- 'execution-prefix=PASS lock_acquired=false rows=0'
    exit 0
fi
if [[ ${1:-} == --timeout-self-test && $# == 2 ]]; then
    [[ -d $2 && ! -L $2 && -z $(/usr/bin/find $2 -mindepth 1 -print -quit) ]]
    supervise 1 ${runner_path} --self-test-worker $2 timeout
    exit $?
fi
if [[ ${1:-} == --zerr-self-test && $# == 2 ]]; then
    [[ -d $2 && ! -L $2 && -z $(/usr/bin/find $2 -mindepth 1 -print -quit) ]]
    supervise 5 ${runner_path} --self-test-worker $2 zerr
    exit $?
fi
if [[ ${1:-} == --self-test-worker && $# == 3 ]]; then
    out=$2
    mode=$3
    lock=${out}/H05C_AA100_TEST.lock
    run_status=${out}/SELF-TEST-STATUS-v1.txt
    lock_record=${out}/SELF-TEST-LOCK-v1.txt
    started=$(/bin/date +%s)
    trap finish EXIT
    trap finish ZERR
    trap 'timed_out=true; exit 124' USR1
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
    /bin/mkdir ${lock}
    lock_held=true
    print -- "lock_path=${lock}\nlock_acquired_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)" >${lock_record}
    if [[ ${mode} == zerr ]]; then
        fail_inside_function() { false; }
        fail_inside_function
    fi
    /usr/bin/perl -e 'sleep 5'
    exit 99
fi
if [[ ${1:-} == --execute && $# == 1 ]]; then
    supervise 120 ${runner_path} --execute-worker
    exit $?
fi
[[ ${1:-} == --execute-worker && $# == 1 ]] || {
    print -u2 -- "usage: $0 --dry-run | --check-prefix DIR | --timeout-self-test DIR | --zerr-self-test DIR | --execute"
    exit 2
}

[[ -d ${out} && ! -L ${out} ]] || exit 2
attempt=${out}/AA100-ATTEMPT-v1.txt
raw=${out}/AA100-RAW-v1.jsonl
snapshots=${out}/AA100-STORAGE-SNAPSHOTS-v1.tsv
custody_tsv=${out}/AA100-INPUT-CUSTODY-v1.tsv
invocation_plan=${out}/AA100-INVOCATION-PLAN-v1.tsv
actual_invocations=${out}/AA100-ACTUAL-INVOCATIONS-v1.tsv
run_status=${out}/RUN-STATUS-v1.txt
lock_record=${out}/LOCK-TIMEOUT-v1.txt
work=${out}/work-v1
rows_dir=${out}/rows-v1
started=$(/bin/date +%s)
trap finish EXIT
trap finish ZERR
trap 'timed_out=true; exit 124' USR1
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
[[ ! -e ${attempt} && ! -e ${raw} && ! -e ${snapshots} && ! -e ${work} ]]
setopt LOCAL_OPTIONS NO_CLOBBER APPEND_CREATE
print -- "attempt=1 started_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ) command=/usr/bin/env H05C_METHOD_CUSTODY_SHA256=${H05C_METHOD_CUSTODY_SHA256:-MISSING} ${runner_path} --execute" >${attempt}
/bin/mkdir ${work} ${rows_dir}
print -- $'pair\torder\tarm\titeration\tfixture_sha256\tbase_database_sha256\tbase_authority_sha256\tbase_expectations_sha256\texpectations_version\texecutable_sha256' >${custody_tsv}
print -- $'sequence\tkind\tpair\tarm\texecutable_sha256\tcommand' >${invocation_plan}
print -- $'sequence\tevent\tutc\tkind\tpair\torder\tarm\titeration\texecutable_sha256\tcommand\texit' >${actual_invocations}
sequence=0
for pair in 0 1 2 3 4 5; do
    order=AB
    (( pair % 2 == 1 )) && order=BA
    iteration=$((930000 + pair))
    (( sequence += 1 ))
    print -- "${sequence}\tprepare\t${pair}\t-\t${control_sha}\t${control} --fast-prepare ${work}/pair-${pair}/prep ${size} write ${iteration}" >>${invocation_plan}
    typeset -a planned_arms
    [[ ${order} == AB ]] && planned_arms=(A B) || planned_arms=(B A)
    for arm in ${planned_arms}; do
        (( sequence += 1 ))
        print -- "${sequence}\trow\t${pair}\t${arm}\t${control_sha}\t${control} --fast-row ${work}/pair-${pair}/${arm} ${size} write ${iteration} false capture-only" >>${invocation_plan}
    done
done
[[ ${sequence} == 18 ]]
print -- $'pair\torder\tarm\tsnapshot\tsnapshot_utc\tmonotonic_ns\tdatabase_sha256\tauthority_sha256\texpectations_sha256\tfixture_sha256\tdatabase_logical_bytes\tdatabase_apparent_bytes\tdatabase_allocated_bytes\tauthority_apparent_bytes\tauthority_allocated_bytes\tjournal_apparent_bytes\tjournal_allocated_bytes\tstore_logical_bytes\tstore_apparent_bytes\tstore_allocated_bytes\texpectations_apparent_bytes\texpectations_allocated_bytes\tintegrity_check\tjournal_present\twal_present\tshm_present' >${snapshots}

cd ${repo}
run_execution_prefix
export BENCHMARK_LOCK=H05C_AA100
/bin/mkdir ${lock} 2>/dev/null || exit 1
lock_held=true
{
    print -- "BENCHMARK_LOCK=${BENCHMARK_LOCK}"
    print -- "lock_path=${lock}"
    print -- "lock_acquired_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)"
    print -- 'complete_study_wall_ceiling_seconds=120'
    print -- 'per_command_ceiling_seconds=60'
} >${lock_record}

remaining() { print -- $((120 - ($(/bin/date +%s) - started))); }
run_capped() {
    local seconds=$(remaining)
    (( seconds > 0 )) || { timed_out=true; return 124; }
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
    (( seconds > 0 )) || { timed_out=true; return 124; }
    (( seconds > 60 )) && seconds=60
    local code=0
    /usr/bin/time -l /usr/bin/perl -e '$seconds=shift; alarm $seconds; exec @ARGV or die $!' ${seconds} "$@" || code=$?
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
apparent() { [[ -e $1 ]] && /usr/bin/stat -f %z "$1" || print 0; }
allocated() { [[ -e $1 ]] && print -- $(( $(/usr/bin/stat -f %b "$1") * 512 )) || print 0; }
record_invocation() {
    local sequence=$1 event=$2 kind=$3 pair=$4 order=$5 arm=$6 iteration=$7 command=$8 exit_code=$9
    print -- "${sequence}\t${event}\t$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)\t${kind}\t${pair}\t${order}\t${arm}\t${iteration}\t${control_sha}\t${command}\t${exit_code}" >>${actual_invocations}
}
record_snapshot() {
    local pair=$1 order=$2 arm=$3 snapshot=$4 database=$5 authority=$6 expectations=$7 source=$8
    local snapshot_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)
    local monotonic_ns=$(/usr/bin/python3 -c 'import time; print(time.clock_gettime_ns(time.CLOCK_MONOTONIC_RAW))')
    local database_apparent=$(apparent ${database})
    local database_allocated=$(allocated ${database})
    local authority_apparent=$(apparent ${authority})
    local authority_allocated=$(allocated ${authority})
    local journal=${database}-journal
    local journal_apparent=$(apparent ${journal})
    local journal_allocated=$(allocated ${journal})
    local journal_present=0 wal_present=0 shm_present=0
    [[ -e ${journal} ]] && journal_present=1
    [[ -e ${database}-wal ]] && wal_present=1
    [[ -e ${database}-shm ]] && shm_present=1
    local database_sha=$(sha ${database})
    local authority_sha=$(sha ${authority})
    local expectations_sha=$(sha ${expectations})
    local fixture_observed_sha=$(sha ${source})
    local page_size=$(/usr/bin/sqlite3 ${database} 'PRAGMA page_size;')
    local page_count=$(/usr/bin/sqlite3 ${database} 'PRAGMA page_count;')
    local database_logical=$((page_size * page_count))
    local store_logical=$((database_logical + authority_apparent))
    local store_apparent=$((database_apparent + authority_apparent + journal_apparent))
    local store_allocated=$((database_allocated + authority_allocated + journal_allocated))
    local integrity=$(/usr/bin/sqlite3 ${database} 'PRAGMA integrity_check;')
    print -- "${pair}\t${order}\t${arm}\t${snapshot}\t${snapshot_utc}\t${monotonic_ns}\t${database_sha}\t${authority_sha}\t${expectations_sha}\t${fixture_observed_sha}\t${database_logical}\t${database_apparent}\t${database_allocated}\t${authority_apparent}\t${authority_allocated}\t${journal_apparent}\t${journal_allocated}\t${store_logical}\t${store_apparent}\t${store_allocated}\t$(apparent ${expectations})\t$(allocated ${expectations})\t${integrity}\t${journal_present}\t${wal_present}\t${shm_present}" >>${snapshots}
}
enrich() {
    local stdout=$1 output=$2 arm=$3 pair=$4 order=$5 executable_sha=$6
    local base_database_sha=$7 base_authority_sha=$8 base_expectations_sha=$9
    local post_database_sha=${10} post_authority_sha=${11} stderr=${12} database=${13}
    local user=$(parse_time ${stderr} user)
    local system=$(parse_time ${stderr} system)
    local rss=$(parse_time ${stderr} rss)
    local peak=$(parse_time ${stderr} peak)
    [[ -n ${user} && -n ${system} && -n ${rss} && -n ${peak} ]]
    /usr/bin/python3 - ${stdout} ${output} ${arm} ${pair} ${order} ${executable_sha} \
        ${base_database_sha} ${base_authority_sha} ${base_expectations_sha} \
        ${post_database_sha} ${post_authority_sha} ${user} ${system} ${rss} ${peak} \
        ${database} $(sha ${runner_path}) <<'PY'
import json, pathlib, sys
(source, output, arm, pair, order, executable_sha, base_database_sha,
 base_authority_sha, base_expectations_sha, post_database_sha,
 post_authority_sha, user, system, rss, peak, database, runner_sha) = sys.argv[1:]
lines = [line for line in pathlib.Path(source).read_text().splitlines() if line.strip()]
if len(lines) != 1:
    raise SystemExit(f"expected one native row, got {len(lines)}")
row = json.loads(lines[0])
residue = [suffix for suffix in ("-journal", "-wal", "-shm")
           if pathlib.Path(database + suffix).exists()]
row.update({
    "schema": "phase4-current-baseline-v1", "acceptance_scope": "baseline",
    "candidate_comparison": False, "measurement_boundary": "durable-submit",
    "runner_sha256": runner_sha, "runner_wall_ceiling_seconds": 120,
    "runner_command_ceiling_seconds": 60,
    "cpu_scope": "whole-child-process; phase-local CPU unavailable",
    "cache_scope": "fresh LayerFS process/connection where declared; OS/filesystem cache warm-or-unknown",
    "aa_schema": "h05c-aa100-row-v1", "aa_label": arm, "aa_pair": int(pair),
    "aa_order": order, "aa_sample_kind": "placebo", "aa_optimization_claim": False,
    "aa_validation_scope": "capture-only",
    "aa_executable_sha256": executable_sha,
    "aa_executable_source_sha256": "3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a",
    "aa_fixture_sha256": "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4",
    "aa_fixture_size": 104857600,
    "aa_base_database_sha256": base_database_sha,
    "aa_base_authority_sha256": base_authority_sha,
    "aa_base_expectations_sha256": base_expectations_sha,
    "aa_expectations_version": "LFS-WP4M-EXPECTATIONS-3",
    "aa_post_database_sha256": post_database_sha,
    "aa_post_authority_sha256": post_authority_sha,
    "screen_residue": residue,
    "external_time": {"user_seconds": float(user), "system_seconds": float(system),
        "maximum_resident_set_bytes": int(rss), "peak_memory_footprint_bytes": int(peak)},
})
pathlib.Path(output).write_text(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
PY
}
run_row() {
    local invocation_sequence=$1 pair=$2 order=$3 arm=$4 iteration=$5 root=$6 database=$7 authority=$8 expectations=$9
    local base_database_sha=$(sha ${database})
    local base_authority_sha=$(sha ${authority})
    local base_expectations_sha=$(sha ${expectations})
    local label=pair-${pair}-${arm}
    local stdout=${rows_dir}/${label}.stdout.json
    local stderr=${rows_dir}/${label}.stderr.txt
    local command="${control} --fast-row ${root} ${size} write ${iteration} false capture-only"
    record_invocation ${invocation_sequence} started row ${pair} ${order} ${arm} ${iteration} ${command} -
    print -- "row_started_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ) pair=${pair} order=${order} arm=${arm} executable_sha256=${control_sha}" >>${out}/ROW-STARTS-v1.txt
    run_row_capped /usr/bin/env \
        LAYERFS_FAST_LANE=1 WP4M_EXECUTABLE_SHA256=${control_sha} \
        WP4M_BASE_COPY_METHOD=physical-byte-copy-identical-database-authority-expectations \
        WP4M_BASE_DATABASE_SHA256=${base_database_sha} \
        WP4M_BASE_AUTHORITY_SHA256=${base_authority_sha} \
        WP4M_BASE_EXPECTATIONS_SHA256=${base_expectations_sha} \
        ${control} --fast-row ${root} ${size} write ${iteration} false capture-only \
        >${stdout} 2>${stderr}
    record_snapshot ${pair} ${order} ${arm} T0 ${database} ${authority} ${expectations} ${root}/S1-100.source
    local post_database_sha=$(sha ${database})
    local post_authority_sha=$(sha ${authority})
    local enriched=${rows_dir}/${label}.enriched.json
    enrich ${stdout} ${enriched} ${arm} ${pair} ${order} ${control_sha} \
        ${base_database_sha} ${base_authority_sha} ${base_expectations_sha} \
        ${post_database_sha} ${post_authority_sha} ${stderr} ${database}
    /bin/cat ${enriched} >>${raw}
    /usr/bin/python3 - ${enriched} <<'PY'
import json, sys
r=json.load(open(sys.argv[1]))
ok=(r["status"]=="PASS" and r["error"] is None and r["q_current"]==0
    and r["transactions"]==1 and r["commits"]==1 and r["screen_residue"]==[])
raise SystemExit(not ok)
PY
    print -- "row_completed_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ) pair=${pair} order=${order} arm=${arm} executable_sha256=${control_sha}" >>${out}/ROW-STARTS-v1.txt
    record_invocation ${invocation_sequence} completed row ${pair} ${order} ${arm} ${iteration} ${command} 0
}

execute_study() {
  actual_sequence=0
for pair in 0 1 2 3 4 5; do
    order=AB
    (( pair % 2 == 1 )) && order=BA
    iteration=$((930000 + pair))
    pair_root=${work}/pair-${pair}
    prep=${pair_root}/prep
    root_a=${pair_root}/A
    root_b=${pair_root}/B
    /bin/mkdir -p ${prep} ${root_a} ${root_b}
    for root in ${prep} ${root_a} ${root_b}; do
        /bin/cp ${fixture} ${root}/S1-100.source
        [[ $(sha ${root}/S1-100.source) == ${fixture_sha} ]]
    done
    (( actual_sequence += 1 ))
    prepare_command="${control} --fast-prepare ${prep} ${size} write ${iteration}"
    record_invocation ${actual_sequence} started prepare ${pair} ${order} - ${iteration} ${prepare_command} -
    run_capped ${control} --fast-prepare ${prep} ${size} write ${iteration}
    record_invocation ${actual_sequence} completed prepare ${pair} ${order} - ${iteration} ${prepare_command} 0
    master=${prep}/db-K64-F64-${size}-full-${iteration}.sqlite
    [[ $(/usr/bin/head -1 ${master}.expectations) == LFS-WP4M-EXPECTATIONS-3 ]]
    for root in ${root_a} ${root_b}; do
        database=${root}/db-K64-F64-${size}-full-${iteration}.sqlite
        /bin/cp ${master} ${database}
        /bin/cp ${master}.authority ${database}.authority
        /bin/cp ${master}.expectations ${database}.expectations
    done
    db_a=${root_a}/db-K64-F64-${size}-full-${iteration}.sqlite
    db_b=${root_b}/db-K64-F64-${size}-full-${iteration}.sqlite
    [[ $(sha ${db_a}) == $(sha ${db_b}) ]]
    [[ $(sha ${db_a}.authority) == $(sha ${db_b}.authority) ]]
    [[ $(sha ${db_a}.expectations) == $(sha ${db_b}.expectations) ]]
    for arm in A B; do
        root=${pair_root}/${arm}
        database=${root}/db-K64-F64-${size}-full-${iteration}.sqlite
        print -- "${pair}\t${order}\t${arm}\t${iteration}\t${fixture_sha}\t$(sha ${database})\t$(sha ${database}.authority)\t$(sha ${database}.expectations)\tLFS-WP4M-EXPECTATIONS-3\t${control_sha}" >>${custody_tsv}
        record_snapshot ${pair} ${order} ${arm} PRE ${database} ${database}.authority ${database}.expectations ${root}/S1-100.source
    done
    typeset -a arms
    [[ ${order} == AB ]] && arms=(A B) || arms=(B A)
    for arm in ${arms}; do
        (( actual_sequence += 1 ))
        root=${pair_root}/${arm}
        database=${root}/db-K64-F64-${size}-full-${iteration}.sqlite
        run_row ${actual_sequence} ${pair} ${order} ${arm} ${iteration} ${root} ${database} ${database}.authority ${database}.expectations
    done
  done

  [[ $(/usr/bin/wc -l <${raw} | /usr/bin/tr -d ' ') == 12 ]]
  [[ $(/usr/bin/wc -l <${custody_tsv} | /usr/bin/tr -d ' ') == 13 ]]
  [[ ${actual_sequence} == 18 && $(/usr/bin/wc -l <${actual_invocations} | /usr/bin/tr -d ' ') == 37 ]]
  /bin/sleep 2.1
  for pair in 0 1 2 3 4 5; do
    order=AB
    (( pair % 2 == 1 )) && order=BA
    iteration=$((930000 + pair))
    for arm in A B; do
        root=${work}/pair-${pair}/${arm}
        database=${root}/db-K64-F64-${size}-full-${iteration}.sqlite
        record_snapshot ${pair} ${order} ${arm} T1 ${database} ${database}.authority ${database}.expectations ${root}/S1-100.source
    done
  done
  [[ $(/usr/bin/wc -l <${snapshots} | /usr/bin/tr -d ' ') == 37 ]]
}

execute_study
record_post_custody
(( $(/bin/date +%s) - started <= 120 )) || { timed_out=true; exit 1; }
/usr/bin/find ${lock} -depth -delete
lock_held=false
print -- "lock_released_utc=$(/bin/date -u +%Y-%m-%dT%H:%M:%SZ)" >>${lock_record}
print -- "A/A attribution runner PASS rows=12 snapshots=36 wall_seconds=$(( $(/bin/date +%s) - started ))"
/bin/chmod 0444 ${attempt} ${raw} ${snapshots} ${custody_tsv} ${invocation_plan} ${actual_invocations} ${out}/*.txt ${rows_dir}/*
/bin/chmod -R a-w ${work}
status_tmp=${run_status}.tmp
print -- "status=PASS timeout=false study_executed_exactly_once=true placebo_rows=12 snapshots=36 wall_seconds=$(( $(/bin/date +%s) - started ))" >${status_tmp}
/bin/chmod 0444 ${status_tmp}
/bin/mv ${status_tmp} ${run_status}
