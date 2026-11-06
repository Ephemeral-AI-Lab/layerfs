# Overlay

[Architecture](../../README.md) › [Engine](../README.md) › Overlay

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Each Workspace's own disposable SQLite file: changed metadata, payload, operation records and their reclamation.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-overlay` |
| Source | [`engine/crates/layerfs-overlay/src`](../../../../engine/crates/layerfs-overlay/src), [`engine/crates/layerfs-overlay/sql`](../../../../engine/crates/layerfs-overlay/sql); the per-Workspace roster and lock live in [`layerfs-store-service/src/overlay`](../../../../engine/crates/layerfs-store-service/src/overlay) |
| Uses | — |
| Used by | [Store calls](../store-calls/README.md), [Workspace](../workspace/README.md), [Commit](../commit/README.md), [Kernel routes](../kernel-routes/README.md), [Daemon](../daemon/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md), [A Commit](../../flows/commit.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Database](database.md) | One SQLite file per Workspace. |
| [Namespace](namespace.md) | Changed inodes and directory entries. |
| [Payload](payload.md) | Changed file bytes: pieces, runs and layers. |
| [Open state](open-state.md) | Open files, directories and kernel references, held in memory. |
| [Lifetimes and reclamation](lifetimes-and-reclamation.md) | When overlay state can be released, and how. |
| [Operation records](operation-records.md) | Indexed operation records. |

## The picture

```text
 daemon process
 +------------------------------------------------------------------+
 | Workspaces roster          template file = DaemonSetup.overlay   |
 |   (store-service)          created, closed before readiness      |
 |                                                                  |
 |   Workspace 1 (namespace 1)          Workspace 2 (namespace 2)   |
 |   +----------------------------+     +------------------------+  |
 |   | Mutex<State>  one lock     |     | Mutex<State>           |  |
 |   |  Overlay                   |     |  Overlay               |  |
 |   |   connection ---------+    |     |   connection ----+     |  |
 |   |   open state     (mem)|    |     |   ...            |     |  |
 |   |   WorkspaceState (mem)|    |     |                  |     |  |
 |   |   reply tickets  (mem)|    |     |                  |     |  |
 |   +-----------------------|----+     +------------------|-----+  |
 +---------------------------|-----------------------------|--------+
                             v                             v
                       <dir>/1.sqlite                <dir>/2.sqlite
            Namespace | Payload | Operation records | cleanup_task
```

A caller thread takes the Workspace lock, runs one short typed job on the
`Overlay`, then releases it. Namespace, payload and operation records are
rows in the file; [open state](open-state.md) and the Workspace's runtime
state are in daemon memory. [Lifetimes and reclamation](lifetimes-and-reclamation.md)
drives `cleanup_task` rows one bounded step at a time.

## What it does

| Held in the Workspace's file | Held in daemon memory (`Overlay` fields) |
| --- | --- |
| `fs_inode`, `fs_entry`: changed inodes and names ([Namespace](namespace.md)) | `open: OpenState`: descriptors, readers, lookups, kernel references, directory cookies, orphan map, base sources ([Open state](open-state.md)) |
| `fs_data`, `fs_truncate`, `fs_data_ref`, `fs_digest`: changed bytes ([Payload](payload.md)) | `workspace: WorkspaceState`: active and captured generation, installed floor, revision, base root, dirty counts, `closed` |
| `fs_predecessor`: rename delta-base hints ([Namespace](namespace.md)) | `tickets: ReplyTickets`: publications whose reply is not yet attempted |
| `commit_record`: indexed operation records ([Operation records](operation-records.md)) | hints `maintenance_ready`, `orphan_seen`, `pieces_seen`; `quarantined`; work counters |
| `cleanup_task`: queued reclamation ([Lifetimes](lifetimes-and-reclamation.md)) | — |

Source: [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs),
[`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql),
[`open/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/open/kernel.rs).

