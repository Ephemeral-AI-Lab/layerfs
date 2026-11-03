#!/bin/bash
# usage: run.sh <layerfs args...> — one logged CLI call for the machine.remove related check
S=/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/mr
export LAYERFS_HOME=$S/home LAYERFS_BUNDLE=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue535-connect-7500a2864564/builds/k3-bundle/2.0.0/linux-aarch64 PATH=$HOME/.microsandbox/bin:$PATH
{ echo "\$ layerfs $*"; echo "# at $(date -u +%FT%TZ)"; t0=$(python3 -c 'import time;print(int(time.time()*1000))')
  $S/layerfs "$@" 2>&1; r=$?; echo "exit $r  wall_ms $(( $(python3 -c 'import time;print(int(time.time()*1000))') - t0 ))"; echo; } | tee -a $S/check.log
