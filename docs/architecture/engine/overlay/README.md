# Overlay

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Each mounted Workspace's own disposable SQLite file of changed names, inodes, bytes and operation records, plus the in-memory open state beside it, served one short job at a time under one lock.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-overlay`; the roster and lock in `layerfs-store-service` |
| Source | [`layerfs-overlay/src`](../../../../engine/crates/layerfs-overlay/src), [`layerfs-overlay/sql/schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql), [`layerfs-store-service/src/overlay`](../../../../engine/crates/layerfs-store-service/src/overlay) |
| Uses | — (SQLite through `rusqlite`; no Store or content I/O) |
| Used by | [Workspace](../workspace/README.md), [Commit](../commit/README.md), [Store calls](../store-calls/README.md), [Kernel routes](../kernel-routes/README.md), [Daemon](../daemon/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md), [A Commit](../../flows/commit.md) |

## Picture

```text
 daemon process
 +---------------------------------------------------------------------+
 | Workspaces roster (store-service)    template <overlay path>        |
 |   slots: namespace -> Starting | Live | Retiring | Failed           |
 |                                                                     |
 |   Workspace n: Shared { route, Mutex<State>, Condvar }              |
 |   +---------------------------------------------------------------+ |
 |   | State.db: Overlay                                             | |
 |   |   connection (EXCLUSIVE, MEMORY journal, synchronous=OFF) --+ | |
 |   |   open: OpenState         (memory: descriptors, lookups..)  | | |
 |   |   workspace: WorkspaceState (memory: generations, revision) | | |
 |   |   tickets: ReplyTickets   (memory: publications to reply)   | | |
 |   | State: backing, maintenance cursor, failure, counters       | | |
 |   +-------------------------------------------------------------|-+ |
 +-----------------------------------------------------------------|---+
                                                                   v
          <dir>/<n>.sqlite  fs_inode fs_entry fs_data fs_truncate
                            fs_data_ref fs_digest fs_predecessor
                            cleanup_task commit_record
```

A caller thread locks one Workspace's `State`, runs one typed job on its
`Overlay`, unlocks, and then drains ready maintenance one step per hold.
Workspaces share no connection, file, pager or lock.

## What it does

### Workspace roster and lock

```text
 daemon start       Workspaces::start: create the template, close it
 mount              open(incarnation): namespace = next; copy template
                    -> <n>.sqlite; Backing(dev, ino, nlink 1); slot Live
 each call          OwnerClient::call(route, class, readiness, job):
                    lock State -> wait behind frontier -> job -> unlock
                    -> drop disposals -> maintain() while pending
 terminal unmount   retire(n): Backing.terminal(); checked close
                    -> Backing.closed(); last Arc dropped -> unlink
 daemon stop        stop(): close every Live connection, terminal=false
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| Template | At daemon start `Overlay::create_observed` builds the schema at the configured path, then `close_database_file` closes it. Its size is recorded; a mount refuses a template whose size changed | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Admission | `open` takes the next namespace (never reused; overflow is `IdentityExhausted`), refuses a zero or duplicate incarnation, and refuses with `AdmissionFull` when the roster already holds `namespaces` slots in any state | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Copy | `Owner::start_copy` copies the template bytes into a fresh `<n>.sqlite` beside it (never a hard link), then `Backing::new` records the file's device, inode and link count 1 | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs), [`backing.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/backing.rs) |
| Bind | `OwnerClient::open` runs `open_workspace(base_root)` as the first job and fixes the `Route` (namespace, engine, incarnation) | [`scopes.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/scopes.rs) |
| One lock | `Shared.state: Mutex<State>` holds the connection, failure, maintenance cursor, backing and counters; every job, reply attempt and close takes it | [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |
| A job | `call` / `call_observed`: the closure "performs at most one transaction and does no provider I/O"; a stale route or stopped Workspace is `Unattempted` with the original input kept | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| Frontier | A waiting Capture makes later mutations wait; a waiting known install makes later base-source acquisitions wait. Waits are on the `Condvar`, before any effect, so traffic cannot extend what the waiter awaits | [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) `behind_frontier`, [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| Reply attempt | `reply_attempted` holds the lock briefly, with no SQL, to settle one reply ticket | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| Deferred job | `call_deferred` leaves the maintenance it readies queued; Commit's releases use it, and the daemon drains it after the Commit's answer. A terminal unmount's maintenance hold stops that drain and every other one ([`hold.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/hold.rs)) | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) `reclaim_after_commit` |
| Panic | A panic in a job is caught; the connection and payload are kept in `WorkspacePanicked` and every later call is refused | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) `retain_panic` |
| Counters | Per service class: jobs, lock wait, hold time, SQL, allocation and payload work; maintenance steps counted apart | [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) `record` |

