# Developing LayerFS

> **Status:** Current contributor guide for the LayerFS repository.

## Toolchain

LayerFS requires Rust 1.85 or newer. Real container FUSE tests additionally
require Docker, `/dev/fuse`, and permission to grant `CAP_SYS_ADMIN` to the
test container.

From the repository root:

```bash
cargo build --workspace
tools/test-fast.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

`tools/test-fast.sh` is the complete native test gate. It builds all workspace
test targets, runs them in bounded parallel workers, and fails if the warm
suite exceeds 120 seconds. Use a focused `cargo test` command when iterating
on one package or test target.

```bash
cargo test -p layerfs-content --test extent_model
```

## Production crates

```text
layerfs-content           canonical objects, CDC, ropes, trees, Diff
layerfs-layerstack-store  one SQLite Store and durable operations
layerfs-workspace         Workspace lifecycle, capture, execution, containers
layerfs-fuse              FUSE adapter and authenticated proxy protocol
layerfs-materialization   explicit directory projection
layerfs-monitor           receipts, snapshots, deduplication analysis
layerfs-sdk               public Rust API
layerfs-cli               command-line adapter over the public SDK
layerfs-daemon            container execution and FUSE control only
```

Keep dependencies flowing toward the SDK: content remains SQL-independent;
the CLI contains no SQL; the daemon owns no Store.

## Development rules

- Preserve typed IDs and authenticate canonical bytes at every durable read.
- Keep hashing, filesystem traversal, FUSE I/O, and execution outside SQLite
  write transactions.
- Bound object pages by count and encoded bytes.
- Put static Store SQL in `crates/layerfs-layerstack-store/sql`; Rust owns
  parameters, transactions, typed decoding, and error mapping.
- Use public SDK operations for end-to-end benchmarks.
- Record the commit, source seal, environment, command, raw output, and exact
  timing boundary for reportable performance work.
- Add the smallest focused test that proves any non-trivial behavior, then run
  its direct dependents before the full gates.

## Live FUSE checks

A real FUSE test must verify the mount type, execute through the mounted path,
commit through the public SDK, reopen the resulting Store state, and cleanly
unmount. A materialized directory is useful for development but is not proof
of the FUSE path.

Container image construction and container readiness are environment setup.
They should be completed before measuring a Workspace lifecycle.
