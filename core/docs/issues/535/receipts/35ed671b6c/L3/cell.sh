#!/bin/bash
# L3 Mac cell runner: one cell per invocation through the frozen k-macos Node
# package, in the shared home that holds the VM machines. LAYERFS_TRACE
# collects the worker-connection trace of the cell next to its receipt.
# usage: cell.sh <Cnn> <machine> <platform> <store: sqlite|pg-loop|pg-remote> <db prefix>
. /private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/27d9a7a0-17b1-4ee7-a64b-834245099cd7/scratchpad/l3/paths.env
cell=$1 machine=$2 platform=$3 store=$4 prefix=$5
export PATH="$HOME/.microsandbox/bin:$PATH" LAYERFS_BUNDLE="$B" LFS533_HOME="$H" LFS533_PRODUCT=35ed671b6c LFS533_LAYERFS="$K/bin/layerfs"
export LAYERFS_TRACE="$R/trace/$platform-$store-$cell.jsonl"
mkdir -p "$R/trace"
unset LFS535_PG_URL LFS535_PG_PASSWORD
case $store in
  sqlite) ;;
  pg-loop) export LFS535_PG_URL="postgres://postgres@127.0.0.1:55553/${prefix}{cell}" LFS535_PG_PASSWORD=env:LFSPW LFSPW="$(cat $S/pgpw)";;
  pg-remote) export LFS535_PG_URL="postgres://postgres@192.168.2.123:55553/${prefix}{cell}" LFS535_PG_PASSWORD=env:LFSPW LFSPW="$(cat $S/pgpw)";;
esac
cd "$W"
perl -e 'alarm 115; exec @ARGV' node "$K/node/test/verify/cells/cell.mjs" "$cell" "$machine" "$R/$platform-$store-$cell.json"
echo "exit $?"
