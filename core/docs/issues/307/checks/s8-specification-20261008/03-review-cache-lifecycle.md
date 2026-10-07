# Cache, concurrency and lifecycle review — retained reviewer report

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Retained verbatim from one explicitly authorized read-only review subagent of
> the S8 specification task, 2026-10-08, local `main` at `32d969776`
> (product pin `f0797c646`). The reviewer ran no build, test, mount or
> measurement and edited nothing. Findings are input; their accepted, rejected
> or deferred disposition is in the [finding ledger](05-finding-ledger.md), and
> the [S8 specification](../../S8-SPECIFICATION-20261008.md) owns every decision.
> Paths and line numbers are the reviewer's citations at that pin.

---

## (a) Scope and pins

Read-only review at HEAD `32d969776` (product pin `f0797c646`). No builds, tests or measurements were run; every behavioural claim below comes from source reading. Paths are relative to `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/crates/` unless prefixed `docs/` (relative to `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/`).

**Read in full**
- `layerfs-workspace/src`: `base/{cache,client,view,file}.rs`, `workspace/{state,view,install,serials}.rs`, `ports/{mod,lengths,captured_runs,files,overlay}.rs`, `operations/{types,attributes}.rs`, `operations/file/{read,write}.rs`, `operations/namespace/list.rs`, `mutation/{driver,eval,facts}.rs`
- `layerfs-daemon/src`: `control/*`, `store/*`, `overlay/{owner,queue,commands,credits,read_port,file_port,captured_run_port,operation_record_port}.rs`, `service/{completion,job_sql}.rs`, `bootstrap.rs`, `lib.rs`
- `layerfs-overlay/src`: `lifetime/{workspace,close,lookup,file_owners,orphan,source,frontier,operation,generation}.rs`, `maintenance/{reclaim,garbage,ready,orphan,source_wait}.rs`, `database/profile.rs`, and `layerfs-overlay/sql/schema.sql`
- `layerfs-storage/src`: `read/{provider,objects,length,locator_cache,prefetch}.rs`, `store/handle.rs`
- `layerfs-persistence/src/store/open.rs`, `layerfs-bridge/src/control_types.rs`, `layerfs-content/src/object/access.rs`, `layerfs-content/src/file/view.rs`
- Docs: `AGENTS.md`, `core/AGENTS.md`, `303/workspace-api/{mount,unmount}.md`, `303/04-concurrency-commit.md`, `307/{R4-UPSTREAM-CACHE,R1-ROOT-BINDING,PRE-S8-F13,PRE-S8-RESOURCE-GROWTH-RESULTS}-20261007.md`

**Read in part or grepped only** (claims about these are limited to the cited lines)
- Source: `mutation/job.rs`, `operations/namespace/{create,remove,rename}.rs`, `construction/**`, `ports/operation_record.rs`, `install*.rs`, overlay `database/{connection,allocation,startup}.rs`, `payload/**`, `namespace/**`, `lifetime/{composition,captured_reader}.rs`, persistence `backend/sqlite/{transaction,connection,profile}.rs` and `history/*`, storage `read/fetch.rs` and policy constants, content `filesystem/read.rs` and `file/mapping/read.rs`, SDK `control.rs` (head only).
- Docs: `303/{README,daemon-sqlite,02-base-overlay,06-cluster-one-integration,08-decisions-provenance,fuse}.md`, `303/fuse-investigation/*`, `307/PRE-S8-COMPLETION`, `307/SERVERLESS-STORE-PLAN`, `docs/architecture/{48,61}`.

**Not read**: `303/workspace-api/{exec,status,commit}.md`, `307/PRE-S8-F14`, `307/SAVE-RESERVATION-DECISION`, `307/BRANCH-OVERWRITE-DECISION`, `layerfs-history` crate, bridge `native/**`, `service/{observations,startup}.rs`, `overlay/indexed_operation_record.rs`.

## (b) Classification

