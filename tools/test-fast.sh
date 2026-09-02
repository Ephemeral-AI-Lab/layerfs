#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
jobs=${LAYERFS_TEST_JOBS:-4}
[[ "$jobs" =~ ^[1-9][0-9]*$ ]] && (( jobs <= 16 )) || {
  printf 'test-fast: LAYERFS_TEST_JOBS must be 1..16\n' >&2
  exit 2
}

temporary=$(mktemp -d "${TMPDIR:-/tmp}/layerfs-test-fast.XXXXXX")
trap 'rm -rf -- "$temporary"' EXIT

cargo test --manifest-path "$repo/Cargo.toml" --workspace --all-features \
  --locked \
  --no-run --message-format=json >"$temporary/artifacts.jsonl"
started=$SECONDS

python3 - "$temporary/artifacts.jsonl" "$jobs" <<'PY'
import concurrent.futures
import json
import os
import subprocess
import sys
import time

manifest, jobs = sys.argv[1], int(sys.argv[2])
executables = {}
with open(manifest, encoding="utf-8") as source:
    for line in source:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        executable = message.get("executable")
        manifest_path = message.get("manifest_path")
        if message.get("reason") == "compiler-artifact" and executable and manifest_path and message.get("profile", {}).get("test"):
            executables[executable] = os.path.dirname(manifest_path)

if not executables:
    raise SystemExit("test-fast: Cargo produced no test executables")

runnable = sorted(executables.items())

def run(item):
    executable, working_directory = item
    started = time.monotonic()
    result = subprocess.run(
        [executable, "--test-threads=1"],
        cwd=working_directory,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return executable, result.returncode, time.monotonic() - started, result.stdout

failed = False
with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
    futures = [pool.submit(run, executable) for executable in runnable]
    for future in concurrent.futures.as_completed(futures):
        executable, status, elapsed, output = future.result()
        print(f"=== {os.path.basename(executable)} ({elapsed:.2f}s, status={status}) ===", flush=True)
        print(output, end="" if output.endswith("\n") else "\n", flush=True)
        failed |= status != 0

if failed:
    raise SystemExit("test-fast: one or more native test executables failed")
PY

elapsed=$((SECONDS - started))
printf 'PASS full workspace native tests in %ss with %s bounded jobs\n' "$elapsed" "$jobs"
(( elapsed <= 120 )) || {
  printf 'test-fast: %ss exceeds the 120s warm-suite ceiling\n' "$elapsed" >&2
  exit 1
}
