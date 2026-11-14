# R. System architecture and concurrency (appendix notes)

Code identity: `origin/layerfs_v2_0_0` at `8cfb6e2734` (read with `git show`,
never checked out; no build, test or benchmark run).

Citation conventions (same as `B-engine.md`):
- `doc:<path>:<lines>` = a page under `docs/architecture/` on the 2.0 ref. Every
  page declares itself "Current architecture guide for LayerFS 2.0.0;
  maintained with the source" and cites source per row (doc:README.md:113-123).
- `src:<path>:<line>` = a source file on the 2.0 ref. **[src]** marks claims I
  re-checked in source for this note.
- `B §n`, `D §n` = the existing verified notes `notes/B-engine.md` and
  `notes/D-speed.md` (not redone here).
- *(inference)* marks a conclusion I drew that no page states directly.

---

## 1. Crate and layer map

**Layering is `core < engine < control < api`**, not `core < engine < api`: the
control layer sits between engine and API; a layer depends only on layers below
it, enforced by `tools/check_product_boundary.py` (doc:README.md:10-38;
doc:control/README.md:9-12). The root manifest builds every crate under
`core/crates/*`, `engine/crates/*`, `control/crates/*`, `api/crates/*`, excluding
the patched `engine/vendor/fuser-0.18.0` (src:Cargo.toml:6-14) **[src]**. At the
tip there are 26 crates: 4 core, 10 engine, 8 control, 4 api (`git ls-tree`).

Naming note: the "isolation" driver of the 2.0 compute-plane decision is the
crate `layerfs-sandbox-backend` at the tip ("was `layerfs-isolation`, D-9",
src:control/AGENTS.md:35-41) **[src]**. Control's three drivers are therefore
`layerfs-platform` (where Sandboxes run), `layerfs-sandbox-backend` (how a
Sandbox runs) and `layerfs-bucket` (backup storage) (doc:control/README.md:16-40).

| Layer | Crate | Owns (one line; from crate `description` and topic table) |
| --- | --- | --- |
| core | `layerfs-content` | Canonical immutable objects and ids, complete-file construction (CDC chunking, edits), filesystem trees, compare and merge (incl. `/.lww`) |
| core | `layerfs-storage` | Physical storage: object encoding (FULL/STORED/PREFIX deltas, zstd), packs, Saves, authenticated reads, the `PackPersistence` port |
| core | `layerfs-history` | Immutable history metadata: LayerStacks, Branches, Commits, Layers, stages, scoped allocation |
| core | `layerfs-telemetry` | Parent/child timing trees |
| engine | `layerfs-persistence` | The one global Store over SQLite or PostgreSQL: schema, sessions, FIFO admission, writer lock, publish lane |
| engine | `layerfs-store-service` | Store engine: typed reads, `file.*` writes via private Workspaces, Branch history, SDK Store-call facade; Store/overlay ports Commits use; Commit publication |
| engine | `layerfs-project` | Project Init: native directory -> first Commit; Store create/seal/install |
| engine | `layerfs-overlay` | Each Workspace's disposable SQLite overlay file, indexed state, bounded jobs, generations |
| engine | `layerfs-workspace` | Workspace filesystem semantics (base view + overlay), shared object cache, Commit construction |
| engine | `layerfs-mount-ports` | Route-neutral kernel request ports and phase timing (shared by FUSE and NFS) |
| engine | `layerfs-fuse` | FUSE session, dispatch and replies over patched fuser 0.18.0 (Linux) |
| engine | `layerfs-nfs` | Loopback NFSv3 export of one Workspace (macOS native) |
| engine | `layerfs-bridge` | Authenticated bounded duplex channel (Noise KK), control/SDK wire records, daemon setup |
| engine | `layerfs-daemon` | The Workspace daemon process: Store open, registry, Mount/Attach/Commit/Unmount, kernel serving |
| control | `layerfs-home` | Home folder: records, locks, `op` intent, names, secrets, traces, call log |
| control | `layerfs-machine` | Machine records and lifecycle; the worker client and protocol |
| control | `layerfs-store` | Store and project records, placement, leases, open-Store cache, sqlite/PostgreSQL management, backup |
| control | `layerfs-sandbox` | Sandbox records, launch, daemon control pool, Workspace calls, exec and cancel |
| control | `layerfs-control` | Management library under the call surface: worker server, inventory, doctor, migrate |
| control (driver) | `layerfs-platform` | One adapter per platform (`native`, `docker`, `apple-container`, `lima`); support table; worker setup |
| control (driver) | `layerfs-sandbox-backend` | How a Sandbox runs on a kernel: `host_process` (macOS) and `namespaces` (Linux) backends, commands, streams |
| control (driver) | `layerfs-bucket` | Bucket drivers (upload/ranged download/stat/delete, attempted once) and whole-Store backup |
| api | `layerfs-api` | The call table (73 calls in 10 families), argument checks, one dispatcher over control |
| api | `layerfs-cli` | The `layerfs` command, and `layerfs worker` inside a machine |
| api | `layerfs-node` | Node addon front end |
| api | `layerfs-python` | Python (PyO3) front end |

Sources: crate `description` fields in each `Cargo.toml` on the ref **[src]**;
topic-to-crate table doc:README.md:47-75; call count doc:api/sdk/README.md:5,
doc:flows/call.md:51. Engine crates never depend on control or API
(doc:engine/README.md:59-66); the daemon depends on every engine crate except
`layerfs-project` (doc:engine/README.md:39-43).

---

## 2. Process architecture

### 2.1 Processes and what each holds

**SDK (caller) process.** Every call enters `layerfs-api`'s dispatcher, which
hands it to a control function; that function decides once, from records read
under the entity's lock, whether the work runs in the caller, in a machine's
worker, or in a Sandbox daemon (doc:control/README.md:58-63).
- Store calls (`fs/file.*`, `branch.*`, `commit.*`, `project.*`, `store.stats`)
  run in-process when the Store is PostgreSQL or a sqlite Store on a local
  `native` machine; there is no daemon route and no fallback
  (doc:control/README.md:65-69; doc:engine/store-calls/README.md:49-55).