| Item | Class | Evidence |
|---|---|---|
| One Store per daemon: 1 writer session, N fixed readers (default 4), 8 MiB shared cache | IMPLEMENTED | `layerfs-daemon/src/store/open.rs:32-66`, `bootstrap.rs:57-91`, `install_types.rs:20-27` |
| Readers picked round-robin, then a blocking per-reader mutex | IMPLEMENTED | `store/open.rs:93-102` |
| Warm re-mount makes zero object demands; history snapshot still paid | PROVED (functional count) | `docs/issues/307/PRE-S8-F13-20261007.md:65-68`, `PRE-S8-COMPLETION-20261007.md:83-85` |
| Cold bind is 10/11/12 object batches for 1/1024/100000 files | PROVED (count) | `PRE-S8-COMPLETION-20261007.md:83-84` |
| Cache is single-mutex LRU; bytes cloned, inserted and freed under the lock | IMPLEMENTED | `layerfs-workspace/src/base/cache.rs:62-105`, `client.rs:87-108,151-165` |
| Oversized objects bypass retention | IMPLEMENTED | `cache.rs:20,83-88` |
| Single-flight or any miss coalescing | absent; PROPOSED | `client.rs:110-112` |
| Fair cold-read admission across Workspaces | absent; PROPOSED | `store/open.rs:93-102` |
| Overlay owner: per-namespace lanes, 6 classes, round-robin by job count, byte credits | IMPLEMENTED | `overlay/queue.rs:81-156,266-280`, `owner.rs:221-319` |
| Owner fairness | PROVED for finite arrivals only (32 × 6 classes × 4 Workspaces) | `PRE-S8-COMPLETION-20261007.md:74`; `PRE-S8-F10-F14-ENGINE-PLAN-20261007.md:88` disclaims infinite-load fairness |
| Event-driven completion | IMPLEMENTED as poll (`try_complete`) plus thread park only; no callback | `service/completion.rs:36-41,123-140` |
| Registry Bound token, activity arbitration between Commit and unmount | IMPLEMENTED, PROVED (F13) | `control/registry.rs`, `control/operations.rs` |
| Native Ready, Exec/request/open accounting, unmount Busy on handles | PROPOSED | `303/workspace-api/unmount.md:92`; nothing in source |
| Lookup/open/read/operation owners as SQL rows; last-owner reclaim | IMPLEMENTED (S6) | `layerfs-overlay/src/lifetime/*`, `maintenance/*` |
| Bulk terminal retirement of owners at unmount | absent; PROPOSED | `lifetime/close.rs:68-83`, `maintenance/reclaim.rs:51-63` |
| Automatic bounded reclamation, live and closed | IMPLEMENTED; functional only, no debt/latency qualification | `owner.rs:400-416,472-492`; `R4-UPSTREAM-CACHE-20261007.md:73-75` |
| Memory: Rust live ownership released at teardown; cache charge stays under 8 MiB | PROVED (diagnostic, not a ceiling) | `PRE-S8-RESOURCE-GROWTH-RESULTS-20261007.md:71-76,97-105` |
| Per-FORGET whole-state collection | absent in active source (old excluded adapter only) | grep for `forget` hits only `contract/custody.rs:84` |

## (c) Findings

### 1. What survives a mount/unmount cycle

- **Store.** `Arc<Store>` is daemon-lifetime. It holds one writable provider plus `HistoryProvider` on the same writer session (`bootstrap.rs:69,78-84`), `Vec<Mutex<Storage>>` readers and one `Arc<CanonicalCache>` (`store/open.rs:32-41`).
- **Reader selection.** `next.fetch_add(1) % len`, then a blocking `lock()` (`open.rs:97-100`). It does not try the next reader, pick the least loaded, or skip an unhealthy one.
- **Reader state.** Each reader keeps a persistent locator cache (≤4096 entries), descriptors and groups (`layerfs-storage/src/read/fetch.rs:19-29`). The decode state is rebuilt per demand: `handle.reader()?` at `store/ports.rs:88,102` creates a new `ReadState` (`read/provider.rs:14-18`, `read/objects.rs:26-34`).
- **Cache.** Capacity `cache_bytes`, key `ObjectId` only, entry `(Vec<u8>, age)`, exact LRU via a second `BTreeMap<(epoch, id)>` (`cache.rs:45-51`). No Workspace, role or authority partition (`cache.rs:13-14` leaves that to the caller).
- **Hit.** Two BTree mutations plus `value.clone()` under the mutex (`cache.rs:74-78`).
- **Insert.** Evicted `Vec`s are dropped and `value.to_vec()` is allocated under the mutex (`cache.rs:90-102`). One lock hold covers every insert of a batch (`client.rs:151-165`), so up to 32 MiB of memcpy.
- **Overlay.** Owner thread, `Shared` queue with fixed lanes, and one connection are daemon-lifetime (`owner.rs:76-81,128-151`). The overlay DB is created at owner start (`owner.rs:138`).
- **Registry.** `Mutex<BTreeMap<WorkspaceId, Entry>>` (`registry.rs:25-30`); the entry is removed on successful unmount (`operations.rs:211-215`).
- **Dropped per cycle.** `BoundWorkspace`, `BaseView`, `BranchSnapshot`, and unused serials — a 1024-serial refill is consumed and never recycled (`workspace/serials.rs:8,40-52`).

