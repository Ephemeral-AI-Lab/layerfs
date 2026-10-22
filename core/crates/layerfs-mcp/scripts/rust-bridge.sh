#!/bin/sh
# macOS test transport: forward MCP stdio from peri on the host into the
# Linux Store container. Linux runs the server directly (see layerfs-mcp.sh).
set -eu

container="${LAYERFS_CONTAINER:-peri-layerfs-rust}"
binary="${LAYERFS_MCP_BIN:-/work/layerfs/core/target/release/layerfs-mcp}"
cwd="${LAYERFS_MCP_CWD:-/work/layerfs}"

set -- -i -w "$cwd" -e "LAYERFS_MCP_CWD=$cwd"
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
