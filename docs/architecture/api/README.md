# API

[Architecture](../README.md) › API

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** What can a program call, and where does each call run?

The API layer is the public surface: the management library, the call table and its one dispatcher, and the front ends (the `layerfs` command, Node and Python).

What a program sees of this layer is the **SDK**: 62 calls in nine
families, the same names and arguments in JavaScript, Python and the
`layerfs` command. The [SDK reference](sdk/README.md) has one page per
call; the other components below explain how those calls are checked,
routed and run.

## The SDK

```text
 import layerfs from "layerfs"     import layerfs        layerfs <family> <call>
 await layerfs.<family>.<call>(    layerfs.<family>.     <positionals> --options
   positionals..., { options })      <call>(..., opt=...)
                \                         |                      /
                 +---- one entry in the call table, one call ----+
                                          |
     project  branch  commit  file  sandbox  workspace  machine  store  backup
```

| Family | Calls (help order) | What it is for |
| --- | --- | --- |
| [`project`](sdk/project/README.md) | [init](sdk/project/init.md), [list](sdk/project/list.md), [get](sdk/project/get.md), [export](sdk/project/export.md), [remove](sdk/project/remove.md) | a folder imported into a Store as `<store>/<project>` |
| [`branch`](sdk/branch/README.md) | [list](sdk/branch/list.md), [get](sdk/branch/get.md), [fork](sdk/branch/fork.md), [merge](sdk/branch/merge.md), [publish](sdk/branch/publish.md), [diff](sdk/branch/diff.md) | lines of work; `publish` makes `main`'s next Layer |
| [`commit`](sdk/commit/README.md) | [list](sdk/commit/list.md), [get](sdk/commit/get.md), [diff](sdk/commit/diff.md) | reading a Branch's history |
| [`file`](sdk/file/README.md) | [read](sdk/file/read.md), [stat](sdk/file/stat.md), [list](sdk/file/list.md), [glob](sdk/file/glob.md), [grep](sdk/file/grep.md), [blame](sdk/file/blame.md), [diff](sdk/file/diff.md), [write](sdk/file/write.md), [edit](sdk/file/edit.md), [rm](sdk/file/rm.md), [mkdir](sdk/file/mkdir.md), [mv](sdk/file/mv.md), [cp](sdk/file/cp.md), [apply](sdk/file/apply.md) | files at a Branch or Commit, without a mount; each write is one Commit |
| [`sandbox`](sdk/sandbox/README.md) | [create](sdk/sandbox/create.md), [list](sdk/sandbox/list.md), [get](sdk/sandbox/get.md), [close](sdk/sandbox/close.md) | a project's Workspace daemon on a machine |
| [`workspace`](sdk/workspace/README.md) | [mount](sdk/workspace/mount.md), [exec](sdk/workspace/exec.md), [commit](sdk/workspace/commit.md), [unmount](sdk/workspace/unmount.md), [status](sdk/workspace/status.md) | a mounted, writable Branch: run commands, then commit |
| [`machine`](sdk/machine/README.md) | [list](sdk/machine/list.md), [platforms](sdk/machine/platforms.md), [get](sdk/machine/get.md), [status](sdk/machine/status.md), [start](sdk/machine/start.md), [stop](sdk/machine/stop.md), [create](sdk/machine/create.md), [connect](sdk/machine/connect.md), [remove](sdk/machine/remove.md), [disconnect](sdk/machine/disconnect.md), [update](sdk/machine/update.md) | where Sandboxes and sqlite Stores live: `native`, `docker`, VMs, ssh hosts |
| [`store`](sdk/store/README.md) | [list](sdk/store/list.md), [get](sdk/store/get.md), [status](sdk/store/status.md), [create](sdk/store/create.md), [connect](sdk/store/connect.md), [push](sdk/store/push.md), [pull](sdk/store/pull.md), [remove](sdk/store/remove.md), [disconnect](sdk/store/disconnect.md) | the database that holds every project's history: sqlite or PostgreSQL |
| [`backup`](sdk/backup/README.md) | [connect](sdk/backup/connect.md), [disconnect](sdk/backup/disconnect.md), [list](sdk/backup/list.md), [create](sdk/backup/create.md), [restore](sdk/backup/restore.md) | copies of sqlite Stores in r2 or MinIO buckets |

| A typical session | Calls |
| --- | --- |
| set up | [store.create](sdk/store/create.md) → [project.init](sdk/project/init.md) → [branch.fork](sdk/branch/fork.md) |
| work in a mounted tree | [sandbox.create](sdk/sandbox/create.md) → [workspace.mount](sdk/workspace/mount.md) → [workspace.exec](sdk/workspace/exec.md) → [workspace.commit](sdk/workspace/commit.md) → [workspace.unmount](sdk/workspace/unmount.md) |
| work without a mount | [file.read](sdk/file/read.md), [file.edit](sdk/file/edit.md), [file.apply](sdk/file/apply.md) |
| ship | [branch.diff](sdk/branch/diff.md) → [branch.merge](sdk/branch/merge.md) → [branch.publish](sdk/branch/publish.md) |

| SDK rule | Where it is explained |
| --- | --- |
| Names are snake_case in every front end; options are a trailing object (JS), keywords (Python) or `--options` (CLI) | [SDK: Calling the SDK](sdk/README.md#calling-the-sdk) |
| Answers are maps, often with `kind`; lists page with `has_more` and `next_page_pos` | [SDK: Answers and errors](sdk/README.md#answers-and-errors) |
| A refusal is `{ code, message, detail }`; `Unknown`, and `Failed` with `attempted`, mean the outcome is uncertain: read state before acting again | [Errors](call-table/errors.md), [uncertain outcomes](call-table/errors.md#uncertain-outcomes) |
| Store calls (`branch`, `commit`, `file`) run in the caller or the Store's machine worker; Workspace calls reach the Sandbox daemon; nothing starts a stopped machine except `machine.start` | [SDK: Where calls run](sdk/README.md#where-calls-run), [Routing table](routing/routing-table.md) |
| A folder scope binds a client to one project, folder and Branch namespace | [SDK: Calling the SDK](sdk/README.md#calling-the-sdk) |
| Every function is generated from the call table; a call is changed in `layerfs-api/src/<family>/<call>.rs` | [Table](call-table/table.md), [Front ends](front-ends/README.md) |

## How the components relate

```text
 program (JS / Python / shell)
      |  the SDK: 62 calls
      v
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
| [SDK](sdk/README.md) | Every call a program can make: 62 calls in nine families, one page per call, with JavaScript, Python and CLI forms. | [project](sdk/project/README.md), [branch](sdk/branch/README.md), [commit](sdk/commit/README.md), [file](sdk/file/README.md), [sandbox](sdk/sandbox/README.md), [workspace](sdk/workspace/README.md), [machine](sdk/machine/README.md), [store](sdk/store/README.md), [backup](sdk/backup/README.md) |
| [Front ends](front-ends/README.md) | The `layerfs` command, the Node addon and the Python module: one call each, over the call table. | [CLI](front-ends/cli.md), [Node](front-ends/node.md), [Python](front-ends/python.md) |

## Rules for this layer

The [api guide](../../../api/AGENTS.md) holds the binding
rules for code in this layer. These pages explain the design; they do not
replace those rules.

---

← [Backups: Backup and restore](../connect/backups/backup-and-restore.md) · [Up: Architecture](../README.md) · [API: Control](control/README.md) →