| Lifetime step | What happens | Source |
| --- | --- | --- |
| Daemon start | `Overlay::create_observed` builds the schema at the configured overlay path, then `close_database_file` closes it: this is the template | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Mount (bind) | `Workspaces::open` copies the template into `<namespace>.sqlite` beside it with `Overlay::copy_template_observed`, then `open_workspace(base_root)` | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs), [`scopes.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/scopes.rs) |
| Serving | One job per lock hold; a maintenance step after a job when work is pending | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| Terminal unmount | Logical `close`, kernel revoke, then `retire`: mark terminal, checked connection close, and `Backing` unlinks the file once both are set | [`backing.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/backing.rs) |
| Daemon stop | `Workspaces::stop` closes every connection with `terminal = false`: files are not deleted | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One file, one `Connection`, one `Overlay` per Workspace. Workspaces share no connection, writer or page cache; each connection sets its own `cache_size` from `pager_kib`. | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) `open`, [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs) |
| One lock (`Shared.state`) per Workspace. The closure run under it "performs at most one transaction and does no provider I/O". A Commit takes it once per job, not for the whole Commit. | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) `call`, [engine guide](../../../../engine/AGENTS.md) |
| No sync: `synchronous=OFF`, `journal_mode=MEMORY`, and `close_database` "performs no sync". | [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs), [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| No space reserved ahead of use: `reserved_tail_bytes` and `cleanup_headroom_bytes` are always 0; the device is checked only after a write I/O failure. | [`physical.rs`](../../../../engine/crates/layerfs-overlay/src/database/physical.rs) |
| One atomic job at a time; a nested job is refused; a failed job rolls back once and restores memory state. | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) `transaction` |
| No busy handler and no replay: `busy_timeout=0`; an uncertain outcome quarantines the engine for good. | [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs), [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| The file is unlinked only after both the terminal mark and a checked connection close. | [`backing.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/backing.rs) `Drop` |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Open state | Grows with open files, readers, lookups and kernel references; no cap | [`open/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/open/kernel.rs) |
| Job windows (not totals) | `WRITE_WINDOW` and `READ_WINDOW` 128 KiB, `PAGE_ROWS` 1,024 (64 before layerfs-project #69), `RECORD_ROWS` 64 (operation-record windows), `OPERATION_RECORD_BYTES` 65 536, `COMPOUND_INODES` 4, `COMPOUND_DIRECTORY_ENTRIES` 2 | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| Database size | `max_pages: None` uses SQLite's format ceiling; the daemon passes `None` | [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs), [`application/owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs) |
| Admitted Workspaces | `DaemonLimits.namespaces`; one more is `AdmissionFull` | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Full device | Whole job rolled back, then `OverlayError::NoSpace` | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| Unknown outcome | `OverlayError::Uncertain`; engine `quarantined`, every later job `Quarantined` | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| Stopped maintenance | Any Workspace's maintenance error refuses new Mounts (`mount:debt`, `Capacity`) | [`control/operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Orphans | Last unlink of a held file keeps it in generation `-1` until its last reference; see [Orphans](lifetimes-and-reclamation.md#orphans) | [`orphan.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/orphan.rs) |
| Low-water | The overlay has none. The serial low-water is a daemon setting for Workspace inode serials | [`application/config.rs`](../../../../engine/crates/layerfs-daemon/src/application/config.rs) |
| Close or unlink fails | `CloseFailure` keeps the connection; `OwnerError::Deletion` keeps the file handle; the file stays | [`error.rs`](../../../../engine/crates/layerfs-overlay/src/contract/error.rs), [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Platform | Linux or macOS; elsewhere `UnsupportedPlatform`. SQLite must be 3.51.0 or 3.53.2 | [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs), [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs) |

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`payload.rs`](../../../../engine/crates/layerfs-overlay/tests/payload.rs) | `layerfs-overlay` | Layered reads match a reference model across writes, resizes, capture and install; shrink cost does not grow with discarded data; holes cost no rows |
| [`indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/tests/indexed_operation_record.rs) | `layerfs-overlay` | Full keys and file scopes never alias; guarded apply; more than 8 MiB of records reclaims in windows after owner release |
| [`orphans.rs`](../../../../engine/crates/layerfs-overlay/tests/orphans.rs) | `layerfs-overlay` | An unlinked, still-held file survives installs and failures and is reclaimed after its last owner; a closed Workspace keeps its last orphan for file disposal |

Each test opens real overlay files in a temporary directory through the
public API (`tests/payload_support`). No root, Docker or environment variable
is needed; the overlay refuses platforms other than Linux and macOS.

```text
cargo +1.85.1 test --locked -p layerfs-overlay --test payload
cargo +1.85.1 test --locked -p layerfs-overlay --test indexed_operation_record
cargo +1.85.1 test --locked -p layerfs-overlay --test orphans
```

These commands were not run while writing this page.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/19-daemon-overlay.md`](../../../../core/docs/architecture/19-daemon-overlay.md): covered in [Database](database.md), [Namespace](namespace.md) and [Payload](payload.md); its schema numbers are historical.
- [`core/docs/architecture/25-terminal-reclaim.md`](../../../../core/docs/architecture/25-terminal-reclaim.md): covered in [Lifetimes and reclamation](lifetimes-and-reclamation.md).
- [`core/docs/architecture/49-indexed-operation-records.md`](../../../../core/docs/architecture/49-indexed-operation-records.md): covered in [Operation records](operation-records.md).
- [`core/docs/architecture/64-overlay-terminology.md`](../../../../core/docs/architecture/64-overlay-terminology.md): covered in [Database](database.md) (current table names, schema 39).

---

← [Project Init: Seal and install](../project-init/seal-and-install.md) · [Up: Engine](../README.md) · [Overlay: Database](database.md) →
