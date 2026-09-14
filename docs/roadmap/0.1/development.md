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
cargo +1.96.0 clippy --workspace --locked -- -D clippy::correctness -D clippy::suspicious -D unused_must_use
git diff --check
```

Clippy blocks compilation errors, correctness and suspicious-code findings,
and ignored values marked `must_use`. Other warnings remain visible and
advisory, including style, complexity and unused-code findings. Do not add
`-D warnings` or make the step `continue-on-error`: the selected fatal checks
must still fail CI. Broader local lint coverage can add `--all-targets
--all-features` before `--` using the same lint levels.

`tools/test-fast.sh` is the complete native gate and fails if the warm suite
exceeds 180 seconds. It still runs every test and benchmark exactly once in
disjoint single-threaded process batches; `tools/test_fast.py` sizes those
batches from the committed execution-order hint `tools/test-fast-timings.json`
and submits them longest-first, so the bounded worker slots stay busy instead of
finishing unevenly. The hint never selects, merges, or skips a test, and a
missing or damaged hint only costs ordering. Regenerate it after large suite
changes (it measures each test with libtest's unstable `--report-time`):

```bash
cargo test --workspace --all-features --locked --no-run --message-format=json >/tmp/artifacts.jsonl
python3 tools/harvest-test-timings.py --manifest /tmp/artifacts.jsonl
```

Use the smallest focused check while iterating, for example:

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
