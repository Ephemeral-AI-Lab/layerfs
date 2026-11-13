# LayerFS architecture

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How LayerFS 2.0 is built, one layer and one topic at a time. This is the
maintained design reference for contributors. To *use* LayerFS, read the
[2.0.0 manual](../versioned/2.0.0/README.md). Terms are in the
[glossary](glossary.md).

## The four layers

A layer may depend only on the layers below it. The boundary guard
[`tools/check_product_boundary.py`](../../tools/check_product_boundary.py)
enforces it.

```text
+----------------------------------------------------------------+
| api       Calls . SDK . Front ends                             |
+----------------------------------------------------------------+
        | depends on
        v
+----------------------------------------------------------------+
| control   Home . Machines . Stores and projects . Sandboxes .  |
|           Backups . Doctor                                     |
+----------------------------------------------------------------+
        |
        v
+----------------------------------------------------------------+
| engine    Store . Store calls . Project Init . Overlay .        |
|           Workspace . Commit . Kernel routes . Bridge . Daemon |
+----------------------------------------------------------------+
        |
        v
+----------------------------------------------------------------+
| core      Objects . Files . Filesystem trees . Compare and     |
|           merge . Physical storage . History . Telemetry       |
+----------------------------------------------------------------+
```

| Layer | Answers |
| --- | --- |
| [Core](core/README.md) | What is a file, a tree, a Commit and a stored object, independent of any process, database or kernel? |
| [Engine](engine/README.md) | How does a Store persist history, and how does a mounted Workspace read, change and Commit it? |
| [Control](control/README.md) | Which machines, Stores, projects and Sandboxes exist; where can a Sandbox run, how does it run there, how is it reached, and how is a Store backed up? |
| [API](api/README.md) | What can a program call, and where does each call run? |

## Every topic

| Layer | Topic | Crates | What it covers |
| --- | --- | --- | --- |
| [Core](core/README.md) | [Objects](core/objects/README.md) | `layerfs-content` | Canonical immutable objects and their IDs: the units every other topic names, stores and compares. |
|  | [Files](core/files/README.md) | `layerfs-content` | Complete-file construction, content-defined chunking, localized edits and bounded logical reads. |
|  | [Filesystem trees](core/filesystem-trees/README.md) | `layerfs-content` | Directories as canonical objects: entries, attributes, symlinks, hard links, updates and validation. |
|  | [Compare and merge](core/compare-and-merge/README.md) | `layerfs-content` | Diffing two trees or files, and merging Branches: decisions, conflict markers and revert. |
|  | [Physical storage](core/physical-storage/README.md) | `layerfs-storage` | How objects become bytes: encoding, delta bases, packs, saves, reads and the persistence ports. |
|  | [History](core/history/README.md) | `layerfs-history` | LayerStacks, Branches, Commits, Layers and stages: the immutable history metadata. |
|  | [Telemetry](core/telemetry/README.md) | `layerfs-telemetry` | Parent/child timing trees, how they are printed or written as JSON, and who records them. |
| [Engine](engine/README.md) | [Store](engine/store/README.md) | `layerfs-persistence` | The one global Store: SQLite and PostgreSQL backends, one writer, short holds and the publish lane. |
|  | [Store calls](engine/store-calls/README.md) | `layerfs-store-service` | Typed reads, file changes and Branch history over the Store, run in the caller's process. |
|  | [Project Init](engine/project-init/README.md) | `layerfs-project` | Turning a native directory into a project's first Commit: import, construction, seal and install. |
|  | [Overlay](engine/overlay/README.md) | `layerfs-overlay` | Each Workspace's own disposable SQLite file: metadata, payload, operation records, roster and reclamation. |
|  | [Workspace](engine/workspace/README.md) | `layerfs-workspace` | Filesystem semantics of a mounted Workspace: the committed base, overlay changes and the effective view. |
|  | [Commit](engine/commit/README.md) | `layerfs-workspace`, `layerfs-store-service` | Capturing a Workspace's frontier, constructing and publishing a Commit, then installing the new base. |
|  | [Kernel routes](engine/kernel-routes/README.md) | `layerfs-mount-ports`, `layerfs-fuse`, `layerfs-nfs` | How the kernel reaches a Workspace: route-neutral mount ports, FUSE and loopback NFSv3. |
|  | [Bridge](engine/bridge/README.md) | `layerfs-bridge` | The wire between callers, workers and daemons: framing, handshake, control and SDK records, process identity. |
|  | [Daemon](engine/daemon/README.md) | `layerfs-daemon` | The Workspace daemon process: startup, control calls, install, Workspace lifetime and forced teardown. |
| [Control](control/README.md) | [Home](control/home/README.md) | `layerfs-home` | The home folder: records, locks, operation intent, names, secrets, traces, call logs and migration. |
|  | [Machines](control/machines/README.md) | `layerfs-machine`, `layerfs-platform` | Where Sandboxes run: platform adapters, the support matrix, workers and how they are reached. |
|  | [Stores and projects](control/stores-and-projects/README.md) | `layerfs-store` | Which Stores and projects exist and where: records, placement, leases, SQLite and PostgreSQL, transfer. |
|  | [Sandboxes](control/sandboxes/README.md) | `layerfs-sandbox`, `layerfs-sandbox-backend` | How a Sandbox runs on a machine's kernel: launch, the daemon channel, exec and the backends. |
|  | [Backups](control/backups/README.md) | `layerfs-store` (backup), `layerfs-bucket` | Whole-Store backup to a bucket and restore, over bucket drivers that know nothing about Stores. |
|  | [Doctor](control/doctor/README.md) | `layerfs-control` (doctor) | Checking a home, its machines, Sandboxes and backups, and reporting what is wrong. |
| [API](api/README.md) | [Calls](api/calls/README.md) | `layerfs-api` | The call table, its checks and the one dispatcher every front end goes through. |
|  | [SDK](api/sdk/README.md) | `layerfs-api` | Every call a program can make, one page per call family. |
|  | [Front ends](api/front-ends/README.md) | `layerfs-cli`, `layerfs-node`, `layerfs-python` | The `layerfs` command, the Node addon, the Python module and the MCP server, over the call table. |

