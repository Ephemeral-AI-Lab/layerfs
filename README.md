# LayerFS

LayerFS V2.1 is a standalone Rust storage and workspace engine for immutable
content-addressed state, copy-on-write workspaces, structural diff, and
generic conditional publication.

Learn how the storage engine works in
[LayerFS: Building Filesystem Storage for AI Agents](https://ephemeral-ai-lab.github.io/layerfs/).

L0 is implemented. It contains the three-crate dependency shape required by
the V2.1 architecture, the checked M6.1.2 canonical codec/error/path surface,
and byte-preserved custody fixtures for the frozen M6.0 identity vectors.
Storage behavior beyond these primitives is implemented incrementally
according to the staged plan.

## Crates

- `layerfs-sdk` — public package; its Rust library namespace is `layerfs`.
- `layerfs-storage` — private, backend-neutral storage implementation crate.
- `layerfs-driver` — private, OS-facing projection and capture crate.

The dependency direction is:

```text
layerfs-sdk -> { layerfs-storage, layerfs-driver }
layerfs-driver -> layerfs-storage
```

The first projection driver is Linux OverlayFS. Sandbox orchestration,
environment lifecycle, CLI/MCP policy, Git filtering, and provider behavior
remain outside this repository's LayerFS contract.

## V2.1 documentation

The normative proposal and implementation plan are maintained in the
documentation repository:

- [Specification](../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/SPEC.md)
- [Architecture](../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/ARCHITECTURE.md)
- [Storage and performance](../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/STORAGE_AND_PERFORMANCE.md)
- [Implementation plan](../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/IMPLEMENTATION_PLAN.md)

## Local checks

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo tree --workspace --edges normal
shasum -a 256 -c tests/fixtures/frozen/SHA256SUMS
```

The L0 runtime crates have no normal external dependencies. The storage crate
has one dev-only BLAKE3 dependency so tests can recompute the frozen M6.0
structural identity vectors; it is not part of the runtime dependency graph.
