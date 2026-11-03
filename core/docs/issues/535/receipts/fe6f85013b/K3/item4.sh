#!/bin/bash
# item 4 (Linux part): platforms vs SUPPORT.json, and macOS-only create refusals. As root in lfs535k3x.
X=$(dirname "$(readlink -f "$0")")
export LAYERFS_HOME=$(mktemp -d /tmp/lfs535k3-i4.XXXXXX); chmod 700 $LAYERFS_HOME
run() { echo "\$ layerfs $*"; echo "# at $(date -u +%FT%TZ)"; "$X/bin/layerfs" "$@" 2>&1; echo "exit $?"; echo; }
echo "# host: $(. /etc/os-release; echo $PRETTY_NAME), kernel $(uname -r), $(uname -m); CLI = K3 Linux build layerfs (build-linux-59fd4d40) sha256 $(sha256sum $X/bin/layerfs | cut -c1-16)…"
echo "# user.max_user_namespaces=$(sysctl -n user.max_user_namespaces); docker: $(command -v docker || echo none)"
echo
run machine platforms
run machine create lfs535k3xl lima
run machine create lfs535k3xa apple-container
run machine create lfs535k3xf firecracker --host lfs535k3xl --memory 2G
run machine list
rm -rf "$LAYERFS_HOME"
