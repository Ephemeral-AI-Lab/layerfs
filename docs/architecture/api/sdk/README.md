# SDK

[Architecture](../../README.md) › [API](../README.md) › SDK

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Every call a program can make, one family per folder and one page per call:
61 calls in 9 families. The same call has the same name and
arguments in JavaScript, Python and the `layerfs` command.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-api`, `layerfs-control` |
| Source | [`api/crates/layerfs-api/src`](../../../../api/crates/layerfs-api/src) |
| Uses | [Call table](../call-table/README.md), [Management](../../control/management/README.md), [Routing and workers](../../control/routing/README.md) |
| Used by | [Front ends](../front-ends/README.md) |

## Families

| Family | Calls | Pages |
| --- | --- | --- |
| [`project`](project/README.md) | 4 | [`init`](project/init.md), [`list`](project/list.md), [`get`](project/get.md), [`remove`](project/remove.md) |
| [`branch`](branch/README.md) | 6 | [`list`](branch/list.md), [`get`](branch/get.md), [`fork`](branch/fork.md), [`merge`](branch/merge.md), [`publish`](branch/publish.md), [`diff`](branch/diff.md) |
| [`commit`](commit/README.md) | 3 | [`list`](commit/list.md), [`get`](commit/get.md), [`diff`](commit/diff.md) |
| [`file`](file/README.md) | 14 | [`read`](file/read.md), [`stat`](file/stat.md), [`list`](file/list.md), [`glob`](file/glob.md), [`grep`](file/grep.md), [`blame`](file/blame.md), [`diff`](file/diff.md), [`write`](file/write.md), [`edit`](file/edit.md), [`rm`](file/rm.md), [`mkdir`](file/mkdir.md), [`mv`](file/mv.md), [`cp`](file/cp.md), [`apply`](file/apply.md) |
| [`sandbox`](sandbox/README.md) | 4 | [`create`](sandbox/create.md), [`list`](sandbox/list.md), [`get`](sandbox/get.md), [`close`](sandbox/close.md) |
| [`workspace`](workspace/README.md) | 5 | [`mount`](workspace/mount.md), [`exec`](workspace/exec.md), [`commit`](workspace/commit.md), [`unmount`](workspace/unmount.md), [`status`](workspace/status.md) |
| [`machine`](machine/README.md) | 11 | [`list`](machine/list.md), [`platforms`](machine/platforms.md), [`get`](machine/get.md), [`status`](machine/status.md), [`start`](machine/start.md), [`stop`](machine/stop.md), [`create`](machine/create.md), [`connect`](machine/connect.md), [`remove`](machine/remove.md), [`disconnect`](machine/disconnect.md), [`update`](machine/update.md) |
| [`store`](store/README.md) | 9 | [`list`](store/list.md), [`get`](store/get.md), [`status`](store/status.md), [`create`](store/create.md), [`connect`](store/connect.md), [`push`](store/push.md), [`pull`](store/pull.md), [`remove`](store/remove.md), [`disconnect`](store/disconnect.md) |
| [`backup`](backup/README.md) | 5 | [`connect`](backup/connect.md), [`disconnect`](backup/disconnect.md), [`list`](backup/list.md), [`create`](backup/create.md), [`restore`](backup/restore.md) |

## The picture

```text
 1. where things live                   2. what is kept
 +---------------------------+          +-------------------------------+
 | machine  list platforms   |  holds   | store  create (sqlite, native)|
 |          create | connect  |--------->|        connect (PostgreSQL)   |
 |          start stop update |  sqlite  |        push pull remove       |
 +---------------------------+  Stores  +-------------------------------+
      | runs Sandboxes                        | holds projects
      v                                       v
 +---------------------------+          +-------------------------------+
 | sandbox  create (project, |<---------| project  init <store>/<name>  |
 |          machine) close   |  of one  |          list get remove      |
 +---------------------------+  project +-------------------------------+
      | mounts Branches                       | has history
      v                                       v
 +---------------------------+  commits +-------------------------------+
 | workspace mount exec      |--------->| branch  fork merge publish    |
 |           commit unmount  |  to its  |         list get diff         |
 |           status          |  Branch  | commit  list get diff         |
 +---------------------------+          | file    read ... write apply  |
                                        +-------------------------------+
 backup: connect a bucket; create / restore a sqlite Store file
