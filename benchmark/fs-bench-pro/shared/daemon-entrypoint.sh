#!/bin/sh
set -eu

umask 077
install -d -m 0700 /run/layerfs
if ! test -f /run/layerfs/capability; then
  dd if=/dev/urandom of=/run/layerfs/capability bs=32 count=1 status=none
fi
test "$(wc -c </run/layerfs/capability)" -eq 32
chmod 0600 /run/layerfs/capability
exec /usr/local/bin/layerfs-daemon
