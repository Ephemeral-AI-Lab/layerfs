# LayerFS 0.1.2 manual

> **Status:** Release candidate draft for LayerFS 0.1.2; publication is blocked
> by issue #20.

LayerFS 0.1.2 preserves the 0.1.1 CLI, canonical-object, Store, daemon, and
Workspace contracts. It adds owner-side regular-file range editing and unifies
ordinary FUSE write/truncate with the same failure-atomic piece engine.

## Read this manual

1. [Quickstart](quickstart.md)
2. [Product specification](specification.md)
3. [CLI reference](cli.md)
4. [Rust SDK reference](sdk.md)
5. [Container runtime](container-runtime.md)
6. [Storage format](storage-format.md)
7. [Limitations](limitations.md)

The terminal evidence and release identity are in the
[0.1.2 release record](../../../release-notes/0.1.2/README.md). The
[0.1.1 manual](../0.1.1/README.md) remains available as the previous contract.
