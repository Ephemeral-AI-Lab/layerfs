# Seal and install

[Architecture](../../README.md) › [Engine](../README.md) › [Project Init](README.md) › Seal and install

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Sealing a new Store file, installing it into a daemon, adding a project to a
live Store instead, and what each route leaves when its outcome is unknown.

| | |
| --- | --- |
| Component | [Project Init](README.md) |
| Source | [`engine/crates/layerfs-project/src/store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs), [`engine/crates/layerfs-project/src/store/install_types.rs`](../../../../engine/crates/layerfs-project/src/store/install_types.rs), [`engine/crates/layerfs-project/src/store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs), [`engine/crates/layerfs-project/src/store/live_open.rs`](../../../../engine/crates/layerfs-project/src/store/live_open.rs) |

## Seal

```text
 ProjectApi::init / create_store
   SqliteHandles::create (Disposable, acquisition tables)
   ... Init, fork, record_init (init only) ...
   drop(storage)
   SqliteHandles::seal(self)
     drop history, acquisition handles
     Arc::try_unwrap(storage, session) ----- still shared? --> Busy
     Session::seal
       writable, not quarantined, autocommit --- else ReadOnly/Uncertain
       Linux: take the cross-process writer hold
       macOS: capture file identity; disable persistent WAL
       PRAGMA wal_checkpoint(TRUNCATE) ------- busy != 0 --> Busy
       close connection; release writer hold
       -wal, -shm, -journal must be absent --- else Refused
       macOS: release unused allocation past the logical end
     --> SealedStore { path, profile, sqlite_version, bytes }
```

| Field (`SealedStore`) | Meaning |
| --- | --- |
| `path` | the closed database file; no sidecar existed at seal |
| `profile` | the profile to open the installed Store with (Disposable) |
| `sqlite_version` | SQLite version that created and sealed the file |
| `bytes` | main-file length after checkpoint and close |

Source: [`store/seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs),
[`lifecycle/seal.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal.rs).

Seal consumes the sole session: it follows `Save::finish` and history
publication, and is never called on a live daemon's shared Store. The macOS
allocation release belongs only to this sole-owner seal. After seal,
`initialize` builds a `StoreManifest` (provider, locator, Disposable profile,
binding, cursor key, stack, branch, root, bytes, host SQLite version; daemon
version absent) and returns a `SealedProject`
([`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs)).
`create_store` seals a Store that holds no project yet
([`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs)).

## Install

```text
 caller: install_file                   daemon: receive_install
 ----------------------                 ------------------------------
 check Disposable, not installed,
 file kind and length == manifest
 send StoreManifest --------------->    Request: Disposable, locator ==
                                        provisioned destination
                                        Claim: create_new temporary file
                    <---------------    Ready
 Transfer: read <= 65519 bytes,
 send one record each ------------->    write; refuse a longer stream
 one more read must return 0
 send INSTALL_FINISH ("LFSF\x01") ->    check marker
                                        Publish: rename temporary
                                        Open: open Store, check binding
                    <---------------    Reply: Installed(manifest with
                                        daemon_sqlite)
 manifest == sent manifest + daemon_sqlite?  else Protocol mismatch
```

