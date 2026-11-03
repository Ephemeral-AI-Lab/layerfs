#!/bin/bash
# lfs535 live-reach step runner: one layerfs call per invocation, logged with
# UTC start time, wall ms and exit code. No retry. The password is read from
# a 0600 file into LFSPW and never printed; output is scrubbed of it anyway.
# usage: run.sh <log> <layerfs args...>
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/reach
. "$S/paths.env"
export LAYERFS_HOME="$H"
export PATH="$HOME/.microsandbox/bin:$PATH"
export LAYERFS_BUNDLE=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue535-verify-417967c19d8d/builds/lfs535-bundle/2.0.0/linux-aarch64
export LFSPW="$(cat "$S/pgpw")"
CLI="$S/bin/layerfs"
log="$1"; shift
pw="$LFSPW"
{
  printf '$ layerfs'; for a in "$@"; do printf ' %q' "$a"; done; printf '\n'
  printf '# at %s load1 %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(sysctl -n vm.loadavg | awk '{print $2}')"
} | sed "s/$pw/<redacted>/g; s#$H#<home>#g; s#$SRC#<src>#g" >> "$log"
t0=$(python3 -c 'import time;print(time.monotonic())')
out=$("$CLI" "$@" 2>&1); rc=$?
t1=$(python3 -c 'import time;print(time.monotonic())')
ms=$(python3 -c "print(round(($t1-$t0)*1000))")
{ printf '%s\n' "$out"; printf 'exit %s  wall_ms %s\n\n' "$rc" "$ms"; } | sed "s/$pw/<redacted>/g; s#$H#<home>#g; s#$SRC#<src>#g" | tee -a "$log"
exit $rc
