# SDK

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Every call a program can make: **73 calls in 10 families**, the same names and arguments in JavaScript, Python and the `layerfs` command, one page per family.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-api` |
| Source | [`api/crates/layerfs-api/src`](../../../../api/crates/layerfs-api/src), one folder per family, one file per call (`<family>/<call>.rs`) |
| Uses | [Calls](../calls/README.md), [Control](../../control/README.md), [Store calls](../../engine/store-calls/README.md) |
| Used by | [Front ends](../front-ends/README.md) |
| Flows | [A call, end to end](../../flows/call.md) |

The count is `layerfs_api::calls()`: ten `Family` entries in
[`lib.rs`](../../../../api/crates/layerfs-api/src/lib.rs) `families`, each
listing its calls in its `mod.rs`. The guard
[`inventory.mjs`](../../../../api/node/test/branch-file/inventory.mjs) checks
the count line above and one `### family.call` heading per call against the
built table.

## Picture

```text
 1. where things live                   2. what is kept
 +---------------------------+          +-------------------------------+
 | machine  list platforms   |  holds   | store  create (sqlite, native)|
 |          create remove    |--------->|        connect (PostgreSQL)   |
 |          start stop update|  sqlite  |        push pull remove       |
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
 |           commit unmount  |  to its  | commit  list get diff         |
 |           status          |  Branch  | file    read ... write apply  |
 +---------------------------+          +-------------------------------+
 backup: buckets <-> sqlite Store files      logs: the home's call log
```

## What it does

