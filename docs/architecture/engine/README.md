# Engine

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** How does a Store persist history, and how does a mounted Workspace read, change and Commit it?

The engine holds the global Store and its calls, Project Init, each
Workspace's overlay and view, Commit, the kernel routes, the daemon process
and the wire between a controller and that daemon. It builds on
[core](../core/README.md) and knows nothing of machines, Sandboxes or the
call table.

## Picture

```text
                           +--------+
            +--------------| Daemon |----------------+
            |              +--------+                |
            v                   |                    v
    +---------------+           v             +-------------+
    | Kernel routes |    +-------------+      |   Bridge    |
    |  FUSE . NFS   |    | Store calls |----->|  the wire   |
    +---------------+    |  + Commit   |      +-------------+
            |            +-------------+             ^
            v              |    |    |               |
    +---------------+      |    |    v               |
    |   Workspace   |<-----+    |  +--------------+  |
    +---------------+      |    |  | Project Init |--+
            |              |    |  +--------------+
            v              v    v         |
    +---------------+    +---------+      |
    |    Overlay    |    |  Store  |<-----+
    +---------------+    +---------+
                              |
                              v
             core: objects, files, physical storage, history
```

Arrows follow the `[dependencies]` of each crate's `Cargo.toml` (test-only
dependencies left out). Store calls also reach the Overlay for the Workspace
roster. Commit spans two crates: construction in `layerfs-workspace`,
publication in `layerfs-store-service`. The daemon depends on every engine
crate except `layerfs-project`.

## Topics

| Topic | Crates | What it covers | Pages |
| --- | --- | --- | --- |
| [Store](store/README.md) | `layerfs-persistence` | The one global Store: every project's history and immutable objects in one SQLite file or one PostgreSQL schema, written by one writer at a time. | [SQLite](store/sqlite.md), [PostgreSQL](store/postgresql.md), [Diagnostics](store/diagnostics.md) |
| [Store calls](store-calls/README.md) | `layerfs-store-service` | Typed reads, file changes and Branch history over the Store, run once in the process that opened it, and the SDK facade that names them. | [Reads](store-calls/reads.md) |
| [Project Init](project-init/README.md) | `layerfs-project` | Turning one native directory into a project's first LayerStack and Branch: import, construction, then seal and install or a live add. | — |
| [Overlay](overlay/README.md) | `layerfs-overlay`, `layerfs-store-service` `overlay/` | Each Workspace's own disposable SQLite file and in-memory open state, the Workspace roster and its one lock. | [Payload](overlay/payload.md), [Lifetimes](overlay/lifetimes.md) |
| [Workspace](workspace/README.md) | `layerfs-workspace` | Filesystem semantics of a mounted Workspace: one immutable committed root plus the Workspace's overlay rows. | [Operations](workspace/operations.md) |
| [Commit](commit/README.md) | `layerfs-workspace` `construction/`, `layerfs-store-service` `store/` | Capturing a Workspace's published frontier, constructing and publishing a Commit, merging with a moved head, and installing the new base. | [Construction](commit/construction.md) |
| [Kernel routes](kernel-routes/README.md) | `layerfs-mount-ports`, `layerfs-fuse`, `layerfs-nfs` | How the kernel reaches a Workspace: route-neutral mount ports, then FUSE or a loopback NFSv3 export, and the coherence both keep. | [FUSE](kernel-routes/fuse.md), [NFS](kernel-routes/nfs.md) |
| [Bridge](bridge/README.md) | `layerfs-bridge` | The wire contract between a controller and a Workspace daemon: the authenticated channel, every record, daemon setup and Store install records. The one home of the wire; control and API pages link here. | — |
| [Daemon](daemon/README.md) | `layerfs-daemon` | The Workspace daemon process: startup, the operator methods, control calls, Workspace lifetime and forced teardown. | [Control calls](daemon/control-calls.md) |

## Rules for this layer

| Rule | Where it is held |
| --- | --- |
| Engine crates depend only on core and other engine crates, never on control or API crates. | [`tools/check_product_boundary.py`](../../../tools/check_product_boundary.py), [engine guide](../../../engine/AGENTS.md) |
| Disposable / WAL / synchronous=OFF is the only permitted Store profile; Durable execution is `NOT_RUN — disabled by owner until explicit reauthorization`. | [Store](store/README.md), [repository guide](../../../AGENTS.md) |
| One attempted operation: no retry, busy handler or replay of a failed or uncertain call. | [Store](store/README.md), [engine guide](../../../engine/AGENTS.md) |
| One global database, one writer, short holds: only the atomic publish step holds the lane. | [Store](store/README.md), [Commit](commit/README.md) |
| SDK Store calls run in the caller's process on Linux; the daemon serves only Workspace calls. | [Store calls](store-calls/README.md) |
| The fuser 0.18.0 patches are the only authorized third-party edits. | [FUSE](kernel-routes/fuse.md), [engine guide](../../../engine/AGENTS.md) |

The [engine guide](../../../engine/AGENTS.md) holds the binding rules for
code in this layer. These pages explain the design; they do not replace
those rules.
