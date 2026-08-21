#!/bin/zsh
set -euo pipefail

runner=${0:A:h}/run-aa100-attribution.zsh
work=$(/usr/bin/mktemp -d /tmp/h05c-aa100-self-test.XXXXXX)
[[ ${work} == /tmp/h05c-aa100-self-test.* && -d ${work} && ! -L ${work} ]]
cleanup() { [[ -d ${work} && ! -L ${work} ]] && /usr/bin/find ${work} -depth -delete; }
trap cleanup EXIT INT TERM HUP

/usr/bin/python3 - ${runner} <<'PY'
from pathlib import Path
import re, sys
text = Path(sys.argv[1]).read_text()
for forbidden in ("phase4_create_edit_benchmark-h05", "15a668739e96", "LFS-H05-EXPECTATIONS"):
    if forbidden in text:
        raise SystemExit(f"forbidden candidate token: {forbidden}")
for number, line in enumerate(text.splitlines(), 1):
    if not line.lstrip().startswith("local "):
        continue
    for match in re.finditer(r"(?:^|\s)([A-Za-z_][A-Za-z0-9_]*)=", line):
        name = match.group(1)
        if re.search(r"\$\{?" + re.escape(name) + r"(?:\}|\b)", line[match.end():]):
            raise SystemExit(f"same-line local dependency at {number}: {name}")
PY