**Bound token**
- Shape: `{workspace: WorkspaceId, namespace: i64}` (`layerfs-bridge/src/control_types.rs:10-15`).
- Meaning: Store binding plus overlay namespace exist; explicitly not kernel readiness (`control_types.rs:133-142`).
- States: `Entry::Binding` placeholder → `Entry::Bound{activity, epoch, published}` (`registry.rs:13-23`). Activity is `Idle | Committing | Closing | Uncertain | LocalFailure`.
- Transitions:
  - `admit` requires Idle and bumps epoch (`registry.rs:57-87`).
  - Commit ends Idle, LocalFailure or Uncertain (`operations.rs:144-159`).
  - Unmount removes the entry, or returns to Idle/Uncertain (`operations.rs:185-215`).
- Staleness: absent → Missing; namespace mismatch → Invalid (`registry.rs:89-131`). `ns` is AUTOINCREMENT and `incarnation` is UNIQUE (`layerfs-overlay/sql/schema.sql:3-4`). Epoch is diagnostic only.
- `Service::operation(token)` refuses only Closing (`registry.rs:43-56`) and counts nothing. The registry does not know about in-flight operations, opens or Execs.

**What Ready needs, extending `Binding` rather than adding a second registry**
- A native state field (Unattached / Attaching / Ready / Detaching / Detached / Retained).
- Exec and open/request gauges kept separate from `Activity`, because Exec is allowed concurrently with Commit.
- A wire reply distinct from `Reply::Bound`.
- Unmount admission that reads those gauges under the same short lock.

### 2. Root bind cost

`Store::bind` (`store/bind.rs:19-80`) does three things:
- **One history read on the writer session:** `branch_snapshot` (`bind.rs:27-31`).
- **`checked_base`** (`bind.rs:82-117`): root object, inode-table path to the root serial, root directory page decode, portable-metadata paths. These are sequential one-ID batches; depth grows O(log N).
- **One overlay `Open` job:** one `INSERT … RETURNING ns` in `BEGIN IMMEDIATE` (`lifetime/workspace.rs:18-31`).

No row per base inode, no scan. A warm cache avoids every object batch.

Still paid when warm:
- the history snapshot;
- roughly 10–12 clone-under-mutex hits and their decodes;
- per-transaction overhead of the Open job: a freelist pragma and three `allocation.state()` calls, each an fstat plus lstat (`database/connection.rs:166-172,173,199,232`, `database/allocation.rs:64-69,120`);
- two registry lock sections.

`BaseView::reader()` re-reads and decodes the root object on every base operation (`base/view.rs:76-78`, `layerfs-content/src/filesystem/read.rs:82-93`); that is a per-request cost, not a bind cost.

### 3. Copies and locks on the base path

- **Hit:** one copy (clone) under the cache mutex (`cache.rs:78`).
- **Miss:**
  - Storage decodes; the `Vec` is moved to the caller (`client.rs:119-130`).
  - The client re-hashes it (`client.rs:126`), a second authentication pass after Storage's own.
  - One copy into the cache under the mutex (`cache.rs:102`).
  - Nothing is locked during provider I/O (`client.rs:109`).
- **Downstream of the cache:**
  - `FileView::open` holds the canonical `Vec` (`layerfs-content/src/file/view.rs:34-42`).
  - `read_range` copies into the sink (`file/view.rs:104-110`).
  - With an overlay span there are two more: `inherited` → `local.data` → sink (`operations/file/read.rs:118-128`).
  - The `LocalRead` is also cloned out of the `Completion` (`overlay/read_port.rs:101`, `file_port.rs:21,55`).

**Internal `Arc` entry is compatible.** `Cache` and `State` are `pub(crate)`, and the only callers are `client.rs:96,159`. The public surface (`CanonicalCache::{new,diagnostics}`, `AuthenticatedObjects → Vec<Vec<u8>>`) is unchanged. It does not reduce copy count (one per hit, one per miss) because the cluster-one trait returns owned `Vec` (`layerfs-content/src/object/access.rs:35`). It moves allocation, memcpy and free out of the critical section.

Semantics the spec needs:
- **Hit:** in one critical section, bump refcount, touch recency, count the hit. Copy outside.
- **Insert:** build the `Arc` outside. In one critical section, re-check `contains`, evict into a local list, insert, update `charged` and `evictions`. Drop the evicted list outside.
- **Failure paths:** oversized or duplicate is a no-op and never an error; lock poison is the only refusal. Today a poisoned lock at the final insert fails an already authenticated demand (`client.rs:151-157`).
- **Counters to pin:** `cache_misses` and `upstream_batches` count attempts before the provider call (`client.rs:102-107`). `authenticated_bytes` is added even when the insert is bypassed or duplicate (`client.rs:159-164`).

### 4. Concurrent identical misses

- There is no coalescing. Each caller's miss subset goes as one batch to its own source (`client.rs:110-112`).
- Two callers missing the same ID both acquire. The second insert is a no-op (`cache.rs:86`).
- Batches are formed only from one caller's `ids` (≤4096 IDs, ≤32 MiB; `client.rs:6-7`).
- Storage reads are all-or-nothing per batch: the first missing ID fails the whole call (`layerfs-storage/src/read/objects.rs:55-62`).

