# LayerFS

LayerFS V2.1 is a standalone Rust storage and workspace engine for immutable
content-addressed state, copy-on-write workspaces, structural diff, and
generic conditional publication.

This repository is the initial implementation scaffold. It contains the
three-crate dependency shape required by the V2.1 architecture; runtime
behavior is implemented incrementally according to the staged plan.

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
cargo check --workspace
cargo test --workspace
```

