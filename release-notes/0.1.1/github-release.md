# LayerFS 0.1.1 Developer Preview

LayerFS 0.1.1 preserves the 0.1.0 public and storage contracts while improving
large existing-directory initialization, localized small-edit Commit,
Workspace Create, and bounded reads.

Highlights:

- bounded initialization with eight existing producers, four fixed slabs, and
  the calling thread as the sole SQLite admission owner;
- no canonical object-segment spool or parent payload copy on the admitted
  direct path;
- exact operation-local metadata reuse without a persistent cache;
- localized Commit planning for ordinary content-only edits; and
- demand-loaded authenticated Workspace bootstrap objects and bounded
  read-ahead.

Correctness, resource, cleanup, FUSE/materialization equality, managed Docker,
native quality, namespace performance, and registered payload gates pass in
the terminal retained evidence. The workspace version is `0.1.1`.

## Start here

LayerFS 0.1.1 is distributed as source. Build the CLI with Rust 1.85 or newer:

```bash
git clone --branch v0.1.1 --depth 1 https://github.com/Ephemeral-AI-Lab/layerfs.git
cd layerfs
cargo build --release -p layerfs-cli
./target/release/layerfs --version
```

Continue with the
[0.1.1 quickstart](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.1/docs/versioned/0.1.1/quickstart.md)
and review the
[release limitations](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.1/docs/versioned/0.1.1/limitations.md).

Prebuilt executables, crates.io packages, and runtime images are not published
for 0.1.1. The release contains deterministic source archives, `Cargo.lock`,
the MIT license, and `SHA256SUMS`.
