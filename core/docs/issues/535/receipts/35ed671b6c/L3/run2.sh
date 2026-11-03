#!/bin/bash
# L3 step runner: one `layerfs` call per invocation (frozen k-macos CLI),
# logged with UTC start, load1, wall ms and exit code. No retry. The
# password is read from a 0600 file into LFSPW and never printed; output is
# scrubbed of it and of the home path anyway.
# usage: run.sh <log> <layerfs args...>
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l3/paths.env
export LAYERFS_HOME="/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T//lfs535l3-mdb.oBBYax"
export PATH="$HOME/.microsandbox/bin:$PATH"
export LAYERFS_BUNDLE="$B"
export LFSPW="$(cat "$S/pgpw")"
log="$1"; shift
pw="$LFSPW"
{
  printf '$ layerfs'; for a in "$@"; do printf ' %q' "$a"; done; printf '\n'
  printf '# at %s load1 %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(sysctl -n vm.loadavg | awk '{print $2}')"
} | sed "s/$pw/<redacted>/g; s#$H#<home>#g" >> "$log"
t0=$(python3 -c 'import time;print(time.monotonic())')
out=$("$K/bin/layerfs" "$@" 2>&1); rc=$?
t1=$(python3 -c 'import time;print(time.monotonic())')
ms=$(python3 -c "print(round(($t1-$t0)*1000))")
{ printf '%s\n' "$out"; printf 'exit %s  wall_ms %s\n\n' "$rc" "$ms"; } | sed "s/$pw/<redacted>/g; s#$H#<home>#g" | tee -a "$log"
exit $rc
