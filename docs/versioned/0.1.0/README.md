# LayerFS 0.1.0 manual

> **Status:** Released manual for LayerFS 0.1.0 Developer Preview, frozen by
> the `v0.1.0` tag.

LayerFS 0.1.0 provides local, branchable agent workspaces backed by one
content-addressed SQLite Store. It supports host materialization, real FUSE
inside managed containers, fresh-process execution, immutable snapshots,
global Store-local deduplication, monitoring, a Rust SDK, and a CLI.

## Read this manual

1. [Quickstart](quickstart.md)
2. [Product specification](specification.md)
3. [CLI reference](cli.md)
4. [Rust SDK reference](sdk.md)
5. [Container runtime](container-runtime.md)
6. [Storage format](storage-format.md)
7. [Limitations](limitations.md)

The reportable 0.1.0 performance and storage evidence is in the
[release benchmark report](../../../release-notes/0.1.0/benchmark-results.md).

## Product boundary

One SDK `Client` binds one local `LayerStackStore`, one Monitor, and one
Workspace manager. A Workspace is ephemeral. Every durable LayerStack, Layer,
Branch, Commit, and canonical object lives in the bound Store.

Container execution is local. The prepared container runs an authenticated
control daemon, a fresh FUSE helper for each Workspace, and a fresh requested
process for each execution. The Store remains in the host SDK process.