```

| To get | You first need | Made by |
| --- | --- | --- |
| a sqlite Store | nothing (`native` is built in) | [store.create](store/create.md) |
| a project | a Store | [project.init](project/init.md) |
| a Branch to work on | a project (`main` is its published line) | [branch.fork](branch/fork.md) |
| a Sandbox | a project and a machine | [sandbox.create](sandbox/create.md) |
| a Workspace | a running Sandbox and a Branch other than `main` | [workspace.mount](workspace/mount.md) |
| a Commit | a Workspace, or a Branch for a `file` write | [workspace.commit](workspace/commit.md), [file.write](file/write.md) |
| a new `main` Layer | a Branch with a Commit | [branch.publish](branch/publish.md) |
| a VM or ssh machine | the platform's tool and the sealed bundle | [machine.create](machine/create.md), [machine.connect](machine/connect.md) |
| a backup | a sqlite Store and a connected bucket | [backup.create](backup/create.md) |

## Calling the SDK

```text
 JavaScript                 Python                    shell
 import layerfs from        import layerfs            layerfs <family> <call>
   "layerfs"                (layerfs.aio: coroutines)   <positionals>
 await layerfs.f.c(a, b,    layerfs.f.c(a, b,           --options
   { opt })                   opt=...)
        \                          |                          /
         +---------- one library call, made once -----------+
                     layerfs_api: check -> scope -> run
```

| | JavaScript | Python | CLI |
| --- | --- | --- | --- |
| Install / import | `import layerfs from "layerfs"` ([`index.js`](../../../../api/node/src/index.js)) | `import layerfs` ([`__init__.py`](../../../../api/python/src/layerfs/__init__.py)) | the `layerfs` command ([`layerfs-cli`](../../../../api/crates/layerfs-cli/src)) |
| Default client | the module's families: `layerfs.store.list()` | the module's families: `layerfs.store.list()` | every command |
| Another home or a scope | `layerfs.client({ home, scope })` | `layerfs.client(home=..., scope=...)` | `LAYERFS_HOME` |
| Home default | `LAYERFS_HOME`, else `~/.layerfs` | same | same |
| Positionals | in table order; an optional one may be left off | in table order; optional ones also as keywords | in table order |
| Options | a trailing object `{ page_size: 10 }` | keywords `page_size=10` | `--page-size 10`; a Bool is a flag (`--paths-only`) |
| Names | snake_case, the table's own | snake_case | `-` for `_` |
| Content / JSON positionals | string, `Uint8Array`/`Buffer`, object | `str`, `bytes`, dict/list | stdin, or `--from FILE` |
| Sizes | `8589934592` or `"8G"` | same | `8G` |
| Whole numbers | exact: a `bigint` is sent as written; a result is a `number` when a safe integer, else a `bigint`; an unsafe `number`, `NaN`, `Map`, `Set` or lone surrogate is refused before the call ([Node](../front-ends/node.md#values)) | `int` | — |
| Bytes | `Buffer` / `Uint8Array` ↔ `{ "base64": … }`; a one-key map with that key is read as bytes | `bytes` | — |
| Types | `index.d.ts`, generated from the table; results are loose (`Result`) | — | — |
| Answer | a Promise of a plain object | a dict (a coroutine in `layerfs.aio`) | one JSON line on stdout |
| Progress | `on_progress: (e) => …` | `on_progress=fn` | `--progress` (JSON lines on stderr) |
| Exec output | `stdout`, `stderr` functions, or `stream: true` | the same | `--stream` |
| Outside the table | `layerfs.doctor()`, `layerfs.close()` | `layerfs.doctor()`, `layerfs.close()` | `layerfs doctor` |

A folder scope `{ project, folder, branch? }` binds a client to one project:
it allows only the `file` calls, `branch` list, get, fork, merge and
publish, and the `workspace` calls; paths must lie under `folder` and
Branch names become `<branch>.<name>`. A scope guards the arguments of a
call, not processes or the filesystem: `workspace.exec` under a scope only
prefixes the command with `cd '.<folder>' &&`, and the command can still
reach anywhere in the mounted tree. `branch` is optional in the option but
every `file` and `branch` call needs it (without it they are refused, see
[Scopes](../call-table/table.md#scopes)); a Commit id is not confined to the
namespace; the Branch of a Workspace is not checked. These gaps wait for an
owner decision and are listed under
[Scopes](../call-table/table.md#scopes)
([`calls/scope.rs`](../../../../control/crates/layerfs-control/src/calls/scope.rs)).
Every function is generated from the call table
([`calls/table.rs`](../../../../control/crates/layerfs-control/src/calls/table.rs));
see [Front ends](../front-ends/README.md).

## Answers and errors

```text
 success:  a map, keys in snake_case, often with kind:
           { kind: "Committed" | "UpToDate" | "Forked" | "Merged" |
                   "Added" | "Mounted" | "exit:<n>" | "ok" | ..., ... }
           lists page with  items, has_more, next_page_pos (next_cursor)
 refusal:  { code, message, detail }
           JS: throws LayerFSError   Python: raises LayerFSError
           CLI: JSON on stderr, exit 125
