#!/usr/bin/env bash
# Shared authenticated daemon startup for SDK and Workspace family runners.
start_benchmark_runtime() {
  local name=$1 image=$2
  local limits=()
  if [[ ${LAYERFS_V013_RESOURCE_PROFILE:-0} == 1 ]]; then
    limits=(--cpus 2 --memory 2g --memory-swap 2g --pids-limit 256)
  fi
  if [[ -n ${LAYERFS_V013_GIT_REFERENCE_HOST:-} ]]; then
    [[ -d $LAYERFS_V013_GIT_REFERENCE_HOST ]] || { echo "qualified Git reference absent" >&2; return 1; }
    limits+=(--mount "type=bind,src=$LAYERFS_V013_GIT_REFERENCE_HOST,dst=/qualified/git-reference,readonly" -e LAYERFS_V013_GIT_REFERENCE=/qualified/git-reference)
  fi
  if [[ -n ${LAYERFS_V013_VERIFIER_EXCHANGE:-} ]]; then
    [[ -d $LAYERFS_V013_VERIFIER_EXCHANGE ]] || { echo "verifier exchange absent" >&2; return 1; }
    limits+=(--mount "type=bind,src=$LAYERFS_V013_VERIFIER_EXCHANGE,dst=/verification")
  fi
  docker run -d --name "$name" --label layerfs.phase1.runtime="$name" --device /dev/fuse --cap-add SYS_ADMIN \
    --security-opt apparmor=unconfined -p 127.0.0.1::41273 "${limits[@]}" "$image" >/dev/null
  container_id=$(docker inspect -f '{{.Id}}' "$name")
  port=$(docker inspect -f '{{(index (index .NetworkSettings.Ports "41273/tcp") 0).HostPort}}' "$name")
  local ready=false
  for _ in $(seq 1 50); do
    if docker exec "$name" test -s /run/layerfs/capability >/dev/null 2>&1; then ready=true; break; fi
    sleep 0.05
  done
  [[ $ready == true ]] || { echo "owned daemon readiness failed: $name" >&2; return 1; }
  capability=$(docker exec "$name" sh -c 'od -An -tx1 -v /run/layerfs/capability | tr -d " \n"')
  [[ ${#capability} == 64 && $capability != *[!0-9a-f]* ]]
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  set -euo pipefail
  start_benchmark_runtime "$@"
  printf '%s\t%s\t%s\n' "$container_id" "$port" "$capability"
fi