### Database

| Table | Holds | Source |
| --- | --- | --- |
| `fs_inode` | One row per changed inode per generation: kind, mode, mtime, nlink, size, `inherited_cutoff`, `born`, `entries`, `subdirs`, shrink `epoch`/`height`, `referenced`, content `sequence` | [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql) |
| `fs_entry` | One row per changed name per generation; `inode` NULL is a whiteout; `inherited` says the lower view binds the name | same |
| `fs_data`, `fs_truncate`, `fs_data_ref`, `fs_digest` | Changed bytes, shrink staircase, reference pieces and append digests: see [Payload](payload.md) | same |
| `fs_predecessor` | The file a rename put this one in place of: an advisory delta base, never a fact about bytes | [`predecessor.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/predecessor.rs) |
| `cleanup_task` | Queued reclamation: see [Lifetimes](lifetimes.md) | [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql) |
| `commit_record` | Indexed operation records, value at most 65 536 bytes | same |

All tables are `STRICT`; the file carries `application_id` 1279676210 and
`user_version` 39, read back at open. No Workspace row exists: the bound
Workspace's `WorkspaceState` lives in memory.

| Profile setting | Value | Source |
| --- | --- | --- |
| `page_size`, `auto_vacuum` | 4096, `NONE` | [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs) |
| `journal_mode`, `synchronous`, `locking_mode` | `MEMORY`, `OFF`, `EXCLUSIVE` | same |
| `mmap_size`, `temp_store`, `foreign_keys` | 0, `MEMORY`, `ON` | same |
| `cache_size` | `-pager_kib` (default 2048 KiB), per connection; a hint, not a resident ceiling | same |
| `max_page_count` | `max_pages`, or `u32::MAX - 1` (SQLite's format ceiling) when `None`, which the daemon passes | same, [`application/owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs) |
| `busy_timeout` | 0 | same |

Every value is read back; a difference is `Invalid("profile readback differs")`.

| Job rule | Source |
| --- | --- |
| A job that writes nothing begins no transaction; the first write begins `BEGIN IMMEDIATE`; success commits SQL and the open-state journal together | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) `transaction` |
| A failed job rolls back once and restores open state, `WorkspaceState`, hints and its reply tickets; a nested job is refused | same |
| SQL text is a fixed set of constants under a 256-entry prepared cache; every statement is counted under one of twelve `StatementKind` families | [`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs), [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs), [`metrics.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/metrics.rs) |
| `resources()` projects row counts, bytes, open-state counts, pages, freelist and `debt_upper_bytes` (an upper bound, not exclusive debt) on demand; ordinary jobs keep no accounting row | [`accounting.rs`](../../../../engine/crates/layerfs-overlay/src/database/accounting.rs) |

### Namespace