- `StoreService` holds one writer session, a read-only control reader, an
  overlay template, and up to **64 private Workspaces** (`CALLS = 64`, "one per
  SDK binding worker thread"; `bind` refuses `Capacity` at the limit), each with
  a 4 MiB overlay pager; its shared object cache is only 64 KiB
  (doc:engine/store-calls/README.md:59-82; src:engine/crates/layerfs-store-service/src/store_service.rs:32-38) **[src]**.
- A file write binds a private, never-mounted Workspace and commits through
  the same capture/construction/merge/lane as a mounted Workspace (B §6;
  doc:flows/merge.md:59).
- Kernel guard before any SQLite open: a private 0700 directory on ext4/xfs/btrfs
  (Linux) or apfs/hfs (macOS); otherwise `Unsupported`, no fallback; PostgreSQL
  skips it (doc:engine/store-calls/README.md:53-55).
- An open-Store cache keeps one open Store per path per process, closed after
  60 s unused, never while a call runs; each process using a Store holds a
  lease file `store/<s>/open/<pid>` (doc:control/stores-and-projects/README.md:59-60, 115).

**Sandbox daemon (`layerfs-daemon`).** "A Sandbox is one daemon a backend
started, plus the commands run in it"; the daemon supervises no command
(doc:control/sandboxes/README.md:18-22; doc:engine/daemon/README.md:178).
- Startup: one protected `--config` file, builds the overlay template, takes
  a fresh incarnation key, opens the global Store directly (SQLite Disposable
  or PostgreSQL), starts FUSE (Linux) or NFS (macOS), prints
  `LAYERFS_DAEMON_LISTEN` (doc:engine/daemon/README.md:43-65).
- Control: a listener with `connections` pool slots + 1 reserved recovery
  slot, one thread per admitted socket, Noise KK authenticated (only the
  controller's public key is accepted) (doc:engine/daemon/README.md:18-39, 93-101;
  doc:engine/bridge/README.md:44-46). The caller side keeps at most 6 pooled
  connections per process "of the daemon's 8" (doc:control/sandboxes/README.md:94;
  src:control/crates/layerfs-sandbox/src/daemon/pool.rs:31) **[src]**.
- Registry `WorkspaceId -> Entry`, at most `namespaces` Workspaces; the control
  layer sets 16 per Sandbox (`WORKSPACES`; doc:control/sandboxes/README.md:93;
  src:control/crates/layerfs-sandbox/src/lifecycle.rs:41; daemon default
  `workspace_capacity: 16`, src:engine/crates/layerfs-daemon/src/install_types.rs:20-27) **[src]**.
- Per Workspace: its own overlay SQLite file copied from the template
  (`EXCLUSIVE`, `MEMORY` journal, `synchronous=OFF`), one lock, in-memory open
  state, a Store reader, and a kernel mount (FUSE: 2 receive-loop threads)
  (doc:engine/overlay/README.md:27-42, 92; doc:engine/kernel-routes/fuse.md:84-104).
  "Workspaces share no connection, file, pager or lock" (doc:engine/overlay/README.md:42).
- Shared across Workspaces: one `CanonicalCache` per Store binding, daemon
  default **64 MiB**, keyed by `ObjectId`, so Workspaces on any Branch share
  objects and install needs no invalidation (doc:engine/workspace/README.md:84, 145;
  src:engine/crates/layerfs-daemon/src/install_types.rs:24) **[src]**.
- The daemon serves no Store calls: `Fork`, `History`, `Branches` are
  `Invalid`; other SDK names `UnknownMethod` (doc:engine/daemon/README.md:110-112, 179).

**Worker (`layerfs worker`).** One per machine whose kernel the caller cannot
open a sqlite Store on (doc:control/machines/workers-and-reach.md:13-18).
- Listens on a guest port that is carried to host `127.0.0.1:<port>`; a 32-byte
  secret is the first frame; hello checks protocol and bundle hash
  (doc:control/machines/workers-and-reach.md:22-39, 67-81, 85-113).
- **One request per connection, one thread per connection**; it runs the
  operation once (`store.call`, `store.file`, `sandbox.host`, `project.*`, ...)
  using its own root as home (doc:control/machines/workers-and-reach.md:112, 127-138).
- Outcome table: connect/secret/hello failure -> request *not* sent
  (`Unavailable`/`Denied`); write failed part way -> `Unknown` "not resent";
  answer lost -> `Unknown` "may have run, and was not resent"
  (doc:control/machines/workers-and-reach.md:115-125).
- Rule: "Workers listen on loopback behind a secret; one attempt per call, no
  retry, no replay; an uncertain outcome is reported, never resent"
  (doc:control/README.md:128).

**Commands.** `workspace.exec` runs `bash -c` through the Sandbox runtime
(`Sandbox::run_in`) in the Workspace directory; the daemon is not involved;
output inline up to 1 MiB per stream, else spooled; no timeout
(doc:control/sandboxes/README.md:61, 73, 97-98).

### 2.2 Machines and routes

| Platform | Hosts | Sandbox backend | Kernel route | sqlite Store calls | Workspace calls |
| --- | --- | --- | --- | --- | --- |
| `native` | macOS, Linux, Windows | macOS `HostProcess`; Linux `namespaces`; Windows none | macOS NFS; Linux FUSE | in caller (Windows: worker) | caller dials daemon directly |
| `docker` | macOS, Linux | `namespaces` inside the worker container | FUSE | worker | via worker (ports publish only at container start) |
| `apple-container` | macOS | `namespaces` | FUSE | worker | via worker (ports publish only at create) |
| `lima` | macOS | `namespaces` | FUSE | worker | direct, over an `ssh -L` forward |

Sources: doc:control/machines/platforms.md:14-19; doc:control/README.md:85-90;
doc:engine/kernel-routes/README.md:46-54. PostgreSQL Store calls and bucket I/O
always run in the caller; a PostgreSQL Store's Sandboxes run on the machines it
is attached to (doc:control/README.md:127). The old per-Sandbox docker
container backend is retired; `docker` is now a machine whose worker container
hosts `namespaces` Sandboxes (doc:control/sandboxes/README.md:54, 66; B §5).
`namespaces` = `unshare(NEWNS|NEWPID|NEWIPC)` under a cgroup v2 directory,
overlayfs root over an unpacked image, user namespaces blocked by seccomp
(B §5; doc:control/sandboxes/README.md:78, 92).

### 2.3 Component diagram (ASCII; used in the appendix)

```text
 HOST                                  MACHINE (native: the host itself;
                                        docker / lima / apple-container: VM)
 +----------------------------+        +----------------------------------+
 | SDK process (Node/Py/CLI)  |  TCP   | worker  (sqlite Store not        |
 |  layerfs-api dispatcher    | 127.0.0.1 local to caller only)         |
 |  control: records, leases  |=======>|  secret, 1 request/connection,   |
 |  StoreService (in-process) | secret |  runs store.call / sandbox.host  |
 |   writer session, readers  |        |  once; opens the Store directly  |
 |   <=64 private Workspaces  |        +----------------------------------+
 |   (file.* Commits)         |        | Sandbox = layerfs-daemon         |
 |  daemon control pool <=6   |=======>|  control slots (+1 reserved)     |
 +-------------+--------------+ NoiseKK|  registry: <=16 Workspaces       |
               |                       |   each: overlay .sqlite, 1 lock, |
               | in-process when       |   Store reader, FUSE | NFS mount |
               | PostgreSQL or local   |  CanonicalCache 64 MiB (shared)  |
               | native sqlite         |  Store session (one writer)      |
               v                       +----------------+-----------------+
 +----------------------------+                         |  commands: bash -c
 | global Store (one per      |<------------------------+  via namespaces /
 | home entry): SQLite WAL    |   opened directly by       host process;
 | file or PostgreSQL schema  |   every opener             daemon not involved
 +----------------------------+
```

---

## 3. Store access model

### 3.1 The pieces

| Mechanism | SQLite | PostgreSQL | Source |
| --- | --- | --- | --- |
| In-process order | FIFO admission queue per session; waiting is readiness with no time limit; exits only `Closed`/`Unknown`/`Integrity` (self-deadlock) | same `Admission` | doc:engine/store/README.md:132-133; src:engine/crates/layerfs-persistence/src/backend/admission.rs:1-15 **[src]** |
| Cross-process writer | `flock(LOCK_EX)` on a separate descriptor of the Store's parent directory (Linux and macOS), taken for every write tx (skipped when nested in the lane) | `SELECT 1 FROM store_lane ... FOR UPDATE` in each Write/Publish tx (skipped when nested) | doc:engine/store/sqlite.md:81-114; doc:engine/store/postgresql.md:115-131 |
| Write tx | `BEGIN IMMEDIATE`, `busy_timeout=0` -> `Busy` only for a writer that bypasses the lane | `READ COMMITTED` + `SET LOCAL synchronous_commit, lock_timeout`; never `SERIALIZABLE`, never retried | doc:engine/store/sqlite.md:68-73, 89, 103-108; doc:engine/store/postgresql.md:117-131 |
| Publish lane | `hold_lane()`: one admission turn + the flock held by one thread (`LaneHold` is `!Send`) across several short transactions | a second (lane) connection holding `store_lane FOR UPDATE` in one READ COMMITTED tx under `lock_timeout`; the thread's writes run nested on the session connection | doc:engine/store/sqlite.md:93-114; doc:engine/store/postgresql.md:151-156 |
| Lane wait bound | none (FIFO + flock; a signal during the wait is `Cancelled`) | `lock_timeout_ms` default 30,000, then `Busy` | doc:engine/store/README.md:163, 178; src:engine/crates/layerfs-persistence/src/store/config.rs:75 **[src]** |
| Reads | `run(false)` plain `BEGIN`; `read_immutable` with no `BEGIN` for rows that never change (`object_location`, `pack`, `pack_unit`, `inode_value_block`) | `REPEATABLE READ READ ONLY`; `Immutable`/`Statement` autocommit | doc:engine/store/sqlite.md:68-79; doc:engine/store/postgresql.md:117-124 |
| Uncertain | `Unknown` quarantines the session; later calls fail without SQL | lost reply in a tx -> `Unknown`, quarantined | doc:engine/store/README.md:137; doc:engine/store/postgresql.md:101 |

Rules: "Reads never take the lane or the cross-process lock"
(doc:engine/store/README.md:135); "Short holds: only head check, merge step and
publish run under the lane" (doc:engine/store/README.md:134); "Work in the lane
that grows with tree size, history or Branch count: a defect"
(doc:engine/store/README.md:171). Concurrent Commits on different Workspaces
are "correct, serialized by the publication lane; not the optimized case"
(doc:engine/commit/README.md:150).

**Precision about "no Busy, no retry".** One attempt is universal: no busy
handler, no retry, no replay (doc:engine/README.md:65). But `Busy` still exists
in three places: (a) a SQLite writer that bypasses the lane (an uncoordinated
raw SQLite process) gets exact `Busy` (doc:engine/store/sqlite.md:107-108;
doc:engine/store-calls/README.md:71-75); (b) PostgreSQL lane wait past
`lock_timeout` (30 s) is `Busy` (doc:engine/store/README.md:163); (c) a second
Commit on the *same* Workspace is `InFlight` (control code `Busy`)
(doc:engine/commit/README.md:140). Coordinated LayerFS writers on SQLite queue
instead of failing.

### 3.2 What runs outside vs inside the lane (one Workspace Commit)

From doc:engine/store/README.md:107-125, doc:engine/commit/README.md:43-52 and
src:engine/crates/layerfs-store-service/src/store/commit.rs:184-410 **[src]**:

Outside the lane (grows with the Commit):
1. Capture: seal overlay generation G, open G+1; copies no rows (Workspace
   lock, one short job) (doc:engine/commit/README.md:59).
2. Construct: one producer thread builds the candidate root from the capture
   (CDC chunking, hashing, trees), feeding the Commit thread's `!Send` Save over
   a 32-message queue (doc:engine/commit/construction.md:110-128).
3. Save waves: locate/reuse, select encoding (FULL/STORED/PREFIX delta), zstd for
   ordinary groups, **seal packs** (one SHA-256 per pack), and **publish ready
   packs during construction** in batches of at most 8,191 rows and
   `publication_bytes` (4 MiB − 1 SQLite, 16 MiB − 1 PostgreSQL)
   (doc:core/physical-storage/README.md:94-118).
   - Each such publication is one ordinary short write transaction:
     - SQLite: admission turn + flock + `BEGIN IMMEDIATE`, writing pack/pack_unit
       rows, `object_location ... ON CONFLICT DO NOTHING`, totals, value blocks,
       ordinal tail, signatures, `COMMIT` (doc:engine/store/sqlite.md:176-186;
       doc:engine/store/README.md:63).
     - PostgreSQL: `BEGIN; SET LOCAL ...; COPY BINARY pack bodies` **before** the
       lane row (ids already reserved, so a slow upload never holds the lane),
       then `SELECT ... FROM store_lane FOR UPDATE`, then one statement inserting
       locations (insert-if-absent; uniqueness holds *because* every location
       insert holds the lane row), value blocks, signatures, totals; `COMMIT`
       (doc:engine/store/postgresql.md:133-143;
       src:engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish.rs:1-48) **[src]**.
   - So "locator inserts hold the lane" is exact for PostgreSQL (the store_lane
     row, briefly, per publication) and holds on SQLite in the sense that every
     write holds the cross-process writer lock; neither is the *publish* lane
     that a Commit holds across head check and publish. *(inference from the
     two backends' docs)*
4. Head read; if moved and not `force`, `merge_roots(base, ours, theirs)` in the
   Commit's own Save (unbounded work, outside the lane) (doc:engine/commit/README.md:65).
5. `Save::finish`, logical size count, binding hints, candidate validation and
   install preparation (read only immutable published objects)
   (src:.../store/commit.rs:208-250) **[src]**.

Inside the lane (`history.publish_lane()`):
6. Re-read the head (`head.lane`); if it moved again, `merge_in_lane` merges only
   what landed in between, with its own small Save, bounded to
   `LANE_MERGE_PATHS = 1,024` serials, else `CommitError::LaneMerge` and nothing
   published (doc:engine/commit/README.md:66, 143;
   src:engine/crates/layerfs-store-service/src/store/commit_merge.rs:24) **[src]**.
   In-lane byte comparison is bounded to `MAXIMUM_TEXT_BYTES`; larger files
   compare by root (doc:core/compare-and-merge/merge.md:55-58).
7. Validate again only if the in-lane merge changed the candidate.
8. `commit_branch`: one history write transaction checking expected head and
   base, inserting or verifying the Commit, recording lineage, advancing the
   Branch (doc:engine/store/README.md:84). Lineage walks follow runs, not
   history length (doc:engine/store/README.md:98-105, 171).
9. Drop the lane; then install (base := new root) under the Workspace lock.

Other history writes (`fork`, `merge_branch`, `add_layer`) are single write
transactions that check their expected heads inside the transaction, ordered by
the one writer; they do not take the multi-transaction lane
(doc:engine/store/README.md:124-125; doc:flows/merge.md:48-51). `branch.merge`
does all merge work in one Save before its final `merge_branch` write
(doc:api/sdk/branch.md:210-211).

"Parallel object writes": separate `Storage` handles may run Saves concurrently
(doc:core/physical-storage/README.md:117), so construction, encoding,
compression and pack sealing of different Commits proceed in parallel threads
and processes; their *publications* interleave at batch granularity, each one
a short serialized write (SQLite) or a body COPY in parallel plus a short
lane-row statement (PostgreSQL). *(inference: the docs state each piece; the
interleaving picture is mine)* The Store remains one database with one writer
(AGENTS "One global database, one writer, short holds"; doc:engine/store/README.md:131-132).

Note for SQLite: while one thread holds the publish lane, the directory flock is
held, so every other process's writes (including other Commits' pack
publications) wait; reads continue. *(inference from doc:engine/store/sqlite.md:93-114)*
On PostgreSQL, other Commits' body COPYs continue during the lane; they wait
only at the lane row.

### 3.3 Two concurrent Commits (timeline; SQLite shown, PostgreSQL noted)

```text
 time -------------------------------------------------------------------->
 Commit A  |cap|construct + Save   |fin|val|=LANE A=|install|
 (ws1)         |  p   p   p   p    |   |   |hd pub  |
 Commit B    |cap|construct + Save      |fin|val|.....wait.....|==LANE B===|install|
 (ws2)           |  p   p   p   p   p   |   |   |             |hd mrg pub |
 Store      p=one short write tx (turn + flock + BEGIN IMMEDIATE);
 writer     A's and B's p interleave; LANE = flock held across hd/mrg/pub
 Readers    ---- never take the flock or the lane; never wait on A or B ----

 cap  capture (seal generation)      hd  head re-read under the lane
 val  validate, prepare install      mrg merge only what A landed (<=1,024
 fin  Save::finish, count, hints          serials), else LaneMerge
 pub  commit_branch: 1 history tx     (PostgreSQL: p = COPY bodies with no
                                        lock, then store_lane row FOR UPDATE
                                        for the location insert only)
```

If B read the head *after* A published (step 4), B's merge with A runs before
the lane in its own Save; only a head that moves between B's pre-lane read and
its lane entry is merged in the lane (doc:engine/commit/README.md:65-66). The
test `lane_merge.rs` shows "A Commit landing while another waits for the lane is
merged in the lane; over the bound is `LaneMerge`" (doc:engine/commit/README.md:161).

---

## 4. Read path

### 4.1 Hops

```text
 process read(2)/stat(2)
  -> kernel page cache (FOPEN_KEEP_CACHE; entry/attr TTL 60 s)       [hit: done]
  -> FUSE request (<=128 KiB: max_readahead = max_write = READ_WINDOW;
     max_background 1; 2 receive loops/mount)  | NFSv3 loopback on macOS
  -> mount-ports KernelData::read -> read_visit: one owner job under the
     Workspace lock: local cells, inherited mask, reference mask
  -> overlay decides? (written bytes / whiteout)                      [done]
  -> outside the lock: BaseView over CanonicalClient (this Workspace's
     Store reader), plan_read <=128 KiB of file mapping
  -> CanonicalCache (daemon 64 MiB, LRU, key ObjectId, charge len+256) [hit]
  -> miss: one upstream demand <=4,096 ids, <=32 MiB; read-ahead adds 8 MiB
  -> storage Reader: LocatorCache (4,096) -> locate -> pack cohorts that fit
     the 2 MiB pack cache -> acquire groups (whole pack or coalesced ranges)
  -> Store: read_immutable / Immutable statements (no BEGIN, no lane)
  -> resolver: decode delta base first, zstd decode, hash each object and
     compare with its ObjectId (authentication); then CanonicalClient
     re-checks ObjectId::for_bytes(bytes) == id
```

Sources: doc:engine/kernel-routes/README.md:72-82, 130-131; doc:engine/kernel-routes/fuse.md:42-44, 84-104;
doc:engine/workspace/operations.md:81-100; doc:engine/workspace/README.md:42-43, 68-89;
doc:core/physical-storage/README.md:130-150; doc:engine/store/sqlite.md:68-79.
- Pack cache: `DEPENDENCY_PACK_CACHE_BYTES = 2 MiB`
  (src:core/crates/layerfs-storage/src/store/policy.rs:108) **[src]**; it is
  per `Reader` (one operation), with `KeptReads` (3 MiB) across demands
  (doc:core/physical-storage/README.md:151).
- Read-ahead: `READ_AHEAD = 8 MiB` past the window, one upstream batch; a
  stream fetches only when the reader has passed the previous window, so each
  reader holds at most two windows ("owner cap: 16 MiB"); the cache needs at
  least 16 MiB to read ahead; streams live with the cache, at most
  allowance/16 MiB of them; random reads and whole-file objects never read
  ahead (src:engine/crates/layerfs-workspace/src/base/client.rs:20-30, 66-73 **[src]**;
  doc:engine/workspace/README.md:87). With the 64 MiB daemon cache this is at
  most 4 followed streams per daemon. *(inference: 64/16)*
- SDK-process reads use the same storage reader but a 64 KiB cache
  (doc:engine/store-calls/README.md:80); so read-ahead is off there *(inference:
  cache < 16 MiB)*.
- A failed base read answers `EIO` for that request only; the mount keeps serving
  (doc:engine/kernel-routes/README.md:167).
- No base I/O under the overlay lock; an overlay job does no Store I/O
  (doc:engine/workspace/README.md:42-43, 147).

### 4.2 Where costs come from (measured items are from D §3; causes marked)

- Per-request fixed cost: kernel requests at most 128 KiB with at most one
  background request (`max_background 1`), so a cold or warm sequential read is a
  chain of daemon round trips. Measured 500 MiB read ~158 MB/s, cold ≈ warm,
  unchanged after 5 overwrite Commits, so delta chains are not the cause; root
  cause not analysed, the window settings are *suspected* (D §3.1).
- Decode + authentication: zstd decode and BLAKE3/ObjectId re-hash of every
  decoded object, plus SQLite pack reads (D §3.1, source-derived suspicion).
- Many small files: one Store demand per file (no prefetch across a directory),
  ~0.4 ms/file vs ~0.2 ms ovnfs (D §3.2).
- What is *not* a cost: mount binds the root with one object acquisition, no
  scan (doc:engine/workspace/README.md:59, 144); history depth does not slow
  reads (D §3.1-3.2).

---

## 5. Lifecycle state machines

### 5.1 Sandbox

```text
 sandbox.create: claim name + op intent (home lock) -> launch once
   -> wait <=30 s for LAYERFS_DAEMON_LISTEN -> Noise KK -> Hello{wait}
   -> ControlReady -> record "running", intent dropped
 daemon phase: InstallPending -> Installing -> ControlReady | Retained
 record state: creating | running | closing | unresolved | gone | lost
 sandbox.close: settle commands (Busy, or cancel each with force);
   MountsActive unless force; stop(5) then delete, each once
```

Sources: doc:control/sandboxes/README.md:55, 58, 63, 90; doc:engine/daemon/README.md:67-91;
doc:api/sdk/sandbox.md:36-37, 124; doc:flows/workspace.md:14-23. `lost`: the record
has an id and no close intent, but the machine runs no such Sandbox
(doc:api/sdk/sandbox.md:124). No idle close of the Sandbox was found in 2.0
(B §5); the control-connection idle reap and the 60 s open-Store close are not
Sandbox lifetime.

### 5.2 Workspace

```text
 mount ----> Binding --(template copy, Store reader)--> Bound{Unattached, Idle}
   Attach --> Attaching --> Ready (FUSE) | Exported (NFS)
             \-> unknown engine mount: Attaching + Uncertain (no 2nd attach)
 Ready + Idle: any process reads/writes (one overlay job per change)
   commit --> Committing --> Idle                       (Committed/UpToDate)
                        \--> Idle + PendingMerge        (Conflict: markers)
                        \--> LocalFailure | Uncertain   (custody kept; next
                                                         Commit refused)
 unmount{discard?} --> dirty? -> Dirty reply (no effect)
   --> Leaving(Probing) --detach Busy--> Ready again (reversible)
   --> detach, drain, revoke, close --> entry removed, overlay file unlinked
   --> any stop after detach --> Retained(stage), owner kept
 ForceUnmount: one abort write, then the same teardown
```

Sources: doc:engine/daemon/control-calls.md:28-75, 105-131, 157-181;
doc:engine/commit/README.md:62-68, 115-119; doc:flows/workspace.md:58-62. Registry
fields: kernel `Unattached | Attaching | Ready | Leaving | Retained | Exported |
ExportLeaving | ExportKept`; activity `Idle | Committing | Closing | Attaching |
Uncertain | LocalFailure`; admission requires `Idle`
(doc:engine/daemon/control-calls.md:30-49). Overlay generations: active G ->
captured -> installed (retired) or, after a definite non-publication,
consolidating (folded back under the active one)
(doc:engine/commit/README.md:59, 70, 106).

### 5.3 Commit outcomes

| Outcome | Meaning | Source |
| --- | --- | --- |
| `UpToDate` (shortcut) | No dirty rows, no capture, base == bound root, no pending merge, head unchanged: no capture, Save, lane or install | doc:engine/commit/README.md:62 |
| `UpToDate` (from `commit_branch`) | Candidate root equals head root; work ran, no Commit record | doc:engine/commit/README.md:63 |
| `Committed` | Published against the bound head; or with `merged` non-null after merging a moved head; `force` publishes over and records `displaced_head` | doc:engine/commit/README.md:64-68; doc:api/sdk/workspace.md:177-181 |
| `Conflict` (merge) | Nothing published; marker-block merge becomes the local base, `PendingMerge` kept, binding moves to the head | doc:engine/commit/README.md:67 |
| `Conflict` (markers refusal) | `CommitError::Markers` while a listed file still holds a complete marker block; nothing published | doc:engine/commit/README.md:145; doc:api/sdk/workspace.md:189 |
| `LaneMerge` | Head moved again by more than 1,024 serials; nothing published | doc:engine/commit/README.md:143 |
| `Busy` / `InFlight` | Another control operation or Commit on the Workspace | doc:engine/commit/README.md:140; doc:api/sdk/workspace.md:190 |
| `Unknown` (uncertain) | Outcome unknown, or published but install/release failed: custody kept, activity `Uncertain`/`LocalFailure`, next Commit refused `Unknown` at phase `admission` | doc:engine/commit/README.md:115-119, 148-149 |

---

## 6. Failure and custody model

**One attempt.** "Every hop is one attempt. Nothing in the path retries,
refreshes or replays a request; waits for a turn, a lock or a worker's hello are
readiness, not retries. A lost answer after the request was sent is `Unknown`,
never resent" (doc:flows/call.md:42-44). Engine rule: no retry, busy handler or
replay (doc:engine/README.md:65). `settle_failure` resolves only a definite
non-publication and "never reads history to guess" (doc:engine/commit/README.md:134).

**Uncertain outcomes are preserved, not resolved.**
- Store layer: `Unknown` quarantines the session (doc:engine/store/README.md:137, 184).
- Commit: publication is one history transaction; "once it commits, the Commit
  exists whether or not the reply arrives" (doc:flows/commit.md:39-42); custody
  (captured reader, operation owner, published outcome) is kept and blocks the
  next Commit (doc:engine/commit/README.md:87-119, 133).
- Kernel replies: a published mutation is never undone after a lost reply
  (doc:engine/workspace/README.md:63; doc:engine/kernel-routes/README.md:151).
- Control: multi-step operations write an `op` intent first; an `Unknown`
  leaves `op {uncertain: true}` and a line in `log/uncertain.jsonl`; once its
  process is gone, later calls are `Unresolved` until a status read or an
  explicit remove/close (doc:api/calls/errors-and-events.md:75-104).
- Private Workspaces of uncertain `file.*` Commits are retained until close
  (doc:engine/store-calls/README.md:82).
- Daemon: up to 64 retained connection failures, each 1 h
  (doc:engine/daemon/README.md:105).
- Lost daemon reply at the SDK: `Unavailable` with `outcome: "unknown"`,
  `retryable: false` (doc:control/sandboxes/README.md:95); a lost daemon reply
  may surface as `Failed` with `attempted: true` (doc:api/calls/errors-and-events.md:102).

**What survives what.**

| Event | Committed history (Store) | Uncommitted Workspace changes | Running commands | Source |
| --- | --- | --- | --- | --- |
| SDK process exits | Survives (Disposable survives process termination); an in-flight `file.*` Commit is either published (one tx) or not | Private overlays are process-local *(inference: lost)* | n/a | doc:engine/store/README.md:149-152 |
| Daemon crash / Sandbox lost | Survives; the Store is shared and deleting a Sandbox never touches it | Lost: reopen finds the daemon gone -> `NoSuchSandbox` "with every Workspace it held lost"; overlays are disposable and never synced | Owned by the runtime, not the daemon; stale exec records are ended and removed by the next start/cancel/unmount/close | doc:control/sandboxes/README.md:58, 81; doc:engine/kernel-routes/README.md:138-139; doc:control/sandboxes/daemon-channel-and-exec.md:110-111 |
| Machine stop | Survives (disks stay) | Lost with the Sandbox; forced `machine.stop` marks Sandbox records `machine_stopped` / `machine_stop_unknown` | Stopped | doc:control/machines/README.md:54; doc:control/sandboxes/README.md:67 |
| Kernel / VM / host crash (SQLite) | **Not guaranteed**: Disposable / WAL / `synchronous=OFF` "a kernel or VM crash can lose or damage it"; Durable is NOT_RUN by owner | Lost | Lost | doc:engine/store/README.md:136, 145-152 |
| PostgreSQL server crash | Governed by `synchronous_commit` (default `Off`: acknowledged before WAL flush) *(inference from PostgreSQL semantics: recent acknowledged transactions may be lost, no corruption)* | — | — | doc:engine/store/postgresql.md:158-168 |

Backups are the off-host recovery path: whole-Store backup to a bucket, with a
heads digest identifying a copy (doc:engine/store/sqlite.md:163-174;
doc:control/README.md:55). Not covered further here.

---

## 7. Multi-agent coordination at the architecture level

- **Units.** Agents work in Workspaces; any number of Branches can be forked
  (metadata-only, one Branch row) at any depth (B §7; doc:flows/merge.md:58).
  Several Workspaces can mount the same Branch: mount names are unique per
  Sandbox, and the daemon test `concurrent_commits.rs` runs "Concurrent Commits
  to one Branch" (doc:api/sdk/workspace.md:56-62;
  src:engine/crates/layerfs-daemon/tests/concurrent_commits.rs:1-6) **[src]**.
  Limits: 16 Workspaces per Sandbox, 64 private Workspaces per SDK process; no
  count limit on Branches (B §7).
