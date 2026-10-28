# LayerFS core

> **Status:** Current general guide.

Core contains four crates: `layerfs-content`, `layerfs-history`,
`layerfs-storage` and `layerfs-telemetry`. They own canonical construction,
history contracts, content encoding/packing and observations. Core has no engine
or API dependencies, including tests, and no rusqlite dependency.

Build membership is defined by the [root workspace manifest](../Cargo.toml).
See the [core guide](AGENTS.md), [engine guide](../engine/AGENTS.md),
[API guide](../api/AGENTS.md), [cluster-one handbook](../docs/general/cluster_one_handbook.md)
and [current operation contracts](docs/issues/303/README.md).

The two workstreams remain unchanged. Store ownership, Project Store Init,
Workspace/Overlay, daemon/FUSE, Bridge and Sandbox composition live in engine;
SDK-facing Rust, binding and JavaScript code live in API. This source relocation
changes no public behavior, format or dependency version. The
[#463 implementation record](docs/issues/463/IMPLEMENTATION.md) records verification,
the retained FP-21 failure exception and branch coordination.
