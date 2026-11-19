# SDK

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Every call a program can make: **85 calls in 10 families**, the same names and arguments in JavaScript, Python, the MCP server and the `layerfs` command, one page per family.

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
listing its calls in its `mod.rs`. `layerfs help --json` prints the built
table: per call its family, name, positionals, options and answer form. The
guard [`inventory.mjs`](../../../../api/node/test/branch-file/inventory.mjs)
checks the count line above and one `### family.call` heading per call
against the built table.

## Picture

```text
 1. where things live                   2. what is kept
 +---------------------------+          +-------------------------------+
 | machine  list get stats   |  holds   | store  create (sqlite)        |
 |          create remove    |--------->|        connect (PostgreSQL)   |
 |          start stop update|  sqlite  |        move remove compact    |
 +---------------------------+  Stores  +-------------------------------+
      | runs Sandboxes                        | holds projects
      v                                       v
 +---------------------------+          +-------------------------------+
 | sandbox  create (project, |<---------| project  init <store>/<name>  |
 |          machine) remove  |  of one  |          list get delete      |
 +---------------------------+  project +-------------------------------+
      | mounts Branches                       | has history
      v                                       v
 +---------------------------+  commits +-------------------------------+
 | workspace mount exec      |--------->| branch  fork merge publish    |
 |           commit unmount  |  to its  | commit  list get diff         |
 |           status cancel   |  Branch  | file    read ... write apply  |
 +---------------------------+          +-------------------------------+
 backup: buckets <-> sqlite Store files      home: the home's state, call
                                                   log, doctor, leftovers
 each of home, project, sandbox, machine, store, backup has its doctor
```

## What it does

