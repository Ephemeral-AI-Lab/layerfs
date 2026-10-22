#!/bin/sh
# Start the LayerFS MCP server for a stdio client.
#
# Linux is the production environment: run the native binary directly.
# macOS is a test environment: run the same binary in the Linux Store
# container through rust-bridge.sh. Set LAYERFS_MCP_TRANSPORT=direct|docker
# to override the OS choice.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../../.." && pwd)

transport="${LAYERFS_MCP_TRANSPORT:-}"
if [ -z "$transport" ]; then
    case "$(uname -s)" in
        Linux) transport=direct ;;
        Darwin) transport=docker ;;
        *)
            echo "layerfs-mcp: unsupported host $(uname -s); set LAYERFS_MCP_TRANSPORT" >&2
            exit 2
            ;;
    esac
fi

case "$transport" in
    direct)
        binary="${LAYERFS_MCP_BIN:-$root/core/target/release/layerfs-mcp}"
        if [ ! -x "$binary" ]; then
            echo "layerfs-mcp: missing $binary; build it first (see README.md)" >&2
            exit 1
        fi
        exec "$binary"
        ;;
    docker)
        exec "$here/rust-bridge.sh"
        ;;
    *)
        echo "layerfs-mcp: LAYERFS_MCP_TRANSPORT must be direct or docker" >&2
        exit 2
        ;;
esac