Constraints the design must respect, derived from source:
- **Placement.** Flights must live at `Store` level (`store/open.rs`), below `StorePorts`. Exact provenance is `Arc<PortError>` and exists only there (`store/ports.rs:36-40,65-78`). The workspace-level client sees only the lossy `ContentError` mapping (`ports.rs:105-111`).
- **Custody.** Each subscriber records the same `Arc<PortError>` into its own `StorePorts` custody.
- **Key.** `ObjectId` within the one validated `Store` (policy checked at `open.rs:51-55`).
- **Flight shape.** A caller's batch splits into IDs already in flight (subscribe) and the remainder (one new batch flight). No per-ID Store calls, no timer.
- **Bounds.** Flights ≤ `read_handles`, waiters per flight ≤ W. On overflow, fall through to an uncoalesced demand, still bounded by the reader mutexes.
- **Transient bytes.** Worst case is flights × 32 MiB (`READ_CANONICAL_BYTES_LIMIT`, `layerfs-storage/src/store/policy.rs:83`). The adapter cannot see lengths before the call, so the spec must state this arithmetic rather than claim a tighter bound.
- **Cancellation.** A waiter drops its subscription only. The flight result is owned by the Store, not by the leader's operation, so neither the leader's unmount nor a waiter's cancellation invalidates it.
- **Failure.** A failed flight delivers its exact error to all current subscribers and is removed. Nothing is cached, and there is no re-acquisition on their behalf.
- **Fate-sharing.** Because Storage is all-or-nothing, a subscriber for ID x fails if the leader's batch contained a missing or corrupt y. This is unavoidable without a cluster-one API change; see (g).

### 5. Memory domains

| Domain | Bound today | Accounted by |
|---|---|---|
| Retained cache | Σ(len+256) ≤ 8 MiB, logical; BTree nodes not measured | `cache.rs:83,95,101`; `ClientWork.charged_cache_bytes` |
| Hit and result transient | ≤32 MiB per caller batch × caller threads; unbounded by design | nobody (`docs/architecture/48-shared-cache-operation-scopes.md:112-119`) |
| Acquisition/decode transient | Per concurrent demand, at most 4 concurrent: pack cache 2 MiB (singleton up to ~16 MiB), group cache 512 KiB, lazy decode arena 1 MiB, output ≤32 MiB | Storage constants `policy.rs:117,141`, `encoding/codec.rs:57`; no daemon gauge |
| Output copies | ≤128 KiB window × 2–3 per read (`READ_WINDOW`, `layerfs-overlay/src/contract/types.rs:14`) | none |
| Borrowed after eviction | Does not exist today (hits are owned clones). With `Arc` entries it becomes a new unaccounted domain | needs a "lent bytes" gauge |
| Overlay SQLite | Pager suggestion 2 MiB, MEMORY journal per transaction, `temp_store=FILE` (`database/profile.rs:19,58-60`) | pager hint only |
| Store SQLite | 5 sessions × 2 MiB suggestion, `temp_store=2` (memory) (`layerfs-persistence/src/backend/sqlite/profile.rs:73-75`) | hint only |
| Owner job credits | 8 MiB total, 64 KiB lifecycle reserve (`owner.rs:25-35`) | `OwnerWork.credited_bytes` |
| Per-mount FUSE buffers | none in source | S8 |

Oversized valid objects: those with `len+256 > capacity` bypass retention and are still returned (`cache.rs:86-88`). The 16 MiB check at `client.rs:120-125` is the canonical format limit (`policy.rs:38`), not a file cap. Cache capacity is therefore not acting as an object or file size cap.

### 6. Overlay scheduler

- **Selection.** `poll` rotates namespaces, one job per namespace per turn (`queue.rs:266-280`). Within a lane it rotates six class queues with capture/mutation/source ordering fences (`queue.rs:106-156`).
- **Admission.** Fails immediately with `AdmissionFull` and returns the command (`owner.rs:244-270`). Default limits: 16 ordinary + 2 lifecycle outstanding per namespace, 8 MiB credits daemon-wide. A slot is released only when the `Completion` is dropped (`credits.rs:48-59`).
- **Job bounds.** 128 KiB read/write windows; 64-row pages (`contract/types.rs:8-14`).
- **Fairness.** Round-robin by job count, with no byte or service-time deficit. A 128 KiB `SourceRead` (32 cells) and a point inode read weigh the same. `303/daemon-sqlite.md:288-293` itself says round-robin alone cannot prove fairness.
- **Head-of-line.** One owner thread; every job and every in-line maintenance step (`owner.rs:400-416`) delays all Workspaces by its service time.
- **Shared lanes.**
  - `Open` uses the namespace-0 lane (`owner.rs:233`), so at most two un-dropped Open completions exist daemon-wide.
  - `AcquireLookup`, `OpenFile`, `CloseFile`, `ReleaseBaseSource`, `ReplyAttempted`, `State` and `Close` all share the two lifecycle slots per Workspace (`commands.rs:249-287`).

