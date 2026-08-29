#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${LAYERFS_BIN_DIR:-${CARGO_HOME:-$HOME/.cargo}/bin}"

case "${1:-}" in
    "") ;;
    --run) ;;
    *) echo "usage: $0 [--run]" >&2; exit 2 ;;
esac

CARGO_TARGET_DIR="$repo_dir/target" cargo build \
    --locked \
    --manifest-path "$repo_dir/Cargo.toml" \
    -p layerfs-tui

install -d "$bin_dir"
install -m 0755 "$repo_dir/target/debug/layerfs-tui" "$bin_dir/layerfs-tui"
echo "installed $bin_dir/layerfs-tui"

if [[ ":$PATH:" != *":$bin_dir:"* ]]; then
    echo "add $bin_dir to PATH to run: layerfs-tui"
fi

if [[ "${1:-}" == "--run" ]]; then
    exec "$bin_dir/layerfs-tui"
fi
