# LayerFS 0.1.x development guide

> **Status:** Current maintainer guidance for the 0.1.x line; not a released
> product contract.

## Toolchain and gates

LayerFS requires Rust 1.85 or newer. Real container FUSE checks additionally
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

`tools/test-fast.sh` is the complete native gate and fails if the warm suite
exceeds 120 seconds. Use the smallest focused check while iterating, for
example:

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

## Rules

- Preserve typed IDs and authenticate canonical bytes at every durable read.
- Keep hashing, traversal, FUSE I/O, and execution outside SQLite write
  transactions.
- Bound object pages by count and encoded bytes.
- Keep static Store SQL in `crates/layerfs-layerstack-store/sql`; Rust owns
  parameters, transactions, typed decoding, and error mapping.
- Use public SDK operations for end-to-end benchmarks.
- Record the commit, environment, command, raw output, and exact timing
  boundary for reportable performance work.
- Add the smallest focused check for non-trivial behavior, then run direct
  dependents before the full gates.

## Real FUSE checks

A real FUSE check verifies the mount type, executes through the mounted path,
commits through the public SDK, reopens the resulting Store state, and cleanly
unmounts. A materialized directory is useful for development but is not FUSE
proof.

Container construction and readiness are environment setup. Complete them
before measuring a Workspace lifecycle.

## References

- [0.1.x roadmap](README.md)
- [0.1.1 checklist](0.1.1/README.md)
- [Benchmark contract](benchmarking.md)
- [Release policy](../../general/release-policy.md)
- [Documentation policy](../../general/documentation-policy.md)
