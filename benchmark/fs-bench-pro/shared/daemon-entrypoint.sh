#!/bin/sh
set -eu

umask 077
install -d -m 0700 /run/layerfs
if [ "${LAYERFS_BENCH_LOCAL_RUNTIME:-0}" = 1 ]; then
  test "${LAYERFS_DAEMON_TCP_LISTEN:-}" = "127.0.0.1:41273"
  test "${LAYERFS_FUSE_HOST:-}" = "127.0.0.1"
fi
if ! test -f /run/layerfs/capability; then
  dd if=/dev/urandom of=/run/layerfs/capability bs=32 count=1 status=none
fi
test "$(wc -c </run/layerfs/capability)" -eq 32
chmod 0600 /run/layerfs/capability
exec /usr/local/bin/layerfs-daemon
