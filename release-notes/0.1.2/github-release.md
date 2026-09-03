# LayerFS 0.1.2 Developer Preview

LayerFS 0.1.2 preserves the v0.1.1 storage, CLI, daemon, projection, and
Workspace lifecycle contracts while adding universal regular-file range
editing.

Highlights:

- one failure-atomic implicit piece engine for ordinary FUSE write, append,
  sparse growth, truncate, and owner-side range replacement;
- singular and same-file batched `Client` APIs using inline bytes or logical
  zero replacement;
- localized Commit emission with no old-payload copy for admitted edits;
- 14 same-count and 25 count-changing performance IDs with separate exact
  verification; and
- a complete durable Store census with the patch-compatible footprint blocker
  recorded honestly.

Correctness, resource, cleanup, FUSE/materialization equality, managed Docker,
native quality, edit-family, and Store-verification gates pass in the terminal
retained evidence. Same-source A/A repeatability uses a symmetric aggregate
family-wall statistic; directional baseline/candidate optimization remains
member-level.

The retained ObjectId/SQLite Store uses 661,061,632 bytes for the primary
100,000-file control, above the 600 MB goal. The owner accepted this exact
patch-compatible blocker. A 562,513,789-byte physical-pack figure is only a
conservative object-storage lower bound and remains deferred to open issue #18.

## Start here

LayerFS 0.1.2 is distributed as source. Build the CLI with Rust 1.85 or newer:

```bash
git clone --branch v0.1.2 --depth 1 https://github.com/Ephemeral-AI-Lab/layerfs.git
cd layerfs
cargo build --release -p layerfs-cli
./target/release/layerfs --version
```

Continue with the
[0.1.2 quickstart](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.2/docs/versioned/0.1.2/quickstart.md)
and review the
[release limitations](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.2/docs/versioned/0.1.2/limitations.md).

Prebuilt executables, crates.io packages, and runtime images are not published
for 0.1.2. The release contains deterministic source archives, `Cargo.lock`,
the MIT license, and `SHA256SUMS`.
