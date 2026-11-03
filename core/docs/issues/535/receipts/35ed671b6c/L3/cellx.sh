#!/bin/bash
# L3 Linux cell runner, as root inside lfs535l3x: one cell per invocation with
# the sealed bundle's daemon/CLI and the Node package + addon built for Linux
# aarch64 at 35ed671b6c (build-linux-6eaf9dbb). Without LFS533_HOME in the
# environment each cell makes its own home; the docker row passes the shared
# home whose docker machine was started by hand (#535 D11).
# usage: cellx.sh <Cnn> <native|docker> <store: sqlite|pg-remote> <db prefix>
X=$(dirname "$(readlink -f "$0")")
B=/root/.cache/layerfs/bundle/2.0.0/linux-aarch64
cell=$1 machine=$2 store=$3 prefix=$4
export PATH="$X:$PATH" LFS533_PRODUCT=35ed671b6c LFS533_DAEMON=$B/layerfs-daemon LFS533_LAYERFS=$B/layerfs
[ "$machine" = docker ] && export LFS533_IMAGE="$(cat /root/sandbox-image.id)"
mkdir -p "$X/receipts/trace"
export LAYERFS_TRACE="$X/receipts/trace/linux-$machine-$store-$cell.jsonl"
unset LFS535_PG_URL LFS535_PG_PASSWORD
if [ "$store" = pg-remote ]; then
  export LFS535_PG_URL="postgres://postgres@192.168.2.123:55553/${prefix}{cell}" LFS535_PG_PASSWORD=env:LFSPW LFSPW="$(cat /root/pgpw)"
fi
cd "$X/sdk/api/node"
timeout 115 "$X/node" test/verify/cells/cell.mjs "$cell" "$machine" "$X/receipts/linux-$machine-$store-$cell.json"
echo "exit $?"
