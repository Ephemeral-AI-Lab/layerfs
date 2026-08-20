#!/bin/zsh
set -euo pipefail

if (( $# != 2 )); then
    print -u2 -r -- "usage: run-phase4-fixed-radix-acceptance.sh EXECUTABLE OUTPUT.jsonl"
    exit 2
fi

executable=$1
output=$2
runner=${0:A}
initial_path=${PATH}

[[ -x ${executable} ]] || { print -u2 -r -- "not executable: ${executable}"; exit 2; }
[[ ! -e ${output} ]] || { print -u2 -r -- "refusing to overwrite: ${output}"; exit 2; }
[[ -d ${output:h} ]] || { print -u2 -r -- "missing output directory: ${output:h}"; exit 2; }
[[ -x /usr/bin/jq && -x /usr/bin/time && -x /usr/bin/shasum ]] || {
    print -u2 -r -- "jq, time, and shasum are required"
    exit 2
}

work=$(/usr/bin/mktemp -d /tmp/layerfs-wp4m-fixed-radix.XXXXXX)
[[ ${work} == /tmp/layerfs-wp4m-fixed-radix.* && -d ${work} && ! -L ${work} ]] || {
    print -u2 -r -- "unsafe temporary directory: ${work}"
    exit 2
}
state=${work}/state
raw=${work}/fixed-radix-acceptance.jsonl
/bin/mkdir ${state}

cleanup() {
    if [[ ${work} == /tmp/layerfs-wp4m-fixed-radix.* && -d ${work} && ! -L ${work} ]]; then
        /usr/bin/find ${work} -depth -delete
    fi
}
trap cleanup EXIT INT TERM

file_sha() {
    /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'
}

started_seconds=$(/bin/date +%s)
run_capped() {
    local elapsed=$(( $(/bin/date +%s) - started_seconds ))
    local remaining=$(( 120 - elapsed ))
    (( remaining > 0 )) || { print -u2 -r -- "fixed-radix 120-second budget exceeded"; return 1; }
    local cap=60
    (( remaining < cap )) && cap=${remaining}
    local exit_status=0
    /usr/bin/perl -e '$seconds = shift; alarm $seconds; exec @ARGV or die "exec: $!\n"' ${cap} "$@" || exit_status=$?
    if (( exit_status != 0 )); then
        print -u2 -r -- "command failed: elapsed=${elapsed}s cap=${cap}s status=${exit_status} command=$1 ${2:-}"
    fi
    return ${exit_status}
}

internal_operation() {
    case $1 in
        write) print -r -- full ;;
        edit-same) print -r -- same-middle ;;
        edit-plus1-early) print -r -- plus1-early ;;
        edit-plus1-middle) print -r -- plus1-middle ;;
        *) return 1 ;;
    esac
}

typeset -a master_files

