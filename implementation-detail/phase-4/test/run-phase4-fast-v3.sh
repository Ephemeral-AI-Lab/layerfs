#!/bin/zsh
set -euo pipefail

if (( $# != 3 )); then
    print -u2 -r -- "usage: run-phase4-fast-v3.sh CHECKPOINT EXECUTABLE OUTPUT"
    exit 2
fi

checkpoint=$1
executable=$2
output=$3

[[ -x ${executable} ]] || { print -u2 -r -- "not executable: ${executable}"; exit 2; }
[[ ! -e ${output} ]] || { print -u2 -r -- "refusing to overwrite: ${output}"; exit 2; }
[[ -d ${output:h} ]] || { print -u2 -r -- "missing output directory: ${output:h}"; exit 2; }

work=$(/usr/bin/mktemp -d /tmp/layerfs-phase4-fast-v3.XXXXXX)
[[ ${work} == /tmp/layerfs-phase4-fast-v3.* && -d ${work} && ! -L ${work} ]] || {
    print -u2 -r -- "unsafe temporary directory: ${work}"
    exit 2
}

cleanup() {
    if [[ ${work} == /tmp/layerfs-phase4-fast-v3.* && -d ${work} && ! -L ${work} ]]; then
        /usr/bin/find ${work} -depth -delete
    fi
}
trap cleanup EXIT INT TERM

file_sha() {
    /usr/bin/shasum -a 256 $1 | /usr/bin/awk '{print $1}'
}

run_capped() {
    /usr/bin/perl -e '$seconds = shift; alarm $seconds; exec @ARGV or die "exec: $!\n"' 120 "$@"
}

internal_operation() {
    case $1 in
        edit-same) print -r -- same-middle ;;
        edit-plus1) print -r -- plus1-middle ;;
        *) print -r -- $1 ;;
    esac
}

validation_scope() {
    case $1 in
        edit-same|edit-plus1) print -r -- capture-only ;;
        materialize-warm) print -r -- warm-materialization ;;
        materialize-fresh) print -r -- fresh-process-materialization ;;
        read-range) print -r -- range-only ;;
        reopen) print -r -- reopen-only ;;
        *) return 1 ;;
    esac
}

delete_row_state() {
    local root=$1
    /usr/bin/find ${root} -maxdepth 1 -type f \
        ! -name 'S1-*.source' \
        ! -name phase4-fast-fixture.json \
        -delete
}

executable_sha=$(file_sha ${executable})
started_seconds=$(/bin/date +%s)
: > ${output}

