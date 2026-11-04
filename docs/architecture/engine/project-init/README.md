# Project Init

[Architecture](../../README.md) › [Engine](../README.md) › Project Init

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Turning a native directory into a project's first Commit: import, namespace construction, seal and install.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-project` |
| Source | [`engine/crates/layerfs-project/src/import`](../../../../engine/crates/layerfs-project/src/import), [`engine/crates/layerfs-project/src/store`](../../../../engine/crates/layerfs-project/src/store) |
| Uses | [Store](../store/README.md), [Core: Filesystem trees](../../core/filesystem-trees/README.md), [Core: Files](../../core/files/README.md) |
| Used by | [API: Control](../../api/control/README.md) |
| Flows | [Project Init](../../flows/init.md) |

A *project* is a named LayerStack in a Store ([glossary](../../glossary.md)).
Init reads one host directory completely and publishes its genesis
LayerStack and first Branch.

## Topics

| Topic | What it covers |
| --- | --- |
| [Import](import.md) | Reading a native directory. |
| [Construction](construction.md) | Building the project's first Commit. |
| [Seal and install](seal-and-install.md) | Sealing and installing the new Store. |

## The picture

```text
 native directory                     Store (SQLite file or PG schema)
 ----------------                     --------------------------------
 source/ --+   Import                 +----------------------------+
           +-> scan and observe ----->| acquisition tables         |
           |   (no link following)    | one operation's work rows  |
           |                          +----------------------------+
           |                                 | keyset windows
           |   Construction                  v
           +-> files: construct_stream --> Save --> objects, packs
               4 workers, 1 Save owner       |
               namespace: build_directory,   |
               build_table, FilesystemRoot   |
                                             v
               discard work rows -> Save::finish -> binding hints
               -> initialize_layerstack -> fork first Branch
                                             |
            +--------------------------------+----------------+
            v                                                 v
  live add (product project.init)          Init and seal (new Store)
  close session; Store stays open          record_init, seal,
  for daemons and SDK processes            install_file -> daemon
```

| Part | Source | Topic |
| --- | --- | --- |
| Scan, sources, backing windows | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs), [`import/source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs), [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs) | [Import](import.md) |
| Files, namespace, history | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs), [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs), [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) | [Construction](construction.md) |
| Store creation, add, seal, install | [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs), [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs), [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs) | [Seal and install](seal-and-install.md) |

## What it does

| Entry (`ProjectApi`) | Reads | Writes | Product caller |
| --- | --- | --- | --- |
| `add_project` | the complete source directory | objects, binding hints, a new LayerStack and its first Branch, through the live Store's writer | `project.init` ([`store/identity.rs`](../../../../api/crates/layerfs-control/src/store/identity.rs), [`project/postgres_init.rs`](../../../../api/crates/layerfs-control/src/project/postgres_init.rs)) |
| `create_store` | nothing | a new empty SQLite Store with acquisition tables, then sealed | `store.create` ([`store/identity.rs`](../../../../api/crates/layerfs-control/src/store/identity.rs)) |
| `create_postgresql_store` | nothing | a new PostgreSQL schema with acquisition tables (feature `postgresql`) | [`project/postgres_init.rs`](../../../../api/crates/layerfs-control/src/project/postgres_init.rs) |
| `init`, `init_observed` | the complete source directory | a new SQLite Store holding one project, Init statistics, then sealed | tests and examples (for example [`host_install.rs`](../../../../engine/crates/layerfs-daemon/tests/support/host_install.rs)) |
| `install`, `install_file` | a sealed Store file | the file, streamed to a daemon over the bridge | [`daemon/channel.rs`](../../../../api/crates/layerfs-control/src/daemon/channel.rs) |
| `projects` | the Store's catalog | nothing | project listing ([`store/identity.rs`](../../../../api/crates/layerfs-control/src/store/identity.rs)) |

Source: [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs),
[`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs),
[`store/mod.rs`](../../../../engine/crates/layerfs-project/src/store/mod.rs).
All routes run the same namespace Init,
[`init_observed`](../../../../engine/crates/layerfs-project/src/import/init.rs);
only the opened handles differ.