**Synchronous blocking that would pin a FUSE dispatch thread**
- `Pending::wait` parks the thread (`service/completion.rs:123-137`). It is called from every port method:
  - `overlay/read_port.rs:25`
  - `file_port.rs:51-53`
  - `captured_run_port.rs:23-25,43-45`
  - `operation_record_port.rs:29-31`
  - `store/bind.rs:57`
  - `store/settle.rs:14`
  - `control/operations.rs:237`
- The reader mutex (`store/open.rs:98`).
- The cache mutex.
- The `StorePorts::attempt` mutex, held across provider I/O (`store/ports.rs:66-77`).

**Existing hook.** `Pending::try_complete` is a non-blocking take (`completion.rs:138-140`). The only wake is `Thread::unpark` of a registered `OnceLock<Thread>` (`completion.rs:39,82-88`). There is no callback or queue notification, and no external wake when credits free up: `credits.rs:58` notifies only the owner's condvar.

**Parking point.** After `try_submit` returns `Pending`, a receive loop could hold the owned reply and the `Pending` with no Workspace, cache or SQL lock held. That needs a new completion notifier in `completion.rs` (generalise `waiter`) and a credit-available notification in `credits.rs`.

**Round-trip count.** `SourceView::lookup` and `stat` issue up to five independent owner jobs for one inherited name (`workspace/view.rs:121-139,78-91`), plus source acquire/release and an optional lookup owner. See contradiction C3 in §10.

### 7. Cold base-read admission

None exists. Any thread that misses goes straight to `Store::read` (`open.rs:93-102`), is assigned a reader by a blind counter, and blocks on that reader's unfair mutex for the whole demand including decode.

Consequences:
- **Starvation.** A scanning Workspace with as many threads as readers can hold all of them. A warm `stat` of a base regular file from another Workspace also needs a reader, because `file_length` is not cached above Storage and goes to `Store::read` on every call (`base/view.rs:139`, `store/ports.rs:80-91`). It queues behind cold decodes.
- **Eviction.** The cache is one global LRU with no role or Workspace segmentation, so an 8 MiB payload scan evicts every tree and directory page.

A fair bounded admission could reuse existing structures:
- per-Workspace FIFO of cold demands keyed by `Route.namespace()`;
- a round-robin cursor, the same pattern as `queue.rs` rotation;
- concurrency equal to `read_handles`;
- pick an idle reader instead of the blind counter;
- route warm length demands separately from payload demands.

### 8. Lifecycle and terminal cleanup

**Ownership records** (`sql/schema.sql`)
- Owner tables, each a row per request with a paired `lease` row: `lookup_owner`, `file_handle`, `file_read`, `captured_reader`, `operation_owner`.
- `file_custody` holds per-serial counters; `base_source` pairs with `workspace.base_readers`.
- `request` holds reply-attempt tickets; `orphan` and `orphan_wait` hold unlinked-file state.
- Generation and frontier live on the `workspace` row (`active`, `captured`, `installed`, `revision`).

**What terminal close does today**
- `close` sets `lifecycle=1` (`lifetime/close.rs:16-27`).
- `queue_closed` enqueues terminal reclaim only if there is no capture, `base_readers=0`, no `lease` row and no `request` row (`close.rs:68-83`).
- Close does not refuse on live handles. The namespace becomes `Held` (`close.rs:49-67`).
- The control unmount still returns `Unmounted` and drops the registry entry (`operations.rs:205-215`).

**Reclamation**
- Automatic and bounded. Closed-namespace steps delete 14 payload rows or 64 metadata rows per step across 12 phases, then the workspace row (`maintenance/reclaim.rs:7-9,51-83`).
- Live maintenance uses a ready index (`maintenance/ready.rs:145-202`).
- One step runs per 8 foreground jobs, or when idle (`owner.rs:400`).

**Debt that can remain**
- **Held forever.** Any un-released `lease`, `request` or `base_source` row keeps the namespace Held. No reclaim phase deletes those tables (`reclaim.rs:51-63`). There is no enumeration by namespace for lookup, open or source owners (only by request id, e.g. `lookup.rs:81-101`), and by design no sweep. Only `pending_publications` pages tickets (`lifetime/frontier.rs:38-57`).
- **Maintenance stops permanently.** After the first maintenance error, reclamation halts for every Workspace until restart (`owner.rs:395,400,412-415`).
- **Admission is not coupled to debt.** Registry capacity counts only bound entries (`operations.rs:65`).

