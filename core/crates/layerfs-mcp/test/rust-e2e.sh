#!/bin/sh
# Run the Rust MCP server's real-Store end-to-end sequence.
set -eu

cd "$(dirname "$0")/../../../.."
container="${LAYERFS_CONTAINER:-peri-layerfs-rust}"
binary="${LAYERFS_MCP_BIN:-/work/layerfs/core/target/release/layerfs-mcp}"

if ! docker exec "$container" test -x "$binary"; then
    echo "missing $binary in $container; build it first (see core/crates/layerfs-mcp/README.md)" >&2
    exit 1
fi

docker exec -i "$container" sh -c "rm -rf /data/home && $binary" < core/crates/layerfs-mcp/test/rust-e2e.jsonl
