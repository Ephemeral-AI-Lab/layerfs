# B. Engine and API: how LayerFS implements "ephemeral computing"

Code identity: `origin/layerfs_v2_0_0` at `8cfb6e2734` (read with `git show`,
never checked out). v1.x comparison: main checkout at `67310175eb`.

Citation conventions:
- `src:<path>:<line>` is a source file on `origin/layerfs_v2_0_0`.
- `doc:<path>:<line>` is a 2.0 architecture page under `docs/architecture/` on
  the same branch. The #671 architecture rewrite (worktree
  `layerfs-docs-671`, `2f1a9ac938`) is already merged into 2.0: 2.0 has no
  `core/docs/architecture/`, and its `docs/architecture/` matches the rewrite.
  The old numbered docs (19, 20, 21, 64, 79, 83, 84, 85) exist only in the
  v1 main checkout, under `core/docs/architecture/`.
- `v1:<path>:<line>` is the main working tree (v1.x).

Every architecture page declares itself "Current architecture guide for
LayerFS 2.0.0; maintained with the source", and each row cites the source
that implements it (doc:README.md:119-121). I spot-checked the most important
claims against source (marked **[verified in src]**). Everything below is
implemented behaviour unless it is marked *proposal* or *v1 only*.

---

## 1. Workspace mount

**What happens** (doc:flows/workspace.md:25-32, 58; doc:engine/daemon/control-calls.md:51-65):

1. The caller sends a mount request naming the project and the Branch. The
   caller reads no Store; the daemon finds the Branch by name
   (doc:api/sdk/workspace.md:69-72; doc:control/README.md:96-100).
2. Daemon registry check: no existing entry, capacity remains, and no
   retained maintenance failure. It then inserts `Entry::Binding`
   (doc:engine/daemon/control-calls.md:46).
3. `Store::bind` (src:engine/crates/layerfs-store-service/src/store/bind.rs:62-123) **[verified in src]**:
   - `readers.mount`: takes a Store read session, reusing an idle one first
     (`IDLE_READERS`=4, doc:engine/store-calls/README.md:164).
   - `bind.snapshot`: one read of the Branch snapshot, which includes the
     effective root (doc:engine/store/README.md:121).
   - `bind.root`: `checked_base` → `BaseView::open`. This is "one canonical
     root acquisition, no scan". It decodes the `FilesystemRoot` and checks
     its `InodeScope` (src:engine/crates/layerfs-workspace/src/base/view.rs:44-61) **[verified]**.
   - `bind.overlay`: `owner.open(...)` copies the template to a fresh
     `<n>.sqlite` and runs `open_workspace(base_root)` as the first job
     (doc:engine/overlay/README.md:62-65).
4. Template copy (src:engine/crates/layerfs-overlay/src/database/startup.rs:59-84) **[verified]**:
   - `create_new` with mode 0600, then one `std::io::copy` of the template
     bytes. "Copies into fresh, independently writable files; never a
     hardlink" (startup.rs:26).
   - The template is built once at daemon start: `Overlay::create_observed`
     writes the schema and then closes the file. Its size is recorded, and a
     mount refuses a template whose size changed
     (doc:engine/overlay/README.md:62).
   - `Backing::new` records the file's device, inode and link count 1
     (doc:engine/overlay/README.md:64).
5. Attach: a FUSE session on Linux, or a loopback NFSv3 export on macOS
   native (doc:flows/workspace.md:59). Serial reservation happens before any
   kernel request can create a file (doc:engine/daemon/control-calls.md:57-63).

**What is NOT done.** "No tree is scanned or copied"
(doc:flows/workspace.md:58). The invariant is: "Mount copies only the
template; it scans and copies no tree" (doc:engine/overlay/README.md:173).
The Workspace invariant is: "Mount binds the root without scanning, copying
or restoring it" (doc:engine/workspace/README.md:144). Nothing filters names:
`.git`, ignored files, dependencies and caches are all visible
(doc:engine/workspace/README.md:58, 143). No Overlay row exists for the
Workspace itself; `WorkspaceState` lives in memory
(doc:engine/overlay/README.md:85-87).

**Why the cost does not depend on tree size.** The fixed work is:
- one snapshot read;
- one root object decode;
- one copy of a schema-only template file, whose size is fixed and checked;
- one overlay open job;
- kernel mount setup.

None of these touches the inode table beyond the root object, and none
touches directories or files.

**Where content is demand-loaded** (doc:engine/workspace/README.md:66-89):
- `BaseView` answers each lookup, stat, list, read or readlink by reading
  only the objects it needs: inode table pages, directory pages, the metadata
  root, and the file mapping. `plan_read` is at most 128 KiB.
- Each fetched object is hashed back to its `ObjectId` (authenticated).
- The `CanonicalCache` is one LRU per Store binding: daemon default 64 MiB,
  shared by every Workspace on the daemon and keyed by object identity, so
  install needs no invalidation (doc:engine/workspace/README.md:83-84, 145).
  The 64 MiB value is set in src:control/crates/layerfs-sandbox/src/lifecycle.rs:76-82.
- Sequential chunked reads fetch the range plus 8 MiB in one demand.
- One demand carries at most 4,096 ids, 16 MiB per object and 32 MiB total
  (doc:engine/workspace/README.md:72-75, 87, 161).
- Base I/O never happens under the overlay lock
  (doc:engine/workspace/README.md:42-43).

---

## 2. Overlay design

