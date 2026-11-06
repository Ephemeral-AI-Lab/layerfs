# LayerFS architecture

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How LayerFS 2.0 is built, one component at a time. This is the maintained
design reference for contributors. To *use* LayerFS, read the
[2.0.0 manual](../versioned/2.0.0/README.md); for why a decision was made, read
its issue under [`core/docs/issues/`](../../core/docs/issues). Terms are in the
[glossary](glossary.md).

## The four layers

A layer may depend only on the layers below it. The boundary guard
[`tools/check_product_boundary.py`](../../tools/check_product_boundary.py)
enforces it.

```text
+----------------------------------------------------------------+
| api       Call table . SDK . Front ends                        |
|                                                                |
+----------------------------------------------------------------+
        | depends on
        v
+----------------------------------------------------------------+
| control   Management . Routing and workers . Support matrix .  |
|           Machines . Network . Sandboxes . Backups             |
+----------------------------------------------------------------+
        |
        v
+----------------------------------------------------------------+
| engine    Store . Store calls . Project Init . Overlay .        |
|           Workspace . Commit . Kernel routes . Daemon          |
+----------------------------------------------------------------+
        |
        v
+----------------------------------------------------------------+
| core      Objects . Files . Filesystem trees . Compare and     |
|           merge . Physical storage . History model . Telemetry |
+----------------------------------------------------------------+
```

| Layer | Answers |
| --- | --- |
| [Core](core/README.md) | What is a file, a tree, a Commit and a stored object, independent of any process, database or kernel? |
| [Engine](engine/README.md) | How does a Store persist history, and how does a mounted Workspace read, change and Commit it? |
| [Control](control/README.md) | Which machines, Stores, projects and Sandboxes exist; where can a Sandbox run, how does it run there, how is it reached, and how is a Store backed up? |
| [API](api/README.md) | What can a program call, and where does each call run? |

## Every component

| Layer | Component | What it covers |
| --- | --- | --- |
| [Core](core/README.md) | [Objects](core/objects/README.md) | Canonical immutable objects and their IDs: the units every other component names, stores and compares. |
|  | [Files](core/files/README.md) | Complete-file construction, content-defined chunking, localized edits and bounded logical reads. |
|  | [Filesystem trees](core/filesystem-trees/README.md) | Directories as canonical objects: entries, attributes, symlinks, hard links, ordering and validation. |
|  | [Compare and merge](core/compare-and-merge/README.md) | Diffing two trees or files, and merging Branches: decisions, conflict markers and revert. |
|  | [Physical storage](core/physical-storage/README.md) | How objects become bytes on disk: encoding, delta bases, packs, saves, reads and the persistence ports. |
|  | [History model](core/history-model/README.md) | LayerStacks, Branches, Commits, Layers and stages: the immutable history metadata. |
|  | [Telemetry](core/telemetry/README.md) | Environment-independent parent/child timing trees, and how they are printed or written as JSON. |
| [Engine](engine/README.md) | [Store](engine/store/README.md) | The one global Store: SQLite and PostgreSQL backends side by side, one writer, short holds and the publish lane. |
|  | [Store calls](engine/store-calls/README.md) | Typed reads, file changes and Branch history over the Store, and the SDK facade that names them. |
|  | [Project Init](engine/project-init/README.md) | Turning a native directory into a project's first Commit: import, namespace construction, seal and install. |
|  | [Overlay](engine/overlay/README.md) | Each Workspace's own disposable SQLite file: changed metadata, payload, operation records and their reclamation. |
|  | [Workspace](engine/workspace/README.md) | Filesystem semantics of a mounted Workspace: the committed base, overlay changes and the effective view. |
|  | [Commit](engine/commit/README.md) | Capturing a Workspace's published frontier, constructing it and publishing a Commit, then installing the new base. |
|  | [Kernel routes](engine/kernel-routes/README.md) | How the kernel reaches a Workspace: route-neutral mount ports, FUSE and loopback NFSv3. |
|  | [Daemon](engine/daemon/README.md) | The Workspace daemon process: startup, control calls, install, Workspace lifetime and forced teardown, over the bridge wire. |
| [Control](control/README.md) | [Support matrix](control/support-matrix/README.md) | One table decides every platform × host × Store-kind combination, and `SUPPORT.json` is generated from it. |
|  | [Management](control/management/README.md) | The management library under the call table: the home and its records, machines and workers, Stores, projects, Sandboxes, Workspaces and backups. |
|  | [Routing and workers](control/routing/README.md) | Where each call runs: in the caller's process, in a machine's worker, or in a Sandbox daemon. |
|  | [Machines](control/machines/README.md) | Where Sandboxes run: one adapter per platform that probes, creates, starts, stops and removes a machine. |
|  | [Network](control/network/README.md) | How callers, workers and daemons reach each other: worker setup, PostgreSQL reach and port forwards. |
|  | [Sandboxes](control/sandboxes/README.md) | How a Sandbox runs on a machine's kernel: the docker, native and namespaces backends, commands and streams. |
|  | [Backups](control/backups/README.md) | Whole-Store backup to a bucket and restore, over bucket drivers that know nothing about Stores. |
| [API](api/README.md) | [Call table](api/call-table/README.md) | The nine call families and the one dispatcher every front end goes through. |
|  | [SDK](api/sdk/README.md) | Every call a program can make: 61 calls in nine families, one page per call. |
|  | [Front ends](api/front-ends/README.md) | The `layerfs` command, the Node addon and the Python module: one call each, over the call table. |

