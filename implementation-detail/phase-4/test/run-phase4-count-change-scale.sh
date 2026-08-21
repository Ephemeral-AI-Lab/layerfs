#!/bin/zsh
set -euo pipefail

(( $# == 2 )) || { print -u2 -- "usage: $0 EXECUTABLE OUTPUT.jsonl"; exit 2; }
executable=$1
output=$2
[[ -x ${executable} && ! -e ${output} && -d ${output:h} ]] || exit 2

work=$(/usr/bin/mktemp -d /tmp/layerfs-count-scale.XXXXXX)
[[ ${work} == /tmp/layerfs-count-scale.* && -d ${work} && ! -L ${work} ]] || exit 2
state=${work}/state
raw=${work}/rows.jsonl
/bin/mkdir ${state}
cleanup() {
    if [[ ${work} == /tmp/layerfs-count-scale.* && -d ${work} && ! -L ${work} ]]; then
        /usr/bin/find ${work} -depth -delete
    fi
}
trap cleanup EXIT INT TERM

started=$(/bin/date +%s)
run_capped() {
    local remaining=$((120 - ($(date +%s) - started)))
    (( remaining > 0 )) || return 1
    local cap=60
    (( remaining < cap )) && cap=${remaining}
    /usr/bin/perl -e '$s=shift; alarm $s; exec @ARGV or die $!' ${cap} "$@"
}
sha() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
engine_operation() {
    [[ $1 == edit-plus1-early ]] && print plus1-early || print plus1-middle
}

executable_sha=$(sha ${executable})
runner_sha=$(sha ${0:A})
typeset -a sizes=(1048576 10485760 104857600 524288000)
typeset -a operations=(edit-plus1-early edit-plus1-middle)
typeset -A db_sha authority_sha expectations_sha
typeset -a master_files

for size in ${sizes}; do
    run_capped ${executable} --count-change-scale-fixture ${state} ${size} >/dev/null
done

register_master() {
    local size=$1 operation=$2 engine=$(engine_operation $2) key=${1}-${2}
    local database=${state}/db-K64-F64-${size}-${engine}-0.sqlite
    [[ -f ${database} && -f ${database}.authority && -f ${database}.expectations ]] || return 1
    db_sha[${key}]=$(sha ${database})
    authority_sha[${key}]=$(sha ${database}.authority)
    expectations_sha[${key}]=$(sha ${database}.expectations)
    master_files+=(${database} ${database}.authority ${database}.expectations)
    print -u2 -- "master size=${size} operation=${operation} elapsed=$(( $(date +%s) - started ))s"
}

for size in ${sizes}; do
    run_capped ${executable} --count-change-scale-prepare ${state} ${size} edit-plus1-early 0 &
    early_pid=$!
    run_capped ${executable} --count-change-scale-prepare ${state} ${size} edit-plus1-middle 0 &
    middle_pid=$!
    wait ${early_pid}
    wait ${middle_pid}
    register_master ${size} edit-plus1-early
    register_master ${size} edit-plus1-middle
done

run_row() {
    local size=$1 operation=$2 iteration=$3 warmup=$4 row_kind=$5 sample=$6 validation=$7
    local engine=$(engine_operation ${operation}) key=${size}-${operation}
    local master=${state}/db-K64-F64-${size}-${engine}-0.sqlite
    local database=${state}/db-K64-F64-${size}-${engine}-${iteration}.sqlite
    /bin/cp ${master} ${database}
    /bin/cp ${master}.authority ${database}.authority
    /bin/cp ${master}.expectations ${database}.expectations
    local stdout=${state}/row-${iteration}.stdout stderr=${state}/row-${iteration}.stderr
    run_capped /usr/bin/time -l /usr/bin/env \
        LAYERFS_FIXED_RADIX_ACCEPTANCE=1 \
        WP4M_EXECUTABLE_SHA256=${executable_sha} \
        WP4M_BASE_COPY_METHOD=fixed-radix-acceptance-master-copy \
        WP4M_BASE_DATABASE_SHA256=${db_sha[${key}]} \
        WP4M_BASE_AUTHORITY_SHA256=${authority_sha[${key}]} \
        WP4M_BASE_EXPECTATIONS_SHA256=${expectations_sha[${key}]} \
        ${executable} --count-change-scale-row ${state} ${size} ${operation} \
        ${iteration} ${warmup} ${validation} >${stdout} 2>${stderr}

    local real=$(/usr/bin/awk '/ real .* user .* sys$/ {print $1; exit}' ${stderr})
    local user=$(/usr/bin/awk '/ real .* user .* sys$/ {print $3; exit}' ${stderr})
    local system=$(/usr/bin/awk '/ real .* user .* sys$/ {print $5; exit}' ${stderr})
    local rss=$(/usr/bin/awk '/maximum resident set size/ {print $1; exit}' ${stderr})
    local peak=$(/usr/bin/awk '/peak memory footprint/ {print $1; exit}' ${stderr})
    [[ -n ${real} && -n ${user} && -n ${system} && -n ${rss} && -n ${peak} ]] || return 1

    /usr/bin/jq -ce \
        --arg operation ${operation} --arg row_kind ${row_kind} --arg validation ${validation} \
        --arg runner_sha ${runner_sha} --argjson sample ${sample} \
        --argjson real ${real} --argjson user ${user} --argjson system ${system} \
        --argjson rss ${rss} --argjson peak ${peak} '
      . as $r |
      ($r.expected_cdc_references - 1) as $old |
      (if $operation == "edit-plus1-early" then 0 else ($old / 2 | floor) end) as $position |
      ($r.phase_counters[] | select(.phase == "precommit_closure")) as $pre |
      ($r.phase_counters[] | select(.phase == "canonical_cas_mapping")) as $mapping |
      select(
        $r.status == "PASS" and $r.error == null and
        $r.profile_id == "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1" and
        $r.qualification_mode == "C1-count-change-construction-proof" and
        $r.transactions == 1 and $r.commits == 1 and
        $r.commit_dispatches == 1 and $r.commit_returns == 1 and
        $r.commit_return_successes == 1 and $r.commit_return_errors == 0 and
        $r.publication_status == "Committed" and $r.q_current == 0 and
        $r.construction_proof_consumptions == 1 and $r.source_bytes_read == 1 and
        $r.suffix_references == ($old - $position) and
        $mapping.incremental_receipt_covered_edges == $old and
        $pre.construction_proof_consumptions == 1 and
        $pre.objects_authenticated == 0 and $pre.canonical_bytes_authenticated == 0 and
        (if $validation == "complete-roundtrip" then
           $r.fresh_reopen_head_wall_ns > 0 and $r.fresh_full_scrub_wall_ns > 0 and
           $r.reconstruction_wall_ns > 0 and $r.range_verification_wall_ns > 0
         else $r.complete_lifecycle_total_wall_ns == $r.durable_capture_total_wall_ns end)
      ) |
      . + {
        schema:"phase4-count-change-scale-v1", milestone:"CP-0008-DIAGNOSTIC",
        row_kind:$row_kind, sample_index:$sample, validation_scope:$validation,
        runner_sha256:$runner_sha, runner_wall_ceiling_seconds:120,
        runner_command_ceiling_seconds:60,
        old_references:$old, insertion_ordinal:$position,
        external_time:{real_seconds:$real,user_seconds:$user,system_seconds:$system,
          maximum_resident_set_bytes:$rss,peak_memory_footprint_bytes:$peak}
      }' ${stdout} >>${raw}
    /bin/rm ${database} ${database}.authority ${database}.expectations ${stdout} ${stderr}
    print -u2 -- "row size=${size} operation=${operation} sample=${sample} validation=${validation}"
}

arm=0
for size in ${sizes}; do
    for operation in ${operations}; do
        for sample in 0 1 2 3; do
            warmup=false row_kind=measured
            (( sample == 0 )) && { warmup=true; row_kind=warmup; }
            run_row ${size} ${operation} $((810000 + arm * 100 + sample)) \
                ${warmup} ${row_kind} ${sample} capture-only
        done
        (( arm += 1 ))
    done
done

roundtrip=0
for operation in ${operations}; do
    run_row 524288000 ${operation} $((820000 + roundtrip)) false roundtrip null complete-roundtrip
    (( roundtrip += 1 ))
done

(( ${#master_files} == 24 )) || exit 1
for size in ${sizes}; do
    for operation in ${operations}; do
        engine=$(engine_operation ${operation})
        key=${size}-${operation}
        master=${state}/db-K64-F64-${size}-${engine}-0.sqlite
        [[ $(sha ${master}) == ${db_sha[${key}]} &&
           $(sha ${master}.authority) == ${authority_sha[${key}]} &&
           $(sha ${master}.expectations) == ${expectations_sha[${key}]} ]] || exit 1
    done
done

/usr/bin/jq -se '
  length == 34 and
  (map(select(.row_kind=="warmup"))|length)==8 and
  (map(select(.row_kind=="measured"))|length)==24 and
  (map(select(.row_kind=="roundtrip"))|length)==2 and
  ([.[]|select(.row_kind!="roundtrip")]|group_by([.size_bytes,.operation])|
    length==8 and all(length==4 and (map(.root_id)|unique|length)==1 and
      (map(.transition_id)|unique|length)==1 and (map(.ordered_closure_digest)|unique|length)==1))
' ${raw} >/dev/null
(( $(date +%s) - started <= 120 )) || exit 1
(setopt NO_CLOBBER; /bin/cat ${raw} >${output})
/bin/chmod 0444 ${output}
print -- "status=PASS rows=34 wall_seconds=$(( $(date +%s) - started )) raw=${output}"