run_row() {
    local root=$1
    local size=$2
    local operation=$3
    local iteration=$4
    local warmup=$5
    local sample_kind=$6
    local internal=$(internal_operation ${operation})
    local scope=$(validation_scope ${operation})
    local binary_validation=complete-roundtrip
    if [[ ${scope} == capture-only ]]; then
        binary_validation=capture-only
    fi

    run_capped ${executable} --fast-prepare ${root} ${size} ${operation} ${iteration}
    local database=${root}/db-K64-F64-${size}-${internal}-${iteration}.sqlite
    local authority=${database}.authority
    local expectations=${database}.expectations
    [[ -f ${database} && -f ${authority} && -f ${expectations} ]] || {
        print -u2 -r -- "prepared row is incomplete: ${operation}/${iteration}"
        exit 1
    }

    local database_sha=$(file_sha ${database})
    local authority_sha=$(file_sha ${authority})
    local expectations_sha=$(file_sha ${expectations})
    local fixture=${root}/S1-$((size / 1024 / 1024)).source
    local fixture_sha=$(file_sha ${fixture})
    local fixture_label=S1-$((size / 1024 / 1024))
    local stdout=${root}/row-${iteration}.stdout
    local stderr=${root}/row-${iteration}.stderr

    run_capped /usr/bin/time -l /usr/bin/env \
        LAYERFS_FAST_LANE=1 \
        LAYERFS_FAST_VALIDATION_SCOPE=${binary_validation} \
        WP4M_EXECUTABLE_SHA256=${executable_sha} \
        WP4M_BASE_COPY_METHOD=fast-lane-isolated-prepared-row \
        WP4M_BASE_DATABASE_SHA256=${database_sha} \
        WP4M_BASE_AUTHORITY_SHA256=${authority_sha} \
        WP4M_BASE_EXPECTATIONS_SHA256=${expectations_sha} \
        ${executable} --fast-row \
        ${root} ${size} ${operation} ${iteration} ${warmup} ${binary_validation} \
        > ${stdout} 2> ${stderr}

    local mutation=false
    if [[ ${operation} == edit-same || ${operation} == edit-plus1 ]]; then
        mutation=true
    fi
    /usr/bin/jq -e \
        --arg executable_sha ${executable_sha} \
        --argjson size ${size} \
        --arg operation ${internal} \
        --arg scope ${scope} \
        --argjson mutation ${mutation} \
        '.status == "PASS" and
         .candidate == "K64-F64" and
         .size_bytes == $size and
         .operation == $operation and
         .executable_sha256 == $executable_sha and
         .q_current == 0 and
         (if $mutation
          then .transactions == 1 and .commits == 1 and
               .commit_dispatches == 1 and .commit_returns == 1 and
               .commit_return_successes == 1 and
               .fresh_reopen_head_wall_ns == 0 and
               .fresh_full_scrub_wall_ns == 0 and
               .reconstruction_wall_ns == 0 and
               .range_verification_wall_ns == 0
          elif $scope == "warm-materialization"
          then .transactions == 0 and .commits == 0 and
               .fresh_reopen_head_wall_ns == 0 and .reconstruction_wall_ns > 0
          elif $scope == "fresh-process-materialization"
          then .transactions == 0 and .commits == 0 and
               .fresh_reopen_head_wall_ns > 0 and .reconstruction_wall_ns > 0
          elif $scope == "range-only"
          then .transactions == 0 and .commits == 0 and
               .range_verification_wall_ns > 0 and (.range_measurements|length) > 0
          else .transactions == 0 and .commits == 0 and
               .fresh_reopen_head_wall_ns > 0 and
               .reconstruction_wall_ns == 0 and .range_verification_wall_ns == 0
          end)' ${stdout} > /dev/null

    local user_seconds=$(/usr/bin/awk '/ real .* user .* sys$/ {print $3}' ${stderr})
    local system_seconds=$(/usr/bin/awk '/ real .* user .* sys$/ {print $5}' ${stderr})
    local rss_bytes=$(/usr/bin/awk '/maximum resident set size/ {print $1}' ${stderr})
    local peak_bytes=$(/usr/bin/awk '/peak memory footprint/ {print $1}' ${stderr})

    /usr/bin/jq -c \
        --arg checkpoint ${checkpoint} \
        --arg operation ${operation} \
        --arg scope ${scope} \
        --arg sample_kind ${sample_kind} \
        --arg fixture_sha ${fixture_sha} \
        --arg fixture_label ${fixture_label} \
        --argjson user_seconds ${user_seconds} \
        --argjson system_seconds ${system_seconds} \
        --argjson rss_bytes ${rss_bytes} \
        --argjson peak_bytes ${peak_bytes} \
        '. + {
          purpose: "performance_baseline",
          milestone: "FAST-LANE",
          operation: $operation,
          fixture: $fixture_label,
          validation_scope: $scope,
          base_copy_method: "fast-lane-isolated-prepared-row",
          fixture_manifest: "phase4-fast-fixture.json",
          fast_checkpoint: $checkpoint,
          fast_sample_kind: $sample_kind,
          fast_fixture_sha256: $fixture_sha,
          user_cpu_seconds: $user_seconds,
          system_cpu_seconds: $system_seconds,
          rss_bytes: $rss_bytes,
          peak_memory_footprint_bytes: $peak_bytes
        }' ${stdout} >> ${output}

    delete_row_state ${root}
    local elapsed_seconds=$(( $(/bin/date +%s) - started_seconds ))
    if (( elapsed_seconds > 300 )); then
        print -u2 -r -- "fast-lane five-minute budget exceeded"
        exit 1
    fi
}

for size in 1048576 10485760 104857600; do
    root=${work}/${size}
    /bin/mkdir ${root}
    run_capped ${executable} --fast-fixture ${root} ${size} > ${root}/fixture.stdout

    if (( size < 104857600 )); then
        sample=0
        for operation in edit-same edit-plus1 materialize-warm materialize-fresh read-range reopen; do
            run_row ${root} ${size} ${operation} $((94000 + size / 1024 / 1024 * 100 + sample)) false smoke
            sample=$((sample + 1))
        done
    else
        run_row ${root} ${size} edit-same 95000 true warmup
        for sample in 1 2 3; do
            run_row ${root} ${size} edit-same $((95000 + sample)) false measured
        done
        run_row ${root} ${size} edit-plus1 95100 false structural-guard
        for operation in materialize-warm materialize-fresh; do
            for sample in 1 2 3; do
                run_row ${root} ${size} ${operation} $((95200 + sample + 10 * ${#operation})) false measured
            done
        done
        for operation in read-range reopen; do
            for sample in 1 2 3 4 5; do
                run_row ${root} ${size} ${operation} $((96000 + sample + 10 * ${#operation})) false measured
            done
        done
    fi
done

/bin/chmod 0444 ${output}
elapsed_seconds=$(( $(/bin/date +%s) - started_seconds ))
rows=$(/usr/bin/wc -l < ${output} | /usr/bin/tr -d ' ')
print -r -- \
    "checkpoint=${checkpoint} status=PASS rows=${rows} wall_seconds=${elapsed_seconds} raw=${output}"