```

| Outcome | Codes | What to do | Calls |
| --- | --- | --- | --- |
| certain refusal, nothing changed | `InvalidInput`, `NotFound`, `NoSuch*`, `Exists`, `NameTaken`, `InUse`, `Busy`, `Capacity`, `Unsupported`, `Unavailable`, … | fix the cause, call again | all |
| refused, but the call made something to look at | `Conflict` (markers written by `workspace.commit`), `Dirty` | read `detail` | workspace, branch.merge, file writes |
| **uncertain** | **`Unknown`**: a worker's answer was lost, or the operation itself reported an unknown outcome | read the state first, then decide; never just repeat a write | Store calls, machine, store, backup, workspace |
| **maybe uncertain** | **`Failed` with `attempted: true`** from a Sandbox daemon exchange | read the Workspace and Branch first | workspace calls |
| an earlier uncertain operation still holds the entity | `Unresolved`; `Unknown` phase `admission` for a Workspace Commit | read its status; clear it with the call the message names | store, machine, sandbox, workspace |

| After an uncertain | Read before acting again |
| --- | --- |
| `file.*` write, `workspace.commit` | [commit.list](commit/list.md), [branch.get](branch/get.md), [workspace.status](workspace/status.md) |
| `branch.fork`, `merge`, `publish` | [branch.get](branch/get.md) |
| `workspace.mount`, `unmount`, `exec` | [sandbox.get](sandbox/get.md), [workspace.status](workspace/status.md) |
| `store.push`, `pull`, `remove` | [store.status](store/status.md) |
| `machine.create`, `connect`, `remove`, `disconnect` | [machine.status](machine/status.md) |
| `backup.create` | [backup.list](backup/list.md) |

A lost reply never undoes a published change, and nothing is resent for
the caller. See [Errors](../call-table/errors.md), in particular
[uncertain outcomes](../call-table/errors.md#uncertain-outcomes).

## Where calls run

| Family | In the caller's process | In the machine's worker | In the Sandbox daemon |
| --- | --- | --- | --- |
| `machine` | every call (records, provider CLIs, ssh) | — (the worker is what `start` starts) | — |
| `store` | records, locks, PostgreSQL, sqlite on `native` | `store.file` steps for sqlite on another machine | — |
| `project` | records; `init`/`remove` for PostgreSQL and sqlite on `native` | `init`, `remove` for sqlite on another machine | — |
| `branch`, `commit`, `file` | PostgreSQL; sqlite on `native` | sqlite on another machine (`store.call`) | — (no daemon serves Store calls) |
| `sandbox` | records; `create`/`close` on `native` | `sandbox.host` on other machines | the daemon it starts and ends |
| `workspace` | records, `ws` parsing | `sandbox.host` (exec runs the command here) | mount, commit, unmount, status |
| `backup` | records, bucket I/O, restore | the snapshot (`stage`, `read`) of a sqlite Store on another machine | — |

A stopped machine is never started by a call other than
[machine.start](machine/start.md): its calls are `Unavailable`. A
PostgreSQL Store's calls always run in the caller's process, and its
password reference is resolved only there. See the
[routing table](../../control/routing/routing-table.md),
[workers](../../control/routing/workers.md) and the
[daemon channel](../../control/routing/daemon-channel.md).

## How to read a call page

Each call page has the same sections: signature, arguments, options, returns,
where it runs, errors, examples and related calls.

---

← [Call table: Errors](../call-table/errors.md) · [Up: API](../README.md) · [SDK: project](project/README.md) →