```text
generation:  -1        1 .. installed   installed+1 .. captured   active
             orphan    retired: never   sealed by Capture         new rows
             domain    in the view      (read by Commit)          go here
                       |<---------- view: (installed, active] ---------->|
lookup of serial s = newest row of s with installed < generation <= active;
no row = the committed base decides (merged by Workspace, outside SQL)
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| Compound job | `apply` publishes at most `COMPOUND_INODES` (4) inodes, `COMPOUND_DIRECTORY_ENTRIES` (2) names and one payload write together, adds one to `revision` and issues one reply ticket | [`compound.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/compound.rs), [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| Grammar only | The overlay checks representable values (`inode::check`, `check_name`); filesystem semantics belong to Workspace | [`inode.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/inode.rs) |
| Created serials | A `created` serial was reserved by the caller; an existing row fails the whole job | [`compound.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/compound.rs) |
| Name reads | `NAME_LAYERS` returns the active row and the newest lower row in one seek; a window returns at most `PAGE_ROWS` (1,024) names of one parent, active and captured | [`directory_entry.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/directory_entry.rs), [`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs) |
| Captured reads | A `CapturedReader` sees only the sealed generation, through the capture indexes; it stays valid across install and failure until its exact release | [`captured_namespace.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/captured_namespace.rs), [`captured_reader.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/captured_reader.rs) |
| Predecessors | Rename over a base file records it; over a local file inherits its predecessor; Commit pages them with `captured_predecessors` | [`predecessor.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/predecessor.rs) |

### Open state

`OpenState` is never written to the file; it changes only inside a job and
commits or rolls back with that job's SQL through undo journals.

| Map | Holds | Source |
| --- | --- | --- |
| `mount`, `kernel`, `parents` | One kernel connection, `(mount, serial)` lookup counts, retained directory parents | [`open/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/open/kernel.rs) |
| `files`, `directories`, `cookies` | Open descriptors, open directories and published READDIR cookie pages | same, [`open/directories.rs`](../../../../engine/crates/layerfs-overlay/src/open/directories.rs) |
| `reads`, `lookups`, `operations`, `captured` | Read windows, lookup references, operation owners, captured readers | [`open/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/open/kernel.rs) |
| `custody`, `sources`, `orphans` | Per-serial opens, lookups and readers; held `BaseSource` windows; orphans | same |

| Rule | Source |
| --- | --- |
| Owner numbers come from one process-wide counter, never reused: a stale token is `Stale`; copying a token acquires nothing | [`file_owners.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/file_owners.rs), [`custody.rs`](../../../../engine/crates/layerfs-overlay/src/contract/custody.rs) |
| Each owner table is indexed by the caller's request too, so custody after a lost reply is observable (`retained_file`, `retained_lookup`); absence never permits a repeat | [`journal.rs`](../../../../engine/crates/layerfs-overlay/src/open/journal.rs) `OwnerTable`, [`lookup.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/lookup.rs) |
| A kernel visit records nothing of its own; a mutation's reply custody (`KernelEffect`) is acquired in the publishing transaction | [`kernel_visit.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_visit.rs), [`kernel_mutation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_mutation.rs) |
| FORGET below zero is `Invalid`; a foreign mount is `Stale`; nothing compensates a lost send | [`lifetime/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel.rs) |
| Cookies 1 and 2 are the dot entries; an offset from before the last rewind lists nothing; detached listings are freed after the lock is released | [`kernel_directory.rs`](../../../../engine/crates/layerfs-overlay/src/contract/kernel_directory.rs), [`open/directories.rs`](../../../../engine/crates/layerfs-overlay/src/open/directories.rs) |

### Operation records

```text
commit_record (operation, file_scope[8], kind u32, key[32]) -> value
acquire_operation(request) -> OperationOwner   (memory, never reused)
  get / get_many / keys / entries   fenced reads, <= RECORD_ROWS (64)
  apply(scope, [change...])         guarded, one atomic job
release_operation(owner) -> OPERATION_RECORD cleanup, one envelope a step
```

| Rule | Source |
| --- | --- |
| The overlay gives records no meaning; Content and Workspace own every value (Commit construction scratch, SDK hard-link maps) | [`ports/operation_record.rs`](../../../../engine/crates/layerfs-workspace/src/ports/operation_record.rs) |
| `apply` checks every expected value (`Missing` or `ExactBytes`) before changing a row; a mismatch is `NotApplied { index, key, actual }` and changes nothing | [`lifetime/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/indexed_operation_record.rs) |
| One batch is strictly ordered and fits `OPERATION_RECORD_BYTES` (65 536); key and entry windows hold at most `RECORD_ROWS` (64) rows | [`contract/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/contract/indexed_operation_record.rs) |
| `explain_indexed_operation_record` returns `EXPLAIN QUERY PLAN` text per statement shape: an operator diagnostic, not a contract | [`indexed_operation_record_plan.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/indexed_operation_record_plan.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One file, one connection, one `Overlay`, one lock per Workspace; Workspaces share no connection, writer, pager or memory budget | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) `open`, [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |
| The lock is held for one short job, never across Store I/O, a whole Exec or a whole Commit | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) `call` |
| No sync and no space reserved ahead of use: `synchronous=OFF`, `close_database` performs no sync, `reserved_tail_bytes` and `cleanup_headroom_bytes` are 0 | [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs), [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs), [`physical.rs`](../../../../engine/crates/layerfs-overlay/src/database/physical.rs) |
| One attempt: `busy_timeout=0`, no replay; an unknown outcome quarantines the engine for good | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| A published mutation stays published: a later maintenance failure is retained, not returned to that job | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) `maintain` |
| Mount copies only the template; it scans and copies no tree | [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs) `copy_template` |
| The file is unlinked only after both the terminal mark and a checked close, and only if its device, inode and link count still match | [`backing.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/backing.rs) `Drop` |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Job windows (bound one job, not a file or Workspace) | `WRITE_WINDOW` 256 KiB, `READ_WINDOW` 128 KiB, `PAGE_ROWS` 1,024, `RECORD_ROWS` 64, `OPERATION_RECORD_BYTES` 65 536 | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| Open state | Grows with open files, lookups, readers and kernel references; no cap (the one resident exception) | [`open/kernel.rs`](../../../../engine/crates/layerfs-overlay/src/open/kernel.rs) |
| Admitted Workspaces | `OwnerConfig.namespaces` (daemon: `DaemonLimits.namespaces`; library default 16); one more is `AdmissionFull` | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`application/owner.rs`](../../../../engine/crates/layerfs-daemon/src/application/owner.rs) |
| Full device | Job rolled back, then `OverlayError::NoSpace` | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs), [`physical.rs`](../../../../engine/crates/layerfs-overlay/src/database/physical.rs) |
| Unknown outcome | `Uncertain`; later jobs `Quarantined` | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| Maintenance failure | Kept as the Workspace's `maintenance_error`; the daemon then refuses new Mounts (`mount:debt`) | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Close or unlink fails | `CloseFailure` keeps the connection; `OwnerError::Deletion` keeps the file handle and marks the slot `Failed` | [`error.rs`](../../../../engine/crates/layerfs-overlay/src/contract/error.rs), [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| Platform and SQLite | Linux or macOS, else `UnsupportedPlatform`; SQLite 3.51.0 or 3.53.2 | [`physical.rs`](../../../../engine/crates/layerfs-overlay/src/database/physical.rs), [`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs) |

