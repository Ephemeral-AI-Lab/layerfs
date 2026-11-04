# API

[Architecture](../README.md) › API

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** What can a program call, and where does each call run?

The API layer is the public surface: the management library, the call table and its one dispatcher, and the front ends (the `layerfs` command, Node and Python).

## How the components relate

```text
 Front ends (CLI, Node, Python)
      |
      v
 Call table ---> Control ---> connect: Machines, Sandboxes, Backups
                    |    \--> engine: Store calls, Project Init
                    v
             Routing and workers
                    : Workspace calls (another process)
                    v
              engine: Daemon
```

Dotted edges cross a process boundary.

## Components

| Component | What it covers | Topics |
| --- | --- | --- |
| [Control](control/README.md) | The management library under the call table: the home and its records, machines and workers, Stores, projects, Sandboxes, Workspaces and backups. | [Home and records](control/home-and-records.md), [Families](control/families.md), [Doctor](control/doctor.md) |
| [Routing and workers](routing/README.md) | Where each call runs: in the caller's process, in a machine's worker, or in a Sandbox daemon. | [Routing table](routing/routing-table.md), [Workers](routing/workers.md), [Daemon channel](routing/daemon-channel.md) |
| [Call table](call-table/README.md) | The nine call families and the one dispatcher every front end goes through. | [Table](call-table/table.md), [Errors](call-table/errors.md) |
| [SDK](sdk/README.md) | Every call a program can make: 61 calls in nine families, one page per call, with JavaScript, Python and CLI forms. | [project](sdk/project/README.md), [branch](sdk/branch/README.md), [commit](sdk/commit/README.md), [file](sdk/file/README.md), [sandbox](sdk/sandbox/README.md), [workspace](sdk/workspace/README.md), [machine](sdk/machine/README.md), [store](sdk/store/README.md), [backup](sdk/backup/README.md) |
| [Front ends](front-ends/README.md) | The `layerfs` command, the Node addon and the Python module: one call each, over the call table. | [CLI](front-ends/cli.md), [Node](front-ends/node.md), [Python](front-ends/python.md) |

## Rules for this layer

The [api guide](../../../api/AGENTS.md) holds the binding
rules for code in this layer. These pages explain the design; they do not
replace those rules.

---

← [Backups: Backup and restore](../connect/backups/backup-and-restore.md) · [Up: Architecture](../README.md) · [API: Control](control/README.md) →