- **Isolation between agents.** Each Workspace has its own overlay file, lock
  and pager; writes are invisible to other Workspaces until committed and the
  other Workspace commits or remounts *(inference: a Workspace's base changes
  only at its own install)* (doc:engine/overlay/README.md:42, 168;
  doc:engine/commit/README.md:70). Within one Workspace, all processes share one
  frontier; Commit captures it regardless of who wrote (doc:engine/commit/README.md:58).
- **Merge-on-commit.** A Commit onto a moved head merges three-way (base = the
  Workspace's bound base, ours = candidate, theirs = current head) before the
  lane, and again in the lane only for what landed meanwhile (§3.2). Different
  paths: merged and committed; same path: text `merge3` hunks with markers,
  binary keeps ours, modify/delete and type conflicts listed
  (doc:core/compare-and-merge/README.md:54-58; B §8).
- **Conflicts.** Nothing is published; markers are written into the Workspace
  as its new local base; the next Commit is refused while markers remain; the
  agent resolves by editing files (doc:engine/commit/README.md:67, 145).
- **`.lww`.** Gitignore-style patterns in `/.lww`; a path both sides changed
  that matches takes the newer mtime (ours on tie). For merge-on-commit the rules
  come from the Branch head (`RulesFrom::Theirs`); for `branch.merge` from `a`
  (ours) (doc:core/compare-and-merge/merge.md:60-78). Decisions are recorded in
  the Commit's `lww` metadata (doc:engine/store/README.md:93-94).
- **`force`.** Publishes over a moved head without merging and records the
  displaced head (doc:engine/commit/README.md:68).
- **`branch.merge`.** Creates a new Branch: fast-forward if one side is an
  ancestor; else merge base, merge units and `/.lww` in one Save; a conflict
  creates nothing; the final `merge_branch` checks both sides unchanged
  (doc:flows/merge.md:60-62).
- **`branch.publish` (to `main`).** `main` is the project's line of Layers, never
  written directly. Publish adds the Branch head (or a named Commit) as `main`'s
  next Layer by compare-and-set on `main`'s head and the Branch's base
  (`HeadMoved` when either moved, never retried). It is **overwrite-only**:
  Layers added to `main` since the Branch's base are reported (`overwrote`,
  at most 50 listed, `overwrote_count`), not merged. Outcomes `Added`,
  `UpToDate`, `NoChanges` (doc:flows/merge.md:48-51, 63; doc:api/sdk/branch.md:220-246).
  So two agents wanting to combine work on `main` must `branch.merge` (or fork
  from `main`'s new head and merge) before publishing; publish alone is
  last-publisher-wins at Layer granularity. *(inference from the overwrite rule)*
- **Contention expectation.** Agent tool calls are separated by model latency,
  so concurrent Commits are expected to be uncommon; the uncontended Commit is
  optimized and contention must be correct and bounded (repository AGENTS.md,
  "One global database, one writer, short holds"; doc:engine/commit/README.md:150).

---

## 8. Gaps and cautions for the paper

- No measured lane hold time or contended-Commit latency is cited here; D §4.1
  records a PostgreSQL lane round-trip reduction (22–38 -> ~10) but no
  wall-clock contended series in the 2.0 docs I read.
- "Parallel object writes" on SQLite are parallel in CPU work only; the writes
  themselves serialize on the directory flock. Say this explicitly.
- The PostgreSQL lane wait can end in `Busy` after 30 s; do not claim "never
  Busy" for PostgreSQL.
- Read-path cost attributions beyond the measured numbers in D §3 are suspected,
  not root-caused.
- The 64 MiB cache, 16 Workspaces, 6/8 connections and 64 private Workspaces
  are defaults/constants at `8cfb6e2734`, not tuned optima.

---

## Paper-ready appendix section: System architecture and concurrency

**Layers and crates.** LayerFS 2.0 is 26 Rust crates in four layers, `core <
engine < control < api`; a layer may depend only on layers below it, and a
guard script enforces this. Core holds environment-free algorithms. Engine turns
them into a persistent Store and mounted Workspaces. Control knows which
machines, Stores and Sandboxes exist and where each call runs. The API layer is
one call table (73 calls in 10 families) behind the CLI, Node and Python front
ends (Table A1).

| Layer | Crate | Responsibility |
| --- | --- | --- |
| core | content | Objects and ids, CDC files, trees, diff and three-way merge, `.lww` |
| core | storage | Delta/zstd encoding, packs, Saves, authenticated reads |
| core | history | LayerStacks, Branches, Commits, Layers |
| core | telemetry | Timing trees |
| engine | persistence | The global Store (SQLite or PostgreSQL), admission, writer lock, publish lane |
| engine | store-service | In-process Store calls, Commit publication, private Workspaces |
| engine | project | Project Init (directory -> first Commit) |
| engine | overlay | Per-Workspace disposable SQLite overlay |
| engine | workspace | Workspace semantics, shared object cache, Commit construction |
| engine | mount-ports / fuse / nfs | Kernel routes: shared ports, FUSE (Linux), loopback NFSv3 (macOS) |
| engine | bridge | Noise-KK authenticated channel and wire records |
| engine | daemon | Sandbox daemon: Store, registry, mounts, Commits |
| control | home / machine / store / sandbox / control | Records, locks, intents; machines and workers; Stores, leases; Sandboxes, exec; worker server, doctor |
| control | platform / sandbox-backend / bucket | Drivers: platform adapters; namespaces or host-process Sandboxes; backup buckets |
| api | api / cli / node / python | Call table and front ends |

*Table A1. Crates at `8cfb6e2734` (the `layerfs-` prefix omitted).*

**Processes.** Figure A1 shows the processes. The agent's SDK process runs every
Store call (file reads and writes, fork, merge, publish) in-process, opening the
Store directly. A file write commits through a private, never-mounted Workspace,
up to 64 per process, using the same Commit pipeline as a mount. A *Sandbox* is
one `layerfs-daemon` on a machine's kernel plus the commands run beside it. The
daemon opens the same Store, holds up to 16 Workspaces, and serves each through
FUSE (Linux) or a loopback NFSv3 export (macOS native). Each Workspace has its
own SQLite overlay file, lock and kernel mount. All Workspaces in a daemon share
one 64 MiB object cache keyed by `ObjectId`, so Workspaces on different Branches
reuse identical objects without invalidation. Commands launched with
`workspace.exec` run through the Sandbox runtime (namespaces with a cgroup on
Linux, a host process on macOS); the daemon only serves files.

When a sqlite Store lives on a kernel the caller cannot open it on (a docker,
lima or apple-container VM, or Windows), calls go to that machine's *worker*.
The worker listens on host loopback, authenticates a secret, takes one request
per connection and runs it once. Daemons are reached over Noise-KK channels from
a pool of at most six per process.

```text
 HOST                                  MACHINE (native = host itself;
                                        docker / lima / apple-container)
 +----------------------------+        +----------------------------------+
 | SDK process (Node/Py/CLI)  | loop-  | worker: secret, 1 request per    |
 |  call table -> control     | back   |  connection, run once; opens the |
 |  Store calls in-process    |=======>|  sqlite Store on its own kernel  |
 |   <=64 private Workspaces  |        +----------------------------------+
 |  daemon pool <=6 conns     | Noise  | Sandbox: layerfs-daemon          |
 +-------------+--------------+  KK    |  <=16 Workspaces, each: overlay  |
               |               =======>|   .sqlite + lock + FUSE/NFS mount|
               v                       |  64 MiB object cache (shared)    |
 +----------------------------+        |  commands: bash -c via runtime   |
 | global Store: SQLite (WAL, |<-------+----------------------------------+
 | sync OFF) or PostgreSQL    |  every opener opens it directly
 +----------------------------+
```

*Figure A1. Process architecture.*

**One database, one writer, short holds.** All committed state lives in one
Store: one SQLite file or one PostgreSQL schema. Writes are ordered at two
levels:
- *Within a process:* a FIFO admission queue. Waiting for a turn is readiness,
  never a retry.
- *Across processes:* an exclusive `flock` on the Store's directory followed by
  `BEGIN IMMEDIATE` with `busy_timeout=0` (SQLite), or `SELECT … FOR UPDATE` on
  a one-row `store_lane` table (PostgreSQL).

Coordinated writers therefore queue rather than receive `Busy`. On PostgreSQL
the lane wait is bounded by `lock_timeout` (30 s), after which the call fails
`Busy`.

Reads never take the lock. Packs, locations and value blocks are immutable once
visible, so their reads run without a transaction.

A Commit splits its work around a *publish lane*: the lock plus an admission
turn, held by one thread across several short transactions. Everything that
grows with the Commit runs before the lane:
- capture (sealing the overlay generation, no copy);
- construction on one producer thread (chunking, hashing, trees);
- encoding, compression and pack sealing in the Commit's Save, which publishes
  packs as it goes in batches of at most 4 MiB on SQLite or 16 MiB on
  PostgreSQL;
- a full three-way merge with the head when the head has moved;
- `Save::finish`, size accounting and validation.

Each batch is an ordinary short write. On PostgreSQL the pack bodies are copied
in before the lane row is locked, so only the location inserts wait on it.

Inside the lane the Commit re-reads the head and merges only what landed since
its pre-lane read, bounded to 1,024 inodes (`LaneMerge` refuses anything
larger). It then runs one history transaction that checks the expected head and
advances the Branch. Construction and compression therefore run in parallel
across Commits. The actual SQLite writes still serialize on the flock, and the
critical section does not grow with tree size or history length (Figure A2).

```text
 time ------------------------------------------------------------------>
 Commit A |cap|construct+Save  p  p  p |fin|=LANE: head,publish=|install|
 Commit B   |cap|construct+Save  p  p  p  p |fin|..wait..|=LANE: head,
                                                 merge(A's delta),publish=|
 Store    p = short write tx (pack batch); A's and B's interleave
 Readers  never take the writer lock or the lane
```

*Figure A2. Two concurrent Commits.*

**Read path.** A read that misses the kernel page cache goes through these
steps:
1. FUSE requests of at most 128 KiB, with one background request at a time.
2. One overlay job under the Workspace lock decides the local bytes.
3. Inherited bytes are read *outside* that lock, from the shared object cache.
4. On a miss, one batched Store demand fetches the objects, plus 8 MiB of
   read-ahead for sequential streams (at most 16 MiB held per reader).
5. The storage reader (with a 2 MiB pack cache) decodes delta bases and zstd,
   and re-hashes every object against its `ObjectId`.

Mounting reads only the root object. Per-request round trips and decoding,
not history depth, dominate read cost.

**Lifecycles and custody.** A Sandbox is created, becomes `ControlReady`, and
lives until it is explicitly closed or its machine stops. A Workspace is
mounted (by copying an overlay template and binding the root) and attached,
then serves any number of processes and Commits. Unmount refuses a dirty
Workspace unless asked to discard it, then detaches, drains, and deletes the
overlay file. A Commit ends in one of these outcomes:
- `Committed` (possibly after merging a moved head);
- `UpToDate`;
- `Conflict`: markers become the Workspace's local base, and the next Commit
  is refused while markers remain;
- uncertain.

Every hop makes one attempt; nothing retries or replays. A lost reply is
reported as `Unknown`, and its custody (published outcome, captured reader,
operation intent) is kept, refusing further Commits until it is inspected.
Committed history survives a daemon or Sandbox loss, which loses only the
disposable overlays. Under the only permitted SQLite profile (WAL,
`synchronous=OFF`) the Store survives process termination, but a kernel or VM
crash can lose or damage it.

**Multi-agent coordination.** Agents isolate in their own Workspaces, on forked
Branches (forking is metadata-only) or on a shared Branch. Commit-time
three-way merge combines disjoint edits automatically. A `/.lww` file declares
paths where the newer mtime wins. `force` overwrites and records the displaced
head. Publishing to `main` is a compare-and-set that *overwrites*: Layers
published since the Branch's base are reported, not merged. Because tool calls
are separated by model latency, LayerFS optimizes the uncontended Commit and
keeps contention correct and bounded.
