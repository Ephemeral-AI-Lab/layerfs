# Agent rules for API

> **Status:** Current general guide (LayerFS 2.0, #533).

Read [repository instructions](../AGENTS.md) first. This layer holds the
public surface:

- `crates/layerfs-api`: the SDK call surface: the call table, argument
  checks, scopes and the JSON text form; one route per call into
  `layerfs-control`. It depends only on the control crates
  (`layerfs-control`, `layerfs-home`) and `layerfs-bridge` and re-exports
  what the front ends use (#536). The
  management library `layerfs-control` it calls is a control crate
  (`control/crates/layerfs-control`; layerfs-project#101 D-2 supersedes
  #536 C1, which placed it here); see the [control guide](../control/AGENTS.md).
- `crates/layerfs-cli`: the `layerfs` command and the machine worker.
- `crates/layerfs-node`: the Node binding, built as the package's addon at
  `node/layerfs.node` by `scripts/build-native.mjs`; the package loads that
  file or the one `LAYERFS_NATIVE` names, nothing else.
- `crates/layerfs-python`: the PyO3 module `layerfs._native`.
- `node/`: the one published JavaScript package, `layerfs`.
- `python/`: the Python package `layerfs`.

The packages are thin shells over the table: each function is one library
call, made once. API crates may depend on control, engine and core crates,
including dev dependencies and test includes; nothing in core, engine or
control may depend on anything here. Layers are
`core < engine < control < api` (#535 D4, #101 D-6); `layerfs-control` reaches
`layerfs-platform` only through its optional `machine` feature, which
`layerfs-api` passes through.

Tests that drive the call surface live in `crates/layerfs-api/tests`;
SDK-facing daemon integration tests and tests of control internals live in
`control/crates/layerfs-control/tests`, with
cheap includes of daemon fixtures from `engine/crates` and Sandbox fixtures
from `control/crates`. Build the
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