## Flows across layers

| Flow | What it follows |
| --- | --- |
| [A call, end to end](flows/call.md) | From a JS, Python or CLI call to its result: the call table, control, and the process the call runs in. |
| [Project Init](flows/init.md) | `project.init`: reading a native directory and publishing the project's first Commit. |
| [A Workspace session](flows/workspace.md) | `sandbox.create`, `workspace.mount`, ordinary file access, `exec` and `unmount`. |
| [A Commit](flows/commit.md) | `workspace.commit`: capture, construction, publish, and install of the new base. |
| [Fork and merge](flows/merge.md) | `branch.fork`, changes on the new Branch, `branch.merge` and `branch.publish`. |

Backup and restore is followed in [Backups](control/backups/README.md).

## Where to start

| If you want to… | Read, in order |
| --- | --- |
| understand LayerFS as a whole | [A call, end to end](flows/call.md), [A Workspace session](flows/workspace.md), [A Commit](flows/commit.md) |
| change how history is stored | [Objects](core/objects/README.md), [Physical storage](core/physical-storage/README.md), [Store](engine/store/README.md), [A Commit](flows/commit.md) |
| work on mounts and file access | [Workspace](engine/workspace/README.md), [Overlay](engine/overlay/README.md), [Kernel routes](engine/kernel-routes/README.md), [Daemon](engine/daemon/README.md) |
| add a platform or Sandbox backend | [Machines](control/machines/README.md), [Sandboxes](control/sandboxes/README.md), [Bridge](engine/bridge/README.md) |
| use or change a call | [SDK](api/sdk/README.md), [Calls](api/calls/README.md), [Where each call runs](control/README.md#where-each-call-runs) |

## How these pages work

```text
docs/architecture/
  README.md              you are here
  glossary.md            every term, with the topic that owns it
  <layer>/README.md      the layer: its question, its topics, its rules
  <layer>/<topic>/
    README.md            the topic (from _templates/topic.md)
    <page>.md            at most 2 extra pages (from _templates/page.md)
  flows/<flow>.md        one operation followed across layers: steps and links
  _templates/            topic.md and page.md
```

| Rule | What it means |
| --- | --- |
| A topic follows the code | Every topic names the crates and source folders it covers, and every crate and major source folder has exactly one owning topic. A topic may span crates when one operation does (Commit). |
| One README per topic, few extra pages | A topic is one `README.md` (about 300 lines at most) plus at most 2 extra pages (about 250 lines each). An extra page covers one code directory of roughly 2.5k lines or more, a separate crate or a backend. Exceptions: Store has 3 extra pages; the SDK has one page per call family. |
| Same sections everywhere | Topic READMEs and extra pages follow the [templates](_templates/README.md): facts, picture, what it does, invariants, limits and failures, demonstration. |
| Diagrams, then tables, then prose | Explain with an ASCII diagram (at most 80 columns) first, a table second, and prose only for what neither can say. |
| Every claim cites source | Each behaviour row, invariant and limit links the source that implements it. |
| Demonstrations point at code that runs | An existing test or example, its exact command and its platform. |
| Current source only | No history, receipts, run logs or open decisions on these pages. Why a decision was made belongs in its issue or pull request; Git history keeps earlier documents. |
| Flows link, they do not repeat | A flow is steps and links into topics; internals live in the topic. |
| Pages change with the source | A change that alters a boundary, format, algorithm or named limit updates its topic in the same commit. |
