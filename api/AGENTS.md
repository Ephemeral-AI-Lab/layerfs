# Agent rules for API

> **Status:** Current general guide (LayerFS 2.0, #533).

Read [repository instructions](../AGENTS.md) first. This layer holds the
public surface:

- `crates/layerfs-api`: the SDK call surface and the only crate that
  speaks the SDK (layerfs-project#107): the call table, argument checks,
  scopes, the dispatcher and the JSON text form; each call is one direct
  call of a control crate's function. It depends only on the control crates
  (`layerfs-inspect`, `layerfs-home`, `layerfs-machine`, `layerfs-store`,
  `layerfs-sandbox`, `layerfs-workspace`) and the engine's
  `layerfs-protocol` and `layerfs-store-service`, and re-exports
  what the front ends use (#536). The
  management library `layerfs-inspect` (was `layerfs-control`) it calls is a control crate
  (`control/crates/layerfs-inspect`; layerfs-project#101 D-2 supersedes
  #536 C1, which placed it here); see the [control guide](../control/AGENTS.md).
- `crates/layerfs-cli`: the `layerfs` command and the machine worker.
- `crates/layerfs-node`: the Node binding, built as the package's addon at
  `node/layerfs.node` by `scripts/native-addon.mjs`; the package loads that
  file or the one `LAYERFS_NATIVE` names, nothing else.
- `crates/layerfs-python`: the PyO3 module `layerfs._native`.
- `node/`: the SDK for Node, `@layerfs/node`.
- `cli/`: the `layerfs` command's package, `@layerfs/cli`.
- `mcp/`: the MCP server over stdio, `@layerfs/mcp` (private).
- `python/`: the Python package `layerfs`.

The packages are thin shells over the table: each function is one library
call, made once. API crates may depend on control, engine and core crates,
including dev dependencies and test includes; nothing in core, engine or
control may depend on anything here. Layers are
`core < engine < control < api` (#535 D4, #101 D-6); `layerfs-machine` builds its
platform adapters only with its optional `machine` feature, which
`layerfs-api` passes through.

Tests that drive the call surface live in `crates/layerfs-api/tests`;
SDK-facing daemon integration tests and tests of control internals live in
`control/crates/layerfs-inspect/tests`, with
cheap includes of daemon fixtures from `engine/crates` and Sandbox fixtures
from `control/crates`. Build the
ordinary daemon binary before running tests that launch it.
`LAYERFS_TEST_DAEMON` may select an explicit binary; otherwise the tests
locate the sibling Cargo output. The owner keeps `layerfs-node` out of CI.

API code follows the engine guide's
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks](../engine/AGENTS.md#checks-and-completion), and the root guide's
[rules you must not break](../AGENTS.md#rules-you-must-not-break). Preserve the
existing unsafe-code boundaries; do not add test hooks or inline tests to
production src.
Do not add a call, argument or result name in JavaScript: change the call's
entry in `crates/layerfs-api/src/<family>/<call>.rs` and the package follows.
`layerfs-api/src` holds only `lib.rs`, the call machinery `calls/` (table
shape, checks, scope, keys, dispatcher, `Context`; layerfs-project#107) and
one folder per family: its `mod.rs` (the family's calls in help order) and
one file per call (its table entry and the function that runs it: one call
of a control crate's per-op function, never an op name as a string). A
test pins this shape (`tests/calls_table.rs`).
