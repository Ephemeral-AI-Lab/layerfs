#!/usr/bin/env bash
# gap3.sh: stale forwarded exec records on the lima machine `lm`, live
source /Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/prelude.sh
R=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/r/l-gap3; rm -rf "$R"; mkdir -p "$R"
cd "$REPO"
ID8=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"][:8])' "$LAYERFS_HOME/layerfs.json")
VM="lfs-$ID8-lm"
GX=(limactl shell --workdir / "$VM" sudo sh -c)
g() { "${GX[@]}" "$1"; }
cg() { g "d=\$(find /sys/fs/cgroup -type d -name 'exec-$1' 2>/dev/null); [ -n \"\$d\" ] && grep -q . \"\$d/cgroup.procs\" && echo \"\$d\" && echo procs: \$(cat \"\$d/cgroup.procs\")"; }
SCAN='for p in /proc/[0-9]*; do c=$(tr "\0" " " < $p/cmdline 2>/dev/null); case "$c" in "sleep 600"*) echo "LEFT $p $c";; esac; done; echo scan-done'
GONE='echo exec cgroups:; find /sys/fs/cgroup -type d -name "exec-*"; echo find-done'
rec() { ls $LAYERFS_HOME/sandbox/$1/workspace/*/exec/$2.json 2>/dev/null; }
# stale NN name id ws sandbox: a background exec whose owning CLI is then killed (SIGKILL);
# its command runs on in the worker, its home record (route machine:lm) is left stale
stale() {
  bg $1a exec-$2 "$3" "$4" 'sleep 600 & sleep 600'
  until_out $1b cgroup-$2 60 cg "$3"
  until_out $1c record-$2 30 sh -c "cat \$(ls $LAYERFS_HOME/sandbox/$5/workspace/*/exec/$3.json)"
  local pid; pid=$(cat "$R/$1a-exec-$2.pid")
  kill -KILL -"$pid"; wait "$pid" 2>/dev/null
  echo "owner CLI pid $pid killed with SIGKILL" > "$R/$1d-killed-$2.txt"
  sleep 1; until_out $1e still-running-$2 10 cg "$3"
}
step 01 sandbox-create 120 "$LFS" sandbox create smkl/app sg lm
step 01b sandbox-record 120 cat $LAYERFS_HOME/sandbox/sg/record.json
step 02 mount 120 "$LFS" workspace mount sg work
WS=$(field "$R/02-mount.txt" ws)
# A: workspace.exec_cancel ends a stale one in the worker
stale 10 cancel stale-1 "$WS" sg
step 10f exec-cancel 120 "$LFS" workspace exec-cancel "$WS" stale-1
step 10g record-gone 120 sh -c "ls $LAYERFS_HOME/sandbox/sg/workspace/*/exec/ 2>&1; echo listed"
step 10h cgroup-gone 120 "${GX[@]}" "$GONE"
step 10i sleeps-left 120 "${GX[@]}" "$SCAN"
step 10j log 120 cat $LAYERFS_HOME/log/exec.jsonl
# B: workspace.unmount ends one first
stale 20 unmount stale-2 "$WS" sg
step 20f unmount 120 "$LFS" workspace unmount "$WS"
step 20h cgroup-gone 120 "${GX[@]}" "$GONE"
step 20i sleeps-left 120 "${GX[@]}" "$SCAN"
step 20j log 120 cat $LAYERFS_HOME/log/exec.jsonl
# C: sandbox.close ends one first
step 30 mount2 120 "$LFS" workspace mount sg work
WS2=$(field "$R/30-mount2.txt" ws)
stale 30 close stale-3 "$WS2" sg
step 30f close 120 "$LFS" sandbox close sg
step 30h cgroup-gone 120 "${GX[@]}" "$GONE"
step 30i sleeps-left 120 "${GX[@]}" "$SCAN"
step 30j log 120 cat $LAYERFS_HOME/log/exec.jsonl
# D: a workspace.exec of the same id ends it first, then runs
step 40 sandbox-create2 120 "$LFS" sandbox create smkl/app sg2 lm
step 40x mount3 120 "$LFS" workspace mount sg2 work
WS3=$(field "$R/40x-mount3.txt" ws)
stale 41 reuse stale-4 "$WS3" sg2
step 41f exec-same-id 120 "$LFS" workspace exec "$WS3" 'echo again' --exec-id stale-4
step 41h cgroup-gone 120 "${GX[@]}" "$GONE"
step 41i sleeps-left 120 "${GX[@]}" "$SCAN"
step 41j log 120 cat $LAYERFS_HOME/log/exec.jsonl
# E: the worker unreachable (its VM stopped outside LayerFS): the record is kept
stale 50 unreached stale-5 "$WS3" sg2
step 50s record-before 120 sh -c "cat $LAYERFS_HOME/sandbox/sg2/workspace/*/exec/stale-5.json"
step 50v vm-stop 300 limactl stop "$VM"
step 51 exec-cancel 120 "$LFS" workspace exec-cancel "$WS3" stale-5
step 52 unmount 120 "$LFS" workspace unmount "$WS3"
step 53 unmount-force 120 "$LFS" workspace unmount "$WS3" --force
step 54 close 120 "$LFS" sandbox close sg2
step 55 close-force 120 "$LFS" sandbox close sg2 --force
step 56 exec-same-id 120 "$LFS" workspace exec "$WS3" true --exec-id stale-5
step 57 record-after 120 sh -c "cat $LAYERFS_HOME/sandbox/sg2/workspace/*/exec/stale-5.json"
step 58 log 120 cat $LAYERFS_HOME/log/exec.jsonl