| Family | Calls, in help order | Runs by default | Source |
| --- | --- | --- | --- |
| [`project`](project.md) | [`init`](project.md#projectinit), [`list`](project.md#projectlist), [`get`](project.md#projectget), [`stats`](project.md#projectstats), [`export`](project.md#projectexport), [`remove`](project.md#projectremove) | `list`, `get`: home records; the rest where the Store's calls run | [`project/`](../../../../api/crates/layerfs-api/src/project) |
| [`branch`](branch.md) | [`list`](branch.md#branchlist), [`get`](branch.md#branchget), [`commits`](branch.md#branchcommits), [`get_commit`](branch.md#branchget_commit), [`fork`](branch.md#branchfork), [`merge`](branch.md#branchmerge), [`publish`](branch.md#branchpublish), [`diff`](branch.md#branchdiff) | where the Store's calls run | [`branch/`](../../../../api/crates/layerfs-api/src/branch) |
| [`commit`](commit.md) | [`list`](commit.md#commitlist), [`get`](commit.md#commitget), [`diff`](commit.md#commitdiff) | where the Store's calls run | [`commit/`](../../../../api/crates/layerfs-api/src/commit) |
| [`file`](file.md) | [`read`](file.md#fileread), [`stat`](file.md#filestat), [`list`](file.md#filelist), [`glob`](file.md#fileglob), [`grep`](file.md#filegrep), [`revision`](file.md#filerevision), [`blame`](file.md#fileblame), [`diff`](file.md#filediff), [`write`](file.md#filewrite), [`edit`](file.md#fileedit), [`rm`](file.md#filerm), [`mkdir`](file.md#filemkdir), [`mv`](file.md#filemv), [`cp`](file.md#filecp), [`apply`](file.md#fileapply) | where the Store's calls run | [`file/`](../../../../api/crates/layerfs-api/src/file) |
| [`sandbox`](sandbox.md) | [`create`](sandbox.md#sandboxcreate), [`list`](sandbox.md#sandboxlist), [`get`](sandbox.md#sandboxget), [`close`](sandbox.md#sandboxclose) | records here; start and stop on the Sandbox's machine | [`sandbox/`](../../../../api/crates/layerfs-api/src/sandbox) |
| [`workspace`](workspace.md) | [`mount`](workspace.md#workspacemount), [`exec`](workspace.md#workspaceexec), [`exec_cancel`](workspace.md#workspaceexec_cancel), [`commit`](workspace.md#workspacecommit), [`unmount`](workspace.md#workspaceunmount), [`status`](workspace.md#workspacestatus) | the Sandbox's daemon; `exec` and `exec_cancel` in the Sandbox's runtime | [`workspace/`](../../../../api/crates/layerfs-api/src/workspace) |
| [`machine`](machine.md) | [`list`](machine.md#machinelist), [`platforms`](machine.md#machineplatforms), [`get`](machine.md#machineget), [`images`](machine.md#machineimages), [`load`](machine.md#machineload), [`prune`](machine.md#machineprune), [`status`](machine.md#machinestatus), [`start`](machine.md#machinestart), [`stop`](machine.md#machinestop), [`create`](machine.md#machinecreate), [`remove`](machine.md#machineremove), [`update`](machine.md#machineupdate) | the caller's process, through the platform adapter | [`machine/`](../../../../api/crates/layerfs-api/src/machine) |
| [`store`](store.md) | [`list`](store.md#storelist), [`get`](store.md#storeget), [`status`](store.md#storestatus), [`stats`](store.md#storestats), [`create`](store.md#storecreate), [`connect`](store.md#storeconnect), [`attach`](store.md#storeattach), [`detach`](store.md#storedetach), [`push`](store.md#storepush), [`pull`](store.md#storepull), [`remove`](store.md#storeremove), [`disconnect`](store.md#storedisconnect) | the caller's process; sqlite file steps in the Store machine's worker | [`store/`](../../../../api/crates/layerfs-api/src/store) |
| [`backup`](backup.md) | [`connect`](backup.md#backupconnect), [`disconnect`](backup.md#backupdisconnect), [`list`](backup.md#backuplist), [`create`](backup.md#backupcreate), [`restore`](backup.md#backuprestore) | the caller's process; `create` stages on the Store's machine | [`backup/`](../../../../api/crates/layerfs-api/src/backup) |
| [`logs`](logs.md) | [`list`](logs.md#logslist), [`reset`](logs.md#logsreset) | the caller's process, home files only | [`logs/`](../../../../api/crates/layerfs-api/src/logs) |

"Where the Store's calls run" is the caller's process for a PostgreSQL Store
and for a sqlite Store on a locally executing machine (`native`, except on
Windows), else the Store machine's worker; no daemon serves a Store call. The
full per-call routing is the single table in [Control](../../control/README.md).
Calling conventions (names, options, answers, paging, scope) are stated once
in the [API layer page](../README.md#calling-conventions).

| To get | You first need | Made by |
| --- | --- | --- |
| a sqlite Store | nothing (`native` is built in) | [`store.create`](store.md#storecreate) |
| a project | a Store | [`project.init`](project.md#projectinit) |
| a Branch to work on | a project (`main` is its published line) | [`branch.fork`](branch.md#branchfork) |
| a Sandbox | a project and a machine | [`sandbox.create`](sandbox.md#sandboxcreate) |
| a Workspace | a running Sandbox and a Branch other than `main` | [`workspace.mount`](workspace.md#workspacemount) |
| a Commit | a Workspace, or a Branch for a `file` write | [`workspace.commit`](workspace.md#workspacecommit), [`file.write`](file.md#filewrite) |
| a new `main` Layer | a Branch with a Commit | [`branch.publish`](branch.md#branchpublish) |
| a VM machine | the platform's tool and the sealed bundle | [`machine.create`](machine.md#machinecreate) |
| a backup | a sqlite Store and a connected bucket | [`backup.create`](backup.md#backupcreate) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One file per call under its family, and the family's `mod.rs` lists them in help order | `the_source_is_one_file_per_call_under_its_family` in [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) |
| Every front end exposes exactly the table's calls with the table's arguments | the generated [coverage matrix](../calls/coverage.md), checked by [`coverage.mjs`](../../../../api/node/test/coverage.mjs) |
| These pages name every call once, under `### family.call`, and state the count above | [`inventory.mjs`](../../../../api/node/test/branch-file/inventory.mjs) |
| A call runs once; nothing retries or replays it | [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Nothing but `machine.start` starts a stopped machine | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Refusals before any effect | `InvalidInput` (unknown call or argument, wrong kind, out of scope) | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| New work on a removed platform | `Unsupported`, before the home is opened | [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| Uncertain outcomes | `Unknown`; `Failed` with `attempted`; `Unresolved` on later calls | [Errors and events](../calls/errors-and-events.md#uncertain-outcomes) |
| Each family's own codes | under each call and in "Family errors" | the family pages |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`inventory.mjs`](../../../../api/node/test/branch-file/inventory.mjs) | `perl -e 'alarm 120; exec @ARGV' node --test api/node/test/branch-file/inventory.mjs` | macOS or Linux, with the built addon (`npm --prefix api run build:native`) and `target/debug/layerfs` | the count line and one heading per call match the built table; branch and file CLI help exposes every option |
| [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls_table` | any Unix host | 73 calls in ten families; one source file per call; unknown calls and arguments refused before any effect |
| [`file-history.mjs`](../../../../api/node/examples/file-history.mjs) | `node api/node/examples/file-history.mjs` | macOS or Linux, with the built addon | a Store, Init, file writes and history reads |

## Pages

| Page | What it covers |
| --- | --- |
| [project](project.md) | Init, project records, stats, export, remove |
| [branch](branch.md) | Branches, saved history, fork, merge, publish, diff; the saved-query rules |
| [commit](commit.md) | a Branch's Commits and saved comparisons |
| [file](file.md) | reads at a saved version; one-Commit writes |
| [sandbox](sandbox.md) | Sandboxes: create, list, get, close |
| [workspace](workspace.md) | mount, exec, cancel, commit, status, unmount |
| [machine](machine.md) | built-in and VM machines |
| [store](store.md) | sqlite and PostgreSQL Stores |
| [backup](backup.md) | buckets and Store backups |
| [logs](logs.md) | the home's call log |
