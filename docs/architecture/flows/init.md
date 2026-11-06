# Project Init

[Architecture](../README.md) › [Flows](README.md) › Project Init

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`project.init`: reading a native directory and publishing the project's first Commit.

`project.init(project, folder, { timeout, on_progress })` adds a new project
to a Store that already exists (`store.create` made it). It is a live add:
the Store stays open for daemons and other processes, and nothing is sealed
or replaced. Terms are in the [glossary](../glossary.md).

## The path

```text
 caller process            machine worker            Store
                           (sqlite Store on           (SQLite file or
                            another kernel)            PostgreSQL schema)
 ------------------------  ------------------------  ------------------
 table check; split
 <store>/<project>
 record unblocked
 progress "check"
 place::plan(StoreRef)
   |
   +-- local sqlite: lease::hold
   |   add_project: kernel guard, entry in the home
   |   ProjectApi::add_project ----------------------> writable open
   |     init_observed: scan, construct, Save -------> short writes
   |                                                 [each: admission
   |                                                  turn, flock,
   |                                                  BEGIN IMMEDIATE]
   |     discard working rows; Save::finish ---------> writes
   |     initialize_layerstack (genesis Layer) -----> 1 write tx
   |     fork first Branch -------------------------> 1 write tx
   |   close session (no seal)
   |
   +-- worker: walk folder here; room check
   =====|== process boundary: 127.0.0.1:<port> ==|
       project.stage x N -->  write the copy
       project.init -------->  lease; add_project on
                               the copy -----------> (same writes)
                               remove the copy
       <---------------------  {stack, layer, root}
   |
   +-- postgresql: postgres_init::add_project ------> (same writes on
                                                     the schema; lane
                                                     = store_lane row)
 progress "record": write the project record
 answer: project summary + layer, root
```

| Step | Component |
| --- | --- |
| 1 | [API: Call table](../api/call-table/README.md) |
| 2 | [Control: Management](../control/management/README.md) |
| 3 | [Engine: Project Init](../engine/project-init/README.md) |
| 4 | [Core: Filesystem trees](../core/filesystem-trees/README.md) |
| 5 | [Core: Files](../core/files/README.md) |
| 6 | [Core: Physical storage](../core/physical-storage/README.md) |
| 7 | [Engine: Store](../engine/store/README.md) |

## Steps

### Where Init runs

| Store kind and machine | Init runs in | Source |
| --- | --- | --- |
| sqlite, machine whose platform runs calls locally (`native`) | the caller's process, under its lease on the Store | [`project/family.rs`](../../../control/crates/layerfs-store/src/project/family.rs) `init` |
| sqlite, any other machine | the machine's worker, on a copy of the folder the caller sent; the caller holds its lease meanwhile | `init_on_worker`, [`project/send.rs`](../../../control/crates/layerfs-store/src/project/send.rs), [`project/init.rs`](../../../control/crates/layerfs-store/src/project/init.rs) `in_worker` |
| postgresql | the caller's process, against the schema | `init_postgresql`, [`project/postgres_init.rs`](../../../control/crates/layerfs-store/src/project/postgres_init.rs) |

