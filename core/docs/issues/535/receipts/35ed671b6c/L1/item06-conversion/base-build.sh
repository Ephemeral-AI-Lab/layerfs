#!/bin/bash
# L1 item 6: the 2.0 base build (58b8d1742b) in the throwaway worktree layerfs-issue535-l1base.
B=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-issue535-l1base
REG=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue535-l1base-860c8face50e
RES=$REG/resources.json
SEED=/Users/yifanxu/.cache/layerfs-worktrees/layerfs-issue535-kit-6d8fa8af9006/cargo-home-macos
cd $B || exit 1
echo "$(date -u +%T) prepare macos"; python3 tools/worktree.py prepare --resources $RES --platform macos --dependency-seed $SEED 2>&1 | tail -3
echo "$(date -u +%T) prepare linux"; python3 tools/worktree.py prepare --resources $RES --platform linux --dependency-seed $SEED 2>&1 | tail -3
export CARGO_HOME=$REG/cargo-home-macos CARGO_TARGET_DIR=$B/target
echo "$(date -u +%T) macos cli"; perl -e 'alarm 600; exec @ARGV' cargo +1.85.1 build --locked --offline -j 4 -p layerfs-cli --features postgresql --bin layerfs 2>&1 | tail -3; echo "CLI-EXIT ${PIPESTATUS[0]}"
echo "$(date -u +%T) macos daemon"; perl -e 'alarm 600; exec @ARGV' cargo +1.85.1 build --locked --offline -j 4 -p layerfs-daemon --bin layerfs-daemon 2>&1 | tail -3; echo "DAEMON-EXIT ${PIPESTATUS[0]}"
shasum -a 256 $B/target/debug/layerfs $B/target/debug/layerfs-daemon
for s in daemon worker helper; do
  echo "$(date -u +%T) bundle $s"; python3 tools/linux_bundle.py $s --resources $RES --timeout 300 --dest $REG/bundle 2>&1 | tail -3; echo "STEP-EXIT $s ${PIPESTATUS[0]}"
done
echo "$(date -u +%T) seal"; python3 tools/linux_bundle.py seal --resources $RES --timeout 300 --dest $REG/bundle 2>&1 | tail -3; echo "SEAL-EXIT ${PIPESTATUS[0]}"
cat $REG/bundle/2.0.0/linux-aarch64/bundle.json
echo BASE-BUILD-DONE