delete_row_state() {
    local row_file
    for row_file in ${state}/*(.N); do
        [[ ${row_file:t} == S1-*.source || ${row_file:t} == wp4m-fixed-radix-fixture-manifest.json ]] && continue
        (( ${master_files[(Ie)${row_file}]} )) && continue
        /bin/rm ${row_file}
    done
}

executable_sha=$(file_sha ${executable})
runner_sha=$(file_sha ${runner})
run_capped ${executable} --fixed-radix-acceptance-fixtures ${state}
print -u2 -r -- "fixtures done: count=3 elapsed_seconds=$(( $(/bin/date +%s) - started_seconds ))"
typeset -A master_database_shas master_authority_shas master_expectations_shas

prepare_master() {
    local size=$1
    local operation=$2
    local engine_operation=$(internal_operation ${operation})
    local key=${size}-${engine_operation}
    (( ${+master_database_shas[${key}]} )) && return 0

    run_capped ${executable} --fixed-radix-acceptance-prepare \
        ${state} ${size} ${operation} 0

    local database=${state}/db-K64-F64-${size}-${engine_operation}-0.sqlite
    local authority=${database}.authority
    local expectations=${database}.expectations
    [[ -f ${database} && -f ${authority} && -f ${expectations} ]] || {
        print -u2 -r -- "incomplete master: ${size}/${operation}"
        return 1
    }
    master_database_shas[${key}]=$(file_sha ${database})
    master_authority_shas[${key}]=$(file_sha ${authority})
    master_expectations_shas[${key}]=$(file_sha ${expectations})
    master_files+=(${database} ${authority} ${expectations})
    print -u2 -r -- "master ready: size=${size} operation=${operation} elapsed_seconds=$(( $(/bin/date +%s) - started_seconds ))"
}

run_row() {
    local size=$1
    local operation=$2
    local iteration=$3
    local warmup=$4
    local row_kind=$5
    local sample_index=$6
    local validation=$7
    local engine_operation=$(internal_operation ${operation})
    local master_key=${size}-${engine_operation}

    print -u2 -r -- "row start: size=${size} operation=${operation} sample=${sample_index} validation=${validation}"

    prepare_master ${size} ${operation}

    local database=${state}/db-K64-F64-${size}-${engine_operation}-${iteration}.sqlite
    local authority=${database}.authority
    local expectations=${database}.expectations
    local master_database=${state}/db-K64-F64-${size}-${engine_operation}-0.sqlite
    local master_authority=${master_database}.authority
    local master_expectations=${master_database}.expectations
    local row_file
    for row_file in ${database} ${authority} ${expectations}; do
        [[ ! -e ${row_file} ]] || { print -u2 -r -- "refusing to overwrite row state: ${row_file}"; return 1; }
    done
    run_capped /bin/cp ${master_database} ${database}
    run_capped /bin/cp ${master_authority} ${authority}
    run_capped /bin/cp ${master_expectations} ${expectations}
    [[ -f ${database} && -f ${authority} && -f ${expectations} ]] || {
        print -u2 -r -- "incomplete copied row: ${size}/${operation}/${iteration}"
        return 1
    }

    local database_sha=$(file_sha ${database})
    local authority_sha=$(file_sha ${authority})
    local expectations_sha=$(file_sha ${expectations})
    [[ ${database_sha} == ${master_database_shas[${master_key}]} &&
       ${authority_sha} == ${master_authority_shas[${master_key}]} &&
       ${expectations_sha} == ${master_expectations_shas[${master_key}]} ]] || {
        print -u2 -r -- "master copy hash mismatch: ${size}/${operation}/${iteration}"
        return 1
    }
    [[ ${PATH} == ${initial_path} ]] || { print -u2 -r -- "runner PATH mutated"; return 1; }
    local fixture=${state}/S1-$((size / 1024 / 1024)).source
    local fixture_label=S1-$((size / 1024 / 1024))
    local fixture_sha=$(file_sha ${fixture})
    local stdout=${state}/row-${iteration}.stdout
    local stderr=${state}/row-${iteration}.stderr

    if ! run_capped /usr/bin/time -l /usr/bin/env \
        LAYERFS_FIXED_RADIX_ACCEPTANCE=1 \
        WP4M_EXECUTABLE_SHA256=${executable_sha} \
        WP4M_BASE_COPY_METHOD=fixed-radix-acceptance-master-copy \
        WP4M_BASE_DATABASE_SHA256=${database_sha} \
        WP4M_BASE_AUTHORITY_SHA256=${authority_sha} \
        WP4M_BASE_EXPECTATIONS_SHA256=${expectations_sha} \
        ${executable} --fixed-radix-acceptance-row \
        ${state} ${size} ${operation} ${iteration} ${warmup} ${validation} \
        > ${stdout} 2> ${stderr}; then
        /bin/cat ${stderr} >&2
        return 1
    fi

    local real_seconds=$(/usr/bin/awk '/ real .* user .* sys$/ {print $1; exit}' ${stderr})
    local user_seconds=$(/usr/bin/awk '/ real .* user .* sys$/ {print $3; exit}' ${stderr})
    local system_seconds=$(/usr/bin/awk '/ real .* user .* sys$/ {print $5; exit}' ${stderr})
    local rss_bytes=$(/usr/bin/awk '/maximum resident set size/ {print $1; exit}' ${stderr})
    local peak_bytes=$(/usr/bin/awk '/peak memory footprint/ {print $1; exit}' ${stderr})
    local block_input=$(/usr/bin/awk '/block input operations/ {print $1; exit}' ${stderr})
    local block_output=$(/usr/bin/awk '/block output operations/ {print $1; exit}' ${stderr})
    [[ -n ${real_seconds} && -n ${user_seconds} && -n ${system_seconds} &&
       -n ${rss_bytes} && -n ${peak_bytes} && -n ${block_input} && -n ${block_output} ]] || {
        print -u2 -r -- "missing external time metric"
        return 1
    }

    local expected_source expected_fixture_sha expected_references old_references insertion_ordinal
    if (( size == 1048576 )); then
        expected_source=f79de600cf44b20c4443e06d2e2b9e8819e956ba5a7bcc9cab4ffd8a08059cf8
        expected_fixture_sha=4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a
        old_references=53
    elif (( size == 10485760 )); then
        expected_source=e40db05d7407b92253e56099df402f03b399990014b2d1397e422ca305472449
        expected_fixture_sha=0c7a66930ae0d1d69fcc0b59942278eeb3a3fd92a8912e3e30963f288a8f430e
        old_references=531
    else
        expected_source=bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7
        expected_fixture_sha=63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4
        old_references=5284
    fi
    [[ ${fixture_sha} == ${expected_fixture_sha} ]] || {
        print -u2 -r -- "fixture SHA-256 mismatch for ${fixture_label}"
        return 1
    }
    expected_references=${old_references}
    insertion_ordinal=0
    if [[ ${operation} == edit-plus1-early || ${operation} == edit-plus1-middle ]]; then
        expected_references=$((old_references + 1))
    fi
    if [[ ${operation} == edit-plus1-middle ]]; then
        insertion_ordinal=$((old_references / 2))
    fi

    /usr/bin/jq -ce \
        --arg operation ${operation} \
        --arg engine_operation ${engine_operation} \
        --arg row_kind ${row_kind} \
        --arg validation ${validation} \
        --arg executable_sha ${executable_sha} \
        --arg runner_sha ${runner_sha} \
        --arg fixture_sha ${fixture_sha} \
        --arg fixture_label ${fixture_label} \
        --arg database_sha ${database_sha} \
        --arg authority_sha ${authority_sha} \
        --arg expectations_sha ${expectations_sha} \
        --arg expected_source ${expected_source} \
        --argjson size ${size} \
        --argjson sample_index ${sample_index} \
        --argjson warmup ${warmup} \
        --argjson expected_references ${expected_references} \
        --argjson old_references ${old_references} \
        --argjson insertion_ordinal ${insertion_ordinal} \
        --argjson real_seconds ${real_seconds} \
        --argjson user_seconds ${user_seconds} \
        --argjson system_seconds ${system_seconds} \
        --argjson rss_bytes ${rss_bytes} \
        --argjson peak_bytes ${peak_bytes} \
        --argjson block_input ${block_input} \
        --argjson block_output ${block_output} '
        . as $row |
        select(
          $row.status == "PASS" and $row.error == null and
          $row.candidate == "K64-F64" and
          $row.profile_id == "b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1" and
          $row.size_bytes == $size and $row.input_size_bytes == $size and
          $row.operation == $engine_operation and $row.warmup == $warmup and
          $row.source_fingerprint == $expected_source and
          $row.expected_cdc_references == $expected_references and
          $row.actual_cdc_references == $expected_references and
          ($row.expected_cdc_sequence_fingerprint | test("^[0-9a-f]{64}$")) and
          ($row.root_id | test("^[0-9a-f]{64}$")) and
          ($row.transition_id | test("^[0-9a-f]{64}$")) and
          ($row.ordered_closure_digest | test("^[0-9a-f]{64}$")) and
          $row.executable_sha256 == $executable_sha and
          $row.pre_edit_database_sha256 == $database_sha and
          $row.pre_edit_authority_sha256 == $authority_sha and
          $row.pre_edit_expectations_sha256 == $expectations_sha and
          $row.base_copy_method == "fixed-radix-acceptance-master-copy" and
          $row.transactions == 1 and $row.commits == 1 and
          $row.commit_dispatches == 1 and $row.commit_returns == 1 and
          $row.commit_return_successes == 1 and $row.commit_return_errors == 0 and
          $row.commit_return_status == "ok" and $row.publication_status == "Committed" and
          $row.commit_timer_equation_matches == true and
          $row.durable_phase_sum_matches == true and $row.lifecycle_phase_sum_matches == true and
          $row.q_current == 0 and ($row.q_high_water | type) == "number" and
          $row.q_equation == "Q1" and $row.measurement_status.logical_q == "O" and
          ($row.w_bytes | type) == "number" and ($row.d_bytes | type) == "number" and
          $row.w_bytes == ($row.canonical_bytes_authenticated + $row.payload_io_bytes +
            64 * $row.objects_authenticated + 256 * $row.tree_node_reconstruction_events +
            256 * $row.directory_entry_reconstruction_events + $row.directory_entry_name_bytes +
            96 * $row.file_reference_reconstruction_events +
            256 * $row.delta_entry_reconstruction_events + $row.delta_entry_path_bytes +
            $row.traversal_spool_bytes_written + $row.receipt_evidence_bytes_hashed) and
          $row.d_bytes <= $row.payload_io_bytes and $row.measurement_status.w_d == "O" and
          ($row.mapping_bytes_rewritten | type) == "number" and
          ($row.pages | type) == "number" and ($row.branches | type) == "number" and
          (if ($operation | startswith("edit-plus1-"))
           then $row.suffix_references == ($old_references - $insertion_ordinal) and
                $row.suffix_bytes > 0 and $row.suffix_objects > 0 and
                $row.pages > 0 and $row.mapping_bytes_rewritten > 0
           else $row.suffix_references == 0 and $row.suffix_bytes == 0 and $row.suffix_objects == 0
           end) and
          (if $validation == "capture-only"
           then $row.fresh_reopen_head_wall_ns == 0 and
                $row.fresh_full_scrub_wall_ns == 0 and
                $row.reconstruction_wall_ns == 0 and
                $row.range_verification_wall_ns == 0 and
                $row.complete_lifecycle_total_wall_ns == $row.durable_capture_total_wall_ns
           else $operation == "write" and $row.fresh_reopen_head_wall_ns > 0 and
                $row.fresh_full_scrub_wall_ns > 0 and $row.reconstruction_wall_ns > 0 and
                $row.range_verification_wall_ns > 0
           end)
        ) |
        . + {
          schema: "wp4m-fixed-radix-acceptance-row-v1",
          purpose: "fixed_radix_acceptance",
          milestone: "WP4-M-FIXED-RADIX",
          engine_operation: $engine_operation,
          operation: $operation,
          row_kind: $row_kind,
          sample_index: $sample_index,
          validation_scope: $validation,
          fixture: $fixture_label,
          fixture_manifest: "wp4m-fixed-radix-fixture-manifest.json",
          fixture_sha256: $fixture_sha,
          runner_sha256: $runner_sha,
          runner_wall_ceiling_seconds: 120,
          runner_command_ceiling_seconds: 60,
          throughput_measurement_admissible: ($row_kind == "measured"),
          external_time: {
            real_seconds: $real_seconds,
            user_seconds: $user_seconds,
            system_seconds: $system_seconds,
            maximum_resident_set_bytes: $rss_bytes,
            peak_memory_footprint_bytes: $peak_bytes,
            block_input_operations: $block_input,
            block_output_operations: $block_output
          },
          suffix_model: (if ($operation | startswith("edit-plus1-")) then {
            kind: "ordinal-fixed-radix-suffix-linear-v1",
            old_references: $old_references,
            insertion_ordinal: $insertion_ordinal,
            rewritten_references: $row.suffix_references,
            rewritten_raw_bytes: $row.suffix_bytes,
            authenticated_objects: $row.suffix_objects,
            rewritten_pages: $row.pages,
            rewritten_branches: $row.branches,
            rewritten_mapping_bytes: $row.mapping_bytes_rewritten
          } else null end)
        }' ${stdout} >> ${raw}

    delete_row_state
    print -u2 -r -- "row done: size=${size} operation=${operation} sample=${sample_index} validation=${validation}"
}

size_index=0
for size in 1048576 10485760 104857600; do
    for sample in 0 1 2 3; do
        warmup=false
        row_kind=measured
        if (( sample == 0 )); then
            warmup=true
            row_kind=warmup
        fi
        iteration=$((970000 + size_index * 10000 + sample))
        run_row ${size} write ${iteration} ${warmup} ${row_kind} ${sample} capture-only
    done
    size_index=$((size_index + 1))
done

operation_index=0
for operation in edit-same edit-plus1-early edit-plus1-middle; do
    for sample in 0 1 2 3; do
        warmup=false
        row_kind=measured
        if (( sample == 0 )); then
            warmup=true
            row_kind=warmup
        fi
        iteration=$((980000 + operation_index * 100 + sample))
        run_row 104857600 ${operation} ${iteration} ${warmup} ${row_kind} ${sample} capture-only
    done
    operation_index=$((operation_index + 1))
done

size_index=0
for size in 1048576 10485760 104857600; do
    run_row ${size} write $((990000 + size_index)) false roundtrip-check null complete-roundtrip
    size_index=$((size_index + 1))
done

(( ${#master_database_shas} == 6 )) || { print -u2 -r -- "master count mismatch"; exit 1; }
(( ${#master_files} == 18 )) || { print -u2 -r -- "master file count mismatch"; exit 1; }
for key in ${(k)master_database_shas}; do
    size=${key%%-*}
    engine_operation=${key#*-}
    master_database=${state}/db-K64-F64-${size}-${engine_operation}-0.sqlite
    [[ $(file_sha ${master_database}) == ${master_database_shas[${key}]} &&
       $(file_sha ${master_database}.authority) == ${master_authority_shas[${key}]} &&
       $(file_sha ${master_database}.expectations) == ${master_expectations_shas[${key}]} ]] || {
        print -u2 -r -- "master mutation detected: ${key}"
        exit 1
    }
done
print -u2 -r -- "custody self-check: masters=6 status=PASS"

/usr/bin/jq -se '
  length == 27 and
  (map(select(.row_kind == "warmup")) | length == 6) and
  (map(select(.row_kind == "measured")) | length == 18) and
  (map(select(.row_kind == "roundtrip-check")) | length == 3) and
  (map(select(.row_kind != "roundtrip-check" and .operation == "write") | .size_bytes) |
    unique == [1048576, 10485760, 104857600]) and
  (all(.[] | select(.row_kind != "roundtrip-check" and .operation != "write");
    .size_bytes == 104857600)) and
  (map(select(.row_kind != "roundtrip-check")) |
   group_by([.size_bytes, .operation]) |
   length == 6 and all(length == 4 and
     (map(select(.row_kind == "warmup")) | length == 1) and
     (map(select(.row_kind == "measured")) | length == 3) and
     (map(.root_id) | unique | length == 1) and
     (map(.transition_id) | unique | length == 1) and
     (map(.ordered_closure_digest) | unique | length == 1))) and
  ([.[] | select(.row_kind == "roundtrip-check")] | all(.operation == "write")) and
  (. as $rows | all($rows[] | select(.row_kind == "roundtrip-check"); . as $check |
    any($rows[]; .row_kind != "roundtrip-check" and .size_bytes == $check.size_bytes and
      .operation == "write" and .root_id == $check.root_id and
      .transition_id == $check.transition_id and
      .ordered_closure_digest == $check.ordered_closure_digest)))
' ${raw} > /dev/null

elapsed_seconds=$(( $(/bin/date +%s) - started_seconds ))
(( elapsed_seconds <= 120 )) || { print -u2 -r -- "fixed-radix 120-second budget exceeded"; exit 1; }
(setopt NO_CLOBBER; /bin/cat ${raw} > ${output})
/bin/chmod 0444 ${output}
print -r -- "status=PASS rows=27 wall_seconds=${elapsed_seconds} raw=${output}"
