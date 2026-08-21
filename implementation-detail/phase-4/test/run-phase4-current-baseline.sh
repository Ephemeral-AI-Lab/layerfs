#!/bin/zsh
set -euo pipefail

(( $# == 2 )) || { print -u2 -- "usage: $0 EXECUTABLE OUTPUT.jsonl"; exit 2; }
executable=$1
output=$2
[[ -x ${executable} && ! -e ${output} && -d ${output:h} ]] || exit 2

work=$(/usr/bin/mktemp -d /tmp/layerfs-current-baseline.XXXXXX)
[[ ${work} == /tmp/layerfs-current-baseline.* && -d ${work} && ! -L ${work} ]] || exit 2
cleanup() {
    if [[ ${work} == /tmp/layerfs-current-baseline.* && -d ${work} && ! -L ${work} ]]; then
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
    case $1 in
        write) print full ;;
        edit-same) print same-middle ;;
        edit-plus1-early) print plus1-early ;;
        edit-plus1-middle) print plus1-middle ;;
        *) print $1 ;;
    esac
}
boundary() {
    case $1 in
        write) print durable-submit ;;
        edit-same|edit-plus1-early|edit-plus1-middle) print same-open-durable-edit ;;
        materialize-warm) print logical-materialization-warm ;;
        materialize-fresh) print fresh-process-logical-materialization ;;
        read-range) print authenticated-boundary-range-suite ;;
        read-range-1m) print authenticated-sequential-1m-range ;;
        reopen) print fresh-process-head-ready ;;
    esac
}

executable_sha=$(sha ${executable})
runner_sha=$(sha ${0:A})
typeset -a sizes=(1048576 10485760 104857600)
typeset -A master_path master_db_sha master_authority_sha master_expectations_sha
typeset -a master_files

for size in ${sizes}; do
    root=${work}/${size}
    /bin/mkdir ${root}
    run_capped ${executable} --fast-fixture ${root} ${size} >/dev/null
done

register_master() {
    local size=$1 key=$2 operation=$3 cli=$4
    local root=${work}/${size} engine=$(engine_operation ${operation})
    if [[ ${cli} == fast ]]; then
        run_capped ${executable} --fast-prepare ${root} ${size} ${operation} 0
    else
        run_capped ${executable} --count-change-scale-prepare ${root} ${size} ${operation} 0
    fi
    local database=${root}/db-K64-F64-${size}-${engine}-0.sqlite
    [[ -f ${database} && -f ${database}.authority && -f ${database}.expectations ]] || return 1
    master_path[${size}-${key}]=${database}
    master_db_sha[${size}-${key}]=$(sha ${database})
    master_authority_sha[${size}-${key}]=$(sha ${database}.authority)
    master_expectations_sha[${size}-${key}]=$(sha ${database}.expectations)
    master_files+=(${database} ${database}.authority ${database}.expectations)
}

run_row() {
    local size=$1 operation=$2 key=$3 iteration=$4 warmup=$5 sample_kind=$6 sample_index=$7
    local root=${work}/${size} engine=$(engine_operation ${operation}) master=${master_path[${size}-${key}]}
    local database=${root}/db-K64-F64-${size}-${engine}-${iteration}.sqlite
    /bin/cp ${master} ${database}
    /bin/cp ${master}.authority ${database}.authority
    /bin/cp ${master}.expectations ${database}.expectations

    local stdout=${root}/row-${iteration}.stdout stderr=${root}/row-${iteration}.stderr
    local validation=complete-roundtrip cli=--fast-row
    [[ ${operation} == write || ${operation} == edit-same || ${operation} == edit-plus1-* ]] && validation=capture-only
    [[ ${operation} == edit-plus1-* ]] && cli=--count-change-scale-row
    print -u2 -- "row start size=${size} operation=${operation} kind=${sample_kind}"
    if ! run_capped /usr/bin/time -l /usr/bin/env \
        LAYERFS_FAST_LANE=1 \
        WP4M_EXECUTABLE_SHA256=${executable_sha} \
        WP4M_BASE_COPY_METHOD=fast-lane-isolated-prepared-row \
        WP4M_BASE_DATABASE_SHA256=${master_db_sha[${size}-${key}]} \
        WP4M_BASE_AUTHORITY_SHA256=${master_authority_sha[${size}-${key}]} \
        WP4M_BASE_EXPECTATIONS_SHA256=${master_expectations_sha[${size}-${key}]} \
        ${executable} ${cli} ${root} ${size} ${operation} ${iteration} ${warmup} ${validation} \
        >${stdout} 2>${stderr}; then
        /bin/cat ${stderr} >&2
        return 1
    fi

    local user=$(/usr/bin/awk '/ real .* user .* sys$/ {print $3; exit}' ${stderr})
    local system=$(/usr/bin/awk '/ real .* user .* sys$/ {print $5; exit}' ${stderr})
    local rss=$(/usr/bin/awk '/maximum resident set size/ {print $1; exit}' ${stderr})
    local peak=$(/usr/bin/awk '/peak memory footprint/ {print $1; exit}' ${stderr})
    [[ -n ${user} && -n ${system} && -n ${rss} && -n ${peak} ]] || return 1

    local mutation=false
    [[ ${operation} == write || ${operation} == edit-* ]] && mutation=true
    if ! /usr/bin/jq -ce \
        --arg public_operation ${operation} --arg purpose product_workflow_baseline \
        --arg sample_kind ${sample_kind} --arg measurement_boundary $(boundary ${operation}) \
        --arg runner_sha ${runner_sha} --argjson sample_index ${sample_index} \
        --argjson mutation ${mutation} --argjson user ${user} --argjson system ${system} \
        --argjson rss ${rss} --argjson peak ${peak} '
      . as $r |
      select(
        $r.status == "PASS" and $r.error == null and
        $r.profile_id == "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1" and
        $r.q_current == 0 and
        (if $mutation then
           $r.transactions == 1 and $r.commits == 1 and
           $r.commit_dispatches == 1 and $r.commit_returns == 1 and
           $r.commit_return_successes == 1 and $r.publication_status == "Committed"
         else $r.transactions == 0 and $r.commits == 0 end) and
        (if $public_operation == "read-range-1m" then
           ($r.range_measurements|length) == 1 and
           $r.range_measurements[0].label == "sequential-1m" and
           $r.range_measurements[0].returned_bytes == 1048576
         elif $public_operation == "read-range" then
           ($r.range_measurements|length) >= (if $r.size_bytes == 104857600 then 7 else 5 end)
         elif ($public_operation|startswith("materialize-")) then $r.reconstruction_wall_ns > 0
         elif $public_operation == "reopen" then $r.fresh_reopen_head_wall_ns > 0
         else true end)
      ) |
      . + {
        schema:"phase4-current-baseline-v1", purpose:$purpose,
        milestone:"CURRENT-BASELINE-V1",
        acceptance_scope:"baseline", candidate_comparison:false, promotion:false,
        operation:$public_operation, sample_kind:$sample_kind, sample_index:$sample_index,
        measurement_boundary:$measurement_boundary,
        throughput_measurement_admissible:(
          $sample_kind == "measured" and
          ($public_operation == "write" or ($public_operation|startswith("materialize-")) or
           $public_operation == "read-range-1m")),
        runner_sha256:$runner_sha, runner_wall_ceiling_seconds:120,
        runner_command_ceiling_seconds:60,
        cpu_scope:"whole-child-process; phase-local CPU unavailable",
        cache_scope:"fresh LayerFS process/connection where declared; OS/filesystem cache warm-or-unknown",
        external_time:{user_seconds:$user,system_seconds:$system,
          maximum_resident_set_bytes:$rss,peak_memory_footprint_bytes:$peak}
      }' ${stdout} >>${work}/rows.jsonl; then
        /bin/cat ${stdout} >&2
        return 1
    fi
    /bin/rm ${database} ${database}.authority ${database}.expectations ${stdout} ${stderr}
    print -u2 -- "row done size=${size} operation=${operation} kind=${sample_kind}"
}

