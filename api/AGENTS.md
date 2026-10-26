# Agent rules for API

> **Status:** Current general guide.

Read [repository instructions](../AGENTS.md) first. This layer holds the
published JavaScript packages only, one folder each: `agent` (@layerfs/agent),
`cli` (@layerfs/cli), `backend` (@layerfs/backend) and `runtime`
(@layerfs/runtime), with their guides in `docs/` and the build scripts in
`scripts/`. It holds no Rust: the SDK method surface `layerfs-api` and the Node
binding `layerfs-node` are engine crates (see the
[engine guide](../engine/AGENTS.md#scope-and-ownership)); `scripts/build-native.mjs`
builds the addon beside its crate at `engine/crates/layerfs-node/layerfs.node`.
The packages load the engine through that addon or the worker; nothing in core
or engine may depend on them.

All API product code, and the engine crates it loads, follow the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](../engine/AGENTS.md#checks-and-completion),
including bounded test commands, build-before-run, locked dependencies, ARM64
flags, per-commit LOC and the Disposable-only profile. Preserve the existing
unsafe-code boundaries; do not add test hooks or inline tests to production src.
The owner leaves Node binding/JavaScript CI targets excluded; changed scope must
still be verified explicitly. The named SDK mount tests remain in nightly's
Linux mount lane. CI reports and does not gate a push.