**One SQLite file per Workspace.** In the daemon this is
`<dir>/<n>.sqlite`. Its tables are `fs_inode`, `fs_entry`, `fs_data`,
`fs_truncate`, `fs_data_ref`, `fs_digest`, `fs_predecessor`, `cleanup_task`
and `commit_record` (doc:engine/overlay/README.md:18-37, 76-87). All tables
are STRICT, `application_id` is 1279676210 and `user_version` is 39.

**Profile** (src:engine/crates/layerfs-overlay/src/database/profile.rs:56-62) **[verified]**:

| Setting | Value |
| --- | --- |
| `page_size` | 4096 |
| `auto_vacuum` | NONE |
| `journal_mode` | MEMORY |
| `synchronous` | OFF |
| `locking_mode` | EXCLUSIVE |
| `mmap_size` | 0 |
| `foreign_keys` | ON |
| `temp_store` | MEMORY |
| `cache_size` | -pager_kib |
| `max_page_count` | `u32::MAX-1` as the daemon passes it |
| `busy_timeout` | 0 |

Every value is read back after it is set (doc:engine/overlay/README.md:89-98).

The daemon's pager is 4 MiB: "one 256 KiB write window's transaction dirties
a few hundred pages" (src:control/crates/layerfs-sandbox/src/lifecycle.rs:84-88).

No sync and no space reserved ahead of use: `close_database` performs no sync,
and the reserved-tail and cleanup-headroom bytes are 0
(doc:engine/overlay/README.md:170).

**Schema highlights.**
- `fs_inode`: one row per changed inode **per generation**. It holds kind,
  mode, mtime, nlink, size, `inherited_cutoff`, the shrink epoch and height,
  `referenced`, and a content sequence.
- `fs_entry`: one row per changed name per generation. A NULL inode is a
  whiteout.
- Lookup takes the newest row with `installed < generation <= active`. No row
  means the committed base decides; Workspace merges the two outside SQL
  (doc:engine/overlay/README.md:78-79, 107-116).
- Payload (doc:engine/overlay/payload.md:34-50):
  - Cells are 4096 B.
  - Dense runs are at most 32 KiB, inside one slot.
  - A validity mask distinguishes written zeros from holes. Holes cost no
    rows.
  - A shrink is a "staircase" of `fs_truncate` steps: it touches one row and
    one step and does a binary search (payload.md:52-72).

**Reference pieces** (doc:engine/overlay/payload.md:74-89). An `fs_data_ref`
row maps `(generation, inode, file_offset)` to `(length, source content
root, source_offset)`. The rule is:

> "file bytes [file_offset, file_offset+length) of this layer == committed
> bytes of content root `source` from source_offset"

A piece holds no bytes. Pieces come from a write job's `WriteClaims`, at most
34 ranges per job. This lets a copy or rewrite of committed bytes be recorded
without copying them, and Commit later reuses those extents.

**Append digests.** For a file this Workspace creates and only appends to, a
BLAKE3 state of about 2 KiB is kept in open state. On last close it is pinned
in `fs_digest`. Commit uses it to reuse an already-saved root ("relink")
(payload.md:91-98; doc:engine/commit/README.md:69).

**One lock per Workspace.** `Shared.state: Mutex<State>` holds the
connection, failure, maintenance cursor, backing and counters. Every job,
reply attempt and close takes it (doc:engine/overlay/README.md:66).

Hold rule: "the closure performs at most one transaction and does no provider
I/O" (src:engine/crates/layerfs-store-service/src/overlay/owner.rs:214-215)
**[verified]**. The invariant is: "held for one short job, never across Store
I/O, a whole Exec or a whole Commit" (doc:engine/overlay/README.md:169).

Workspaces share no connection, file, pager or lock (README.md:42, 168).

Mutations use a two-round plan (doc:engine/workspace/README.md:91-113):
1. An owner job decides using local rows, or returns `Needs`.
2. Base facts are supplied with no lock held.
3. A second job applies the change in one transaction.

A compound job publishes at most 4 inodes, 2 names and one payload write
(doc:engine/overlay/README.md:120).

**Open-file and kernel-reference state in daemon memory.** `OpenState` is
never written to the file. It changes only inside a job and commits or rolls
back with that job's SQL through undo journals. It holds:
- the kernel connection;
- `(mount, serial)` lookup counts and retained parents;
- open descriptors and directories, and READDIR cookie pages;
- read windows, operation owners and captured readers;
- custody records, base-source windows and orphans.

(doc:engine/overlay/README.md:127-145)

It is unbounded: "Grows with open files, lookups, readers and kernel
references; no cap (the one resident exception)"
(doc:engine/overlay/README.md:181; also repository AGENTS.md).

**Bounded job windows** (doc:engine/overlay/README.md:180; doc:engine/workspace/README.md:156-157):

| Window | Limit |
| --- | --- |
| `WRITE_WINDOW` | 256 KiB |
| `READ_WINDOW` | 128 KiB (also FUSE `max_write`) |
| `PAGE_ROWS` | 1,024 |
| `RECORD_ROWS` | 64 |
| `OPERATION_RECORD_BYTES` | 64 KiB |

These bound one job, not a file or a Workspace. "Total files, names, edits or
Workspace size: No cap."

Maintenance (FOLD, RETIRE, STALE, ORPHAN, and so on) runs as one-page steps
(14 cells or 64 rows) after the lock is released (doc:engine/overlay/lifetimes.md:88-132).

---

## 3. Commit

Flow: doc:flows/commit.md:9-58. Driver: `commit_admitted`
(src:engine/crates/layerfs-store-service/src/store/commit.rs:87-330) **[verified]**.

