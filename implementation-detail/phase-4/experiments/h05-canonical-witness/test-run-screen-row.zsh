#!/bin/zsh
set -euo pipefail

runner=${0:A:h}/run-screen.sh
runner_path=${runner}
work=$(/usr/bin/mktemp -d /tmp/h05-run-row-self-test.XXXXXX)
[[ ${work} == /tmp/h05-run-row-self-test.* && -d ${work} && ! -L ${work} ]]
cleanup() {
    [[ ${work} == /tmp/h05-run-row-self-test.* && -d ${work} && ! -L ${work} ]] &&
        /usr/bin/find ${work} -depth -delete
}
trap cleanup EXIT INT TERM HUP

# Fail if a local declaration refers to a variable created earlier on that same
# line: zsh expands the whole declaration before making those names available.
/usr/bin/python3 - ${runner} <<'PY'
from pathlib import Path
import re, sys
for number, line in enumerate(Path(sys.argv[1]).read_text().splitlines(), 1):
    if not line.lstrip().startswith("local "):
        continue
    for match in re.finditer(r"(?:^|\s)([A-Za-z_][A-Za-z0-9_]*)=", line):
        name = match.group(1)
        if re.search(r"\$\{?" + re.escape(name) + r"(?:\}|\b)", line[match.end():]):
            raise SystemExit(f"same-line local dependency at {number}: {name}")
PY

# Extract and execute the actual enrich and run_row functions. Everything below is a
# harmless dependency stub; neither frozen operand is read or invoked.
eval "$(/usr/bin/awk '
    /^run_row_capped\(\) \{/ { capture=1 }
    capture && /^cd \$\{repo\}$/ { exit }
    capture { print }
' ${runner})"

remaining() { print 1; }
timed_out=false
cap_marker=${work}/cap.pid
set +e
run_row_capped /usr/bin/perl -e 'open my $f, ">", $ARGV[0] or die $!; print $f "$$\n"; close $f; select undef, undef, undef, 5' \
    ${cap_marker} >${work}/cap.stdout 2>${work}/cap.stderr
cap_code=$?
set -e
[[ ${cap_code} == 142 && ${timed_out} == true && -s ${cap_marker} ]]
cap_pid=$(/usr/bin/tr -d '\n' <${cap_marker})
for _ in {1..20}; do
    /bin/kill -0 ${cap_pid} 2>/dev/null || break
    /bin/sleep 0.01
done
! /bin/kill -0 ${cap_pid} 2>/dev/null

out=${work}/out
rows_dir=${out}/rows-v1
root=${work}/row
raw=${out}/SCREEN-RAW-v1.jsonl
smoke_raw=${out}/PROTECTED-SMOKE-v1.jsonl
size=104857600
fixture_sha=63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4
control_source_sha=3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a
candidate_source_sha=e675d2fc7646745eaf709f61703ff84098949ce4319cb4e6882b96698d95d031
/bin/mkdir -p ${rows_dir} ${root}

database=${root}/db-K64-F64-${size}-full-920000.sqlite
print -n -- database >${database}
print -n -- authority >${database}.authority
print -- $'LFS-H05-EXPECTATIONS-1\ncanonical_commitment=abababababababababababababababababababababababababababababababab' >${database}.expectations

stub=${work}/harmless-row-stub.zsh
{
    print -- '#!/bin/zsh'
    print -- 'set -euo pipefail'
    print -- '[[ $# == 7 && $1 == --fast-row && $3 == 104857600 && $4 == write && $5 == 920000 && $6 == true && $7 == capture-only ]]'
    print -- '[[ $2 == /tmp/h05-run-row-self-test.*/row ]]'
    print -- '[[ ${LAYERFS_FAST_LANE:-} == 1 ]]'
    print -- '[[ ${WP4M_BASE_COPY_METHOD:-} == physical-byte-copy-identical-database-authority-expectations ]]'
    print -- '[[ ${WP4M_EXECUTABLE_SHA256:-} == $(/usr/bin/shasum -a 256 $0 | /usr/bin/awk '\''{print $1}'\'') ]]'
    print -- 'for value in ${WP4M_BASE_DATABASE_SHA256:-} ${WP4M_BASE_AUTHORITY_SHA256:-} ${WP4M_BASE_EXPECTATIONS_SHA256:-}; do [[ ${#value} == 64 && $value != *[^0-9a-f]* ]]; done'
    print -- 'print -- '\''{"status":"PASS","error":null,"q_current":0,"transactions":0,"commits":0}'\'''
} >${stub}
/bin/chmod 0755 ${stub}

sha() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
expectation_value() {
    /usr/bin/awk -F= -v key=$2 '$1 == key {print $2; exit}' $1
}
lower_hex_64() {
    /usr/bin/python3 -c 'import re,sys; raise SystemExit(re.fullmatch(r"[0-9a-f]{64}", sys.argv[1]) is None)' "$1"
}
run_row_capped() { /usr/bin/time -l "$@"; }
parse_time() {
    case $2 in
        user) print 0.1 ;;
        system) print 0.2 ;;
        rss) print 1000 ;;
        peak) print 900 ;;
    esac
}

setopt NO_CLOBBER APPEND_CREATE
run_row screen B 0 warmup AB full write ${root} 920000 ${stub} --fast-row

[[ -s ${rows_dir}/screen-p0-B-full.stdout.json ]]
[[ -s ${rows_dir}/screen-p0-B-full.stderr.txt ]]
[[ -s ${rows_dir}/screen-p0-B-full.enriched.json ]]
[[ $(/usr/bin/wc -l <${raw} | /usr/bin/tr -d ' ') == 1 ]]
[[ ! -e ${smoke_raw} ]]
[[ $(/usr/bin/wc -l <${out}/ROW-STARTS-v1.txt | /usr/bin/tr -d ' ') == 2 ]]
for suffix in -journal -wal -shm; do [[ ! -e ${database}${suffix} ]]; done
/usr/bin/python3 - ${raw} ${runner} <<'PY'
import hashlib, json, pathlib, sys
row=json.loads(open(sys.argv[1]).read())
assert row["status"] == "PASS" and row["error"] is None and row["q_current"] == 0
assert row["screen_arm"] == "B" and row["screen_pair"] == 0
assert row["screen_sample_kind"] == "warmup" and row["screen_order"] == "AB"
assert row["screen_smoke_operation"] is None
assert row["schema"] == "phase4-current-baseline-v1"
assert row["acceptance_scope"] == "baseline" and row["candidate_comparison"] is False
assert row["measurement_boundary"] == "durable-submit"
assert row["runner_wall_ceiling_seconds"] == 120 and row["runner_command_ceiling_seconds"] == 60
assert row["runner_sha256"] == hashlib.sha256(pathlib.Path(sys.argv[2]).read_bytes()).hexdigest()
assert row["external_time"] == {"user_seconds":0.1,"system_seconds":0.2,
                                "maximum_resident_set_bytes":1000,
                                "peak_memory_footprint_bytes":900}
PY

print -- 'run_row self-test PASS filenames=screen-p0-B-full status=PASS cap_exit=142 orphan=absent cleanup=PASS frozen_invocations=0'
