#!/bin/sh
# Forward MCP stdio from peri on the host into the Linux Store container.
set -eu

container="${LAYERFS_CONTAINER:-peri-layerfs-rust}"
binary="${LAYERFS_MCP_BIN:-/work/layerfs/core/target/release/layerfs-mcp}"

set -- -i
if [ -n "${LAYERFS_HOME:-}" ]; then
    set -- "$@" -e "LAYERFS_HOME=$LAYERFS_HOME"
fi
if [ -n "${LAYERFS_PROJECT:-}" ]; then
    set -- "$@" -e "LAYERFS_PROJECT=$LAYERFS_PROJECT"
fi
if [ -n "${LAYERFS_STORE_PATH:-}" ]; then
    set -- "$@" -e "LAYERFS_STORE_PATH=$LAYERFS_STORE_PATH"
fi

exec docker exec "$@" "$container" "$binary"