**Admission.** A `committing` compare-exchange allows one Commit per
Workspace; a second gets `CommitError::InFlight`, shown as control code
`Busy` (commit.rs:61-66; doc:engine/commit/README.md:125, 140).

**UpToDate shortcut** (commit.rs:113-147) **[verified]**. The Commit returns
`UpToDate` with no capture, Save, lane or install when all of these hold:
- `dirty_inodes == 0` and `dirty_directory_entries == 0`;
- there is no captured or consolidating generation;
- `base_root` equals the bound root;
- there is no pending conflict;
- one Branch-snapshot read shows the same head and base Layer.

A second case: if the constructed candidate equals the head's root,
`commit_branch` answers `UpToDate` and creates no Commit record
(doc:engine/commit/README.md:62-63).

**Capture of the published frontier, from any process.** "Every change a
filesystem request already published to the overlay, from any process,
whatever launched it and whether it still runs. No Exec, command or process
identity enters capture" (doc:engine/commit/README.md:58). Kernel visits
carry a mount, serial and handle only (doc:engine/workspace/README.md:60).

`Overlay::capture` copies no rows: active generation G becomes captured, G+1
becomes active, and the dirty counts restart (doc:engine/overlay/lifetimes.md:36-40).

The frontier is finite. A waiting Capture makes later mutations wait (before
any effect), and capture waits until no reply ticket is pending
(doc:engine/overlay/lifetimes.md:42-64). Dirty shared-mapping pages not yet
sent as WRITE are outside the capture: "a command's exit is not a flush
fence" (doc:engine/commit/README.md:79-81).

**Construction** (doc:engine/commit/construction.md):
- One producer thread reads only the sealed generation through a
  `CapturedReader`, plus the immutable base bound to the reader's root. It
  never reads active state (construction.md:65-69).
- Passes run in order: names, then values, then the backed streamed update
  `update_filesystem_streamed_backed` (construction.md:39-51).
- Per-inode handling (construction.md:54-63):

| Inode change | What Commit does |
| --- | --- |
| Fresh file whose digest names a saved root | Reuse that root |
| Fresh file | Build from captured runs, with a rename predecessor (or the base file at the same path) as delta base |
| Changed base file | `apply_indexed_edits_view_backed` over the retained `FileView` |
| mtime only | Base row kept |

- Construction state lives in indexed operation records in the overlay file.
  "No namespace-sized collection is held in memory" (construction.md:92-95).
- The producer-to-Save queue holds 32 messages, about 8–32 MiB in flight
  (construction.md:125).
- It shares Content, Storage and Persistence with Project Init; there is no
  Commit-only encoder (doc:engine/commit/README.md:130).

**Save, then the publication lane** (commit.rs:183-306; doc:engine/store/README.md:107-125).

Before the lane:
1. Read the head. If it moved and `force` is false, run `merge_candidate`
   (a three-way `merge_roots`) in the same Save.
2. `Save::finish`, the logical count, binding hints, and validation and
   install preparation.

Under the lane (`publish_lane()` holds one admission turn plus the
cross-process writer: a directory `flock` for SQLite, or the `store_lane` row
for PostgreSQL):
1. Re-read the head. If it moved again, `merge_in_lane` merges only what
   landed, bounded by `LANE_MERGE_PATHS`=1,024 serials; beyond that the
   Commit fails with `CommitError::LaneMerge` and nothing is published.
2. `commit_branch` runs one write transaction: it checks the expected head
   and base, inserts the Commit, and advances the Branch
   (doc:core/history/README.md:82-101).

Reads never take the lane (doc:engine/store/README.md:135). The concurrency
model is one global DB, one writer, short holds; concurrent Commits are
correct but "not the optimized case" (doc:engine/commit/README.md:150).

**Install.** `prepare_base_install` rebinds the base to the new root without
a tree walk. After publish, `install_prepared_base` waits for
`base_readers == 0`, then one SQL step sets `base_root`, sets
`installed := G`, and queues `RETIRE G`. Rows written after the capture stay
on top of the view (doc:engine/commit/README.md:70-71). Kernel caches:
installing the Workspace's own root sends nothing; for a merge root the
daemon sends `inval_inode`/`inval_entry` per stale serial before replying
(README.md:72; doc:engine/kernel-routes/README.md:135).

**Why the cost follows the change.**

- *Localized edits* (doc:core/files/edits.md:50-126):
  - A chunked file is a B+tree of extent pages (64–128 entries, at most 8 KiB
    per page) over chunks of at most 32 KiB.
  - An edit splits the stored extents at its boundaries, discards the
    replaced middle unread, chunks only the replacement, and rebuilds pages
    only on the join paths. Unread pages keep their IDs.
  - Cost table: mapping pages read grow with "paths from the root to each
    edit boundary"; payloads written grow with the replacement bytes.
  - Measured in a test: one overwrite of three extents reads 3 pages on an
    8 MB file and 4 on a 24 MB file, with the same 3 payloads in both
    (edits.md:122-124; test `core/crates/layerfs-content/tests/edit_localized.rs`).
- *Reuse of unchanged subtrees* (doc:core/filesystem-trees/updates.md:49-56;
  doc:core/filesystem-trees/README.md:86-90):
  - "A directory with no changes keeps its stored root. A content-only file
    change touches only the inode table."
  - The sorted B+tree engine "reads only the children its changes reach …
    every unread child keeps its ID". Counters `pages_reused` and
    `untouched_subtrees` record this.
  - Directories are B+trees of name → serial. Names bind serials, not
    objects, so a file content change does not rewrite its parent directory,
    only inode-table leaves on the path.