**Per-FORGET collection.** None exists. Lookup ownership is per-request rows, costing about 5 statements and one write transaction per acquire (`lifetime/lookup.rs:23-56`, `file_owners.rs:8-29`). There is no counted `FORGET(n)` primitive.

**What S8 must add**
- Request, open, lookup, session and process gauges on the mount session.
- Unmount Busy before effect.
- A cancelled/parked-reply fence.
- A detached state after kernel detach and worker join.
- Bounded bulk owner retirement for a closed-and-detached namespace: new reclaim phases over `lease`, `request`, `base_source` and the owner tables.
- Cleanup-pending reporting, reusing `CleanupState` and `Resources`.
- An unknown state that retains custody.

### 9. Stable identity

- **Inode number.** It is the canonical inode serial from the committed inode table. `BaseStat.serial` (`base/view.rs:22-27`) is stable across fresh mounts of the same root.
- **Across install.** Commit refuses a candidate that changes the root inode identity (`store/commit.rs:111-115`).
- **New inodes.** Serials come from the History allocator per scope and are never recycled (`store/ports.rs:129-150`; `layerfs-persistence/src/history/allocation.rs:30-83`). Whether the committed tree keeps them is S10 work (unimplemented) and not verified.
- **Stored metadata.** kind, mode, mtime seconds/nanoseconds, nlink, size, plus internal fields (`schema.sql:20-36`). Mode is limited to ≤0777 for files, ≤01777 for directories, exactly 0777 for symlinks (`schema.sql:25`, `operations/attributes.rs:26-35`).
- **Synthesized by S8.** uid, gid, atime, blocks, rdev, FUSE generation. ctime is reported as mtime (`operations/types.rs:40-41`).
- **Link counts.** `namespace_refs` is the name count for files, 0 for the root, and explicitly not POSIX directory nlink (`workspace/view.rs:14,29,48`; `mutation/eval.rs:39-43`). Directory `st_nlink` must be synthesized.
- **File kinds.** Only file, directory and symlink exist (`schema.sql:24`).

### 10. Reuse, and contradictions with #303

**Reuse rather than re-propose**
- `FileLengths` port — length without payload (`ports/lengths.rs`, `layerfs-storage/src/read/length.rs`).
- `BaseView::plan_read` / `BaseRead::emit` — a 128 KiB window streamed to a sink (`base/view.rs:151-183`).
- `FileRead` — read custody that survives descriptor close (`lifetime/file_owners.rs:220-288`).
- Captured-run ports.
- `ReplyAttempted` tickets and `PendingPublications`.
- The `Retained*` observers for lost replies.
- `try_submit`, which returns the original command on refusal.
- `Pending::try_complete`.
- The `NamespaceJob` Needs/facts rounds (`mutation/driver.rs:43-63`).
- Base-source fencing of install (`queue.rs:116-131`).
- `CleanupState`, `MaintenanceIdle`, `Resources`.
- The `ClientWork`, `StoreWork`, `OwnerWork` and `JobWork` receipts.

**Contradictions**
- **C1 — write-handle contention.** `06-cluster-one-integration.md:162-167` and `SERVERLESS-STORE-PLAN-20261007.md:148,250-251` say each handle, including the write handle, sits behind its own mutex ("mutual exclusion, not a retry"). Source has no daemon mutex on the writer (`store/open.rs:33-34`). The persistence session uses `try_lock` and returns Busy (`backend/sqlite/transaction.rs:136-140,183-187`). So in-process contention between a Commit publication wave, a serial refill and a mount `branch_snapshot` yields Busy. K30 (`08-decisions-provenance.md:117`) says no writer gate. The read-only `HistoryProvider` of each reader is opened and then discarded (`bootstrap.rs:74-76`).
- **C2 — unmount Busy.** `unmount.md:92` requires Busy when Execs or handles are active; source arbitrates only Commit (finding 8).
- **C3 — consistent read job.** `daemon-sqlite.md:273-274` requires compound reads forming one reply to be one consistent owner job. `SourceView::lookup`/`stat` combine up to five independent jobs with a recheck (`workspace/view.rs:77-140`).
- **C4 — parking.** `fuse.md:66` and `daemon-sqlite.md:289-290` require parked waiters that do not hold workers. Every port blocks (finding 6).
- **C5 — debt admission.** `unmount.md:282` (U9) and `mount.md:252` (M7) require bounded aggregate debt and admission; none exists (finding 8).

### 11. Optional refinements

