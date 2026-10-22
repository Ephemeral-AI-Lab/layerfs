#!/bin/sh
# Run the Rust MCP server's real-Store end-to-end sequence.
#
# This is the macOS test harness. Linux runs the server directly; see
# core/crates/layerfs-mcp/README.md.
set -eu

cd "$(dirname "$0")/../../../.."
container="${LAYERFS_CONTAINER:-peri-layerfs-rust}"
binary="${LAYERFS_MCP_BIN:-/work/layerfs/core/target/release/layerfs-mcp}"
test_root="${LAYERFS_E2E_ROOT:-/data/mcp-e2e}"

if ! docker exec "$container" test -x "$binary"; then
    echo "missing $binary in $container; build it first (see core/crates/layerfs-mcp/README.md)" >&2
    exit 1
fi

sequence=$(mktemp "${TMPDIR:-/tmp}/layerfs-mcp-e2e.XXXXXX")
output=$(mktemp "${TMPDIR:-/tmp}/layerfs-mcp-e2e.out.XXXXXX")
trap 'rm -f "$sequence" "$output"' EXIT
sed "s|/data/mcp-e2e|$test_root|g" core/crates/layerfs-mcp/test/rust-e2e.jsonl > "$sequence"

if ! docker exec -i "$container" sh -c "rm -rf '$test_root' && mkdir -p '$test_root/home' && '$binary'" < "$sequence" > "$output"; then
    cat "$output"
    echo "e2e: MCP server failed" >&2
    exit 1
fi
cat "$output"

if grep -Eq '"isError"[[:space:]]*:[[:space:]]*true' "$output"; then
    echo "e2e: a tool call returned isError" >&2
    exit 1
fi
