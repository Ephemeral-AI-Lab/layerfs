# Agent rules for core

> **Status:** Current general guide.

Read [repository instructions](../AGENTS.md) first. Core contains the four
algorithm and contract crates: Content, History, Storage and Telemetry. The
[root workspace manifest](../Cargo.toml) decides membership.

## Scope and dependencies

Core depends only on core, including build and dev dependencies and test source
includes. It has no rusqlite dependency. The
[boundary guard](../tools/check_product_boundary.py) checks every crate against
one layer map and exact production-edge allowlist; unknown crates fail closed.
Engine composition belongs under [engine/](../engine/AGENTS.md), and public SDK
composition belongs under [api/](../api/AGENTS.md). Neither layer is a fallback
implementation for core. The retired v0.1.6 reference remains absent; its recovery
tag and historical evidence are described in the root guide.

Start with the [cluster-one handbook](../docs/general/cluster_one_handbook.md)
and [CAS/CDC/delta guide](../docs/general/cas_cdc_deltaencoding_handbook.md).
Preserve canonical compatibility, public ports, source pins and limitations.
For Workspace integration also read the
[current design index](docs/issues/303/README.md) and the relevant primary
contracts. Research and old Stage handoffs do not select new algorithms or
reopen a closed campaign.

## Construction and completion boundaries

Content owns canonical filesystem/file construction. Storage owns encoding,
packing and object read/write ports. History owns publication and mutable
history contracts. Telemetry owns observations and timing. Project adapts native
inputs; Workspace adapts an immutable base plus captured Overlay changes. They
reuse the same Content and Storage pipeline. See the engine guide's
[shared construction boundaries](../engine/AGENTS.md#shared-construction-and-completion-boundaries).
Do not add a second encoder, constructor or source include to supply integration.

Storage keeps its narrowly audited FFI module `src/encoding/codec.rs` under
`deny(unsafe_code)`; Content and Telemetry forbid unsafe code. Preserve the
existing unsafe boundaries and extend guard coverage when required shipped
formats are introduced. No third-party source or dependency changes are implied.

## Checks and completion

All product layers follow the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [verification rules](../engine/AGENTS.md#checks-and-completion). Build and
test through the selected worktree's registered private resources from the root
manifest. Build tests first with `--no-run`, then execute each selection under an
explicit wall timeout of at most 120 seconds. Preserve repository ARM64 flags,
locked dependencies, exact failure custody, the Disposable-only execution policy
and per-commit production LOC comparisons. Migrations retain cheap tests of
surviving behavior and delete tests that require retired scaffolding, with the
root guide's required behavior coverage and deletion record.

For docs-only changes verify local links/anchors, status, claims and whitespace;
Rust/runtime checks do not apply. CI reports after a push and never gates it.
