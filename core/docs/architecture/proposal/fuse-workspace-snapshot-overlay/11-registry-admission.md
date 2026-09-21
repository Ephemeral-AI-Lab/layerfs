# Demand-grown Workspace registry

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Correction from `d3d767393fb332a7cd104265ff8c3f2df2ba2be3`, 2026-09-21.

The initial R1 host reserved registry storage proportional to configured
`MAX_COUNT`. That contradicted 04 C-01's explicit prohibition on a preallocated
registry and made a large positive count refuse before any Workspace existed.
The corrected host reserves its fixed 8,192-byte working allowance independently
of the count. It grows registry capacity only when an attach needs a new slot.
`MAX_COUNT` remains a finite admission ceiling, not an allocation instruction.

The aggregate accounting is the fixed host allowance plus the actual retained
`Vec<Entry>` capacity times `size_of::<Entry>()`, each live registry ID's byte
length, and existing Workspace/callback/reply reservations. Each ID is an
exact-length `Box<str>`. Growth reserves a replacement allocation before moving
entries and charges both old and replacement capacity during the move; the old
charge is released only after the old allocation is dropped. Allocator capacity
is checked and charged. Growth refuses on count, overflow, allocation or budget
failure. There is no compaction or shrink-on-close policy.

Removal releases the entry's ID and admission slot after checked cleanup, but
unused retained vector capacity stays charged. Failed close keeps the entry and
count until explicit cleanup succeeds. Attach continues to reserve its slot
before performing I/O, without holding the registry/state lock across that I/O.
Startup validates room for one maximum-length ID, one Workspace's fixed tables
and the existing call/read progress windows; it does not promise that every
configured count fits the memory budget simultaneously.

Product changes are in Workspace `runtime/host.rs`, `runtime/lifecycle.rs` and
`backing/budget.rs`; external assertions are in `tests/readable.rs`. No public
method or extra thread is introduced. This correction changes bookkeeping from
the earlier [R1](08-readable-implementation.md) and
[Status](09-daemon-status.md) proofs; their recorded counts and identities remain
historical and are not relabelled.

## Verification

The nine Workspace tests pass on the native host and Linux aarch64. Linux runs
the root-UID permission test that the non-root macOS test explicitly skips.
New assertions compare a one-slot host with `usize::MAX` admission at the same
8 MiB budget, verify exact-count refusal/reuse, retained capacity charging and
failed-close count retention. All-target Workspace Clippy passes with warnings
denied. Changed Rust files are formatted; the product-boundary guard and six
self-tests pass. [Exact test/check output](evidence/registry-20260921/) is retained.

```sh
CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-workspace
CARGO_TARGET_DIR="$PWD/core/target" \
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-workspace --all-targets -- -D warnings
```

The Linux check uses `rust:1.85.1-bookworm`, the repository at `/work`, the existing
registry mounted read-only and `CARGO_TARGET_DIR=/work/core/target-linux`; it runs
the same locked Workspace tests with `--offline`. These are functional API
checks, not a fresh mounted resource receipt, allocation peak or performance
claim. Whole-workspace integration checks follow R2. Writable admission,
payload/metadata disk quotas, snapshot pins and npm remain separate prerequisites.

## Production source comparison

Production LOC: **99,276 -> 99,335 (delta +59)**. Reference: 65,417 unchanged;
replacement core: 33,859 -> 33,918 (+59). The same `tools/production_loc.py`
(blob `b5b9617d08204977176302311e0b2c72a811b420`) counts first-parent and staged
Git archives of `crates` and `core/crates`, including runtime SQL and excluding
tests, examples, fixtures, tools, docs, manifests, comments and blank lines.
No legacy code is retired. The commit records the exact snapshot identities.