- *Logical size*: `logical_size_after_with` reads only inode pages that
  differ between the two tables (updates.md:119-133).
- *Relink*: a fresh file whose append digest names an already-saved root
  reuses it (doc:engine/commit/README.md:69).

---

## 4. Unmount and reclamation

Source: doc:engine/daemon/control-calls.md:105-155; doc:engine/overlay/lifetimes.md:138-155.

**Dirty refusal.** Unless `discard` is set, a state job checks dirty counts,
captured and consolidating generations, `base_root` against the binding root,
and pending merge. A dirty Workspace replies
`Dirty(DirtyReport{paths<=256, …, truncated})` with no effect. Paths come from
64-row windows plus a bounded base walk (`WALK 4096`, `DEPTH 256`). Force
skips the check.

**Detach.** The kernel's reversible `Busy` is the fence: "every caller,
descriptor, working directory and mapping holds a mount reference". There is
no daemon-side request count.

**Drain.** `session.drain()`. If it is stalled at Join, the daemon makes one
abort write and drains again. If it is still stopped, it ends in
`Retained(...)` with exact custody.

**Revoke.** `kernel_revoke(mount)` runs, then `BoundWorkspace::close()` closes
the overlay and the Store reader.

**Delete the overlay.** `retire_namespace` does a checked close, then unlinks
the file only if both the terminal mark and the checked close succeeded and
the device, inode and link count still match. "Rows are not swept before
deletion: the file goes as a whole." The queued maintenance is deleted with
the file.

