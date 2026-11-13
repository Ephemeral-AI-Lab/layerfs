# Project Init

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Turning one complete native directory into a project's genesis LayerStack and first Branch: import, construction, then seal and install or a live add.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-project` |
| Source | [`engine/crates/layerfs-project/src/import`](../../../../engine/crates/layerfs-project/src/import), [`engine/crates/layerfs-project/src/store`](../../../../engine/crates/layerfs-project/src/store) |
| Uses | [Store](../store/README.md), [Core: Files](../../core/files/README.md), [Core: Filesystem trees](../../core/filesystem-trees/README.md), [Core: Physical storage](../../core/physical-storage/README.md), [Core: History](../../core/history/README.md), [Bridge](../bridge/README.md) (install) |
| Used by | [Control: Stores and projects](../../control/stores-and-projects/README.md), [Control: Sandboxes](../../control/sandboxes/README.md) (install), [Daemon](../daemon/README.md) (receives install) |
| Flows | [Project Init](../../flows/init.md) |

A project is a named LayerStack in a Store. Init reads the whole source:
`.git`, ignored files, dependencies, caches and outputs are imported; there
is no Git-ignore filtering.

## Picture

```text
 source dir                              Store (SQLite file or PG schema)
   | Scan: breadth first, name order,    +------------------------------+
   | never follow links  --------------->| acquisition tables (one      |
   |                                     | operation's working rows)    |
   |                                     +------------------------------+
   | Files: 4 workers construct_stream          | keyset windows
   |   --> batches --> 1 Save owner             v
   | Tree: build_directory, build_table, FilesystemRoot --> Save
   v
 discard working rows -> Save::finish -> binding hints
   -> initialize_layerstack -> fork first Branch
        |                                        |
        v                                        v
 live add (project.init)                  init / create_store
 close session; Store stays open          seal -> install_file --> daemon
```

## What it does

### Entries