## Demonstration

Each test opens real overlay files in a temporary directory through the
public API; none needs root, Docker or an environment variable.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`payload.rs`](../../../../engine/crates/layerfs-overlay/tests/payload.rs) | `cargo +1.85.1 test --locked -p layerfs-overlay --test payload` | Linux, macOS | Layered reads match a model across writes, shrinks, capture and install; holes cost no rows |
| [`indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/tests/indexed_operation_record.rs) | `cargo +1.85.1 test --locked -p layerfs-overlay --test indexed_operation_record` | Linux, macOS | Keys never alias; guarded apply; records reclaim in windows after release |
| [`captured_namespace.rs`](../../../../engine/crates/layerfs-overlay/tests/captured_namespace.rs) | `cargo +1.85.1 test --locked -p layerfs-overlay --test captured_namespace` | Linux, macOS | A captured reader sees sealed rows only |
| [`kernel_memory.rs`](../../../../engine/crates/layerfs-overlay/tests/kernel_memory.rs) | `cargo +1.85.1 test --locked -p layerfs-overlay --test kernel_memory` | Linux, macOS | Requested allocation of kernel-reference and custody maps for 5,000 remembered files |

## Pages

| Page | What it covers |
| --- | --- |
| [Payload](payload.md) | Changed file bytes: cells, runs, shrink layers, reference pieces, digests and read and write streams. |
| [Lifetimes](lifetimes.md) | Generations, capture and install, reply tickets, orphans, `cleanup_task` maintenance and terminal file disposal. |