eval "$(/usr/bin/awk '
    /^run_capped\(\) \{/ { capture=1 }
    capture && /^execute_study$/ { exit }
    capture { print }
' ${runner})"

out=${work}/out
rows_dir=${out}/rows-v1
study_work=${work}/study-work
raw=${out}/AA100-RAW-v1.jsonl
snapshots=${out}/AA100-STORAGE-SNAPSHOTS-v1.tsv
custody_tsv=${out}/AA100-INPUT-CUSTODY-v1.tsv
actual_invocations=${out}/AA100-ACTUAL-INVOCATIONS-v1.tsv
size=104857600
control_sha=9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7
runner_path=${runner}
fixture=${work}/fixture
print -n source >${fixture}
fixture_sha=$(/usr/bin/shasum -a 256 ${fixture} | /usr/bin/awk '{print $1}')
/bin/mkdir -p ${rows_dir} ${study_work}

stub=${work}/control-stub.zsh
{
    print -- '#!/bin/zsh'
    print -- 'set -euo pipefail'
    print -- 'if [[ $1 == --fast-prepare ]]; then'
    print -- '  [[ $# == 5 && $3 == 104857600 && $4 == write ]]'
    print -- '  db=$2/db-K64-F64-104857600-full-$5.sqlite'
    print -- '  /usr/bin/sqlite3 $db '\''CREATE TABLE probe(id INTEGER PRIMARY KEY, value BLOB); INSERT INTO probe VALUES(1, zeroblob(4096));'\'''
    print -- '  /usr/bin/python3 - $db.authority <<'\''PY'\'''
    print -- 'from pathlib import Path'
    print -- 'import sys'
    print -- 'Path(sys.argv[1]).write_bytes(b"a" * 32)'
    print -- 'PY'
    print -- '  print -- '\''LFS-WP4M-EXPECTATIONS-3\nmanifest_blake3=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'\'' >$db.expectations'
    print -- '  exit 0'
    print -- 'fi'
    print -- '[[ $# == 7 && $1 == --fast-row && $3 == 104857600 && $4 == write && $6 == false && $7 == capture-only ]]'
    print -- '[[ ${LAYERFS_FAST_LANE:-} == 1 && ${WP4M_EXECUTABLE_SHA256:-} == 9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7 ]]'
    print -- '[[ ${WP4M_BASE_COPY_METHOD:-} == physical-byte-copy-identical-database-authority-expectations ]]'
    print -- 'print -- '\''{"status":"PASS","error":null,"q_current":0,"transactions":1,"commits":1}'\'''
} >${stub}
/bin/chmod 0755 ${stub}
control=${stub}

sha() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
functions -c run_row_capped capped_under_test
remaining() { print 1; }
timed_out=false
pid_file=${work}/capped.pid
set +e
capped_under_test /usr/bin/perl -e 'open my $fh, ">", $ARGV[0] or die $!; print $fh $$; close $fh; sleep 5' ${pid_file} >/dev/null 2>&1
cap_code=$?
set -e
[[ ${cap_code} == 142 && -s ${pid_file} ]]
! /bin/kill -0 $(<${pid_file}) 2>/dev/null
remaining() { print 60; }
run_capped() { "$@"; }
run_row_capped() { /usr/bin/time -l "$@"; }
print -- $'pair\torder\tarm\tsnapshot\tsnapshot_utc\tmonotonic_ns\tdatabase_sha256\tauthority_sha256\texpectations_sha256\tfixture_sha256\tdatabase_logical_bytes\tdatabase_apparent_bytes\tdatabase_allocated_bytes\tauthority_apparent_bytes\tauthority_allocated_bytes\tjournal_apparent_bytes\tjournal_allocated_bytes\tstore_logical_bytes\tstore_apparent_bytes\tstore_allocated_bytes\texpectations_apparent_bytes\texpectations_allocated_bytes\tintegrity_check\tjournal_present\twal_present\tshm_present' >${snapshots}
print -- $'pair\torder\tarm\titeration\tfixture_sha256\tbase_database_sha256\tbase_authority_sha256\tbase_expectations_sha256\texpectations_version\texecutable_sha256' >${custody_tsv}
print -- $'sequence\tevent\tutc\tkind\tpair\torder\tarm\titeration\texecutable_sha256\tcommand\texit' >${actual_invocations}
setopt NO_CLOBBER APPEND_CREATE
work=${study_work}
execute_study

[[ $(/usr/bin/wc -l <${raw} | /usr/bin/tr -d ' ') == 12 ]]
[[ $(/usr/bin/wc -l <${snapshots} | /usr/bin/tr -d ' ') == 37 ]]
[[ $(/usr/bin/wc -l <${out}/ROW-STARTS-v1.txt | /usr/bin/tr -d ' ') == 24 ]]
[[ $(/usr/bin/wc -l <${actual_invocations} | /usr/bin/tr -d ' ') == 37 ]]
/usr/bin/python3 - ${raw} ${snapshots} ${actual_invocations} ${runner} <<'PY'
import csv, hashlib, json, pathlib, sys
rows = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
plan = [(pair, arm, "AB" if pair % 2 == 0 else "BA") for pair in range(6)
        for arm in ("AB" if pair % 2 == 0 else "BA")]
assert [(r["aa_pair"], r["aa_label"], r["aa_order"]) for r in rows] == plan
assert all(r["runner_sha256"] == hashlib.sha256(pathlib.Path(sys.argv[4]).read_bytes()).hexdigest() for r in rows)
snapshots = list(csv.DictReader(pathlib.Path(sys.argv[2]).open(), delimiter="\t"))
assert [r["snapshot"] for r in snapshots].count("PRE") == 12
assert [r["snapshot"] for r in snapshots].count("T0") == 12
assert [r["snapshot"] for r in snapshots].count("T1") == 12
assert min(int(r["monotonic_ns"]) for r in snapshots if r["snapshot"] == "T1") - max(int(r["monotonic_ns"]) for r in snapshots if r["snapshot"] == "T0") >= 2_000_000_000
events = list(csv.DictReader(pathlib.Path(sys.argv[3]).open(), delimiter="\t"))
assert len(events) == 36 and {r["executable_sha256"] for r in events} == {"9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"}
assert sum(r["kind"] == "prepare" and r["event"] == "started" for r in events) == 6
assert sum(r["kind"] == "row" and r["event"] == "started" for r in events) == 12
PY

print -- 'A/A runner self-test PASS cap_exit=142 cap_orphan=absent actual_schedule=6-prepares,12-rows actual_snapshots=36 actual_ledger=36-events candidate_tokens=absent frozen_invocations=0'
