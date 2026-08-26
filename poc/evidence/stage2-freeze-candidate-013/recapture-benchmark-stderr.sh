#!/usr/bin/env bash
set -euo pipefail

image="layerfs-fuse:frozen-bd1cd22"
evidence="$(cd "$(dirname "$0")" && pwd)"
full_scenarios='create 1000 files,stat 1000 files,rm 1000 files,mkdir tree (10x10x10),find tree,write 64 MiB,copy 64 MiB,read 64 MiB,pure read 64 MiB,pure copy 64 MiB,overwrite 64 MiB,git init + commit 100 files'

run_phase() {
  local phase="$1" base="$2" reps="$3" warmup="$4" randomized="$5" scenarios="$6" control="$7"
  local name="layerfs-stage2-final013-stderr-$phase"
  local volume="layerfs_stage2_final013_stderr_${phase//-/_}_store"
  local owned="/var/tmp/layerfs-owned"
  local prefix="stderr-recapture-$phase"

  if docker container inspect "$name" >/dev/null 2>&1; then
    echo "container already exists: $name" >&2
    return 1
  fi
  if docker volume inspect "$volume" >/dev/null 2>&1; then
    echo "volume already exists: $volume" >&2
    return 1
  fi
  for suffix in bench.json benchmark.stdout benchmark.stderr docker-inspect.json terminal.json; do
    if [[ -e "$evidence/$prefix-$suffix" ]]; then
      echo "output already exists: $prefix-$suffix" >&2
      return 1
    fi
  done

  docker volume create "$volume" >/dev/null
  docker run -d --name "$name" --platform linux/arm64 --init --cpus 1 \
    --memory 3g --pids-limit 512 --device /dev/fuse:rwm \
    --cap-add SYS_ADMIN --network none \
    --tmpfs /tmp:rw,nosuid,nodev,size=1g,mode=1777 \
    -v "$volume:/var/lib/layerfs" "$image" \
    --store /var/lib/layerfs/store.sqlite --mount /workspace \
    --spool "$owned/spool" --receipt "$owned/terminal.json" \
    --ref main --integrity trusted --uid 0 --gid 0 >/dev/null
  for _ in $(seq 1 100); do
    docker exec "$name" mountpoint -q /workspace && break
    sleep 0.05
  done
  docker inspect "$name" > "$evidence/$prefix-docker-inspect.json"
  docker exec "$name" sh -c 'cat /sys/fs/cgroup/cpu.max' \
    > "$evidence/$prefix-cpu.max.txt"
  docker exec "$name" sh -c 'cat /proc/self/mountinfo' \
    > "$evidence/$prefix-mountinfo.txt"

  docker exec \
    -e SCENARIOS="$scenarios" -e REPS="$reps" -e WARMUP="$warmup" \
    -e RANDOMIZE_TARGETS="$randomized" -e MOUNT=/workspace -e BASE="$base" \
    -e OUTPUT_JSON="$owned/$prefix-bench.json" \
    -e CAPTURE_PREFIX="$prefix" -e CONTROL="$control" \
    "$name" bash -c '
      set +e
      bash /usr/local/bin/fs-bench.sh \
        > "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-benchmark.stdout" \
        2> "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-benchmark.stderr"
      benchmark_exit=$?
      printf "%s\n" "$benchmark_exit" \
        > "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-benchmark.exit"
      if [[ "$CONTROL" != none && "$benchmark_exit" -eq 0 ]]; then
        python3 /usr/local/bin/verify_fs_bench.py \
          "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-bench.json" \
          "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-benchmark.stdout" \
          "$CONTROL" \
          "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-verification.json" \
          > "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-verifier.stdout" \
          2> "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-verifier.stderr"
        verifier_exit=$?
        printf "%s\n" "$verifier_exit" \
          > "/var/tmp/layerfs-owned/$CAPTURE_PREFIX-verifier.exit"
      fi
      exit "$benchmark_exit"
    '

  for suffix in bench.json benchmark.stdout benchmark.stderr benchmark.exit; do
    docker cp "$name:$owned/$prefix-$suffix" "$evidence/$prefix-$suffix"
  done
  if [[ "$control" != none ]]; then
    for suffix in verification.json verifier.stdout verifier.stderr verifier.exit; do
      docker cp "$name:$owned/$prefix-$suffix" "$evidence/$prefix-$suffix"
    done
  fi
  docker kill --signal TERM "$name" >/dev/null
  test "$(docker wait "$name")" = 0
  docker cp "$name:$owned/terminal.json" "$evidence/$prefix-terminal.json"
  docker rm "$name" >/dev/null
  docker volume rm "$volume" >/dev/null
}

run_phase smoke /var/tmp 1 1 0 \
  'create 1000 files,stat 1000 files,pure read 64 MiB' none
run_phase readiness-var /var/tmp 1 0 1 "$full_scenarios" overlay
run_phase readiness-tmp /tmp 1 0 1 "$full_scenarios" tmpfs
run_phase authoritative-var /var/tmp 3 1 1 "$full_scenarios" overlay
run_phase authoritative-tmp /tmp 3 1 1 "$full_scenarios" tmpfs