| Phase (`InstallPhase`) | Side | Meaning | Source |
| --- | --- | --- | --- |
| `Request` | both | send or check the authenticated manifest | [`control.rs`](../../../../engine/crates/layerfs-bridge/src/control.rs) |
| `Claim` | daemon | exclusively create the temporary file | [`daemon/install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `Transfer` | both | stream the declared bytes and the finish marker | [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs) |
| `Publish` | daemon | one rename to the target | [`daemon/install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `Open` | daemon | open the Store and check the initial binding | [`daemon/install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `Reply` | both | the original acknowledgement | [`store/install.rs`](../../../../engine/crates/layerfs-project/src/store/install.rs) |

The record size is `MAX_PLAINTEXT_BYTES` = `u16::MAX - 16`
([`profile.rs`](../../../../engine/crates/layerfs-bridge/src/duplex/profile.rs)).
The caller holds one fixed buffer; nothing grows with file size. The caller
never opens the file as a database. `InstallWork` counts read calls, read
bytes, sent records and sent bytes
([`install_types.rs`](../../../../engine/crates/layerfs-project/src/store/install_types.rs)).
On success the connection stays usable for later control calls; on a failure
after the request was attempted, the caller closes its send side (the fence),
unless the failure was a channel error. The daemon side is described in
[Daemon: Startup](../daemon/startup.md) and [Daemon: Bridge](../daemon/bridge.md).

## Live open

The product `project.init` adds a project to a Store that is already in use.
It runs the same namespace Init over a writable session on that Store; its
Saves and history writes go through the Store's admission and writer lane, one
short job at a time, while daemons and SDK processes keep using it.

```text
 ProjectApi::add_project(AddRequest)
   disposable(store)            SQLite: Disposable required; PG passes
   Opened::writable(selection, binding, cursor_key)
        Sqlite(SqliteHandles::open_writable)
        Postgres(PostgresHandles::open_writable)   feature "postgresql"
   layer_stack_named(name) exists? -----------------> Exists
   init_observed(storage, history, acquisition)
        no acquisition tables  --> NoAcquisition (nothing scanned)
        another add's rows     --> Busy (nothing begun)
   fork first Branch
   handles.close()              admission closed; no seal, no replace
   --> AddedProject { initialized, branch, sqlite_version }
```

| Route | Store | Seals? | Statistics | Source |
| --- | --- | --- | --- | --- |
| `init` | new SQLite file | yes | `record_init` | [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs) |
| `create_store` | new SQLite file, no project | yes | none | [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs) |
| `create_postgresql_store` | new PostgreSQL schema, no project | no (closed) | none | [`store/live_open.rs`](../../../../engine/crates/layerfs-project/src/store/live_open.rs) |
| `add_project` | existing SQLite file or PostgreSQL schema | no (closed) | none | [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs) |

Built without the `postgresql` feature, a PostgreSQL selection fails
`Persistence(BackendUnavailable)`. `sqlite_version` is the session's SQLite
version, or `postgresql <server version>`
([`store/live_open.rs`](../../../../engine/crates/layerfs-project/src/store/live_open.rs)).

## Uncertain Init

| Outcome | Returned | Left in the Store or on disk | Source |
| --- | --- | --- | --- |
| Definite failure during import | the original `ProjectError` | nothing: phase set `Failed`, working rows and record removed | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs) |
| Unknown Store, history or backing outcome | `ProjectError::Uncertain { cause, retained }` | the operation's record and rows exactly as left | [`import/backing.rs`](../../../../engine/crates/layerfs-project/src/import/backing.rs) |
| Removing working rows failed | `ProjectError::Cleanup { cause, error, retained }` | remaining rows; nothing published | [`import/error.rs`](../../../../engine/crates/layerfs-project/src/import/error.rs) |
| Init-and-seal failed after some step | `InitFailure { request, error, initialized, branch }` | the output file and any acknowledged Init or Branch; no deletion or replay | [`store/init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs) |
| Add failed | `AddFailure { request, error, initialized }` | the live Store with any acknowledged Init | [`store/live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs) |
| Install failed | `InstallFailure` with phase, `request_attempted`, `accepted`, `acknowledged`, work, error, fence error | the sealed source file; nothing is deleted | [`install_types.rs`](../../../../engine/crates/layerfs-project/src/store/install_types.rs) |

`RetainedAcquisition` names the operation (`Owner`: operation and epoch) and,
when one read of its charges succeeded, its exact `AcquisitionWork`
([`import/error.rs`](../../../../engine/crates/layerfs-project/src/import/error.rs)).
While such rows remain, a later add into the same Store is refused `Busy`.
The port offers `abandoned` and `discard_abandoned` to remove rows of an
operation whose owner is fenced
([`contract.rs`](../../../../core/crates/layerfs-storage/src/port/acquisition/contract.rs));
no caller in `api/` or `connect/` uses them. Every case is reported once;
nothing is resent.

## Related

- [Construction](construction.md): the steps before seal.
- [Store: Lifecycle](../store/lifecycle.md): open, handles and seal.
- [Store: SQLite](../store/sqlite.md), [Store: PostgreSQL](../store/postgresql.md): the backends a live add opens.
- [Daemon: Startup](../daemon/startup.md), [Daemon: Bridge](../daemon/bridge.md): the receiving side of install.
- [API: Control](../../api/control/README.md): `store.create` and `project.init`.
- [Flow: Project Init](../../flows/init.md)

---

← [Project Init: Construction](construction.md) · [Up: Project Init](README.md) · [Engine: Overlay](../overlay/README.md) →
