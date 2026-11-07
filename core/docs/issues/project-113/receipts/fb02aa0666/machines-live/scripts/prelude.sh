#!/usr/bin/env bash
set -u
export LAYERFS_HOME=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/ml/home
export LAYERFS_BUNDLE=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/bundle/2.0.0/linux-aarch64
REPO=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-issue113
LFS=$REPO/target/release/layerfs
TO=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/w113/timeout.py
export CARGO_HOME=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue113-58962e949af8/cargo-home-macos
step() {  # step NN name secs cmd...  -> $R/NN-name.txt
  local f="$R/$1-$2.txt" s=$3; shift 3
  { printf '$'; printf ' %q' "$@"; echo; } > "$f"
  python3 "$TO" "$s" "$@" >> "$f" 2>&1; local c=$?
  echo "exit $c" >> "$f"; echo "$(basename "$f"): exit $c"; return $c
}
field() {
  python3 -c 'import json,sys
for l in open(sys.argv[1]):
    if l.startswith("{"): print(json.loads(l).get(sys.argv[2],"")); break' "$1" "$2"
}
# bg NN name id ws script: starts `layerfs workspace exec <ws> <script> --exec-id <id>` in its own
# process group, output to $R/NN-name.txt; its pid in $R/NN-name.pid
bg() {
  local f="$R/$1-$2.txt"; local id=$3 ws=$4 script=$5
  echo "\$ $LFS workspace exec $ws '$script' --exec-id $id   (background)" > "$f"
  python3 -c 'import os,sys; os.setpgid(0,0); os.execv(sys.argv[1], sys.argv[1:])' "$LFS" workspace exec "$ws" "$script" --exec-id "$id" >> "$f" 2>&1 &
  echo $! > "$R/$1-$2.pid"; echo "$(basename "$f"): started pid $!"
}
# reap NN name: waits (at most 60 s) for the background exec, records its exit
reap() {
  local f="$R/$1-$2.txt" pid; pid=$(cat "$R/$1-$2.pid")
  for i in $(seq 1 600); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
  if kill -0 "$pid" 2>/dev/null; then kill -KILL -"$pid"; echo "KILLED: did not end in 60 s" >> "$f"; fi
  wait "$pid"; local c=$?; echo "exit $c" >> "$f"; echo "$(basename "$f"): exit $c"
}
# until NN name secs cmd...: runs cmd every 0.2 s until it succeeds with output (at most secs)
until_out() {
  local f="$R/$1-$2.txt" s=$3; shift 3
  { printf '$ (until it prints)'; printf ' %q' "$@"; echo; } > "$f"
  local end=$((SECONDS + s)) out
  while [ $SECONDS -lt $end ]; do
    out=$("$@" 2>&1); if [ $? -eq 0 ] && [ -n "$out" ]; then echo "$out" >> "$f"; echo "exit 0" >> "$f"; echo "$(basename "$f"): exit 0"; return 0; fi
    sleep 0.2
  done
  echo "$out" >> "$f"; echo "exit 1 (gave up after $s s)" >> "$f"; echo "$(basename "$f"): exit 1"; return 1
}