### Init and Commit share Content, Storage and Persistence

| Work | Init route | Commit route | Shared library |
| --- | --- | --- | --- |
| File content | `construct_stream` over the host file | `construct_runs` or indexed edits over captured data | Content |
| Directories and inode table | `build_directory`, `build_table` from backed rows | `update_filesystem_streamed_backed` | Content |
| Objects to packs | `Save::accept`, `Save::finish` | the same Save | Storage |
| Rows and history | `HistoryCatalog` on the Store's handles | `commit_branch` on the same Store | Persistence |

Init adds no encoder of its own: `layerfs-project` depends only on
`layerfs-content`, `layerfs-storage`, `layerfs-history`, `layerfs-persistence`,
`layerfs-bridge` and `layerfs-telemetry`
([`Cargo.toml`](../../../../engine/crates/layerfs-project/Cargo.toml)). The
Commit side is described in [Commit: Construction](../commit/construction.md).

## Invariants

| Rule | Enforced by |
| --- | --- |
| The source is the complete tree: `.git`, ignored files, dependencies, caches and outputs are imported; there is no Git-ignore filtering. | `InitRequest::source` ([`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs)); [`complete_root.rs`](../../../../engine/crates/layerfs-project/tests/complete_root.rs) |
| Initial full-root acquisition is explicit: only a `ProjectApi` call runs it. Mount and Commit never do. | No production dependency on `layerfs-project` from `layerfs-workspace`, `layerfs-daemon`, `layerfs-store-service` or `layerfs-nfs` (dev-dependencies only; see each crate's `Cargo.toml`) |
| One encoder: file and tree objects come from Content and go through the Storage Save. | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs), [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| A SQLite Store must use the Disposable profile. | `initialize`, `live::disposable`, `install_file` ([`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs), [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs), [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs)) |
| Symbolic links are stored, never followed. | `read_link` ([`import/source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs)) |
| Each host regular-file identity is constructed once; hard links share its serial. | `Bound`, `check_aliases` ([`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs), [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs)) |
| Working rows are removed before the tree Save finishes and before any history is published. | `init_observed` ([`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs)) |
| Every backing call, Store write and install send is one attempt; nothing is retried. | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs), [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs) |
| Input-sized state lives in the acquisition backing; resident buffers are fixed windows. | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs); [`namespace_scaling.rs`](../../../../engine/crates/layerfs-project/tests/namespace_scaling.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Platform | Init needs Unix metadata; elsewhere `ProjectError::Unsupported` | [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| Resident child buffer and job queue | 512 rows (`WINDOW_ROWS`) | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs) |
| Backing read window | 512 rows, 256 KiB (`Limits::MAXIMUM`) | [`contract.rs`](../../../../core/crates/layerfs-storage/src/port/acquisition/contract.rs) |
| Backing write window | 4096 rows, 1 MiB (`WRITE_WINDOW_ROWS`, `WRITE_WINDOW_BYTES`) | [`contract.rs`](../../../../core/crates/layerfs-storage/src/port/acquisition/contract.rs) |
| File constructor threads | 4 (`INIT_WORKERS`) | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Deadline | `ProjectError::Deadline`; the caller supplies it (`timeoutMs` in the API) | [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs), [`project/init.rs`](../../../../api/crates/layerfs-control/src/project/init.rs) |
| Device, FIFO or socket in the source | `ProjectError::Unsupported` | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Set-user-ID, set-group-ID, sticky regular file; bad mtime | `InvalidInput { Mode \| Mtime }` | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Source changed while read | `InvalidInput { Changed }` | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Store inside the source | `ProjectError::BackingInsideSource`, nothing begun | [`import/source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |
| Position or serial overflow | `ProjectError::Capacity` | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs), [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| Unknown Store, history or backing outcome | `ProjectError::Uncertain`; working rows left for their owner | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs) |
| Removing working rows failed | `ProjectError::Cleanup`; nothing was published | [`import/error.rs`](../../../../engine/crates/layerfs-project/src/import/error.rs) |
| Add: name taken, no acquisition tables, another add's rows remain | `InitError::Exists`, `NoAcquisition`, `Busy` | [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs) |

Init sets no total file, entry or byte cap. What an uncertain Init leaves
behind is in [Seal and install](seal-and-install.md#uncertain-init).

## Demonstration

The commands below are written from source and were not run while writing
this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`init_sqlite.rs`](../../../../engine/crates/layerfs-project/tests/init_sqlite.rs) | `layerfs-project` | Init into a Disposable SQLite Store matches the in-memory root, passes the namespace oracle and leaves no working rows; refusals for missing acquisition tables and a Store inside the source |
| [`complete_root.rs`](../../../../engine/crates/layerfs-project/tests/complete_root.rs) | `layerfs-project` | `.git`, ignored files, `node_modules`, caches, outputs, hard links and symlink targets are all imported exactly |
| [`namespace_scaling.rs`](../../../../engine/crates/layerfs-project/tests/namespace_scaling.rs) | `layerfs-project` | Backed counts grow with the tree while the child buffer and both windows stay fixed |
| [`init_postgresql.rs`](../../../../engine/crates/layerfs-project/tests/init_postgresql.rs) | `layerfs-project` | The same folder added to a SQLite and a PostgreSQL Store gives the same root |
| [`verify_namespace.rs`](../../../../engine/crates/layerfs-project/examples/verify_namespace.rs) | `layerfs-project` | Independent check of an existing Store's genesis root against a manifest |

```text
 init_sqlite:  Fixture(100 | 1000 files) --> SqliteHandles::create
               --> init --> verify_namespace oracle
               --> root == root from memory ports
               --> acquisition.abandoned() is empty --> seal
```

| Run | Command | Needs |
| --- | --- | --- |
| `init_sqlite` | `cargo +1.85.1 test --locked -p layerfs-project --test init_sqlite` | macOS or Linux for the Init tests; other platforms run only the refusal test |
| `complete_root` | `cargo +1.85.1 test --locked -p layerfs-project --test complete_root` | Unix; the SQLite-backed test is `#[cfg(target_os = "macos")]` |
| `namespace_scaling` | `cargo +1.85.1 test --locked -p layerfs-project --test namespace_scaling` | Unix (memory ports) |
| `init_postgresql` | `cargo +1.85.1 test --locked -p layerfs-project --features postgresql --test init_postgresql` | Unix, feature `postgresql`, `LAYERFS_TEST_POSTGRES_URL`; without it the test reports NOT_RUN |
| `verify_namespace` | `cargo +1.85.1 run --locked -p layerfs-project --example verify_namespace -- <store> <unused> <root-hex> <stack-hex> <manifest.tsv> <manifest-sha256> disposable` | `LAYERFS_HISTORY_CURSOR_KEY` (hex); binding `layerfs-bench-pro`; it seals the Store at the end |

`verify_namespace` defaults to the Durable profile when the last argument is
omitted. Durable execution is disabled by the owner, so pass `disposable`.
Its second argument is not read.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/07-importing.md`](../../../../core/docs/architecture/07-importing.md) — historical only (an unexecuted v0.1.7 proposal).
- [`core/docs/architecture/43-backed-initial-acquisition.md`](../../../../core/docs/architecture/43-backed-initial-acquisition.md) — covered in [Import](import.md) and [Construction](construction.md).
- [`core/docs/architecture/44-acquisition-backing.md`](../../../../core/docs/architecture/44-acquisition-backing.md) — covered in [Import: Backing](import.md#backing).
- [`core/docs/architecture/67-native-store-install.md`](../../../../core/docs/architecture/67-native-store-install.md) — covered in [Seal and install](seal-and-install.md#install).

---

← [Store calls: SDK facade](../store-calls/sdk-facade.md) · [Up: Engine](../README.md) · [Project Init: Import](import.md) →
