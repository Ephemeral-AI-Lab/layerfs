# 06 — Cluster-one integration: the daemon opens the Store

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Rewritten 2026-10-07 for the owner's serverless direction
> ([decisions K28–K33](08-decisions-provenance.md#3-decisions-of-this-design)).
> It replaces the host-mediated contract last committed at `af963a718`. Product
> source is unchanged by this revision: Persistence still opens only on macOS,
> and the SDK runtime, Bridge data codec and daemon upstream still exist. This
> revision ran no build, workload or measurement.

This is the integration map for the [operation documents](README.md#primary-design-documents).
The [deepest-file plan](../307/SERVERLESS-STORE-PLAN-20261007.md) lists every
file that changes, with production LOC before and estimated after.

## 1. The contract in brief

```text
 macOS host                                Docker Linux VM (one kernel)
 --------------------------------          ---------------------------------------
 Project Init (native, unchanged)          named volume, mounted in daemons only
   Handles::create -> import -> seal         store.sqlite  (+ -wal, -shm while open)
   one sealed file                                ^          ^          ^
 install: copy the file once  ----------->        |          |          |
                                             daemon A   daemon B   daemon C
 control only  <------------------------>    each daemon, in one process:
   mount / Exec / Commit /                     overlay.sqlite   local mutable state
   status / unmount                            Store connection objects, Save, history
                                               Workspace + FUSE -> ordinary Bash
```

- **Reads** go Workspace port → `Reader` → SQLite. Nothing crosses a transport.
- **Commit** is capture → construct → `Save` accept/finish → stage → publish →
  local base install, in one process on one thread.
- **The host** builds the first Store and sends control commands. It is never
  in the data path and holds no Store handle after install.
- **Coordination is the database.** There is no coordinator, server or lock
  service. One short write transaction runs at a time across all daemons, and
  history transitions are conditional on exact expected state.
- **No retry.** A write that meets another writer returns `Busy` before any
  effect. Nothing waits, polls or re-issues.

A mounted SQLite file is the first provider of this shape. Storage and History
already reach the database through `PackPersistence` and `HistoryCatalog`;
another database can replace SQLite behind those ports without touching the
daemon adapter. To keep that true, every Store call is either an idempotent put
or get of immutable content by id, or one conditional transition with exact
expected state; reads are batched by id; `Busy` is a typed outcome; and the
daemon adapter names no SQLite type, path or lock. The
[plan](../307/SERVERLESS-STORE-PLAN-20261007.md#rules-that-keep-a-later-database-swap-cheap)
lists the rules.

## 2. Cluster-one APIs the daemon calls

These direct calls, including atomic stage-and-publish, are implemented. The daemon calls them
directly; no wire form of any of them remains.

| Operation | API | Obligation |
| --- | --- | --- |
| Open | `Handles::open_writable(config, binding, cursor_key)` | Once per daemon, before readiness. Verifies the file; never converts or migrates it |
| Storage | `Storage::new(handles.storage.clone())`, `policy()`, `capacities()` | Store-derived policy; no per-call open |
| Base objects | `Storage::reader()`, `Reader::read_objects(ids)` | Authenticated canonical bytes in demand order |
| File length | `Reader::file_lengths(ids)` | No payload read |
| Branch at mount | `HistoryCatalog::branch_snapshot(branch)` | Coherent root, head, base, scope and profile |
| Inode serials | `reserve_inodes(request)` | Database-owned range; never recycled |
| Save | `Storage::begin_save()`, `Save::sink()`/`accept`, `Save::finish()` | A stack local of the Commit; earlier published waves survive a later failure |
| Same-Save reads | `AuthenticatedObjects for Save` | Construction reads what it just produced |
| Stage and publish | `HistoryCatalog::stage_and_commit` in a single write transaction (O-23) | Exact captured expectations, the saved candidate and a conditional head transition; `Committed`, `UpToDate` or the exact conflict, with no stage left behind |
| Discard | `discard_stage(DiscardRequest)` | Exact owned token; only for a stage created by the separate calls |

Content construction and reads are unchanged: `FilesystemRead`, `FileView`,
`construct_stream`, `apply_edits`, `update_filesystem` and `FinalizedConsumer`
as in the [cluster-one handbook](../../../../cluster_one_handbook.md).

## 3. The shared Store

### 3.1 Placement

The Store is one SQLite file on a named volume inside the Docker VM, mounted
into every daemon container. WAL needs shared memory and POSIX locks on one
kernel, so the volume must not be a host bind mount (virtiofs) or a network
filesystem. This is a deployment invariant; the product cannot check it without
a new system-call dependency.

### 3.2 Profiles

Both profiles use WAL and differ only in synchronization (owner ruling O-21,
implemented in [F1–F4](../307/PRE-S8-F1-F4-20261007.md)).

| Profile | Journal | `synchronous` | Survives |
| --- | --- | --- | --- |
| Durable | WAL | FULL | Process and kernel crash, to the extent the VM disk honors a sync |
| Disposable | WAL | OFF | Process crash. A kernel or VM crash may lose or damage the Store |

The superseded Disposable profile used a memory journal with rollback locking. It is
correct for one process and wrong for several: a daemon killed mid-commit
leaves a torn file with no journal, every other daemon reads the damage, and
readers and the writer block each other. Under WAL a killed process cannot tear
the file, and readers never block.

Open verifies `journal_mode = wal`, page size, schema identity and catalog
binding, and sets only connection-local pragmas. A Store in another journal
mode is refused, consistent with the no-migration rule. Mixed profiles on one
file cannot be detected; the profile is a provisioning fact the host gives every
daemon.

### 3.3 One attempt, exact outcome

| Caller | Transaction | Contended outcome |
| --- | --- | --- |
| Reader | WAL snapshot; unframed or deferred `BEGIN` | Not blocked by a writer |
| Writer | One `BEGIN IMMEDIATE` | `Busy` at `BEGIN`: nothing ran, nothing to undo, session healthy |

- The write lock is taken once, at `BEGIN IMMEDIATE`. A WAL commit takes no new
  lock, so `Busy` cannot appear later in the transaction.
- No transaction reads and then upgrades to a write, so a stale-snapshot refusal
  cannot appear mid-transaction.
- `busy_timeout` stays zero and stays checked. No busy handler, sleep, loop or
  writer gate exists in Persistence, Storage or the daemon.
- `Busy` reaches the caller as `PersistenceError::Busy` and
  `HistoryError::Busy`, never as a formatted string.
- An unknown outcome (I/O error, failed rollback) still quarantines that
  daemon's session. Other daemons are unaffected.

What a caller sees:

| Operation | On `Busy` |
| --- | --- |
| Serial reservation | The mutation that needed a serial is refused; no local change |
| Save publication wave | The Commit fails with "Store busy". Waves already published stay as unreferenced immutable objects. The Workspace keeps every change; a later explicit Commit is a new operation |
| Stage and publish | Exact refusal with no stage row; local custody of the capture is retained as the [Commit contract](workspace-api/commit.md) already requires |

### 3.4 State under other writers

Every value a writer depends on is either fixed when the Store is created or
read inside its own `BEGIN IMMEDIATE` transaction: pack and ordinal cursors,
metadata windows, the stage token, the inode high-water mark and Branch heads.
Object locators are first-writer-wins, so two daemons publishing the same
object agree. Advisory rows (pool signatures) are last-writer-wins and cost
only reuse quality. The [plan](../307/SERVERLESS-STORE-PLAN-20261007.md#storage-and-history-under-several-writers)
carries the per-item review and the corrections it found.

Not available to a daemon:

- **Acquisition** (the Init import tables). Its abandoned-operation cleanup
  treats any other process's operation as abandoned. Only host Init uses it.
- **Checkpoint TRUNCATE and space reclamation.** Host seal only; there is no
  public checkpoint call. Daemons rely on SQLite's passive auto-checkpoint.
- **Collection.** No collector exists and none runs under several writers.

## 4. The daemon adapter

`layerfs-daemon/src/store/` replaces `upstream/`.

```text
 store/open.rs    one Handles + Storage + HistoryProvider at startup
 store/ports.rs   AuthenticatedObjects, FileLengths, InodeSerials over the Store
 store/bind.rs    mount: Branch snapshot, root checks, base view, Workspace bind
 store/commit.rs  Save scope, candidate checks, stage, publish, discard, new base
```

- **One write handle and a fixed set of read handles per daemon**, opened once
  at startup. Save, history and serial reservation use the write handle; object
  and length reads use the read set, so a daemon's own publication never stalls
  its base reads. Each handle is used by one thread at a time under its own
  mutex, per bounded job. Taking a mutex inside the process is mutual
  exclusion, not a retry. No lock spans a Commit.
- **The shared immutable object cache sits above the read handles.** An object
  never changes, so most reads never reach the Store.
- **Few write transactions per Commit:** one combined initial id reservation per Save, explicitly counted block refills
  for unbounded streams, bounded publication batches, and one stage-and-publish
  history transaction. The owner approved counted refills in the
  [reservation decision](../307/SAVE-RESERVATION-DECISION-20261007.md).
- **Commit is one synchronous function.** `Save<'_>` is a local borrowed from a
  `Storage` on the Commit thread. There is no Save registry, capability,
  token table, reply custody or supervisor.
- **Order is fixed:** finish Save, check the candidate root, stage and publish
  in one transaction, install the new base locally. A refused or conflicting
  publish therefore leaves no stage row.
- **Root checks** are the bounded ones that exist today (profile, scope, root
  serial, root inode, root listing page, portable metadata), at mount and before
  stage. Whole-root topology qualification is the explicit paid Content pass; it
  runs in the daemon over overlay operation records and is never hidden in mount.
- **Admission of constructed objects.** Objects now come from Content
  constructors in the same process, the path Project Init already uses with no
  re-derivation. The untrusted-wire admission step leaves with the wire.

## 5. Host control and install

The host has five commands: mount, Exec, Commit, status and terminal unmount,
with the semantics of the [operation contracts](README.md#primary-design-documents).
Exec has no automatic deadline. A control call's own deadline never terminates
Bash.

| Step | Where | What |
| --- | --- | --- |
| Init | Host, unchanged | `Handles::create`, native import, first Branch |
| Seal | Host | Checkpoint TRUNCATE with no reader, drop the handle, verify no `-wal`, `-shm` or `-journal` sidecar remains. Returns one file and a manifest: provider kind, Store locator, profile, both SQLite versions, binding key, cursor key, layer stack, Branch, root |
| Install | Daemon subcommand, driven by the host | Write to a new temporary name in the volume, then rename. An existing Store is refused |

The native one-time handoff is implemented and proven in
[F5](../307/PRE-S8-F5-20261007.md), with both actual SQLite versions recorded.
After install the host cannot open the installed Store: the file is inside the
VM. Fork and paged history reads are implemented as authenticated native control
commands in [F13](../307/PRE-S8-F13-20261007.md). Mount returns the prepared
Store/engine binding; S8 adds the required kernel attachment/readiness.

## 6. Store visibility

Bash run inside a Workspace must not see or open the Store or the overlay
database. Nothing confines Exec today; the current daemon has no Exec. The
required shape:

- the volume is mounted at a path only the daemon's user can traverse;
- Bash runs as a different, unprivileged user;
- the Exec launcher enters a private mount namespace and detaches the Store and
  overlay paths before `exec`.

The user change is required. Without it `/proc/<daemon>/root` and
`/proc/<daemon>/fd` reopen the file. Which user Bash runs as is open (O-8,
O-19).

## 7. Authority

Peer authorization guarded a network boundary that no longer exists. What
remains:

- which containers are given the volume;
- the authenticated control command that names a Workspace and Branch;
- hiding the volume from Bash.

Every daemon therefore holds whole-Store authority. Per-Branch and per-object
scoping between daemons is gone. This is a consequence of the direction, stated
so it is accepted knowingly.

Immutability still does not supply authority, reference closure, mutable
history or safe collection. Bytes are saved before a root references them
inside one publication, and a Branch moves only by conditional transition.

## 8. Retired by this contract

| Retired | Why it existed |
| --- | --- |
| SDK `runtime/` (service, supervisor, handlers, custody, sessions, wake) | Served Store operations to a remote daemon |
| SDK `client/` | Called them from the daemon |
| Bridge `codec/`, `contract/`, `native/framing.rs` | Fragmented and credited data frames |
| Daemon `upstream/` | Bound the daemon to the host runtime |
| #307 S9 R1 application assembly, R3 restart custody, R4 remote Save | Not built; no boundary left to serve |

The Bridge keeps its authenticated native record channel for control, selected
by the pre-S8 assignment. Its prior data codec/contract/framing, SDK runtime and
daemon upstream are retired in [F12](../307/PRE-S8-F12-20261007.md).

## 9. Prerequisites

| ID | Requirement | State |
| --- | --- | --- |
| P1 | Semantic admission and runtime authority adapters | Withdrawn with the wire; bounded root checks remain (§4) |
| P2 | Locked Linux build of Content, Storage, History and Persistence with bundled SQLite | Required; Persistence is new to Linux |
| P3–P7, P12–P14 | Content and Init bounds | Unchanged by this revision |
| P8 | Shared-engine payload, capture, orphan and failure algorithms | Unchanged |
| P10 | Unknown-history resolver | Unchanged: `Uncertain` stays terminal |
| P15 | Persistence opens on Linux; WAL profiles; typed `Busy`; seal | New |
| P16 | Two daemons on one volume: concurrent Saves, a Branch race, `Busy` leaves no effect | New proof |
| P17 | Exec confinement hides the Store and overlay paths | New, with S8 |

## 10. Failure boundaries

- An unknown Store outcome quarantines one daemon's session, not the Store.
- A killed daemon leaves its locks to the kernel; the next opener recovers the
  WAL. Its unreferenced published objects are harmless and are not collected.
- A shared daemon overlay failure still affects that daemon's Workspaces.
- Unknown stage, transition or discard retains custody; nothing is resent or
  deleted on a guess. Terminal unmount never deletes shared objects or history.
- WAL size has no bound under overlapping readers; passive checkpoints can
  starve. This is recorded, not solved.
- Durable on Docker Desktop reaches the VM's virtual disk. Host power-loss
  durability is not claimed.

## 11. Platform and acceptance

Init runs on macOS. The daemon, FUSE and Exec run on Linux. The host builds the
Store with the system SQLite and daemons open it with the bundled one; the file
format is portable and the schema identity is checked at open. ARM64 builds keep
the repository build inputs.

Qualification must show the full-root per-tool-call path and long-lived
multi-call Workspaces with incremental Commits, on a Store shared by more than
one daemon, under the declared profile. Historical host-mediated receipts keep
their topology and verdicts; selections whose subject is the removed transport
are `NOT_RUN — mechanism removed` beside a prospectively registered successor
(O-16).
