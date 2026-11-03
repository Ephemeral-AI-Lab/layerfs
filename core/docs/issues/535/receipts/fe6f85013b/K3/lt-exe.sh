#!/bin/bash
# usage: runexe.sh <mode:plain|fuse> <package-dir rel> <exe rel to target> [extra docker args...] -- [test args...]
# Runs one prebuilt Linux test executable in a fresh container with a fresh
# ext4-backed /tmp volume, under a 120 s wall limit; removes both afterwards.
mode=$1; pkg=$2; exe=$3; shift 3
extra=(); while [ $# -gt 0 ] && [ "$1" != "--" ]; do extra+=("$1"); shift; done; shift
W=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-issue535-verify
ID=layerfs-issue535-verify-417967c19d8d
vol=lfs-verify-tmp-$$-$RANDOM
docker volume create --label layerfs.verify.scratch=$ID $vol >/dev/null
caps=(); [ "$mode" = fuse ] && caps=(--device /dev/fuse --cap-add SYS_ADMIN --security-opt apparmor=unconfined)
s=$(date +%s)
docker run --rm --pull never --label layerfs.verify.scratch=$ID \
  --mount type=bind,src=$W,dst=/work,readonly \
  --mount type=volume,src=lfs-dev-$ID-target,dst=/work/target,readonly \
  --mount type=volume,src=$vol,dst=/tmp \
  "${caps[@]}" "${extra[@]}" -w /work/$pkg --entrypoint /bin/sh \
  sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6 \
  -c 'chmod 1777 /tmp; exec timeout 115 "$@"' sh /work/target/$exe "$@"
rc=$?
echo "### rc=$rc wall=$(( $(date +%s)-s ))s"
docker volume rm $vol >/dev/null
exit $rc
