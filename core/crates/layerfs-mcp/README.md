# layerfs-mcp

Native MCP stdio server for peri. It opens a sealed LayerFS Store through
`layerfs-api` and exposes the Store tools; Node and the JavaScript SDK are not
in the loop.

Scope: `project_init`, `project_list_branches`, branch fork/merge/publish,
`fs` read/list/stat/write/apply/rm/mkdir/mv/cp/edit/glob/grep, and `store_open`.
The `bash` tool runs `/bin/sh -c` inside the Store container; it is not
LayerFS-backed, so command changes do not create a branch commit.
Workspace `mount`/`exec` are not included: they need the Linux Sandbox daemon
and FUSE.

## Build

Store calls require Linux. On aarch64 the crate requires the ARMv8 AEAD flags:

```sh
cd core
export RUSTFLAGS="--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2"
cargo build -p layerfs-mcp --release
```

The binary is `core/target/release/layerfs-mcp`.

## Store location

The Store installation requires a filesystem that carries SQLite locks
between kernels: ext4, xfs or btrfs. The container's overlayfs and `/tmp` are
refused by the kernel guard. Use a Docker named volume, for example `/data`.

```sh
docker run -d --name peri-layerfs-rust \
  -v "$PWD:/work/layerfs" \
  -v peri-layerfs-store:/data \
  -w /work/layerfs \
  rust:1.85-bookworm sleep infinity
```

## Run

Run peri inside the container, or point peri at the wrapper
`core/crates/layerfs-mcp/scripts/rust-bridge.sh`, which forwards stdio through
`docker exec`:

```json
{
  "mcpServers": {
    "layerfs-store": {
      "type": "stdio",
      "command": "./core/crates/layerfs-mcp/scripts/rust-bridge.sh",
      "env": {
        "LAYERFS_HOME": "/data/home",
        "LAYERFS_PROJECT": "rust-demo",
        "LAYERFS_STORE_PATH": "/data/home/rust-demo.sqlite"
      }
    }
  }
}
```

`LAYERFS_PROJECT` and `LAYERFS_STORE_PATH` make the server open an existing
project record at startup. Without them, call `project_init` first.

## Test

With the container from above and the release binary built:

```sh
sh core/crates/layerfs-mcp/test/rust-e2e.sh
```

The script runs `project_init` → `branch_fork` → `fs_write` → `fs_read` →
`branch_merge` → `fs_list` → `project_list_branches` against a real Store in
`/data/home`.
