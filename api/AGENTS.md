# Agent rules for API

> **Status:** Current general guide (LayerFS 2.0, #533).

Read [repository instructions](../AGENTS.md) first. This layer holds the one
published JavaScript package, `layerfs`, in `node/`, the Python package
`layerfs` in `python/` (its PyO3 module is the engine crate
`layerfs-python`), and the addon build script in `scripts/`. It holds no Rust: the library `layerfs-api` (the call
table and its one dispatcher) and the Node binding `layerfs-node` are engine
crates (see the [engine guide](../engine/AGENTS.md#scope-and-ownership));
`scripts/build-native.mjs` builds the addon beside its crate at
`engine/crates/layerfs-node/layerfs.node`. The package is a thin shell over
the table: each function is one library call, made once. Nothing in core or
engine may depend on it.

All API product code, and the engine crates it loads, follow the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](../engine/AGENTS.md#checks-and-completion),
including bounded test commands, build-before-run, locked dependencies, ARM64
flags, per-commit LOC and the Disposable-only profile. Preserve the existing
unsafe-code boundaries; do not add test hooks or inline tests to production src.
Do not add a call, argument or result name in JavaScript: change the table in
`engine/crates/layerfs-api/src/calls/table.rs` and the package follows. CI
reports and does not gate a push.