register_master 104857600 write write fast
for sample in 0 1 2 3; do
    warmup=false sample_kind=measured
    (( sample == 0 )) && { warmup=true; sample_kind=warmup; }
    run_row 104857600 write write $((840000 + sample)) ${warmup} ${sample_kind} ${sample}
done

for size in 1048576 10485760; do
    register_master ${size} write write fast
    register_master ${size} read materialize-fresh fast
    run_row ${size} write write $((830000 + size / 1048576)) false smoke null
    index=0
    for operation in materialize-warm materialize-fresh read-range read-range-1m reopen; do
        run_row ${size} ${operation} read $((831000 + size / 1048576 * 10 + index)) false smoke null
        (( index += 1 ))
    done
done

register_master 104857600 same edit-same fast
for sample in 0 1 2 3; do
    warmup=false sample_kind=measured
    (( sample == 0 )) && { warmup=true; sample_kind=warmup; }
    run_row 104857600 edit-same same $((840100 + sample)) ${warmup} ${sample_kind} ${sample}
done

register_master 104857600 read materialize-fresh fast
index=0
for operation in materialize-warm materialize-fresh read-range read-range-1m reopen; do
    for sample in 0 1 2 3; do
        warmup=false sample_kind=measured
        (( sample == 0 )) && { warmup=true; sample_kind=warmup; }
        run_row 104857600 ${operation} read $((840200 + index * 100 + sample)) \
            ${warmup} ${sample_kind} ${sample}
    done
    (( index += 1 ))
done
register_master 104857600 plus-early edit-plus1-early scale
register_master 104857600 plus-middle edit-plus1-middle scale
run_row 104857600 edit-plus1-early plus-early 850001 false structural-guard null
run_row 104857600 edit-plus1-middle plus-middle 850002 false structural-guard null

(( ${#master_files} == 27 )) || exit 1
for key in ${(k)master_path}; do
    master=${master_path[${key}]}
    [[ $(sha ${master}) == ${master_db_sha[${key}]} &&
       $(sha ${master}.authority) == ${master_authority_sha[${key}]} &&
       $(sha ${master}.expectations) == ${master_expectations_sha[${key}]} ]] || exit 1
done

/usr/bin/jq -se '
  length == 42 and
  (map(select(.sample_kind=="smoke"))|length)==12 and
  (map(select(.sample_kind=="warmup"))|length)==7 and
  (map(select(.sample_kind=="measured"))|length)==21 and
  (map(select(.sample_kind=="structural-guard"))|length)==2 and
  ([.[]|select(.size_bytes==104857600 and (.sample_kind=="warmup" or .sample_kind=="measured"))]|
    group_by(.operation)|length==7 and all(length==4 and
      (map(select(.sample_kind=="warmup"))|length)==1 and
      (map(select(.sample_kind=="measured"))|length)==3))
' ${work}/rows.jsonl >/dev/null
(( $(date +%s) - started <= 120 )) || exit 1
(setopt NO_CLOBBER; /bin/cat ${work}/rows.jsonl >${output})
/bin/chmod 0444 ${output}
print -- "status=PASS rows=42 wall_seconds=$(( $(date +%s) - started )) raw=${output}"
