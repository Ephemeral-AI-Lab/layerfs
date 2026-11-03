#!/bin/bash
# L1 (copied from K1) cell series: one cell per runner invocation, one after another, each under
# a 115 s alarm. usage: series.sh <machine> <store: sqlite|pg-loop|pg-remote> <log> <cells...>
# Env (from the lane's env file): K (frozen macOS builds), R (receipt folder),
# LAYERFS_BUNDLE, LFS533_DAEMON, LFS533_LAYERFS, LFS533_PRODUCT; optional
# LFS533_HOME (shared home), LFS535_PG_URL / LFS535_PG_PASSWORD for PostgreSQL.
machine=$1; store=$2; log=$3; shift 3
for c in "$@"; do
  echo "$(date -u +%FT%TZ) start $machine $store $c" >> "$log"
  perl -e 'alarm 115; exec @ARGV' node "$K/node/test/verify/cells/cell.mjs" "$c" "$machine" "$R/$machine-$store-$c.json" >> "$log" 2>&1
  rc=$?  # (until this fix the logged rc was the date substitution's, always 0; receipts carry the verdict)
  echo "$(date -u +%FT%TZ) end $machine $store $c rc=$rc" >> "$log"
done
echo "SERIES DONE $machine $store" >> "$log"