| Refinement | Evidence | Cost / interference |
|---|---|---|
| Role-segmented allowance (navigation vs payload) | Structural only (finding 7); no receipt | Needs object role at insert; splits 8 MiB; mis-sizing hurts sequential reads |
| Lock sharding | None. Arc-izing the cache already removes memcpy and free from the lock | Breaks exact global LRU and the single `charged` counter |
| File-length fact cache keyed by content root | Structural: every warm `stat` reaches a reader | Small fixed entries; immutable, so no invalidation |
| Decoded directory/inode page cache | None; R4 notes only the per-operation 64-page Content cache (`R4-UPSTREAM-CACHE-20261007.md:37-40`) | Duplicates bytes already in the canonical cache; a second memory domain |
| Negative entries | None; `fuse.md:155` lists them as "investigate" | Base absence is immutable per root, but overlay creates need a coherence proof |

None of these should enter S8 without the count proofs in (f).

## (d) Hazards with counterexample interleavings

- **H1 — mount refused by own-daemon Commit.** A's Commit thread is inside a publication batch on the writer session. B's mount calls `branch_snapshot`, `try_lock` fails, and the mount is refused Busy with no cross-daemon contention. The same happens for B's first `create`: serial refill → Busy → the create fails, against `fuse.md:66`.
- **H2 — one failure poisons a shared operation.** If S8 shares one `StoreOperation` per mount, request R1's miss holds the `attempt` mutex across I/O, so R2's miss waits (`ports.rs:66-77`). If R1 fails, every later demand on that mount returns R1's failure forever (`ports.rs:70-72`).
- **H3 — dead reader stays in rotation.** An I/O error quarantines one reader session (`transaction.rs:141-143,165-168`). `Store::read` keeps selecting it, so one in four cold demands and length reads fail until restart.
- **H4 — unmount with kernel references.** The kernel holds K lookup references at detach and FORGET is not guaranteed. Either S8 issues K `ReleaseLookup` write transactions through two lifecycle slots on the shared owner, or the namespace stays Held with payload never reclaimed.
- **H5 — debt outruns reclaim.** A writes continuously, 32 cells per job. B cycles mount → write → unmount. Reclaim deletes at most 14 cells per 8 foreground jobs, so closed namespaces accumulate. `max_pages` is `None` (`profile.rs:20`), so the bound is disk. Nothing refuses new mounts.
- **H6 — cache flush by a scan.** B runs `cat` over 16 MiB of cold data and evicts A's inode-table and directory pages. A's next lookup is a 10–12 batch cold walk competing for the readers B holds.
- **H7 — duplicate cold acquisition.** Four threads miss the same chunk and occupy all four readers with identical decodes. A fifth Workspace's `stat` waits behind all of them.
- **H8 — third concurrent mount refused.** Two Open completions are un-dropped (namespace-0 lane); a third mount gets `AdmissionFull` → Capacity instead of queueing.
- **H9 — placeholder leak.** An uncertain Open leaves `Entry::Binding` in place forever (`operations.rs:81-87`). It consumes a capacity slot and cannot be observed, because `bound()` returns Busy for it.
- **H10 — refetch loop on window overflow.** A mixed hit/miss batch that exceeds 32 MiB errors before insert (`client.rs:142-148`), so the fetched bytes are discarded and identical demands refetch every time.

## (e) Recommended specification decisions

| # | Decision | Status | Owning files | Kind |
|---|---|---|---|---|
| D1 | Each FUSE request gets its own `StorePorts` failure scope, built from a session-held `Arc<BoundWorkspace>`. The registry mutex and snapshot clone (`operation.rs:27`) stay off the request path. | mandatory | `daemon/store/operation.rs` | change |
| D2 | Extend `Binding` with a native state and Exec/open/request gauges. Add a Ready reply. Unmount admission checks the gauges and returns Busy before effect. | mandatory | `daemon/control/{registry,operations,status}.rs`, `bridge/control_types.rs` | change |
| D3 | Completion notifier (queue or callback wake) and credit-available notification, so replies park without a thread. Callers copy out and drop the `Completion` before replying. | mandatory | `daemon/service/completion.rs`, `overlay/credits.rs` | change |
| D4 | One compound owner job per lookup/getattr/readdir reply, reusing the Needs/facts round pattern. | mandatory | `workspace/workspace/view.rs`, `daemon/overlay/commands.rs` | change |
| D5 | Lookup ownership model: counted acquire/release keyed by serial (`file_custody.lookups` already exists), plus bounded bulk terminal retirement of `lease`, `request`, `base_source` and owner rows for a closed-and-detached namespace. | mandatory | `overlay/lifetime/{lookup,close}.rs`, `overlay/maintenance/reclaim.rs`, `sql/schema.sql` | new + change |
| D6 | Cache entries become `Arc`; clone, alloc and free move outside the mutex; counters atomic with the map change; add a lent-bytes gauge. | mandatory | `workspace/base/{cache,client}.rs` | change |
| D7 | Store-level single-flight under the finding-4 constraints. | mandatory if S8 uses concurrent dispatch | `daemon/store/{open,ports}.rs` | new |
| D8 | Reader selection picks an idle, healthy reader. A quarantined reader is excluded and reported in status; it is not retried. | mandatory | `daemon/store/open.rs` | change |
| D9 | Fair cold-batch admission per Workspace. | mandatory, to be confirmed by the O5 count proof | `daemon/store/open.rs` | new |
| D10 | Mount snapshot via a reader `HistoryProvider`. | pending owner decision 1 | `daemon/bootstrap.rs`, `store/{open,bind}.rs` | change |
| D11 | Status surfaces `maintenance_failure()` and closed-namespace debt; mount admission refuses on declared debt/headroom. | mandatory | `daemon/control/{status,operations}.rs` | change |
| D12 | Size lifecycle slots and the namespace-0 lane for FUSE concurrency, or define park-on-`AdmissionFull` through D3. | mandatory | `daemon/overlay/owner.rs` (config) | change |
| D13 | Byte- or service-weighted (deficit) lane selection. | optional pending O8 | `daemon/overlay/queue.rs` | change |
| D14 | Length-fact cache; role-segmented allowance. | optional pending O5/O9 | `workspace/base/*`, `daemon/store/*` | new |
| D15 | Directory `st_nlink`, uid/gid, ctime = mtime and generation synthesis rules; explicit refusal for unsupported kinds. | mandatory | S8 adapter | new |