## Flows across layers

| Flow | What it follows |
| --- | --- |
| [A call, end to end](flows/call.md) | From a JS, Python or CLI call to its result: the call table, control, routing, and the process the call runs in. |
| [Project Init](flows/init.md) | `project.init`: reading a native directory and publishing the project's first Commit. |
| [A Workspace session](flows/workspace.md) | `sandbox.create`, `workspace.mount`, ordinary file access, `exec` and `unmount`. |
| [A Commit](flows/commit.md) | `workspace.commit`: capture, construction, publish, and install of the new base. |
| [Fork and merge](flows/merge.md) | `branch.fork`, changes on the new Branch, `branch.merge` and `branch.publish`. |
| [Backup and restore](flows/backup.md) | `backup.create` and `backup.restore`: from a Store to a bucket and back. |

## Where to start

| If you want to… | Read, in order |
| --- | --- |
| understand LayerFS as a whole | [A call, end to end](flows/call.md), [A Workspace session](flows/workspace.md), [A Commit](flows/commit.md) |
| change how history is stored | [Objects](core/objects/README.md), [Physical storage](core/physical-storage/README.md), [Store](engine/store/README.md), [A Commit](flows/commit.md) |
| work on mounts and file access | [Workspace](engine/workspace/README.md), [Overlay](engine/overlay/README.md), [Kernel routes](engine/kernel-routes/README.md), [Daemon](engine/daemon/README.md) |
| add a platform or Sandbox backend | [Support matrix](control/support-matrix/README.md), [Machines](control/machines/README.md), [Network](control/network/README.md), [Sandboxes](control/sandboxes/README.md) |
| use or change a call | [SDK](api/sdk/README.md), [Call table](api/call-table/README.md), [Management](control/management/README.md), [Routing and workers](control/routing/README.md) |

## How these pages work

```text
docs/architecture/
  README.md            you are here
  glossary.md          every term, with the page that owns it
  <layer>/README.md    the layer: its question, components, rules
  <layer>/<component>/
    README.md          the component: facts, topics, picture, invariants,
                       limits, demonstration
    <topic>.md         one part of the component, one heading per subtopic
  flows/<flow>.md      one operation followed across layers
  _templates/          the component and topic page templates
```

| Rule | What it means |
| --- | --- |
| Diagrams, then tables, then prose | Explain with an ASCII diagram first, a table second, and prose only for what neither can say. |
| One folder per component | A component is a unit of behaviour, not a crate: it may span crates, and one crate may hold several components. |
| Same sections everywhere | Component pages and topic pages follow the [templates](_templates). |
| Every page shows where it is | A breadcrumb at the top, a facts table, and previous / up / next links at the bottom. Following "next" from here visits every page once. |
| Demonstrations point at code that runs | An existing test or example, its exact command and platform. |
| Pages change with the source | A change that alters a boundary, format, algorithm or named limit updates its page in the same commit. |
| Status says how far a page is | "Outline" pages list what they will cover; "Current architecture guide" pages are written and maintained. |
| Older papers stay where they are | The numbered papers in [`core/docs/architecture/`](../../core/docs/architecture/README.md) are linked from the pages that replace them. |

---

[Glossary](glossary.md) →