| Family | Calls, in help order | Runs by default | Source |
| --- | --- | --- | --- |
| [`home`](home.md) (7) | [`init`](home.md#homeinit), [`get`](home.md#homeget), [`log`](home.md#homelog), [`clear_log`](home.md#homeclear_log), [`doctor`](home.md#homedoctor), [`leftovers`](home.md#homeleftovers), [`prune`](home.md#homeprune) | the caller's process; `get` reads files only and never opens the home; `doctor` and `leftovers` ask running machines' workers, `prune` runs on its leftover's machine | [`home/`](../../../../api/crates/layerfs-api/src/home) |
| [`project`](project.md) (8) | [`init`](project.md#projectinit), [`list`](project.md#projectlist), [`get`](project.md#projectget), [`stats`](project.md#projectstats), [`export`](project.md#projectexport), [`delete`](project.md#projectdelete), [`hide_path`](project.md#projecthide_path), [`doctor`](project.md#projectdoctor) | `list`, `get`: home records, then each ready Store's own list; the rest where the Store's calls run | [`project/`](../../../../api/crates/layerfs-api/src/project) |
| [`branch`](branch.md) (6) | [`list`](branch.md#branchlist), [`get`](branch.md#branchget), [`fork`](branch.md#branchfork), [`merge`](branch.md#branchmerge), [`publish`](branch.md#branchpublish), [`remove`](branch.md#branchremove) | where the Store's calls run | [`branch/`](../../../../api/crates/layerfs-api/src/branch) |
| [`commit`](commit.md) (3) | [`list`](commit.md#commitlist), [`get`](commit.md#commitget), [`diff`](commit.md#commitdiff) | where the Store's calls run | [`commit/`](../../../../api/crates/layerfs-api/src/commit) |
| [`file`](file.md) (14) | [`read`](file.md#fileread), [`stat`](file.md#filestat), [`list`](file.md#filelist), [`glob`](file.md#fileglob), [`grep`](file.md#filegrep), [`history`](file.md#filehistory), [`blame`](file.md#fileblame), [`write`](file.md#filewrite), [`edit`](file.md#fileedit), [`rm`](file.md#filerm), [`mkdir`](file.md#filemkdir), [`mv`](file.md#filemv), [`cp`](file.md#filecp), [`apply`](file.md#fileapply) | where the Store's calls run | [`file/`](../../../../api/crates/layerfs-api/src/file) |
| [`sandbox`](sandbox.md) (7) | [`create`](sandbox.md#sandboxcreate), [`list`](sandbox.md#sandboxlist), [`get`](sandbox.md#sandboxget), [`remove`](sandbox.md#sandboxremove), [`files`](sandbox.md#sandboxfiles), [`log`](sandbox.md#sandboxlog), [`doctor`](sandbox.md#sandboxdoctor) | records here; start and stop on the Sandbox's machine; `files`, `log`: where the Sandbox's run folder is | [`sandbox/`](../../../../api/crates/layerfs-api/src/sandbox) |
| [`workspace`](workspace.md) (7) | [`list`](workspace.md#workspacelist), [`mount`](workspace.md#workspacemount), [`exec`](workspace.md#workspaceexec), [`cancel`](workspace.md#workspacecancel), [`commit`](workspace.md#workspacecommit), [`unmount`](workspace.md#workspaceunmount), [`status`](workspace.md#workspacestatus) | the Sandbox's daemon; `exec` and `cancel` in the Sandbox's runtime; `list` from Workspace records | [`workspace/`](../../../../api/crates/layerfs-api/src/workspace) |
| [`machine`](machine.md) (14) | [`list`](machine.md#machinelist), [`get`](machine.md#machineget), [`images`](machine.md#machineimages), [`load_image`](machine.md#machineload_image), [`prune`](machine.md#machineprune), [`stats`](machine.md#machinestats), [`files`](machine.md#machinefiles), [`log`](machine.md#machinelog), [`start`](machine.md#machinestart), [`stop`](machine.md#machinestop), [`create`](machine.md#machinecreate), [`remove`](machine.md#machineremove), [`update`](machine.md#machineupdate), [`doctor`](machine.md#machinedoctor) | the caller's process, through the platform adapter; `stats`, `files`, `log`: where the machine's data is | [`machine/`](../../../../api/crates/layerfs-api/src/machine) |
| [`store`](store.md) (13) | [`list`](store.md#storelist), [`get`](store.md#storeget), [`create`](store.md#storecreate), [`connect`](store.md#storeconnect), [`disconnect`](store.md#storedisconnect), [`save_auth`](store.md#storesave_auth), [`auths`](store.md#storeauths), [`remove_auth`](store.md#storeremove_auth), [`update`](store.md#storeupdate), [`move`](store.md#storemove), [`remove`](store.md#storeremove), [`compact`](store.md#storecompact), [`doctor`](store.md#storedoctor) | the caller's process; sqlite file steps in the Store machine's worker | [`store/`](../../../../api/crates/layerfs-api/src/store) |
| [`backup`](backup.md) (6) | [`connect`](backup.md#backupconnect), [`disconnect`](backup.md#backupdisconnect), [`list`](backup.md#backuplist), [`create`](backup.md#backupcreate), [`restore`](backup.md#backuprestore), [`doctor`](backup.md#backupdoctor) | the caller's process; `create` stages on the Store's machine | [`backup/`](../../../../api/crates/layerfs-api/src/backup) |

"Where the Store's calls run" is the caller's process for a PostgreSQL Store
and for a sqlite Store on a locally executing machine (`native`, except on
Windows), else the Store machine's worker; no daemon serves a Store call. The
full per-call routing is the single table in [Control](../../control/README.md).
Calling conventions (names, options, the short and long form, `--with`,
paging, progress, refusals) are stated once in the
[API layer page](../README.md#calling-conventions) and in
[Calls](../calls/README.md).

Each family page gives, per call: the shell line, the Node and Python
calls, the positionals and options as built (kind, default, required), the
short and long keys of the answer, its `--with` sections, paging and
progress, the refusals the call names with their `fix`, and its MCP profile
(`work` for the table's `agent` profile, `admin` for `full`). Python spells
`with` as `with_` and `from` as `from_`.

| To get | You first need | Made by |
| --- | --- | --- |
| a sqlite Store | nothing (`native` is built in) | [`store.create`](store.md#storecreate) |
| a PostgreSQL Store | a saved login | [`store.save_auth`](store.md#storesave_auth), [`store.connect`](store.md#storeconnect) |
| a project | a Store | [`project.init`](project.md#projectinit) |
| a Branch to work on | a project (`main` is its published line) | [`branch.fork`](branch.md#branchfork) |
| a Sandbox | a project and a machine | [`sandbox.create`](sandbox.md#sandboxcreate) |
| a Workspace | a running Sandbox and a Branch other than `main` | [`workspace.mount`](workspace.md#workspacemount) |
| a Commit | a Workspace, or a Branch for a `file` write | [`workspace.commit`](workspace.md#workspacecommit), [`file.write`](file.md#filewrite) |
| a new `main` head | a Branch with a Commit | [`branch.publish`](branch.md#branchpublish) |
| a VM machine | the platform's tool and the sealed bundle | [`machine.create`](machine.md#machinecreate) |
| a backup | a sqlite Store and a connected bucket | [`backup.create`](backup.md#backupcreate) |

## Removed with no alias

The 1.x names below are gone in 2.0; nothing answers to them.

| Was (removed with no alias) | Now |
| --- | --- |
| `branch.commits`, `branch.get_commit`, `branch.diff` | `commit.list`, `commit.get`, `commit.diff` |
| `file.revision`; `file.diff` | `file.history`; `commit.diff --path` |
| `project.remove_file` | `project.hide_path` |
| `sandbox.close` | `sandbox.remove` |
| `workspace.exec_cancel` | `workspace.cancel` |
| `machine.load`, `machine.status`, `machine.space`, `machine.platforms` | `machine.load_image`, `machine.get --with live`, `machine.stats --with space`, `home.get --with platforms` |
| `store.status`, `store.stats`, `store.push`, `store.pull` | `store.get --with live`, `store.get --with stats`, `store.move` |
| the `logs` family (`logs.*`, among them `logs.machine` and `logs.sandbox`) | `home.log`, `home.clear_log`, `machine.log`, `sandbox.log` |
| `doctor.fix` and the shell's `layerfs doctor` | `home.prune`; the six family doctors |
| paging by `page`, `page_size`, `page_pos`, `has_more`, `next_page_pos`, `continuation` | `--limit N --cursor C` in, `{items, next_cursor}` out |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One file per call under its family, and the family's `mod.rs` lists them in help order (the doctors' shared code is under `calls/doctor/`) | `the_source_is_one_file_per_call_under_its_family` in [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) |
| Every front end exposes exactly the table's calls with the table's arguments | the generated [coverage matrix](../calls/coverage.md), checked by [`coverage.mjs`](../../../../api/node/test/coverage.mjs) |
| These pages name every call once, under `### family.call`, and state the count above | [`inventory.mjs`](../../../../api/node/test/branch-file/inventory.mjs) |
| A call runs once; nothing retries or replays it | [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| Nothing but `machine.start` starts a stopped machine | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Refusals before any effect | `InvalidInput` (unknown call or argument, wrong kind, a section the call does not list, out of scope); a missing required option | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| New work on a removed platform | `Unsupported`, before the home is opened | [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| Uncertain outcomes | `Unknown`; a refusal with `attempted: true` beside its code (`Failed`, or a daemon's `Unavailable`); `Unresolved` on later calls | [Errors and events](../calls/errors-and-events.md#uncertain-outcomes) |
| Each call's own refusals | under each call and in "Family errors" | the family pages |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`inventory.mjs`](../../../../api/node/test/branch-file/inventory.mjs) | `perl -e 'alarm 120; exec @ARGV' node --test api/node/test/branch-file/inventory.mjs` | macOS or Linux, with the built addon (`npm --prefix api run build:native`) and `target/debug/layerfs` | the count line and one heading per call match the built table; branch and file CLI help exposes every option |
| [`calls_table.rs`](../../../../api/crates/layerfs-api/tests/calls_table.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test calls_table` | any Unix host | one source file per call; unknown calls and arguments refused before any effect |
| [`file-history.mjs`](../../../../api/node/examples/file-history.mjs) | `node api/node/examples/file-history.mjs` | macOS or Linux, with the built addon | a Store, Init, file writes and history reads |

## Pages

| Page | What it covers |
| --- | --- |
| [home](home.md) | the home's state, call log, doctor and leftovers |
| [project](project.md) | Init, project records and state, stats, export, delete, hidden paths, the project doctor |
| [branch](branch.md) | Branches: fork, merge, publish, remove |
| [commit](commit.md) | a Branch's Commits, one Commit, comparisons |
| [file](file.md) | reads at a Branch's head or a Commit; one-Commit writes |
| [sandbox](sandbox.md) | Sandboxes: create, list, get, remove, files, log, doctor |
| [workspace](workspace.md) | list, mount, exec, cancel, commit, unmount, status |
| [machine](machine.md) | built-in and VM machines, their images, stats, files and logs |
| [store](store.md) | sqlite and PostgreSQL Stores, saved logins, moves, compaction |
| [backup](backup.md) | buckets, Store backups and restores |
