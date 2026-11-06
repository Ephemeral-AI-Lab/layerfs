# Agent rules for API

> **Status:** Current general guide (LayerFS 2.0, #533).

Read [repository instructions](../AGENTS.md) first. This layer holds the
public surface:

- `crates/layerfs-api`: the SDK call surface: the call table, argument
  checks, scopes and the JSON text form; one route per call into
  `layerfs-control`. It depends only on `layerfs-control` and
  `layerfs-bridge` and re-exports what the front ends use (#536).
- `crates/layerfs-control`: the management library, by entity (`common/`,
  `machine/` with the worker protocol, `store/`, `project/`, `backup/`,
  `sandbox/`, `workspace/`, `daemon/`): home records, placement, the
  worker protocol and the daemon channel (#536 C1). It is one library with
  no `layerfs-sdk` crate (amends #533 O21a's wording, keeps its intent).
- `crates/layerfs-cli`: the `layerfs` command and the machine worker.
- `crates/layerfs-node`: the Node binding, built as the package's addon at
  `node/layerfs.node` by `scripts/build-native.mjs`; the package loads that
  file or the one `LAYERFS_NATIVE` names, nothing else.
- `crates/layerfs-python`: the PyO3 module `layerfs._native`.
- `node/`: the one published JavaScript package, `layerfs`.
- `python/`: the Python package `layerfs`.

The packages are thin shells over the table: each function is one library
call, made once. API crates may depend on connect, engine and core crates,
including dev dependencies and test includes; nothing in core, engine or
connect may depend on anything here. Layers are
`core < engine < connect < api` (#535 D4); `layerfs-control` reaches
`layerfs-machine` only through its optional `machine` feature, which
`layerfs-api` passes through.

Tests that drive the call surface live in `crates/layerfs-api/tests`;
SDK-facing daemon integration tests and tests of control internals live in
`crates/layerfs-control/tests`, with
cheap includes of daemon fixtures from `engine/crates` and Sandbox fixtures
from `connect/crates`. Build the
ordinary daemon binary before running tests that launch it.
`LAYERFS_TEST_DAEMON` may select an explicit binary; otherwise the tests
locate the sibling Cargo output. The owner keeps `layerfs-node` out of CI.

All API product code follows the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](../engine/AGENTS.md#checks-and-completion),
including bounded test commands, build-before-run, locked dependencies, ARM64
flags, per-commit LOC and the Disposable-only profile. Preserve the existing
unsafe-code boundaries; do not add test hooks or inline tests to production src.
Do not add a call, argument or result name in JavaScript: change the call's
entry in `crates/layerfs-api/src/<family>/<call>.rs` and the package follows.
`layerfs-api/src` holds only `lib.rs` and one folder per family: its
`mod.rs` (the family's calls in help order) and one file per call (its table
entry and the function that runs it, a call into `layerfs-control`); the call
machinery (checks, scope, dispatcher) is `layerfs-control/src/calls/`. A
test pins this shape (`tests/calls_table.rs`). CI reports and does not gate a
push.
