#!/bin/bash
# R2 step runner (copied from L2): one `layerfs` call per invocation, logged with UTC start,
# load1, wall ms and exit code. No retry. Home and scratch paths scrubbed.
# usage: run.sh <log> <layerfs args...>
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/r2/env.sh
log="$1"; shift
scrub() { sed "s#$H#<home>#g; s#$S#<scratch>#g"; }
{
  printf '$ layerfs'; for a in "$@"; do printf ' %q' "$a"; done; printf '\n'
  printf '# at %s load1 %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(sysctl -n vm.loadavg | awk '{print $2}')"
} | scrub >> "$log"
t0=$(python3 -c 'import time;print(time.monotonic())')
out=$("$K/bin/layerfs" "$@" 2>&1); rc=$?
t1=$(python3 -c 'import time;print(time.monotonic())')
ms=$(python3 -c "print(round(($t1-$t0)*1000))")
{ printf '%s\n' "$out"; printf 'exit %s  wall_ms %s\n\n' "$rc" "$ms"; } | scrub | tee -a "$log"
exit $rc
