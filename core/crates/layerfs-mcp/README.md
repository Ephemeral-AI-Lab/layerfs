# layerfs-mcp

Native MCP stdio server for peri. It opens a sealed LayerFS Store through
`layerfs-api` and exposes the Store tools; Node and the JavaScript SDK are not
in the loop.

**Linux is the runtime.** The server is an ordinary Linux stdio process that
opens the Store directly. There is no daemon, network listener or container in
the production path. macOS is a test environment: Docker supplies the Linux
kernel and Store filesystem, and a bridge forwards stdio.

Scope: `project_init`, `project_list_branches`, branch fork/merge/publish,
`fs` read/list/stat/write/apply/rm/mkdir/mv/cp/edit/glob/grep, and `store_open`.
The `bash` tool runs `/bin/sh -c` in the Store environment; it is not
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
between kernels: ext4, xfs or btrfs. overlayfs, tmpfs and network filesystems
are refused by the kernel guard. On a Linux host, put `LAYERFS_HOME` on a local
ext4/xfs/btrfs directory. The macOS test container uses a Docker named volume
(for example `/data`); the container's own overlayfs and `/tmp` are not usable.

## Run on Linux (production)

Build the binary, then point the MCP client at `scripts/layerfs-mcp.sh`. On
Linux the launcher runs the native binary directly:

```json
{
  "mcpServers": {
    "layerfs": {
      "type": "stdio",
      "command": "/path/to/layerfs/core/crates/layerfs-mcp/scripts/layerfs-mcp.sh",
      "env": {
        "LAYERFS_HOME": "/var/lib/layerfs/home",
        "LAYERFS_PROJECT": "rust-demo",
        "LAYERFS_STORE_PATH": "/var/lib/layerfs/home/rust-demo.sqlite",
        "LAYERFS_MCP_CWD": "/path/to/workspace"
      }
    }
  }
}
```

`LAYERFS_PROJECT` and `LAYERFS_STORE_PATH` make the server open an existing
project record at startup. Without them, call `project_init` first.
`LAYERFS_MCP_CWD` is the default `cwd` for the `bash` tool; without it the
server uses its own working directory.

Pointing the client directly at `core/target/release/layerfs-mcp` is equivalent.
`LAYERFS_MCP_BIN` overrides the binary the launcher starts.

## Test on macOS (Docker)

Docker is the macOS test harness, not the production path. Start a Linux
container with a named volume:

```sh
docker run -d --name peri-layerfs-rust \
  -v "$PWD:/work/layerfs" \
  -v peri-layerfs-store:/data \
  -w /work/layerfs \
  rust:1.85-bookworm sleep infinity
```

Point the client at the same launcher. On Darwin it forwards stdio through
`rust-bridge.sh`:

```json
{
  "mcpServers": {
    "layerfs": {
      "type": "stdio",
      "command": "/path/to/layerfs/core/crates/layerfs-mcp/scripts/layerfs-mcp.sh",
      "env": {
        "LAYERFS_HOME": "/data/home",
        "LAYERFS_PROJECT": "rust-demo",
        "LAYERFS_STORE_PATH": "/data/home/rust-demo.sqlite"
      }
    }
  }
}
```

`LAYERFS_MCP_TRANSPORT=direct|docker` overrides the OS choice. The Docker
bridge sets `LAYERFS_MCP_CWD=/work/layerfs` by default; set it to another
container path if the workspace lives elsewhere. `bash` commands run in the
container on macOS, but on the host with the MCP user's privileges on Linux.

## Test

With the macOS test container above and the release binary built:

```sh
sh core/crates/layerfs-mcp/test/rust-e2e.sh
```

The script runs `project_init` → `branch_fork` → `fs_write` → `fs_read` →
`branch_merge` → `fs_list` → `project_list_branches` and the mutation tools
against a real Store under `/data/mcp-e2e`. It removes and recreates only that
test directory; it does not touch `/data/home`.