The whole unmount runs under a maintenance hold (#679). Committed history is
never touched (doc:flows/workspace.md:44-45). Daemon stop closes connections
with `terminal=false`, so files are not deleted (lifetimes.md:155).

Live reclamation happens during use: RETIRE of installed generations, orphan
migration, and stale cells. After a Commit's answer it is drained on a
separate `layerfs-reclaim` thread (lifetimes.md:120-131).

---

## 5. Sandbox lifecycle (precision matters for the "ephemeral" claim)

### 2.0 (target branch)

**What a Sandbox is.** "A Sandbox is one daemon a backend started, plus the
commands run in it" (doc:control/sandboxes/README.md:18-22). Backends
(doc:control/sandboxes/backends.md:13-53):

| Host | Backend | What it is |
| --- | --- | --- |
| macOS native | `native` / `HostProcess` | A host process served over loopback NFS; no isolation beyond the host user |
| Linux and every lima / apple-container / docker *machine* | `namespaces` | `unshare(NEWNS\|NEWPID\|NEWIPC)` under a cgroup v2 directory, an overlayfs root over an unpacked image, PID-1 helper; isolation class `MicroVm`, i.e. "the VM or container holding the kernel is the boundary" |

The old per-Sandbox **docker container backend is retired**: a docker record
is refused `Unsupported` (doc:control/sandboxes/README.md:54, 66). "docker" is
now a *machine* platform whose worker container hosts namespaces Sandboxes.

**Lifetime: explicit and long-lived.** `sandbox.create(project, name,
machine?)` starts a *named* Sandbox. It lives until `sandbox.close`,
`machine.stop`, or a crash (doc:api/sdk/sandbox.md:15-23, 44-84).

**No idle close was found in 2.0.**
- The v1 `idle_exit_s` field is gone.
- `IDLE_TTL_MS = 60_000` reclaims only *idle control-connection slots*
  (src:control/crates/layerfs-sandbox/src/lifecycle.rs:47-50;
  src:engine/crates/layerfs-daemon/src/application/connection.rs:151-176).
- Comments in `reopen.rs:30-33` still say a Sandbox "closed when idle". Treat
  them as stale or unclear, and verify with the owner before claiming any
  idle close for 2.0.

**Create cost.**
- Steps: claim the name under the home lock, build a fresh `DaemonSetup`,
  launch once, then wait up to 30 s for the daemon's `LAYERFS_DAEMON_LISTEN`
  line (polled every 10 ms). The handshake is Noise KK followed by
  `Hello`/`ControlReady` (doc:control/sandboxes/README.md:55, 90;
  backends.md:118-123; doc:flows/workspace.md:14-23).
- Namespaces extra steps: unpack the image once per digest (cached under
  `<root>/images`); copy the daemon, helper and runtime layer into the
  Sandbox directory; create the cgroup (backends.md:148-181).
- At startup the daemon builds the overlay template and opens the global
  Store (doc:flows/workspace.md:55).
- No timing figure is established in the docs I read; do not quote one
  without a measurement.

**Pooled or per Sandbox (reused across calls):**
- the daemon process;
- its open Store and the 64 MiB shared object cache;
- the overlay template;
- the image cache;
- up to 6 caller-side control connections per process (daemon admits 8)
  (doc:control/sandboxes/README.md:94).

**Per Workspace:** the overlay `.sqlite` copy, a Store reader (reused from 4
idle), the kernel mount and in-memory open state. Limit: **16 Workspaces per
Sandbox** (`WORKSPACES`; src:control/crates/layerfs-sandbox/src/lifecycle.rs:41)
**[verified]**; a mount into a full Sandbox is `Capacity`.

**Per call:** `workspace.exec` runs `bash -c` through the Sandbox runtime.
The daemon is not involved, and the command gets its own cgroup
`<sandbox>/ws-<id>/exec-<id>` (doc:api/sdk/workspace.md:80-120, 153-156).

**Answer to "container per call or reused?"** Reused. Neither 2.0 nor v1
starts a container or Sandbox per Bash call. The ephemeral unit is the
**Workspace** (overlay file + kernel mount), created per mount and deleted at
unmount. The compute host (daemon, namespaces and image) persists across
calls.

**Composition is explicit.** `workspace.exec` "does not commit … There is no
runtime timeout, no implicit Commit, no automatic unmount"
(doc:api/sdk/workspace.md:83, 120). The MCP server adds no Commit or unmount
of its own (doc:api/front-ends/mcp.md:229). "Mount → run → commit → unmount
per Bash call" is therefore an orchestration pattern the integrator issues as
four calls, not a single built-in API. The repository rule states
per-tool-call as "the expected common case [that] needs fast bootstrap"
(AGENTS.md "LayerFS mental model").

### v1.x (main) differences

- **Packing and numbering**: one Docker container per Sandbox, numbered
  automatically. A mount picks the lowest-numbered running Sandbox of the
  image with a free slot, else starts "the lowest free number"
  (v1:core/docs/issues/373/README.md:180-186;
  v1:engine/crates/layerfs-node/src/sandbox.rs:344-347).
- **16 Workspaces per Sandbox** (v1 sandbox.rs `WORKSPACES = 16`).
- **2-minute idle close**: `IDLE_EXIT_S = 120`. The daemon exits after 2
  minutes with no Workspace and no control call, its container stops, and the
  next create reclaims it (v1:engine/crates/layerfs-node/src/sandbox.rs:44-47;
  v1:core/docs/issues/373/README.md:184, 342-344).
- **Fixed daemon port** 30421 in the container, `STARTUP` 30 s (v1 sandbox.rs:30-33).

The "16 Workspaces / 2-minute idle close / numbering" facts are **v1**. In
2.0 the names are explicit, there is no auto-placement, and no idle close was
found.

---

## 6. In-process Store calls from the SDK

**One route, in the caller's process.** `fs.*`, `branch.*`, `project.*` and
`store.stats` run in the process that opened the Store, with no daemon route
and no fallback. The daemon serves only Workspace calls
(doc:engine/store-calls/README.md:47-52).

**How a file call commits without a Sandbox.** `files::commit`
(doc:engine/store-calls/README.md:113-142):
1. Resolve the Branch. `main` is ReadOnly; `main` changes only via
   `branch.publish`.
2. Bind a **private Workspace** with a fresh `WorkspaceId`, never listed or
   mounted. It gets its own overlay copied from the process's template.
3. Apply each change as `Mutation` jobs (`write`, `edit`, `mkdir`, `chmod`,
   `rm`, `cp`, `mv`). `edit` is a one-occurrence replace scanned in 32 KiB
   windows.
4. Run `commit_captured_via(message, CommitVia::Files)`: the **same**
   capture, construction, merge and lane as a mounted Workspace.
5. Close the Workspace and retire its overlay namespace.

Limits: 64 private Workspaces per process (`CALLS`;
src:engine/crates/layerfs-store-service/src/store_service.rs:32) **[verified]**.
Each private overlay's pager is 4 MiB. The file-call Store cache is only
64 KiB (`CACHE_BYTES`, store-calls README.md:77-83).

**Kernel guard** (src:engine/crates/layerfs-store-service/src/kernel_guard.rs:13-18, 101, 124-128) **[verified]**.
Before opening any file, the guard requires a real 0700 directory owned by
the effective user, and a filesystem of:
- Linux: ext4, xfs or btrfs;
- macOS (new in 2.0): apfs or hfs, i.e. native Sandbox on one shared macOS
  kernel.

Anything else is `Unsupported` with no fallback. PostgreSQL Stores (feature
`postgresql`) skip the guard (store-calls README.md:53-55).

**Where each call runs** (doc:control/README.md:65-91):

| Store | Where Store calls run |
| --- | --- |
| PostgreSQL | All in the caller |
| sqlite on the local machine | The caller |
| sqlite on another machine (e.g. a docker/lima VM) | Sent once to that machine's **worker** (`store.call`, loopback + secret), which opens the Store directly |

Per platform:

| Platform | macOS | Linux | Windows |
| --- | --- | --- | --- |
| native | caller | caller | worker |
| docker | worker | worker | — |
| apple-container | worker | — | — |
| lima | worker | — | — |

Worker rules: loopback, secret, one attempt, no retry or replay; an uncertain
outcome is reported (control/README.md:127-129).

**v1 vs 2.0.**
- v1: direct Store opening is Linux-only; the kernel guard is
  `#[cfg(target_os="linux")]` ext4/xfs/btrfs
  (v1:engine/crates/layerfs-store-service/src/kernel_guard.rs:9-15).
- v1 macOS/Windows: owner-approved exception through one loopback worker per
  LayerFS home, a Linux container on macOS or a WSL2 distro on Windows, using
  the real SDK and opening the Store directly. Later widened to any
  integration via `LAYERFS_WORKER_ENDPOINT` (AGENTS.md exception paragraphs).
- 2.0: generalizes this into machines + workers, adds macOS-native direct
  opening (apfs) and PostgreSQL.

Cross-process safety: one writer through a FIFO admission queue plus a
directory `flock` on the Store's parent, so openers queue instead of hitting
SQLite `Busy` (doc:engine/store/README.md:131-133, 161; flows/call.md:56-58).

Store profile: Disposable / WAL / `synchronous=OFF` only. "a kernel or VM
crash can lose or damage it" (doc:engine/store/README.md:136, 145-152).

---

## 7. Fork

**What fork writes.** `fork(tx, ForkRequest)`
(src:engine/crates/layerfs-persistence/src/history/branch.rs:241-298) **[verified]**:
1. Check that the stack exists, and that the Branch identity and name are
   free.
2. Resolve the source:
   - `Layer(id)`: base = that Layer, no head.
   - `Commit{branch, commit}`: the Commit must be in that Branch's ancestry
     (`ancestry_contains`). Base = the Commit's base Layer, head = the Commit.
3. **One `INSERT_BRANCH`** with `forked_from`.

"Metadata only; no file is copied … One short write through the Store's
single writer; no tree is read" (doc:api/sdk/branch.md:157-180;
doc:flows/merge.md:58; doc:engine/store-calls/README.md:148).