## (f) Proof obligations (falsifiable)

- **O1.** Warm fresh mount: `StoreWork.object_batches` delta = 0, history read transactions = 1, owner jobs = 1 (Open). Fails if any is larger.
- **O2.** Warm LOOKUP of an unmodified base name: owner jobs ≤ 2, write transactions ≤ (count the spec declares), `StoreWork.length_batches` ≤ 1.
- **O3.** N threads missing one ID: `object_batches` delta = 1 and `object_ids` delta = 1. A waiter cancelled mid-flight changes neither the count nor the other waiters' results.
- **O4.** A failed flight: every subscriber's `ports.failure()` is pointer-equal to the same `Arc<PortError>`; no second Store call; nothing inserted.
- **O5.** After Workspace B reads 16 MiB cold, Workspace A's previously warm `stat` path has `object_batches` delta = 0 (with segmentation) or = D (declared and measured). A's reader-wait count stays ≤ the declared bound while B scans with 4 threads.
- **O6.** Maximum bytes memcpy'd under the cache mutex per hit and per insert = 0; maximum frees under the mutex = 0 (instrumented counters).
- **O7.** Unmount with K outstanding kernel lookups and no FORGET: foreground owner jobs ≤ constant; `CleanupState` reaches `Gone`; reclaim rows processed ≥ K.
- **O8.** A issues 128 KiB reads continuously and B issues point reads. B's completed jobs per A job ≥ the declared ratio over an unbounded arrival window (F10 covers only finite arrivals).
- **O9.** A repeated mount/write/unmount cycle while a peer writes continuously: closed-namespace count and database pages plateau, or mount is refused with a typed resource result. Never unbounded growth.
- **O10.** No dispatch thread is parked in `Pending::wait` while the owner queue is non-empty (thread-state count = 0).
- **O11.** A quarantined reader receives 0 demands after the first Unknown.
- **O12.** With a serial refill, a mount and a Commit publication overlapping in one daemon: outcomes match owner decision 1 exactly (Busy count = 0, or typed Busy with the declared errno and no local change).

## (g) Owner decisions

1. **In-process write-handle contention (C1/H1).**
   - Option A, K30 as implemented: Busy reaches mount and, as an errno, Bash `create`.
   - Option B, the 06 §4 text: a daemon-local mutex on the write handle.
   - These contradict each other and `fuse.md:66`. The decision should also say whether mount's `branch_snapshot` may use a reader session.
2. **Single-flight fate-sharing.** With all-or-nothing Storage batches, one Workspace's demand composition can fail another's subscribed demand. Either accept that with exact provenance, or disallow cross-Workspace subscription to multi-ID flights.
3. **Lookup ownership model (D5).** Either an SQL write transaction per kernel lookup reference, or in-memory counts with SQL custody only for unlinked or held inodes. Also whether bulk terminal retirement after native detach is an authorised schema/maintenance change.
4. **Normal unmount with live kernel handles or Execs.** Busy (per `unmount.md:92`), or logical close leaving a Held namespace as the engine does today.
5. **Maintenance failure policy.** Today the first error stops all reclamation for the daemon. Decide whether that is fail-stop for new mounts or continue with reported debt.
6. **Directory `st_nlink` and unsupported file kinds** (FIFO, socket, device; setuid/setgid/sticky on files). Synthesized values and refusal errnos need a ruling because they are visible to Bash.
