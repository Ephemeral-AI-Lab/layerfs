#!/usr/bin/env bash
# gap3c.sh: close the Sandboxes the reboot left; sandbox close --force ends a stale command first
source /Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/prelude.sh
R=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/r/l-gap3
cd "$REPO"
ID8=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"][:8])' "$LAYERFS_HOME/layerfs.json")
VM="lfs-$ID8-lm"
GX=(limactl shell --workdir / "$VM" sudo sh -c)
g() { "${GX[@]}" "$1"; }
cg() { g "d=\$(find /sys/fs/cgroup -type d -name 'exec-$1' 2>/dev/null); [ -n \"\$d\" ] && grep -q . \"\$d/cgroup.procs\" && echo \"\$d\" && echo procs: \$(cat \"\$d/cgroup.procs\")"; }
SCAN='for p in /proc/[0-9]*; do c=$(tr "\0" " " < $p/cmdline 2>/dev/null); case "$c" in "sleep 600"*) echo "LEFT $p $c";; esac; done; echo scan-done'
GONE='echo exec cgroups:; find /sys/fs/cgroup -type d -name "exec-*"; echo find-done'
step 67 close-sg-force 120 "$LFS" sandbox close sg --force
step 68 close-sg2-force 120 "$LFS" sandbox close sg2 --force
step 70 sandbox-create3 120 "$LFS" sandbox create smkl/app sg3 lm
step 71 mount 120 "$LFS" workspace mount sg3 work
WS=$(field "$R/71-mount.txt" ws)
bg 72a exec-closeforce stale-6 "$WS" 'sleep 600 & sleep 600'
until_out 72b cgroup 60 cg stale-6
until_out 72c record 30 sh -c "cat \$(ls $LAYERFS_HOME/sandbox/sg3/workspace/*/exec/stale-6.json)"
pid=$(cat "$R/72a-exec-closeforce.pid"); kill -KILL -"$pid"; wait "$pid" 2>/dev/null; echo "owner CLI pid $pid killed with SIGKILL" > "$R/72d-killed.txt"
sleep 1; until_out 72e still-running 10 cg stale-6
step 73 close-force 120 "$LFS" sandbox close sg3 --force
step 74 cgroup-gone 120 "${GX[@]}" "$GONE"
step 75 sleeps-left 120 "${GX[@]}" "$SCAN"
step 76 log-tail 120 tail -1 $LAYERFS_HOME/log/exec.jsonl
step 77 sandbox-list 120 "$LFS" sandbox list