**Any starting point** (branch.md fork table):

| Source | New Branch starts at |
| --- | --- |
| `main` | Its head Layer (no head) |
| Branch name | Its head Commit, else its base Layer |
| Layer id | That Layer |
| Commit id | That Commit, on the Branch its metadata names |

**Fork of fork at any depth.** A Branch can fork from any Branch. History
says "Branches, depth, lineage walk: no count limit"
(doc:core/history/README.md:207). Ancestry queries are answered from lineage
tables "exactly and with no history-size bound"
(doc:core/history/README.md:143-148; doc:engine/store/README.md:98-105).

Caveats:
- `ancestry_contains` is a lineage-table query, not O(1). Its cost is not
  quantified in the docs.
- The store README phrases the Commit case as "on the source Branch's
  first-parent chain" (doc:engine/store/README.md:83), while history README
  says "in that Branch's ancestry". Wording to reconcile.

Subsequent mount of the fork: same constant-cost bind. Workspaces of
different Branches on one daemon share cached objects by ObjectId
(doc:engine/workspace/README.md:145).

---

## 8. Merge, conflicts and `.lww`

**At Commit landing (merge-on-commit).**
- If the head moved and `force` is false, `merge_roots(base, ours, theirs)`
  runs in the Commit's Save before the lane, plus a bounded in-lane step
  (§3).
- On a conflict nothing is published. The merged tree **with marker blocks**
  becomes the Workspace's local base, `PendingMerge` is kept, and the binding
  moves to the head. The next Commit is refused (`CommitError::Markers`) while
  a listed file still holds a complete marker block
  (doc:engine/commit/README.md:65-67, 145; doc:core/compare-and-merge/README.md:54-58).
- Marker labels are `workspace` and `<branch>@<head>` (merge.md:147).
- `force` publishes over the head and records the displaced head.

**`branch.merge`.** Creates a *new* Branch:
- fast-forward if one side is an ancestor;
- else merge base, optional merge units, and `merge_roots` in one Save before
  the lane, then one `merge_branch` transaction;
- a conflict drops the Save and answers `Conflict`
  (doc:engine/store-calls/README.md:149; doc:flows/merge.md:27-38).

**Algorithm** (doc:core/compare-and-merge/merge.md:11-58).
- Trivial root equalities return at once.
- Otherwise the merge walks only `inode_changes(base, from)` per serial (256
  per window), so a move follows its serial.
- Decision order:
  1. same / one side unchanged;
  2. a `/.lww` match: the newer mtime wins, ours on a tie;
  3. both bounded text: `merge3`, conflict hunks become marker blocks;
  4. binary: keep ours, `binary` conflict;
  5. modify/delete: kept side with deletion markers;
  6. kind mismatch: `type` conflict.
- In the lane, bytes are compared only up to `MAXIMUM_TEXT_BYTES`; larger
  files compare by root, so the hold does not grow with file size.

**`.lww`** (merge.md:60-78). Gitignore-style patterns: `!`, anchored `/`,
`*`, `?`, `**`, and `%include` of up to 16 files. For merge-on-commit, the
rules come from the Branch head (`RulesFrom::Theirs`); for `branch.merge`,
from ours. A deleted path takes its nearest surviving parent's mtime.
Decisions persist in the Commit's `lww` metadata (not in identity)
(doc:core/history/README.md:64).

**Merge units** (glob lists): a unit both sides changed comes whole from the
later writer; the loser is reverted in the same Save
(doc:engine/store-calls/README.md:154-157).

---

## 9. FUSE and macOS NFS specifics

**fuser 0.18.0 with exactly three authorized patches**
(doc:engine/kernel-routes/fuse.md:145-184). The source is vendored at
`engine/vendor/fuser-0.18.0` via `[patch.crates-io]` and checked by
`tools/check_fuser_integrity.py`.

| Patch | What it changes |
| --- | --- |
| signed timestamps | Upstream `e48279f` |
| session lifecycle | `into_runner`, `SessionRunner` and `SessionMonitor`, exact phases and outcomes |
| receive buffer | Each loop allocates `max_write+4096` instead of 16 MiB+4 KiB, so 2 loops × 132 KiB per mount |

**Negotiation** (src:engine/crates/layerfs-fuse/src/mount/profile.rs:31-46) **[verified]**:
- requires `FUSE_ASYNC_READ | FUSE_BIG_WRITES`, plus `FUSE_MAX_PAGES` if
  offered, and nothing else;
- `max_write` = `max_readahead` = 128 KiB;
- `max_background` 1, congestion threshold 1, time granularity 1 ns.

**Kernel writeback off.** `FUSE_WRITEBACK_CACHE` is never selected; every
`write(2)` is published before it returns (doc:engine/kernel-routes/README.md:153).

