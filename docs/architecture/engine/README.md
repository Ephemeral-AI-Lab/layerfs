# Engine

[Architecture](../README.md) › Engine

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** How does a Store persist history, and how does a mounted Workspace read, change and Commit it?

The engine keeps persistence, the daemon, the overlay, Workspaces and the kernel routes. It builds on core and knows nothing of machines, networks or the call table.

## How the components relate

```text
                     +--------+
                     | Daemon |
                     +--------+
           +-------------+-------------+
           v             v             v
    +-------------+  +--------+  +-------------+
    |Kernel routes|  | Commit |->| Store calls |
    +-------------+  +--------+  +-------------+
           |             |          |       |
           v             v          v       v
    +------------------------------+  +-------+   +--------------+
    |  Workspace  ---->  Overlay   |  | Store |<--| Project Init |
    +------------------------------+  +-------+   +--------------+
```

Kernel routes, Commit and Store calls each use both Workspace and Overlay.
Commit also writes through Store calls to the Store.

Edges follow the crate dependencies in each `Cargo.toml`.

## Components

| Component | What it covers | Topics |
| --- | --- | --- |
| [Store](store/README.md) | The one global Store: SQLite and PostgreSQL backends side by side, one writer, short holds and the publish lane. | [Lifecycle](store/lifecycle.md), [Backends](store/backends.md), [SQLite](store/sqlite.md), [PostgreSQL](store/postgresql.md), [History publication](store/history-publication.md), [Metadata and objects](store/metadata-and-objects.md) |
| [Store calls](store-calls/README.md) | Typed reads, file changes and Branch history over the Store, and the SDK facade that names them. | [Service](store-calls/service.md), [Reads](store-calls/reads.md), [Writes](store-calls/writes.md), [Branches](store-calls/branches.md), [SDK facade](store-calls/sdk-facade.md) |
| [Project Init](project-init/README.md) | Turning a native directory into a project's first Commit: import, namespace construction, seal and install. | [Import](project-init/import.md), [Construction](project-init/construction.md), [Seal and install](project-init/seal-and-install.md) |
| [Overlay](overlay/README.md) | Each Workspace's own disposable SQLite file: changed metadata, payload, operation records and their reclamation. | [Database](overlay/database.md), [Namespace](overlay/namespace.md), [Payload](overlay/payload.md), [Open state](overlay/open-state.md), [Lifetimes and reclamation](overlay/lifetimes-and-reclamation.md), [Operation records](overlay/operation-records.md) |
| [Workspace](workspace/README.md) | Filesystem semantics of a mounted Workspace: the committed base, overlay changes and the effective view. | [Base](workspace/base.md), [Operations](workspace/operations.md), [Mutation](workspace/mutation.md), [Ports](workspace/ports.md), [State](workspace/state.md) |
| [Commit](commit/README.md) | Capturing a Workspace's published frontier, constructing it and publishing a Commit, then installing the new base. | [Capture](commit/capture.md), [Construction](commit/construction.md), [Publish and merge](commit/publish-and-merge.md), [Install](commit/install.md) |
| [Kernel routes](kernel-routes/README.md) | How the kernel reaches a Workspace: route-neutral mount ports, FUSE and loopback NFSv3. | [Ports](kernel-routes/ports.md), [FUSE](kernel-routes/fuse.md), [fuser patches](kernel-routes/fuser-patches.md), [NFS](kernel-routes/nfs.md), [Coherence](kernel-routes/coherence.md) |
| [Daemon](daemon/README.md) | The Workspace daemon process: startup, control calls, install, Workspace lifetime and forced teardown, over the bridge wire. | [Startup](daemon/startup.md), [Control calls](daemon/control-calls.md), [Forced teardown](daemon/forced-teardown.md), [Bridge](daemon/bridge.md), [Operator](daemon/operator.md) |

## Rules for this layer

The [engine guide](../../../engine/AGENTS.md) holds the binding
rules for code in this layer. These pages explain the design; they do not
replace those rules.

---

← [Telemetry: Output](../core/telemetry/output.md) · [Up: Architecture](../README.md) · [Engine: Store](store/README.md) →
