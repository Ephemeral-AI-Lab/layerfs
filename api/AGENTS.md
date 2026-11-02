# Agent rules for API

> **Status:** Current general guide (LayerFS 2.0, #533).

Read [repository instructions](../AGENTS.md) first. This layer holds the
public surface:

- `crates/layerfs-api`: the library, the call table and its one dispatcher.
- `crates/layerfs-cli`: the `layerfs` command and the machine worker.
- `crates/layerfs-node`: the Node binding, built as an addon beside its crate
  at `crates/layerfs-node/layerfs.node` by `scripts/build-native.mjs`.
- `crates/layerfs-python`: the PyO3 module `layerfs._native`.
- `node/`: the one published JavaScript package, `layerfs`.
- `python/`: the Python package `layerfs`.

The packages are thin shells over the table: each function is one library
call, made once. API crates may depend on connect, engine and core crates,
including dev dependencies and test includes; nothing in core, engine or
connect may depend on anything here. Layers are
`core < engine < connect < api` (#535 D4); `layerfs-api` reaches
`layerfs-machine` only through its optional `machine` feature.

SDK-facing daemon integration tests live in `crates/layerfs-api/tests`, with
cheap includes of daemon and Sandbox fixtures from `engine/crates`. Build the
ordinary daemon binary before running tests that launch it.
`LAYERFS_TEST_DAEMON` may select an explicit binary; otherwise the tests
locate the sibling Cargo output. The owner keeps `layerfs-node` out of CI.

All API product code follows the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](../engine/AGENTS.md#checks-and-completion),
including bounded test commands, build-before-run, locked dependencies, ARM64
flags, per-commit LOC and the Disposable-only profile. Preserve the existing
unsafe-code boundaries; do not add test hooks or inline tests to production src.
Do not add a call, argument or result name in JavaScript: change the table in
`crates/layerfs-api/src/calls/table.rs` and the package follows. CI
reports and does not gate a push.
