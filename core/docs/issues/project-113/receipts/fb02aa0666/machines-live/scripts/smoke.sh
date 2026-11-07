#!/usr/bin/env bash
# smoke.sh <machine> <prefix>: the release-CLI smoke on a created, started VM machine
source /Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/prelude.sh
M=$1; P=$2
R=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/r/$P-smoke; rm -rf "$R"; mkdir -p "$R"
cd "$REPO"
ID8=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"][:8])' "$LAYERFS_HOME/layerfs.json")
VM="lfs-$ID8-$M"
case "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("platform",""))' "$LAYERFS_HOME/machine/$M/record.json")" in
  lima) GX=(limactl shell --workdir / "$VM" sudo sh -c) ;;
  apple-container) GX=(container exec "$VM" sh -c) ;;
esac
T=$(mktemp -d /tmp/lfsm.XXXXXX); SRC=$T/src; mkdir -p "$SRC"; printf 'hello\n' > "$SRC/readme.txt"
S=smk$P; SB=sb$P
echo "T=$T LAYERFS_HOME=$LAYERFS_HOME VM=$VM head=$(git rev-parse --short HEAD) bundle=$(head -c 200 $LAYERFS_BUNDLE/bundle.json)" > "$R/00-env.txt"
step 01 store-create 120 "$LFS" store create $S
step 02 store-push 120 "$LFS" store push $S $M
step 03 project-init 120 "$LFS" project init $S/app "$SRC" --timeout 60000
step 04 sandbox-create 120 "$LFS" sandbox create $S/app $SB $M
step 05 branch-fork 120 "$LFS" branch fork $S/app work main
step 06 mount 120 "$LFS" workspace mount $SB work
WS=$(field "$R/06-mount.txt" ws); echo "ws=$WS" >> "$R/06-mount.txt"
step 07 exec 120 "$LFS" workspace exec "$WS" 'echo hi > f && cat f'
g() { "${GX[@]}" "$1"; }
cg() { g "d=\$(find /sys/fs/cgroup -type d -name 'exec-$1' 2>/dev/null); [ -n \"\$d\" ] && grep -q . \"\$d/cgroup.procs\" && echo \"\$d\" && echo procs: \$(cat \"\$d/cgroup.procs\") && cat /proc/\$(head -1 \"\$d/cgroup.procs\")/cmdline | tr '\\0' ' '"; }
SCAN='for p in /proc/[0-9]*; do c=$(tr "\0" " " < $p/cmdline 2>/dev/null); case "$c" in "sleep 600"*) echo "LEFT $p $c";; esac; done; echo scan-done'
GONE='echo exec cgroups:; find /sys/fs/cgroup -type d -name "exec-*"; echo ws cgroups:; find /sys/fs/cgroup -type d -name "ws-*"; echo find-done'
bg 08a exec-long "long-1" "$WS" 'sleep 600 & sleep 600'
until_out 08b exec-cgroup 60 cg long-1
step 08c exec-duplicate 120 "$LFS" workspace exec "$WS" true --exec-id long-1
step 08d unmount-busy 120 "$LFS" workspace unmount "$WS"
step 08e close-busy 120 "$LFS" sandbox close $SB
step 08f exec-cancel 120 "$LFS" workspace exec-cancel "$WS" long-1 --grace-ms 5000
reap 08a exec-long
step 08g cgroup-gone 120 "${GX[@]}" "$GONE"
step 08h sleeps-left 120 "${GX[@]}" "$SCAN"
step 08i exec-cancel-again 120 "$LFS" workspace exec-cancel "$WS" long-1
F="$R/08j-exec-generated.txt"; echo "\$ $LFS workspace exec $WS 'sleep 600' --progress   (background; stderr in 08j-exec-generated.err)" > "$F"
python3 -c 'import os,sys; os.setpgid(0,0); os.execv(sys.argv[1], sys.argv[1:])' "$LFS" workspace exec "$WS" 'sleep 600' --progress >> "$F" 2> "$R/08j-exec-generated.err" &
echo $! > "$R/08j-exec-generated.pid"
until_out 08k progress-line 60 head -1 "$R/08j-exec-generated.err"
GID=$(python3 -c 'import json,sys; print(json.loads(open(sys.argv[1]).readline())["progress"]["exec"]["id"])' "$R/08j-exec-generated.err")
echo "generated id: $GID" >> "$R/08k-progress-line.txt"
until_out 08l generated-cgroup 60 cg "$GID"
step 08m exec-cancel-generated 120 "$LFS" workspace exec-cancel "$WS" "$GID" --grace-ms 0
reap 08j exec-generated
step 08n cgroup-gone-generated 120 "${GX[@]}" "$GONE"
step 09 commit 120 "$LFS" workspace commit "$WS" -m "smoke $M"
step 10 unmount 120 "$LFS" workspace unmount "$WS"
step 11 file-read 120 "$LFS" file read $S/app work /f
step 12a mount2 120 "$LFS" workspace mount $SB work
WS2=$(field "$R/12a-mount2.txt" ws); echo "ws=$WS2" >> "$R/12a-mount2.txt"
bg 12b exec-long2 "long-2" "$WS2" 'sleep 600 & sleep 600'
until_out 12c exec-cgroup2 60 cg long-2
step 12d unmount-force 120 "$LFS" workspace unmount "$WS2" --force
reap 12b exec-long2
step 12e cgroup-gone2 120 "${GX[@]}" "$GONE"
step 12f sleeps-left2 120 "${GX[@]}" "$SCAN"
step 13a mount3 120 "$LFS" workspace mount $SB work
WS3=$(field "$R/13a-mount3.txt" ws); echo "ws=$WS3" >> "$R/13a-mount3.txt"
bg 13b exec-long3 "long-3" "$WS3" 'sleep 600 & sleep 600'
until_out 13c exec-cgroup3 60 cg long-3
step 13d close-force 120 "$LFS" sandbox close $SB --force
reap 13b exec-long3
step 13e sleeps-left3 120 "${GX[@]}" "$SCAN"
step 13f cgroup-gone3 120 "${GX[@]}" "$GONE"
step 14a sandbox-create2 120 "$LFS" sandbox create $S/app ${SB}b $M
step 14b sandbox-close2 120 "$LFS" sandbox close ${SB}b
step 15 sandbox-list 120 "$LFS" sandbox list
rm -rf "$T"; echo "removed $T" > "$R/99-rm-temp.txt"