The route follows [Routing table: by Store kind](../control/routing/routing-table.md#by-store-kind):
Init runs where the Store's calls run.

### Sequence

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 1 | The table checks `project` and `folder`; `timeout` defaults to 3,600,000 ms. The folder must be absolute; the Store must be recorded and hold no blocking intent. Progress `check`. | [SDK: project](../api/sdk/project/README.md) | [`project/init.rs` (api)](../../../api/crates/layerfs-api/src/project/init.rs), [`project/family.rs`](../../../control/crates/layerfs-store/src/project/family.rs) |
| 2 | Worker route only: the caller walks the folder on its own host and refuses what Init would refuse, checks the machine has room, then sends the copy in `project.stage` requests (progress `send`). | [Routing: workers](../control/routing/workers.md) | [`project/send.rs`](../../../control/crates/layerfs-store/src/project/send.rs), [`project/staged.rs`](../../../control/crates/layerfs-store/src/project/staged.rs) |
| 3 | `add_project` (progress `install`): the deadline from `timeoutMs`; the kernel guard on the Store's directory; the Store file must exist; the project's entry in the home is created exclusively; fresh LayerStack and Branch ids are drawn. | [Management](../control/management/README.md) | [`identity.rs`](../../../control/crates/layerfs-store/src/identity.rs) `add_project` |
| 4 | `ProjectApi::add_project` opens writable handles on the live Store (SQLite must be Disposable) and refuses a stack name already taken. | [Project Init: seal and install](../engine/project-init/seal-and-install.md#live-open) | [`store/live.rs`](../../../engine/crates/layerfs-project/src/store/live.rs) |
| 5 | Import: `init_observed` scans the complete folder without following links, records working rows in the acquisition tables, and keeps input-sized state there, not in memory. | [Project Init: import](../engine/project-init/import.md) | [`import/init.rs`](../../../engine/crates/layerfs-project/src/import/init.rs), [`import/scan.rs`](../../../engine/crates/layerfs-project/src/import/scan.rs) |
| 6 | Construction: file content becomes chunked file objects (4 constructor threads, one Save owner); directories and the inode table are built from the backed rows. Objects go through the Storage Save into packs. | [Project Init: construction](../engine/project-init/construction.md), [Files: construction](../core/files/construction.md), [Trees: representation](../core/filesystem-trees/representation.md), [Physical storage: saves](../core/physical-storage/saves.md) | [`import/files.rs`](../../../engine/crates/layerfs-project/src/import/files.rs), [`import/namespace.rs`](../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| 7 | Publish: the working rows are discarded, `Save::finish` completes the objects, the logical size is counted, binding hints are recorded, and `initialize_layerstack` writes the stack and its genesis Layer in one transaction. | [Store: history publication](../engine/store/history-publication.md#branch-and-commit-rows), [History model: entities](../core/history-model/entities.md#layerstacks) | [`import/init.rs`](../../../engine/crates/layerfs-project/src/import/init.rs) |
| 8 | The first Branch is forked from the genesis Layer (catalog name `main`); the session closes. The Store is not sealed. | [Project Init: seal and install](../engine/project-init/seal-and-install.md#live-open) | [`store/live.rs`](../../../engine/crates/layerfs-project/src/store/live.rs) |
| 9 | The project's entry is saved with its manifest; progress `record`; the caller writes or stamps the project record and answers the project summary with `layer` and `root`. | [Management: home and records](../control/management/home-and-records.md#records) | [`identity.rs`](../../../control/crates/layerfs-store/src/identity.rs), [`project/family.rs`](../../../control/crates/layerfs-store/src/project/family.rs) |

Every Store write in steps 5–8 is a short write transaction through the
Store's one writer: admission turn, the Linux `flock` on the Store's
directory, then `BEGIN IMMEDIATE` (PostgreSQL: the `store_lane` row). Init
does not hold the publish lane across transactions, so daemons and SDK
processes keep reading and publishing between its writes
([Store: one writer](../engine/store/backends.md#one-writer)).

### Seal and install

The live add neither seals nor installs. `store.create` runs
`ProjectApi::create_store`, which makes an empty Store with acquisition
tables and seals it. Sandbox daemons then open that Store file directly:
every product Sandbox start sets `DaemonStore::Sqlite { install: Some(..) }`,
so the daemon opens the existing Store and the one-time install stream is not
sent ([`lifecycle.rs`](../../../control/crates/layerfs-sandbox/src/lifecycle.rs),
[`launch/local.rs`](../../../control/crates/layerfs-sandbox/src/launch/local.rs),
[`store.rs` (bridge)](../../../engine/crates/layerfs-bridge/src/store.rs) `opens_existing`).
Seal and the install stream are described in
[Project Init: seal and install](../engine/project-init/seal-and-install.md).

### What an uncertain Init leaves

| Case | Left behind | Why it is kept |
| --- | --- | --- |
| Engine outcome unknown (`ProjectError::Uncertain`) | The acquisition operation's record and working rows; the project's entry in the home | Nothing may guess the outcome; a later add into the same Store is refused `Busy` while the rows remain ([Seal and install: uncertain Init](../engine/project-init/seal-and-install.md#uncertain-init)) |
| Worker answer lost | Whatever the worker's add did; the caller writes no project record, so `project list` does not show it | The worker removes its folder copy when its Init ends; nothing is resent |
| Definite refusal before any Store write (name taken, no acquisition tables, another add working, refused source entry) | Nothing: the entry this call made is removed | `add_failure` in [`identity.rs`](../../../control/crates/layerfs-store/src/identity.rs) |

## Failures along the way

| Step | What can fail | What the caller sees |
| --- | --- | --- |
| 1 | `project` not `<store>/<project>`, bad project name, folder not absolute | `InvalidInput` |
| 1 | No such Store | `NotFound` "no Store named …; `store create` makes one" |
| 1 | Store record holds another operation | `Busy` or `Unresolved` |
| 2 | Project record already exists | `Exists` |
| 2 | Folder entry Init would refuse, or a read error | refused before anything is sent (`InvalidInput` or `Failed`) |
| 2 | Not enough free disk on the machine | `Capacity`, with `needed` and `free` |
| 2 | Worker not reached | `Unavailable`; nothing sent |
| 2 | A stage or the `project.init` answer lost | `Unknown`, naming the copy the worker removes when its Init ends; no project record |
| 3 | A PostgreSQL Store in a build without the `postgresql` feature | `Unsupported` |
| 3 | Bad `timeoutMs` | `InvalidInput` |
| 3 | Kernel guard refuses the Store's filesystem | `Unsupported` |
| 3 | No Store file | `NoStore` |
| 3–4 | Project name taken (entry or stack) | `Exists` |
| 5 | Store made without acquisition tables | `Unsupported` |
| 5 | Another add's working rows remain | `Busy` |
| 5–6 | Device, FIFO or socket; set-user-ID or sticky bits; source changed while read | `InvalidInput` "adding … refused <path>: …" or `Failed` |
| 5–8 | Deadline passed, Store or history outcome unknown, cleanup failed | `Failed`, its message the engine's `InitError`; the entry is kept for diagnosis |

`ProjectError::Uncertain` reaches the caller as `Failed`, not `Unknown`:
`add_failure` maps only `Exists`, `NoAcquisition`, `Busy` and invalid source
input to their own codes ([`identity.rs`](../../../control/crates/layerfs-store/src/identity.rs)).
The message carries the retained operation.

## Demonstration

These commands were written from source and were not run while writing this
page.

| Test | What it sets up | What it checks | Command | Needs |
| --- | --- | --- | --- | --- |
| [`init_sqlite.rs`](../../../engine/crates/layerfs-project/tests/init_sqlite.rs) | Fixtures of 100 and 1,000 files; a new Disposable SQLite Store | The Init root equals the root built with memory ports and passes the namespace oracle; no working rows remain; a Store without acquisition tables and a Store inside the source are refused | `cargo +1.85.1 test --locked -p layerfs-project --test init_sqlite` | macOS or Linux for the Init tests; other platforms run only the refusal test |
| [`project_family.rs`](../../../api/crates/layerfs-api/tests/project_family.rs) | A home, a Store on `native`, a folder, and an in-process worker for a `lima` machine record | `project.init` as a live add through the call table, then file and Branch calls, a move to the worker machine, and `project.remove` | `cargo +1.85.1 test --locked -p layerfs-api --test project_family` | Unix; a private (0700) home; loopback TCP |
| [`project_init_worker.rs`](../../../api/crates/layerfs-api/tests/project_init_worker.rs) | A Store behind an in-process worker and a folder under the caller's temp directory | The caller copies the folder to the worker, which adds it and removes the copy; the copy holds what Init reads; a copy that fails midway is discarded | `cargo +1.85.1 test --locked -p layerfs-api --test project_init_worker` | Unix; loopback TCP |

[`api/crates/layerfs-api/tests/init.rs`](../../../api/crates/layerfs-api/tests/init.rs),
listed in the outline, calls `ProjectApi::init` (the Init-and-seal route of
tests and examples), not `project.init`; it checks a sealed Store with a
complete root and first Branch and that a refusal keeps existing output.

---

← [Flow: A call, end to end](call.md) · [Up: Flows](README.md) · [Flow: A Workspace session](workspace.md) →
