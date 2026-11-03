#!/bin/bash
# item 4 (Linux part): platforms vs SUPPORT.json, and macOS-only create refusals
# in the support table's words. As root in lfs535l3x, L3 Linux build's layerfs.
X=$(dirname "$(readlink -f "$0")")
export LAYERFS_HOME=$(mktemp -d /tmp/lfs535l3-i4.XXXXXX); chmod 700 $LAYERFS_HOME
run() { echo "\$ layerfs $*"; echo "# at $(date -u +%FT%TZ)"; "$X/bin/layerfs" "$@" 2>&1; echo "exit $?"; echo; }
echo "# host: $(. /etc/os-release; echo $PRETTY_NAME), kernel $(uname -r), $(uname -m); CLI = L3 Linux build layerfs (build-linux-6eaf9dbb) sha256 $(sha256sum $X/bin/layerfs | cut -c1-16)…"
echo "# user.max_user_namespaces=$(sysctl -n user.max_user_namespaces); docker: $(command -v docker || echo none)"
echo
run machine platforms
run machine create lfs535l3xl lima
run machine create lfs535l3xa apple-container
run machine create lfs535l3xf firecracker --host lfs535l3xl --memory 2G
run machine list
rm -rf "$LAYERFS_HOME"
