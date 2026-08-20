#!/bin/zsh
set -euo pipefail

if (( $# < 5 || $# > 6 )); then
    print -u2 -r -- \
        "usage: run-phase4-fast.sh CHECKPOINT EXECUTABLE FIXTURE MANIFEST OUTPUT [MEASURED_SAMPLES]"
    exit 2
fi

checkpoint=$1
executable=$2
fixture=$3
fixture_manifest=$4
output=$5
measured_samples=${6:-5}

[[ -x ${executable} ]] || { print -u2 -r -- "not executable: ${executable}"; exit 2; }
[[ -f ${fixture} ]] || { print -u2 -r -- "missing fixture: ${fixture}"; exit 2; }
[[ -f ${fixture_manifest} ]] || {
    print -u2 -r -- "missing fixture manifest: ${fixture_manifest}"
    exit 2
}
[[ ! -e ${output} ]] || { print -u2 -r -- "refusing to overwrite: ${output}"; exit 2; }
[[ -d ${output:h} ]] || { print -u2 -r -- "missing output directory: ${output:h}"; exit 2; }
[[ ${measured_samples} == <1-> ]] || {
    print -u2 -r -- "MEASURED_SAMPLES must be a positive integer"
    exit 2
}

work=$(/usr/bin/mktemp -d /tmp/layerfs-phase4-fast.XXXXXX)
[[ ${work} == /tmp/layerfs-phase4-fast.* && -d ${work} && ! -L ${work} ]] || {
    print -u2 -r -- "unsafe temporary directory: ${work}"
    exit 2
}

cleanup() {
    if [[ ${work} == /tmp/layerfs-phase4-fast.* && -d ${work} && ! -L ${work} ]]; then
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

/bin/cp ${fixture} ${work}/S1-100.source
/bin/cp ${fixture_manifest} ${work}/wp4m-retained-fixture-manifest.json

fixture_sha=$(file_sha ${fixture})
executable_sha=$(file_sha ${executable})
started_seconds=$(/bin/date +%s)
: > ${output}

for (( sample = 0; sample <= measured_samples; sample++ )); do
    warmup=false
    if (( sample == 0 )); then
        warmup=true
    fi
    iteration=$((90000 + sample))

    run_capped ${executable} --prepare-row \
        ${work} K64-F64 104857600 full ${iteration}

    database=${work}/db-K64-F64-104857600-full-${iteration}.sqlite
    authority=${database}.authority
    expectations=${database}.expectations
    [[ -f ${database} && -f ${authority} && -f ${expectations} ]] || {
        print -u2 -r -- "prepared row is incomplete: ${iteration}"
        exit 1
    }

    database_sha=$(file_sha ${database})
    authority_sha=$(file_sha ${authority})
    expectations_sha=$(file_sha ${expectations})
    stdout=${work}/row-${iteration}.stdout
    stderr=${work}/row-${iteration}.stderr

    run_capped /usr/bin/time -l /usr/bin/env \
        WP4M_EXECUTABLE_SHA256=${executable_sha} \
        WP4M_BASE_COPY_METHOD=fast-lane-isolated-prepared-row \
        WP4M_BASE_DATABASE_SHA256=${database_sha} \
        WP4M_BASE_AUTHORITY_SHA256=${authority_sha} \
        WP4M_BASE_EXPECTATIONS_SHA256=${expectations_sha} \
        ${executable} --row \
        ${work} K64-F64 104857600 full ${iteration} ${warmup} \
        > ${stdout} 2> ${stderr}

    /usr/bin/jq -e \
        --arg executable_sha ${executable_sha} \
        '.status == "PASS" and
         .candidate == "K64-F64" and
         .size_bytes == 104857600 and
         .operation == "full" and
         .executable_sha256 == $executable_sha and
         .transactions == 1 and
         .commits == 1 and
         .commit_dispatches == 1 and
         .commit_returns == 1 and
         .commit_return_successes == 1 and
         .q_current == 0 and
         .durable_phase_sum_matches == true and
         .lifecycle_phase_sum_matches == true' \
        ${stdout} > /dev/null

    user_seconds=$(/usr/bin/awk '/ real .* user .* sys$/ {print $3}' ${stderr})
    system_seconds=$(/usr/bin/awk '/ real .* user .* sys$/ {print $5}' ${stderr})
    rss_bytes=$(/usr/bin/awk '/maximum resident set size/ {print $1}' ${stderr})
    peak_bytes=$(/usr/bin/awk '/peak memory footprint/ {print $1}' ${stderr})

    /usr/bin/jq -c \
        --arg checkpoint ${checkpoint} \
        --arg fixture_sha ${fixture_sha} \
        --argjson sample ${sample} \
        --argjson warmup ${warmup} \
        --argjson user_seconds ${user_seconds} \
        --argjson system_seconds ${system_seconds} \
        --argjson rss_bytes ${rss_bytes} \
        --argjson peak_bytes ${peak_bytes} \
        '. + {
          fast_checkpoint: $checkpoint,
          fast_sample: $sample,
          warmup: $warmup,
          fast_fixture_sha256: $fixture_sha,
          user_cpu_seconds: $user_seconds,
          system_cpu_seconds: $system_seconds,
          rss_bytes: $rss_bytes,
          peak_memory_footprint_bytes: $peak_bytes
        }' ${stdout} >> ${output}

    /usr/bin/find ${work} -maxdepth 1 -type f \
        ! -name S1-100.source \
        ! -name wp4m-retained-fixture-manifest.json \
        -delete

    elapsed_seconds=$(( $(/bin/date +%s) - started_seconds ))
    if (( elapsed_seconds > 300 )); then
        print -u2 -r -- "fast-lane five-minute budget exceeded"
        exit 1
    fi
done

/bin/chmod 0444 ${output}
elapsed_seconds=$(( $(/bin/date +%s) - started_seconds ))
print -r -- \
    "checkpoint=${checkpoint} status=PASS warmup=1 measured=${measured_samples} wall_seconds=${elapsed_seconds} raw=${output}"
