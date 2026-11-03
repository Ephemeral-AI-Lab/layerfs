#!/bin/bash
# R3 Linux cell runner (from L3's cellx.sh), as root inside lfs535r3x: one cell
# per invocation with the sealed bundle's daemon/CLI (185b0240…, 54161c9e2d) and
# the Node package + addon built for Linux aarch64 at 54161c9e2d
# (build-linux-c4b95c16). The docker row passes LFS533_HOME, the shared home
# whose docker machine was started by hand (#535 D11).
# usage: cellx.sh <Cnn> docker <store: sqlite|pg-remote> <db prefix>
X=$(dirname "$(readlink -f "$0")")
B=/root/.cache/layerfs/bundle/2.0.0/linux-aarch64
cell=$1 machine=$2 store=$3 prefix=$4
export PATH="$X:$PATH" LFS533_PRODUCT=54161c9e2d LFS533_DAEMON=$B/layerfs-daemon LFS533_LAYERFS=$B/layerfs
[ "$machine" = docker ] && export LFS533_IMAGE="$(cat /root/sandbox-image.id)"
mkdir -p "$X/receipts/trace"
export LAYERFS_TRACE="$X/receipts/trace/linux-$machine-$store-$cell.jsonl"
unset LFS535_PG_URL LFS535_PG_PASSWORD
if [ "$store" = pg-remote ]; then
  export LFS535_PG_URL="postgres://postgres@192.168.2.123:55563/${prefix}{cell}" LFS535_PG_PASSWORD=env:LFSPW LFSPW="$(cat /root/pgpw)"
fi
cd "$X/sdk/api/node"
timeout 115 "$X/node" test/verify/cells/cell.mjs "$cell" "$machine" "$X/receipts/linux-$machine-$store-$cell.json"
echo "exit $?"