**Cache coherence** (doc:engine/kernel-routes/README.md:116-140):
- Replies are built only from published state.
- Entry and attribute TTL is 60 s.
- `FOPEN_KEEP_CACHE` on, `FOPEN_DIRECT_IO` off.
- WRITE returns the full count or an error.
- Hard links share one serial, so they are one kernel inode.
- The reply ticket is released after the one reply attempt.
- `inval_inode`/`inval_entry` are sent after installing a root other than the
  one the mount shows.
- `FLUSH`/`FSYNC` answer `ENOSYS` once.

Access is decided by the kernel: `allow_other,default_permissions`. xattrs,
locks, fallocate, copy_file_range and similar are NotImplemented; FIFOs,
sockets and devices are refused (README.md:96, 146-147).

Identity is stable: one serial per inode for the mount's life, across
rename, Commit and install (README.md:148).

Threads: 2 receive loops per mount, and a receive loop runs its request
itself (fuse.md:46, 88-103).

**macOS route: loopback NFSv3** (doc:engine/kernel-routes/nfs.md;
doc:engine/daemon/control-calls.md:90-103).
- `layerfs-nfs` serves one Workspace as a loopback NFSv3 export over the same
  mount ports, with a "synthetic kernel" holding lookup refs and implicit
  descriptors (65,536 lookups, 1,024 files, 256 directories).
- Client options include `actimeo=60`, `locallocks` and 256 KiB rsize/wsize
  (nfs.md:201-204).
- WRITE answers `FILE_SYNC` (published).
- Before each Commit the daemon flushes the client with
  `sync_volume_np(<dir>, SYNC_VOLUME_WAIT)`; a failed flush refuses the
  Commit.
- After installing another root, it refreshes through a mode-preserving
  SETATTR.
- AppleDouble `._` files can be refused.
- Known limit: application SQLite in WAL mode inside an NFS Workspace fails
  `SQLITE_CANTOPEN` (README.md:172; nfs.md:212-222).

Route per platform: `nfs` for macOS native only; `fuse` for everything else
supported; Windows unsupported (README.md:47-54).

---

## 10. What weakens or qualifies the paper's claims

1. **"Ephemeral compute" is not per-call containers.** In both v1 and 2.0 the
   Sandbox (daemon + namespaces/container + image) is long-lived and reused.
   Only the Workspace view is per mount. 2.0 Sandboxes live until an explicit
   `sandbox.close` (no idle close found in 2.0); v1 had a 2-minute idle exit.
   Phrase the thesis as "state is separated from compute; a compute host can
   be discarded and any compute host can mount any Branch", not "a fresh
   container per call".
2. **Per-call mount/commit/unmount is composed by the client.** It takes four
   explicit SDK calls; `exec` never commits or unmounts
   (doc:api/sdk/workspace.md:120).
3. **Mount is constant but not zero.** It costs:
   - a Store snapshot read;
   - one root decode;
   - a template file copy;
   - an overlay open;
   - a kernel mount (FUSE handshake, or an NFS export plus the
     `mount_nfs` helper, up to 60 s helper wait).

   It requires an already running Sandbox, whose creation waits up to 30 s
   and may unpack an image.
4. **Read costs grow with what is touched.** Demand reads pay real Store I/O.
   The cache is per daemon (64 MiB) and the file-call cache only 64 KiB, so
   cold Workspaces or file calls re-read pages. Other read costs:
   - Directory link counts are derived by listing child directories (windows
     of 256 rows), cached up to 16,384 entries (doc:engine/workspace/README.md:88).
   - A `find` or `npm install` over a large tree pays per-object reads.
   - `STATFS` is a fixed declared value (README.md:169).
5. **Commit work that can grow beyond the edit:**
   - A file rewritten wholesale (temp file + rename, or a tool that rewrites
     the file) is a *fresh* file. It is chunked in full, with only a delta
     base (predecessor or same path) or digest reuse to soften the cost.
   - The whole-file representation (below the small-file threshold) reads
     every kept byte. A whole-file base with a chunked result rescans the
     whole result (edits.md:124-126).
   - The dirty check walks base paths, bounded at 4096.
   - Merge naming without hints falls back to a breadth-first `locate`
     (compare-and-merge README.md:88-94).
   - In-lane merge is capped at 1,024 serials, else the Commit fails
     `LaneMerge`.
   - Concurrent Commits serialize on one writer.
   - One Commit at a time per Workspace.
6. **Resident state grows.** Overlay open state (descriptors, lookups,
   kernel refs) is uncapped. There are 16 Workspaces per Sandbox and 64
   private Workspaces per SDK process.
7. **Capture is not a process fence.** Unflushed `mmap` dirty pages and work
   by still-running descendants are not captured. Bash exit proves nothing
   about descendants (doc:engine/commit/README.md:79-81; AGENTS.md).
8. **Durability is weak by design.** The Store is Disposable/WAL/sync OFF
   ("a kernel or VM crash can lose or damage it"), and the overlay is never
   synced. Durable execution is `NOT_RUN — disabled by owner until explicit
   reauthorization`.
9. **Platform scope.**
   - Direct Store calls need ext4/xfs/btrfs (Linux) or apfs/hfs (macOS).
   - Other hosts go through a worker in a Linux VM or container.
   - Windows has no Sandbox backend; it only reaches a WSL2/VM worker.
   - macOS native gives no isolation (a host process).
   - The namespaces backend relies on the enclosing VM or container as its
     boundary (`MicroVm` class).
10. **Project Init is a full scan and copy.** This is one-time and expected,
    but it grows with the tree (doc:engine/project-init/README.md; import
    walks and copies the folder, doc:control/README.md:70).
