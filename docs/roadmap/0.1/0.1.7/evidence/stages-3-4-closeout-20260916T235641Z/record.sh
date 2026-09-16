#!/bin/sh
# Records one verification command, its exit code, its wall time and its raw
# combined output. Append-only: a re-run appends a new block, and no failing
# command is ever removed from the log.
#
# usage: record.sh <log-file> <label> <command ...>
set -u
log="$1"
shift
label="$1"
shift
start=$(python3 -c 'import time; print(time.time())')
{
  printf '=== %s | %s | workdir=%s\n' "$label" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(pwd)"
  printf '$ %s\n' "$*"
} >>"$log"
"$@" >>"$log" 2>&1
status=$?
end=$(python3 -c 'import time; print(time.time())')
{
  printf '%s\n' "--- exit $status | wall $(python3 -c "print(f'{$end-$start:.3f}')")s"
} >>"$log"
exit $status