| Entry (`ProjectApi`) | Reads | Writes | Product caller |
| --- | --- | --- | --- |
| `add_project` | the complete source | objects, hints, a LayerStack and first Branch, through the live Store's writer lane | `project.init`: [`project/add.rs`](../../../../control/crates/layerfs-store/src/project/add.rs), [`project/postgres_init.rs`](../../../../control/crates/layerfs-store/src/project/postgres_init.rs) |
| `create_store` | nothing | a new empty SQLite Store with acquisition tables, sealed | `store.create`: [`identity.rs`](../../../../control/crates/layerfs-store/src/identity.rs) |
| `create_postgresql_store` | nothing | a new PostgreSQL schema with acquisition tables, in one transaction (feature `postgresql`) | [`project/postgres_init.rs`](../../../../control/crates/layerfs-store/src/project/postgres_init.rs) |
| `init`, `init_observed` | the complete source | a new SQLite Store holding one project and its Init statistics, sealed | tests and examples |
| `install`, `install_file` | a sealed Store file | the file, streamed to a daemon | [`daemon/channel.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/channel.rs) |
| `projects` | the catalog | nothing | [`identity.rs`](../../../../control/crates/layerfs-store/src/identity.rs) |

Every route runs the same namespace Init,
[`init_observed`](../../../../engine/crates/layerfs-project/src/import/init.rs);
only the opened handles differ ([`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs),
[`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs)).

### Import

| Step | What happens | Source |
| --- | --- | --- |
| Root | A real directory, not a symlink, at position 0. The Store's directory, resolved through links, must not be the source or under it (`BackingInsideSource`, before anything begins). | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs), [`import/source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |
| Order | Directories read in position order, so each parent's children are contiguous and sorted by name bytes. | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Observe | `symlink_metadata`, never followed; a symlink target is read once, re-stat'ed and kept as bytes. Dangling, external and cyclic links are stored as they are. | [`import/source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |
| Wide directory | At 512 children the buffer is written unpositioned; the backing orders them and `place_wide` positions them a window at a time. | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Hard links | The backing answers each regular file's canonical position (first path with that device and inode). Later paths bind the same serial and are never constructed; `check_aliases` re-stats them. Equal bytes alone make no alias. | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs), [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |

| Kind | Stored mode | Allowed bits |
| --- | --- | --- |
| Directory | `mode & 0o7777` | `0o1777` |
| Regular file | `mode & 0o7777`, with a 44-byte host evidence stamp (length, mode, mtime, ctime) | `0o777` |
| Symlink | `0o777` | `0o777` |
| Device, FIFO, socket | refused `Unsupported` | — |

### Construction

```text
 init_observed                                     InitStep
   check_root, outside_source, acquisition.begin   Scan
   Save #1: scan rows + symlink targets; finish
   Save #2: files (4 workers -> 1 owner); finish   Files
   reserve_inodes(scope, entries)                  Tree
   Save #3: directories, inode table, root
   discard working rows                            Publish
   Save #3 finish; logical size; record_new_bindings
   initialize_layerstack(genesis_root)
 caller: fork first Branch; seal or close          Seal
```

| Step | Library call | Source |
| --- | --- | --- |
| Files | Content `construct_stream`, one job per host identity | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Serials | `reserve_inodes`: one contiguous range, serial = start + position; scope from `scope_for_seed` | [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| Directories, table, root | Content `build_directory`, `empty_directory` (once), `build_table`, `FilesystemRoot::new` | [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| Binding hints | Every binding of the new root, 4096 rows per write | [`import/hints.rs`](../../../../engine/crates/layerfs-project/src/import/hints.rs) |
| Genesis and Branch | `initialize_layerstack`, then `fork` from the head Layer | [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs), [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs) |

Three Saves keep each step reading only finished objects. Init adds no
encoder: `layerfs-project` depends only on Content, Storage, History,
Persistence, Bridge and Telemetry, the same libraries a Workspace Commit uses
([`Cargo.toml`](../../../../engine/crates/layerfs-project/Cargo.toml),
[Commit](../commit/README.md)).

```text
 owner thread (holds Save and backing)
   backing.jobs --> job feed sync_channel(512) --> workers 1..4
                    construct_stream, stamp checks, deadline
   <-- ImportBatch sync_channel(QUEUE_SLOTS = 32): <= 256 KiB,
       <= 512 objects, <= 512 completions
   Save::accept; complete_files every 4096 roots
```

Init keeps this parallel profile (`INIT_WORKERS` = 4, fixed in source); every
other measured operation uses one construction producer. The owner waits on
batches with a 500 ms timeout only to check the deadline; that is a
readiness wait, not a retry ([`import/batch.rs`](../../../../engine/crates/layerfs-project/src/import/batch.rs)).

### Seal and install

```text
 SqliteHandles::seal                     install_file  -->  daemon
   sole session (else Busy)                manifest      Request, Claim
   wal_checkpoint(TRUNCATE) (busy -> Busy) <= 65,519 B   Transfer
   close; no -wal/-shm/-journal remains    per record
   macOS: release unused allocation        "LFSF\x01"    Publish (rename)
   --> SealedStore{path, profile, ..}                    Open, check binding
                                           <-- Installed(manifest + daemon
                                               SQLite version)
```

| Rule | Source |
| --- | --- |
| Seal consumes the sole session after `Save::finish` and publication; it is never called on a live daemon's shared Store. `initialize` then builds a `StoreManifest` and returns a `SealedProject`. | [`store/seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs), [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs) |
| Install checks Disposable, file kind and length against the manifest, streams `MAX_PLAINTEXT_BYTES` records from one fixed buffer, then the finish marker; it never opens the file as a database. A reply manifest that differs from the sent one plus the daemon's SQLite version is a protocol mismatch. | [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs), [`control.rs`](../../../../engine/crates/layerfs-bridge/src/control.rs) |
| On a failure after the request was attempted the caller closes its send side (the fence), unless the failure was a channel error. | [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs) |

The receiving side is in [Daemon](../daemon/README.md); the wire records are
in [Bridge](../bridge/README.md).

### Live add

`add_project` opens a writable session on a Store already in use (SQLite
file, or a PostgreSQL schema with the feature), refuses a taken name
(`Exists`), runs the same Init through the Store's admission and writer lane,
forks the first Branch and closes the session: no seal, no replace. On
PostgreSQL the release of the last operation empties the acquisition tables
([`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs),
[`store/live_open.rs`](../../../../engine/crates/layerfs-project/src/store/live_open.rs),
[`acquisition/cleanup.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/cleanup.rs)).

| Route | Store | Seals | Init statistics |
| --- | --- | --- | --- |
| `init` | new SQLite file | yes | `record_init` |
| `create_store` | new SQLite file, no project | yes | none |
| `create_postgresql_store` | new PostgreSQL schema | no | none |
| `add_project` | existing SQLite file or PostgreSQL schema | no | none |

## Invariants

| Rule | Enforced by |
| --- | --- |
| The source is the complete tree; nothing is filtered. | `InitRequest::source` in [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs); [`complete_root.rs`](../../../../engine/crates/layerfs-project/tests/complete_root.rs) |
| Full-root acquisition is explicit: only a `ProjectApi` call runs it; mount and Commit never do. | No production dependency on `layerfs-project` from Workspace, daemon, Store-call or NFS crates (dev-dependencies only); see each [`Cargo.toml`](../../../../engine/crates/layerfs-store-service/Cargo.toml) |
| One encoder: objects come from Content and go through the Storage Save. | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs), [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| A SQLite Store must be Disposable. | `explicit Disposable Store profile required` in [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs), [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs), [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs) |
| Symbolic links are stored, never followed. | [`import/source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |
| Each host file identity is constructed once; hard links share its serial. | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs), [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Working rows are removed before the tree Save finishes and before any history is published. | [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| Input-sized state lives in the acquisition backing; resident buffers are fixed windows. No total file, entry or byte cap. | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs); [`namespace_scaling.rs`](../../../../engine/crates/layerfs-project/tests/namespace_scaling.rs) |
| Every backing call, Store write and install send is one attempt; an unknown outcome is kept, never replayed or deleted. | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs), [`store/install_types.rs`](../../../../engine/crates/layerfs-project/src/store/install_types.rs) |

Disposable / WAL / synchronous=OFF is the only permitted profile. Durable
execution is `NOT_RUN — disabled by owner until explicit reauthorization`.

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Platform | Unix; elsewhere `ProjectError::Unsupported` | [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| Child buffer, job queue | 512 rows (`WINDOW_ROWS`) | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs) |
| Backing read / write window | 512 rows, 256 KiB / 4096 rows, 1 MiB | [`contract.rs`](../../../../core/crates/layerfs-storage/src/port/acquisition/contract.rs) |
| One object over `CANONICAL_LIMIT` | `ObjectLimitExceeded` | [`import/batch.rs`](../../../../engine/crates/layerfs-project/src/import/batch.rs) |
| Deadline | `Deadline`; set by the caller | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs), [`project/init.rs`](../../../../control/crates/layerfs-store/src/project/init.rs) |
| Device, FIFO or socket | `Unsupported` | [`import/scan.rs`](../../../../engine/crates/layerfs-project/src/import/scan.rs) |
| Set-ID or sticky file; mtime nanoseconds out of range | `InvalidInput { Mode \| Mtime }` | [`import/error.rs`](../../../../engine/crates/layerfs-project/src/import/error.rs) |
| Source changed while read (stamp, length or symlink re-stat) | `InvalidInput { Changed }`, no second read | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Store inside the source | `BackingInsideSource`, nothing begun | [`import/source.rs`](../../../../engine/crates/layerfs-project/src/import/source.rs) |
| Worker or channel gone | `WorkerUnavailable` | [`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs) |
| Position or serial overflow (`i64::MAX`) | `Capacity` | [`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs) |
| Unknown Store, history or backing outcome | `Uncertain { cause, retained }`: record and rows left exactly | [`import/error.rs`](../../../../engine/crates/layerfs-project/src/import/error.rs) |
| Removing working rows failed | `Cleanup { cause, error, retained }`; nothing published | same |
| Add: name taken, no acquisition tables, another add's rows remain | `Exists`, `NoAcquisition`, `Busy` | [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs) |
| Seal with the session still shared, or checkpoint busy | `Busy` | [`store/seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs), [`lifecycle/seal.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal.rs) |
| PostgreSQL schema exists; feature off | `Refused { "schema exists" }`; `BackendUnavailable` | [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs) |
| Init, add or install failed | `InitFailure`, `AddFailure`, `InstallFailure` (phase, attempted, accepted, acknowledged): the output file and anything acknowledged are kept | [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs), [`store/install_types.rs`](../../../../engine/crates/layerfs-project/src/store/install_types.rs) |

While retained working rows remain, a later add into that Store is `Busy`.
The acquisition port offers `abandoned` and `discard_abandoned` for rows of
a fenced owner; no product caller uses them.

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`init_sqlite.rs`](../../../../engine/crates/layerfs-project/tests/init_sqlite.rs) | `cargo +1.85.1 test --locked -p layerfs-project --test init_sqlite` | macOS or Linux (elsewhere only the refusal test) | Disposable Init matches the in-memory root, passes the namespace oracle, leaves no working rows; refusals for missing acquisition tables and a Store inside the source. |
| [`complete_root.rs`](../../../../engine/crates/layerfs-project/tests/complete_root.rs) | `cargo +1.85.1 test --locked -p layerfs-project --test complete_root` | Unix; the SQLite-backed case macOS only | `.git`, ignored files, `node_modules`, caches, outputs, hard links and symlink targets imported exactly. |
| [`acquisition_custody.rs`](../../../../engine/crates/layerfs-project/tests/acquisition_custody.rs) | `cargo +1.85.1 test --locked -p layerfs-project --test acquisition_custody` | Unix | Backing placement refusal, cleanup before publish, and untouched rows on an unknown outcome. |
| [`namespace_scaling.rs`](../../../../engine/crates/layerfs-project/tests/namespace_scaling.rs) | `cargo +1.85.1 test --locked -p layerfs-project --test namespace_scaling` | Unix | Backed counts grow with the tree while buffers and windows stay fixed. |
| [`init_postgresql.rs`](../../../../engine/crates/layerfs-project/tests/init_postgresql.rs) | `cargo +1.85.1 test --locked -p layerfs-project --features postgresql --test init_postgresql` | Unix with `LAYERFS_TEST_POSTGRES_URL`; otherwise it reports NOT_RUN | The same folder in SQLite and PostgreSQL gives the same root and no leftover acquisition rows. |
| [`verify_namespace.rs`](../../../../engine/crates/layerfs-project/examples/verify_namespace.rs) | `cargo +1.85.1 run --locked -p layerfs-project --example verify_namespace -- <store> <unused> <root-hex> <stack-hex> <manifest.tsv> <manifest-sha256> disposable` | macOS or Linux; `LAYERFS_HISTORY_CURSOR_KEY` (hex) | Independent check of a Store's genesis root against a manifest; seals the Store at the end. Always pass `disposable`: an omitted profile selects Durable. |
