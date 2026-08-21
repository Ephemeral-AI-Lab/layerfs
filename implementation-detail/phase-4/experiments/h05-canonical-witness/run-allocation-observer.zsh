#!/bin/zsh
set -euo pipefail

(( $# == 1 )) || { print -u2 -- "usage: $0 OUTPUT-DIR"; exit 2; }
out=$1
[[ -d ${out} && ! -L ${out} && -z $(/usr/bin/find ${out} -mindepth 1 -print -quit) ]] || exit 2

work=${out}/files-v1
raw=${out}/ALLOCATION-OBSERVER-RAW-v1.tsv
result=${out}/ALLOCATION-OBSERVER-RESULT-v1.json
/bin/mkdir ${work}
print -- "$0 ${out}" >${out}/COMMAND-v1.txt
{
    /usr/bin/uname -a
    /usr/bin/sw_vers
    /usr/bin/sqlite3 --version
    /bin/df ${out}
    /sbin/mount | /usr/bin/grep ' /System/Volumes/Data '
    print -- 'filesystem_observer=stat st_size and st_blocks*512'
    print -- 'physical_io_inference=forbidden'
} >${out}/ENVIRONMENT-v1.txt

sha() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
apparent() { /usr/bin/stat -f %z "$1"; }
allocated() { print -- $(( $(/usr/bin/stat -f %b "$1") * 512 )); }
residue() {
    local database=$1
    [[ -e ${database}-journal ]] && print -n 1 || print -n 0
    [[ -e ${database}-wal ]] && print -n $'\t1' || print -n $'\t0'
    [[ -e ${database}-shm ]] && print $'\t1' || print $'\t0'
}
record() {
    local pair=$1 order=$2 arm=$3 copy_sequence=$4 mutation_sequence=$5 snapshot=$6
    local database=$7 seed_sha=$8 pre_sha=$9 post_sha=${10} changes=${11}
    local integrity=$(/usr/bin/sqlite3 ${database} 'PRAGMA integrity_check;')
    local residue_fields=$(residue ${database})
    print -- "${pair}\t${order}\t${arm}\t${copy_sequence}\t${mutation_sequence}\t${snapshot}\t${seed_sha}\t${pre_sha}\t${post_sha}\t$(apparent ${database})\t$(allocated ${database})\t${integrity}\t${residue_fields}\t${changes}" >>${raw}
}

seed=${work}/seed.sqlite
/usr/bin/sqlite3 ${seed} <<'SQL' >/dev/null
PRAGMA page_size=4096;
PRAGMA journal_mode=DELETE;
PRAGMA synchronous=FULL;
PRAGMA temp_store=FILE;
PRAGMA mmap_size=0;
CREATE TABLE payload(id INTEGER PRIMARY KEY, generation INTEGER NOT NULL, body BLOB NOT NULL);
INSERT INTO payload VALUES(1, 0, randomblob(16777216));
VACUUM;
SQL
seed_sha=$(sha ${seed})
[[ $(/usr/bin/sqlite3 ${seed} 'PRAGMA integrity_check;') == ok ]]
[[ ! -e ${seed}-journal && ! -e ${seed}-wal && ! -e ${seed}-shm ]]
print -- "seed_sha256=${seed_sha}\nseed_apparent_bytes=$(apparent ${seed})\nseed_allocated_bytes=$(allocated ${seed})\npayload_bytes=16777216\npairs=6\norders=AB,BA,AB,BA,AB,BA" >${out}/SEED-CUSTODY-v1.txt

print -- $'pair\torder\tarm\tcopy_sequence\tmutation_sequence\tsnapshot\tseed_sha256\tpre_sha256\tpost_sha256\tapparent_bytes\tallocated_bytes\tintegrity_check\tjournal_present\twal_present\tshm_present\tchanges' >${raw}
copy_sequence=0
mutation_sequence=0
for pair in 0 1 2 3 4 5; do
    order=AB
    (( pair % 2 == 1 )) && order=BA
    pair_root=${work}/pair-${pair}
    /bin/mkdir -p ${pair_root}/A ${pair_root}/B
    for arm in A B; do
        (( copy_sequence += 1 ))
        database=${pair_root}/${arm}/probe.sqlite
        /bin/cp ${seed} ${database}
        pre_sha=$(sha ${database})
        [[ ${pre_sha} == ${seed_sha} ]]
        record ${pair} ${order} ${arm} ${copy_sequence} 0 PRE ${database} ${seed_sha} ${pre_sha} - 0
    done
    typeset -a arms
    [[ ${order} == AB ]] && arms=(A B) || arms=(B A)
    for arm in ${arms}; do
        (( mutation_sequence += 1 ))
        database=${pair_root}/${arm}/probe.sqlite
        pre_sha=$(sha ${database})
        changes=$(/usr/bin/sqlite3 ${database} <<'SQL' | /usr/bin/awk '/^[0-9]+$/ { value=$0 } END { print value }'
PRAGMA journal_mode=DELETE;
PRAGMA synchronous=FULL;
PRAGMA temp_store=FILE;
PRAGMA mmap_size=0;
BEGIN IMMEDIATE;
UPDATE payload SET generation=1, body=zeroblob(length(body)) WHERE id=1;
SELECT changes();
COMMIT;
SQL
)
        [[ ${changes} == 1 ]]
        post_sha=$(sha ${database})
        record ${pair} ${order} ${arm} ${copy_sequence} ${mutation_sequence} T0 ${database} ${seed_sha} ${pre_sha} ${post_sha} ${changes}
    done
done

/bin/sleep 2
for pair in 0 1 2 3 4 5; do
    order=AB
    (( pair % 2 == 1 )) && order=BA
    for arm in A B; do
        database=${work}/pair-${pair}/${arm}/probe.sqlite
        record ${pair} ${order} ${arm} 0 0 T1 ${database} ${seed_sha} ${seed_sha} $(sha ${database}) 1
    done
done

/usr/bin/python3 - ${raw} ${result} <<'PY'
import csv, json, pathlib, sys
raw, destination = map(pathlib.Path, sys.argv[1:])
rows = list(csv.DictReader(raw.open(), delimiter="\t"))
reasons = []
if len(rows) != 36:
    reasons.append("row-count")
for row in rows:
    for key in ("apparent_bytes", "allocated_bytes"):
        try:
            if int(row[key]) < 0:
                reasons.append(f"negative:{key}")
        except ValueError:
            reasons.append(f"malformed:{key}")
    if row["integrity_check"] != "ok" or any(row[key] != "0" for key in ("journal_present", "wal_present", "shm_present")):
        reasons.append("integrity-or-residue")
unstable_pairs = []
time_unstable = []
post_hashes = set()
for pair in range(6):
    group = [row for row in rows if int(row["pair"]) == pair]
    for snapshot in ("PRE", "T0", "T1"):
        snap = [row for row in group if row["snapshot"] == snapshot]
        if len(snap) != 2:
            reasons.append(f"shape:{pair}:{snapshot}")
            continue
        if snapshot == "PRE":
            if len({row["pre_sha256"] for row in snap}) != 1 or any(row["pre_sha256"] != row["seed_sha256"] for row in snap):
                reasons.append(f"pre-hash:{pair}")
        else:
            if len({row["post_sha256"] for row in snap}) != 1:
                reasons.append(f"post-hash:{pair}:{snapshot}")
            post_hashes.update(row["post_sha256"] for row in snap)
        if len({row["apparent_bytes"] for row in snap}) != 1:
            reasons.append(f"apparent:{pair}:{snapshot}")
        if len({row["allocated_bytes"] for row in snap}) != 1:
            unstable_pairs.append({"pair": pair, "snapshot": snapshot,
                                   "A": int(next(row for row in snap if row["arm"] == "A")["allocated_bytes"]),
                                   "B": int(next(row for row in snap if row["arm"] == "B")["allocated_bytes"])})
    for arm in ("A", "B"):
        t0 = next((row for row in group if row["arm"] == arm and row["snapshot"] == "T0"), None)
        t1 = next((row for row in group if row["arm"] == arm and row["snapshot"] == "T1"), None)
        if t0 and t1 and (t0["post_sha256"], t0["apparent_bytes"]) != (t1["post_sha256"], t1["apparent_bytes"]):
            reasons.append(f"time-content:{pair}:{arm}")
        if t0 and t1 and t0["allocated_bytes"] != t1["allocated_bytes"]:
            time_unstable.append({"pair": pair, "arm": arm, "T0": int(t0["allocated_bytes"]), "T1": int(t1["allocated_bytes"])})
if len(post_hashes) != 1:
    reasons.append("cross-pair-post-hash")
if any(row["snapshot"] != "PRE" and row["changes"] != "1" for row in rows):
    reasons.append("changes")
valid = not reasons
unstable = valid and bool(unstable_pairs or time_unstable)
result = {
    "schema": "h05b-allocation-observer-v1",
    "status": "PASS" if valid else "FAIL",
    "classification": ("EXACT_ALLOCATED_EQUALITY_UNSTABLE" if unstable else
                       "H05B_NOT_JUSTIFIED" if valid else "INVALID_PROBE"),
    "h05b_amendment_eligible": unstable,
    "rows": len(rows), "pairs": 6, "post_sha256": next(iter(post_hashes), None),
    "unstable_pairs": unstable_pairs, "time_unstable": time_unstable,
    "reasons": sorted(set(reasons)),
}
destination.write_text(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n")
print(json.dumps(result, indent=2, sort_keys=True))
raise SystemExit(not valid)
PY

/bin/chmod -R a-w ${out}