11. **NFS route semantic gaps.** Examples: WAL SQLite apps in a Workspace on
    macOS; FSYNC answers ENOSYS on FUSE (`FILE_SYNC` on NFS). Apps that rely
    on `fsync` durability get "published", not "durable".
12. **No measured numbers here.** These notes establish mechanisms and
    bounds only. Every performance figure in the paper must come from
    `layerfs-benchmark` receipts.

---

## Paper-ready design summary (~800 words)

LayerFS separates an agent's *state* from its *compute*. All state lives in
one global Store: a single SQLite file in WAL mode (or one PostgreSQL schema)
that holds every project's immutable, content-addressed objects and its
history of Layers, Branches and Commits. Compute is any process that needs a
POSIX view of a Branch. The engine connects the two in two ways.

**File calls run without compute.** The SDK's file and history calls (`fs.*`,
`branch.*`, `project.*`) execute inside the caller's own process, which opens
the Store directly. There is no server and no daemon. A file change binds a
private, never-mounted Workspace backed by its own throw-away overlay
database, applies the edits as short overlay transactions, and commits
through exactly the same capture, construct and publish path that mounted
Workspaces use. On hosts whose kernel cannot safely share the Store file
(SQLite locking requires one kernel and a local ext4/xfs/btrfs or apfs
filesystem), calls are sent once to a loopback worker on the Store's machine,
with no retry or replay.

**Bash calls mount a complete Workspace.** A Workspace is the whole tree,
including `.git`, ignored files and dependencies, presented as a mutable
filesystem over one immutable committed root. Mounting a Branch costs the
same regardless of tree size. The daemon:
1. reads the Branch snapshot;
2. decodes and authenticates the root object;
3. copies a pre-built, schema-only overlay template into a fresh SQLite file;
4. attaches a FUSE session (Linux) or a loopback NFSv3 export (macOS).

Nothing is scanned, copied or installed. Every directory page, inode-table
page and file extent is fetched on demand from the Store, hashed against its
object ID, and cached in a per-daemon LRU shared by all Workspaces and keyed
by object identity.

**Each Workspace owns its overlay.** The overlay is a private SQLite database
(`journal_mode=MEMORY`, `synchronous=OFF`, `locking_mode=EXCLUSIVE`,
`temp_store=MEMORY`) recording only changes: names and whiteouts, inodes, and
file bytes stored as 4 KiB cells and 32 KiB dense runs.
- *Reference pieces* describe ranges equal to already committed bytes without
  copying them.
- Open files, lookups and kernel references live in daemon memory and are
  journalled with each transaction.
- A single per-Workspace lock is held for one short job: at most one
  transaction, never across Store I/O, a command or a Commit.
- Base reads happen between jobs.
- Every job is bounded by a window (256 KiB writes, 128 KiB reads,
  1,024-row pages), so no file, edit or Workspace size is capped.

Any process that can see the mount, launched by any executor, uses ordinary
system calls. No command is registered with the filesystem, and FUSE kernel
writeback is disabled, so every `write(2)` is published before it returns.

**Commit cost follows the change.**
1. *Capture* seals the overlay's active generation as the published frontier
   and opens the next one. It copies no rows and filters nothing by process.
2. *Construction* runs on one producer thread. It reads only the sealed rows
   and the immutable base:
   - Unchanged directories keep their stored roots.
   - A content-only change touches only inode-table leaves.
   - A changed file applies localized edits to its extent B+tree: it splits
     at edit boundaries, chunks only the new bytes, and rebuilds only the
     pages on the join paths.
   - A file recreated with identical bytes reuses its stored root through an
     append digest.
3. If the Branch head moved, the candidate is merged three-way with the head
   in the same Save, before publication.
4. *Publication* takes a short lane that holds the Store's single writer. It
   re-reads the head, merges only what landed meanwhile (bounded), and
   advances the Branch in one transaction.
5. *Install* rebinds the Workspace's base to the new root without a tree
   walk. Changes made after the capture stay on top.

If nothing changed, the Commit answers `UpToDate` without capturing, saving
or taking the lane.

**Ephemerality.** Unmount refuses a dirty Workspace unless told to discard.
It then detaches the mount (the kernel's busy check is the fence), drains
in-flight requests, revokes kernel references and deletes the overlay file
as a whole. Committed history is never touched. A Workspace is therefore a
disposable view whose only lasting product is a Commit.

The hosting compute (a *Sandbox*: one daemon plus a Linux namespaces/cgroup
environment, or a host process on macOS) is long-lived and reused across
calls. It holds up to 16 Workspaces and shares its object cache. Any Sandbox
on any machine attached to the Store can mount any Branch. Losing a Sandbox
loses only uncommitted overlays.

**History operations are metadata.**
- Forking writes one Branch row naming a Layer or Commit as its start; it
  reads no tree and copies no object, at any fork depth.
- `branch.merge` produces a new Branch: a fast-forward, or a merge Commit
  built by a serial-keyed three-way merge.
- Text conflicts become marker blocks. For merge-on-commit they are installed
  into the Workspace and block the next Commit until resolved.
- A `/.lww` pattern file declares paths where the newer modification wins.

Every step is one attempt: there are no retries, busy handlers or replays,
and an uncertain outcome is kept and reported, not guessed.

**Caveats a careful reader should see.**
- The Store runs with `synchronous=OFF`: it survives process death but not a
  kernel or VM crash.
- Demand reads still pay I/O in proportion to what a command touches.
- A file rewritten whole is re-chunked whole, softened by a delta base.
- Concurrent Commits are correct but serialized by the single writer.
- In-memory open state grows with open files and kernel references.
