#!/usr/bin/env bash
# gap3b.sh: after gap3.sh: the VM back, the kept record settled; close --force of a stale one
source /Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/prelude.sh
R=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/r/l-gap3
cd "$REPO"
ID8=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["id"][:8])' "$LAYERFS_HOME/layerfs.json")
VM="lfs-$ID8-lm"
GX=(limactl shell --workdir / "$VM" sudo sh -c)
WS3=$(field "$R/40x-mount3.txt" ws); WS2=$(field "$R/30-mount2.txt" ws)
step 60 machine-start 300 "$LFS" machine start lm
step 61 sandbox-list 120 "$LFS" sandbox list
step 62 exec-cancel 120 "$LFS" workspace exec-cancel "$WS3" stale-5
step 63 record-gone 120 sh -c "ls $LAYERFS_HOME/sandbox/sg2/workspace/*/exec/ 2>&1; echo listed"
step 64 log-tail 120 tail -1 $LAYERFS_HOME/log/exec.jsonl
