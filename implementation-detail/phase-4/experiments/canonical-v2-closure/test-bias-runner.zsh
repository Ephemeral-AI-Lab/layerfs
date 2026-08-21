#!/bin/zsh
set -euo pipefail

runner=${0:A:h}/run-bias-confirmation.zsh
tmp=$(/usr/bin/mktemp -d /tmp/canonical-v2-bias-test.XXXXXX)
[[ ${tmp} == /tmp/canonical-v2-bias-test.* && -d ${tmp} && ! -L ${tmp} ]]
cleanup() { [[ -d ${tmp} && ! -L ${tmp} ]] && /usr/bin/find ${tmp} -depth -delete; }
trap cleanup EXIT INT TERM HUP

/usr/bin/python3 - ${runner} <<'PY'
from pathlib import Path
import re, sys
text=Path(sys.argv[1]).read_text()
for number,line in enumerate(text.splitlines(),1):
    if not line.lstrip().startswith("local "):
        continue
    for match in re.finditer(r"(?:^|\s)([A-Za-z_][A-Za-z0-9_]*)=",line):
        name=match.group(1)
        if re.search(r"\$\{?"+re.escape(name)+r"(?:\}|\b)",line[match.end():]):
            raise SystemExit(f"same-line local dependency {number}:{name}")
required=["prepare=AB execute=AB","prepare=BA execute=BA","prepare=BA execute=AB","prepare=AB execute=BA"]
assert all(value in text for value in required)
PY

eval "$(/usr/bin/awk '
  /^write_invocation_plan\(\) \{/ {capture=1}
  capture && /^write_invocation_plan$/ {exit}
  capture {print}
' ${runner})"
eval "$(/usr/bin/awk '
  /^run_capped\(\) \{/ {capture=1}
  capture && /^cd \$\{repo\}$/ {exit}
  capture {print}
' ${runner})"

out=${tmp}/out; rows_dir=${out}/rows-v1; work=${out}/work-v1
raw=${out}/SCREEN-RAW-v1.jsonl; smoke_raw=${out}/PROTECTED-SMOKE-v1.jsonl
actual_invocations=${out}/ACTUAL-INVOCATIONS-v1.tsv; invocation_plan=${out}/INVOCATION-PLAN-v1.tsv
/bin/mkdir -p ${rows_dir} ${work}
print -- $'sequence\tevent\tutc\tkind\tpair\tarm\tprepare_order\texecute_order\texecutable_sha256\tcommand\texit' >${actual_invocations}
print -- $'sequence\tkind\tpair\tarm\tprepare_order\texecute_order\texecutable_sha256\tcommand' >${invocation_plan}

stub=${tmp}/stub.zsh
{
  print -- '#!/bin/zsh'
  print -- 'set -euo pipefail'
  print -- 'print -- '\''{"status":"PASS","error":null,"q_current":0,"transactions":1,"commits":1}'\'''
} >${stub}
/bin/chmod 0755 ${stub}
control=${stub}; candidate=${stub}; control_sha=$(/usr/bin/shasum -a 256 ${stub}|/usr/bin/awk '{print $1}'); candidate_sha=${control_sha}
size=104857600; fixture_sha=$(/usr/bin/shasum -a 256 ${stub}|/usr/bin/awk '{print $1}')
runner_path=${runner}; control_source_sha=${control_sha}; candidate_source_sha=${control_sha}
sha() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
write_invocation_plan
[[ $(/usr/bin/wc -l <${invocation_plan}|/usr/bin/tr -d ' ') == 39 ]]
/usr/bin/python3 - ${invocation_plan} <<'PY'
import csv, pathlib, sys
rows=list(csv.DictReader(pathlib.Path(sys.argv[1]).open(),delimiter="\t"))
assert len(rows)==38
measured=[r for r in rows if r["kind"]=="measured"]
assert {(r["prepare_order"],r["execute_order"]) for r in measured}=={("AB","AB"),("BA","BA"),("BA","AB"),("AB","BA")}
assert len(measured)==8
PY

functions -c run_row_capped capped_under_test
remaining() { print 1; }; timed_out=false; pid_file=${tmp}/pid
set +e
capped_under_test /usr/bin/perl -e 'open my $f,">",$ARGV[0] or die $!; print $f $$; close $f; sleep 5' ${pid_file} >/dev/null 2>&1
cap_code=$?
set -e
[[ ${cap_code} == 142 && -s ${pid_file} ]]; ! /bin/kill -0 $(<${pid_file}) 2>/dev/null

root=${work}/row; /bin/mkdir -p ${root}; print -n source >${root}/S1-100.source
database=${root}/db-K64-F64-${size}-full-920004.sqlite
/usr/bin/sqlite3 ${database} 'CREATE TABLE t(x);'
/usr/bin/python3 - ${database}.authority <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_bytes(b"a"*32)
PY
print -- $'LFS-CANONICAL-V2-EXPECTATIONS-1\ncanonical_commitment=5bf63ef8adad7bb373be4e968759997b385590d076d75a1488e1958b64a3f8e2' >${database}.expectations
control=${stub}; candidate=${stub}; fixture_sha=$(sha ${root}/S1-100.source)
remaining() { print 60; }; run_row_capped() { /usr/bin/time -l "$@"; }; invocation_sequence=0
run_row screen B 4 measured BA AB full write ${root} 920004 ${stub} --fast-row
[[ $(/usr/bin/wc -l <${raw}|/usr/bin/tr -d ' ') == 1 && $(/usr/bin/wc -l <${actual_invocations}|/usr/bin/tr -d ' ') == 3 ]]
/usr/bin/python3 - ${raw} ${runner} <<'PY'
import hashlib,json,pathlib,sys
r=json.loads(pathlib.Path(sys.argv[1]).read_text())
assert (r["screen_prepare_order"],r["screen_order"],r["screen_preparation_position"],r["screen_execution_position"])==("BA","AB",1,2)
assert r["runner_sha256"]==hashlib.sha256(pathlib.Path(sys.argv[2]).read_bytes()).hexdigest()
assert "Unavailable" in r["external_time"]["instructions"] and "Unavailable" in r["external_time"]["cycles"]
PY

timeout_dir=${tmp}/timeout; zerr_dir=${tmp}/zerr; /bin/mkdir ${timeout_dir} ${zerr_dir}
set +e
${runner} --timeout-self-test ${timeout_dir}; timeout_code=$?
${runner} --zerr-self-test ${zerr_dir}; zerr_code=$?
set -e
[[ ${timeout_code} == 124 && ${zerr_code} == 1 ]]
[[ ! -e ${timeout_dir}/CANONICAL_V2_SCREEN.timeout-test.lock && ! -e ${zerr_dir}/CANONICAL_V2_SCREEN.timeout-test.lock ]]
rg -q 'timeout=true' ${timeout_dir}/TIMEOUT-SELF-TEST-STATUS-v1.txt
rg -q 'timeout=false' ${zerr_dir}/TIMEOUT-SELF-TEST-STATUS-v1.txt

print -- 'runner self-test PASS plan=38 actual-run-row=PASS cap=142 orphan=absent timeout=124 zerr=1 frozen_invocations=0'
