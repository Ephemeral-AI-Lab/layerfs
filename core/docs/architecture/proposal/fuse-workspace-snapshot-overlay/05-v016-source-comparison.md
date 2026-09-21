# v0.1.6 source audit and Pair 1 comparison

> **Status: Research; informative and not a product contract.**
> Audited 2026-09-21. Reference source is the exact v0.1.6
> release `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; the implemented shared
> foundation is reviewed main `152b9c3a2e8ec2536a1d63601b681e1f7ef34455`.
> Pair 1 remains proposed. This document supplies source findings, not a new
> measurement, mounted qualification, performance win or durability guarantee.

The replacement should preserve the release's local range edits, disk-backed
payload ownership, coherent kernel-visible edits and live-successor snapshot
semantics. Its strongest concrete opportunities are to reuse the existing
logical service boundary, eliminate repeated whole-frontier allocation, and
bound aggregate resident metadata and retained backing. Merely using disk,
adding COW, avoiding whole-file copy-up or allowing writes during a save does
not establish an improvement: the exact release already has those mechanisms.

The design and delivery owners remain [01](01-workspace-fuse-contract.md),
[02](02-overlay-snapshot.md), [03](03-commit-integration.md) and
[04](04-implementation-and-verification.md). The [packet index](README.md)
records the current decisions and open rulings. This audit does not change
their resource policies or invent a second implementation plan.

## 1. Audit scope and corrections to the earlier mechanism sketch

The established
[mechanism-from-source research](https://github.com/Ephemeral-AI-Lab/layerfs/blob/ae1d514d2/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-205-save-split-20260920T081022Z/fuse-mechanism-from-source.md)
was read first and is retained unchanged. It described a different source
checkpoint. The following exact-release findings must govern this comparison.

| Earlier shorthand | Exact v0.1.6 source finding | Consequence |
| --- | --- | --- |
| Files above 8 KiB are uncached at every layer | The daemon has a shared **32 MiB immutable range/name/page cache**. Demanded content ranges enter it without a file-size threshold. FUSE opens use kernel `KEEP_CACHE`. | Above 8 KiB bypasses optional whole-file prefetch, not all caches. Repeated `execve` is still useful, but is not a guaranteed uncached experiment. |
| Pending payload uses a host spool | The normal container live route writes replacement bytes in the **execution-side daemon's** `/snapshots/<workspace-id>/` backing. Host construction pulls frozen ranges. | Disk-backed pending payload is already present in the reference topology. |
| Snapshot capture is a fixed-cost pointer operation | `capture_frontier` takes the dirty set, clones its IDs and expands changed-directory child IDs. | No payload copy or I/O is performed there, but the source does not establish O(1) capture. |
| Bounded snapshot pages bound snapshot memory/work | Each page first allocates all frontier IDs and scans past prior IDs; the host collects all decoded records before construction. | A bounded page or transfer window is not an aggregate metadata bound or bounded per-page traversal. |
| One global construction gate means one construction producer | The process-wide gate serializes candidate builds, but the worker selector defaults to available parallelism capped at eight; a separate small-content worker cap is four. | The standing single-producer rule must be enforced in the replacement and in later matched arms; do not infer it from the gate alone. |
| Four spool segments establish a resident-memory ceiling | The window repeatedly issues advisory `POSIX_FADV_DONTNEED`; dirty, mapped and otherwise retained pages are not thereby proven absent. | Observe process and kernel/container memory separately. A successful append case does not qualify dense overwrite/open/read behavior. |
| Constructed core content makes later reads effectively free | Current C1 still acquires authenticated canonical objects and reconstructs requested data. A WholeFile representation acquires its canonical object before slicing the requested range. | Neither an eager/lazy crossover nor a repeated-read win follows from source alone. |

Sources: [immutable cache][v-cache], [cached opens][v-open],
[daemon mount/backing][v-mount], [capture][v-frozen], [snapshot pages][v-snapshot-pages],
[host frozen input][v-input], [worker selection][v-workers],
[spool hints][v-spool-hints] and [current C1 reads][c-read].

This is a complete module and call-path audit of the release's FUSE crate and
the relevant daemon, SDK, Workspace, snapshot and host-construction seams. It
is not a formal correctness proof of every branch of the entire release, an
audit of third-party implementations, or a new execution of legacy tests.
Source tests are evidence of intended scenarios, not proof that they pass on
the present machine. Historical receipts are owned by the
[benchmark qualification map](06-benchmark-qualification-map.md) and are not
recomputed or promoted here.

### 1.1 Reproducible source navigation and size

The table counts physical lines from immutable Git blobs, including comments,
blank lines and inline tests. These are **not production LOC**, performance
numbers or estimates of code that can be deleted. Method:

```text
git ls-tree -r --name-only 44cf748486863ab7c21ca47e731bd88e2b9a7b4a \
  crates/layerfs-fuse/src
For each returned file:
  git show 44cf748486863ab7c21ca47e731bd88e2b9a7b4a:<path>
  count the resulting byte string's splitlines()
```

All paths below are under the pinned release's
[`crates/layerfs-fuse/src/`][v-fuse-tree].

| File | Physical lines | Audited responsibility | Pair 1 treatment |
| --- | ---: | --- | --- |
| `adapter.rs` | 162 | Dispatch, inode/handle resolution, attributes and errno | Preserve semantics in thin Linux adapter; avoid copying the broad port abstraction without need |
| `bin/layerfs-fuse.rs` | 174 | Standalone mount CLI, connection and backing lifecycle | Daemon owns new mount lifecycle; standalone route is optional scope |
| `filesystem.rs` | 1,438 | Kernel callbacks and capability negotiation | Required behavior retained or explicitly classified by 01 |
| `handles.rs` | 43 | File-handle table | Preserve stable inode lifetime; do not bind a normal handle permanently to its open-time contents |
| `host_mount.rs` | 112 | Fuser session creation, mount verification/unmount | Linux adapter responsibility; fail unsupported capabilities explicitly |
| `immutable_read_cache.rs` | 322 | Shared authenticated-scope range/name/page FIFO | Initial new profile adds no equivalent content cache; this is a performance-policy reduction to test |
| `inode_table.rs` | 18 | Kernel inode mapping | Preserve non-reuse and lifetime semantics |
| `lib.rs` | 58 | Module/export/feature wiring | Use replacement product boundaries and declaration-only module files |
| `live_owner.rs` | 4,279 | Mutable state, kernel coherence, reads/writes, SDK edits, snapshot/control service | Split ownership between Workspace and projection; do not discard the algorithms hidden in this large file |
| `live_runtime.rs` | 529 | Shared workers, admission classes, physical/kernel jobs and cuts | Reuse scheduling principles; the exact replacement scheduling/capacity design must prove progress |
| `live_transport.rs` | 870 | Live facts/control/snapshot sockets and framing | Reuse Pair 3 transport primitives instead of porting a parallel protocol; control semantics still need definition |
| `live_wire.rs` | 896 | Legacy facts, node/piece and control wire values | Replace private fact/backing IDs with agreed public logical inputs; no canonical-object RPC |
| `local_spool.rs` | 509 | Packed execution-side payload backing, range ownership and hints | Preserve range immutability and write-before-publication; improve explicit accounting and bounded metadata |
| `port.rs` | 482 | FilesystemPort interface, futures, replies and telemetry seams | Keep only the local adapter/Workspace surface actually needed |
| `protocol.rs` | 1,357 | Separate older filesystem-proxy request/response codec | Do not carry it into Pair 1 as a second remote content/storage implementation |
| `proxy_client.rs` | 2,118 | Older syscall proxy, queued creates/unlinks, read-ahead/write coalescing, ordering | Remote syscall proxy is unnecessary at the new boundary; any omitted behavior is still a compatibility decision |
| `proxy_host.rs` | 433 | Older proxy listener, capability check, deferred errors and control | Pair 3 supplies shared transport/authentication mechanisms; daemon control is a new agreed surface |
| `write_metrics.rs` | 869 | Atomic operation, copy, frame, cache and phase counters | Preserve observations needed for qualification, without importing every legacy diagnostic field |
| **Total** | **14,669** | FUSE crate only | Excludes Workspace/core algorithms, SDK, daemon and Store construction |

The five transport/codec files `live_transport`, `live_wire`, `protocol`,
`proxy_client` and `proxy_host` total 5,674 physical lines. That is a navigation
subtotal, not a saving: Pair 3 code already exists elsewhere, and some of these
files contain state, ordering and error behavior the new daemon still needs.
Relocation, shared implementation reuse and compatibility reduction must be
reported separately from algorithmic simplification.

## 2. Exact release topology and control paths

### 2.1 The principal container route

```text
 HOST: SDK + Workspace manager + Store       CONTAINER: daemon + caller kernel

 SDK.create_workspace_session
   | -> Workspace shell / Branch context
   | -> RemoteWorkspace / BackingOwner
   |       immutable-base service listener <----- d: demanded facts/read ranges
   |                                      <----- c: connected control channel
   |                                      <----- o: connected observation channel
   |                                      <----- s: connected snapshot channel
   |
   `-- daemon Mount control request -----------> mount registry / owner check
                                                |
                                      LiveOwner::connect (SEED)
                                                |
                                      mount_host / one FUSE session
                                                |
                                  +-------------+------------------+
                                  |                                |
                               caller VFS                    LiveWorkspace
                                  |                                |
                            mounted request.root        private payload segments
                                                       /snapshots/<workspace-id>

 Commit control: host -> c -> capture G in LiveOwner
 Frozen input:  host -> s -> changed records and disk byte ranges
 Completion:   host -> c -> exact generation/version guarded reconciliation
 Ordinary I/O: kernel -> local LiveOwner; only missing base ranges/facts use d
```

The role letters label distinct sockets, not four separate implementations.
The execution-side owner opens them to the host listener. The mount lifecycle
protocol is separate from the live backing/control protocol: daemon protocol
`Mount`, `WorkspaceReady`, `Close` and `WorkspaceClosed` carry lifecycle state;
`EDIT_BEGIN/PART/END` and `CAPTURE/COMPLETE_*` travel on the live control channel.
[Mount startup][v-mount], [daemon protocol][v-daemon-protocol],
[RemoteWorkspace wiring][v-remote], [role handoff][v-roles].

The reference host-local FUSE route uses `RemoteWorkspace::start_local` and the
same live owner/handler without a TCP round trip. Its backing is under the host
Workspace spool's `live-backing` child. Therefore neither “all reference FUSE
calls cross a socket” nor “the reference reader and Store are always co-located”
is a valid description of every release route. [Local/direct placement][v-remote].

The container daemon rejects a pre-existing private backing directory during
mount startup. The standalone CLI and local-host helper have different
stale-directory removal behavior. The new design must own its directory and
cleanup policy explicitly rather than copying whichever startup helper is
shortest. Mount paths and private backing are different resources.

### 2.2 Proposed deployment and two distinct client surfaces

```text
 EXECUTION MACHINE                           SERVICE MACHINE

 proposed LAYERFS_WORKSPACE_ROOT
   +-- workspace/
   |     `-- <workspace-id>/  <--- Linux FUSE mount and ordinary applications
   `-- private-backing/
         `-- owned extents + indexed metadata; never mounted as the user tree

 host/remote SDK ---- agreed daemon control ----> layerfs-workspace
                                                  WorkspaceHost
                                                    |
 kernel --> layerfs-fuse ------------------------> Workspace
                                                    |
                         local state / G / D1 / disk ownership
                                                    |
                         existing bridge logical client ------> common handlers
                                                               C1 + C2 + C5
```

The owner-selected replacement crates are `core/crates/layerfs-fuse` and
`core/crates/layerfs-workspace`, assembled in the daemon process. Workspace
groups its runtime, filesystem, overlay, backing and Commit responsibilities;
this packaging change introduces no extra transport hop and is not a measured
code-size or performance improvement. The root reference crates retain their
historical source identity. [Proposed file map](04-implementation-and-verification.md#3-proposed-production-layout)

`LAYERFS_WORKSPACE_ROOT` and the new mount/Workspace control surface are
**proposed**, not existing release or Pair 3 environment variables. The
execution machine interprets this path in its own namespace. `LAYERFS_ENDPOINT`
is an existing Pair 3 connection setting; the container's localhost is not
implicitly its host. Existing `LAYERFS_STORE` and history catalog configuration
remain service-side. See [storage and configuration](01-workspace-fuse-contract.md).

The new local Rust methods do not by themselves implement host SDK access to
a container's mounted Workspace. Pair 1 needs an agreed, authenticated daemon
control contract for attach/mount/status, live range edits, Commit and exact
stage actions, unmount and clean close. Existing bridge framing/authentication
capabilities should be reused; the current service `EditFile` operation saves
an immutable content result and does not mutate the pending mounted Workspace.
The proposed [local production API and dependency direction](04-implementation-and-verification.md#41-local-production-api-and-dependency-direction)
names the WorkspaceHost/Workspace methods separately from that daemon endpoint.

### 2.3 SDK edits are not FUSE writes

```text
 sdk.edit_workspace_file_ranges([same Workspace, same path])
       |
       v
 Workspace manager: validate batch identity, select remote owner
       |  does not take the full-Commit lifecycle mutex on remote route
       v
 RemoteWorkspace::edit
       | EDIT_BEGIN(path, member count)
       | EDIT_PART(start, delete length, inline bytes or zero length) ...
       | EDIT_END
       v
 LiveOwner control transaction
       | local kernel-cache cut / metadata-only lookup / prepare all edits
       | apply consistent file state / reconcile cache visibility
       v
 existing mounted readers and later FUSE operations observe edited inode

 No requirement to write replacement bytes through the mounted pathname.
 Kernel coherence can still cause laundering/invalidation callbacks.
```

The release explicitly validates one Workspace and path for the batch, sends
the group on the control lane and prepares member edits before final apply.
It uses metadata-only lookup for this route, suppressing optional sibling and
content acquisition. Each inline edit has a 1 MiB limit; the underlying
Workspace inline budget and piece bounds remain separate. The ordinary
remote edit route does not acquire the lifecycle mutex held by Commit.
[SDK entry][v-sdk], [Workspace dispatch][v-sdk-route],
[control encoding][v-sdk-wire], [owner edit transaction][v-sdk-owner].

The edit coordinates are **ordered current-result coordinates**: each
`extend_splices` call replaces a range in `prepared.next`, so a later member
can edit bytes introduced by an earlier member. The batch is prepared without
mutating the inode, then applied once after all members and revision checks.
This local SDK contract is distinct from the current C1 EditFile stream's
restrictions on targeting earlier replacement bytes. Lowering must normalize
the local final view into supported service inputs without changing inherited
SDK semantics. [Ordered splice preparation][v-splices].

The cache cut here must not be conflated with Commit capture. It drains
ordinary callbacks, permits the needed folio/writeback progress, reconciles
kernel state and then applies the SDK transaction. A claim of “zero
edit-caused FUSE WRITE” requires route-specific verification: it cannot be
achieved by writing through FUSE, and the existence of cache-laundering code
means callback counts cannot be inferred solely from this diagram.

## 3. Mount, scheduler and multi-Workspace admission

The release creates one fuser session per mount. `n_threads=1` and
`clone_fd=false` limit the receive loop, not all work to one blocking thread.
Ready futures are polled on ingress; pending work moves to the shared Tokio
runtime. Backing/kernel work enters bounded blocking jobs. This already avoids
a new dispatch thread for each ordinary FUSE callback. The daemon also has
separate mount/control/execution lifecycle threads; the whole process is not
a two-thread program. [Mount configuration][v-host-mount], [runtime][v-runtime].

| Exact-release setting | Value and scope | What it does not establish |
| --- | --- | --- |
| Tokio runtime | 2 async workers and at most 2 blocking workers per `LiveRuntime`; `shared()` is process-wide | Total process thread count or total memory |
| Ordinary scheduler | 256 requests and 32 MiB transfer reservations shared by scheduler clones | Per-Workspace fairness or RSS ceiling |
| Live allocation semaphore | 128 MiB per shared runtime | Complete accounting of every collection, cache, kernel page and thread stack |
| Physical work | 2 concurrent physical permits shared by the runtime | Two canonical construction workers |
| Kernel work | 1 concurrent invalidation/kernel blocking job | No callback dependence or deadlock without separate progress paths |
| Control/writeback admission | 32 requests and 4 MiB transfer reservations | A dedicated service-level history queue |
| Lifecycle admission | 2 requests and 8 MiB transfer reservations | Limit on retained frozen state across all Workspaces |
| Snapshot admission | 4 requests and 8 MiB transfer reservations | Four complete frozen inputs or an aggregate host-input memory cap |
| Optional prefill | One rendezvous worker, queue capacity 0, 256 KiB configured stack | Guaranteed prefill, cache residence or memory saving |
| Kernel FUSE settings | max write/readahead requested at 1 MiB each, max background 64, congestion threshold 48 | Actual demand count, negotiated limit or measured parallelism on every kernel |

The receive path uses non-waiting admission for owned live callbacks so a full
ordinary queue does not conceal writeback/release behind the sole receiver.
Release/releasedir have special cleanup admission. Physical admission happens
before entering Tokio's blocking queue. Lifecycle and snapshot lanes reserve
their own capacity. Preserve these **progress properties**, not necessarily
these constants. [Admission and job ownership][v-runtime],
[callback admission][v-callback-admission].

The backing listener's `Semaphore::new(3)` is a handler/accept reservation,
not a three-open-socket limit: `c/o/s` handlers place their sockets in retained
slots and return, releasing that handler permit. The normal route has one data
connection plus control, observer and snapshot sockets. Do not use the number
three as its complete connection/FD budget. [Listener][v-listener], [roles][v-roles].

### 3.1 Legacy root/Workspace count is not unlimited

The daemon computes:

```text
admission_limit = clamp((RLIMIT_NOFILE.soft - 32, saturating) / 8, 1, 256)

admitted registry load = mounted-or-starting Workspaces + active-or-starting execs
refuse a new mount or exec when registry load >= admission_limit
```

Mount identity and root duplication are separately rejected. This is a
descriptor-derived heuristic with a hard 256 ceiling, not a proof that every
mount consumes exactly eight FDs or that payload segments, sessions, cgroup
observers and process output can never exhaust resources. Each retained
payload segment also owns an open file in the release. The new finite
WorkspaceHost registry needs explicit resource accounting; it must not
silently replace this with unlimited mount creation.
[Mount count check][v-mount], [execution count and RLIMIT calculation][v-admission].

### 3.2 Reference and proposed Commit concurrency differ

```text
 RELEASE, TWO DISTINCT WORKSPACES

 Workspace A: lifecycle lock -> capture GA -> pull input A --+--> build A -> publish
                                                            |
 Workspace B: lifecycle lock -> capture GB -> pull input B --+--> wait global build
                                                                 gate -> build B

 Live writes/SDK edits use each execution-side owner's live state meanwhile.
 Each owner has its own snapshot slot. Retained host inputs can overlap.

 CURRENT PROPOSAL, ONE CONSUMER

 consumer retained-G admission: one frozen submission total
 Workspace A owns GA ---> A's correctness slot stays held through staged/unknown state
 Workspace B capture ---> explicit capacity/refusal until retained-G admission is free

 Separate active client/network admission and service admission also apply.
 Idle retained G must not hold a network mutex needed by ordinary operations.
```

The release's Workspace lifecycle mutex is per Workspace. Capture and frozen
input acquisition happen before the process-global construction mutex. The
source comment describing “at most one queued” does not by itself implement
a finite waiting queue around `std::sync::Mutex`; this audit does not promote
it to an enforced queue limit. [Worker state][v-worker], [Commit route][v-commit],
[construction gate and worker selection][v-workers].

The proposed one retained frozen submission **per consumer**, a proposed
single active remote call for that consumer, existing service-wide two active
operations with no queue, and C2's two private saves per Store are different
constraints. They must not be collapsed into “one Commit lock.” The initial
consumer policy is tighter than the release's per-Workspace snapshot slots.
It is an explicit compatibility and multi-Workspace throughput risk. No
limit is raised by this audit. Same-Workspace correctness and cross-Workspace
admission/fairness require separate tests in [04](04-implementation-and-verification.md).

Many-Workspace, many-Branch and mixed-concurrency selections must preserve
their declared lanes and complete selections. Summed concurrent API timers
cannot be compared directly with elapsed wall time. A smaller set of admitted
Workspaces or a serialized workload is not evidence that the replacement
beats the original concurrent workflow.

## 4. Lazy metadata, immutable reads and kernel cache

### 4.1 The actual tiers

```text
 kernel page/dentry/inode caches
           |
           | miss / callback
           v
 LiveWorkspace live names, inode records, directory state and pieces
           |
           +-- pending byte range --> local immutable disk extent
           +-- zero range ---------> zero fill, no byte backing
           `-- immutable base -----> daemon scope cache (32 MiB shared)
                                           |
                                           | cache miss
                                           v
                               one READ_BASE logical range request
                                           |
                                           v
                              host canonical read_range + authentication
                                           |
                              SnapshotReader cache / Store / OS caches
```

The host's grouped lookup can return the requested inode plus up to 127
siblings from the already validated directory leaf. Complete-leaf knowledge
is separately signalled; a partial page does not prove absence. Optional
content export is at most 8 KiB per file and 512 KiB per lookup/page response.
An 8,192-entry exported-content hint set suppresses repeat optional exports
until it is cleared. Failed optional prefetch does not turn into a successful
demanded read. Metadata-only lookup omits this speculation.
[Host lookup and prefetch][v-backing].

`ImmutableReadCache` is a process-shared FIFO across content ranges, names and
pages, with owner scopes preventing acquisitions from different owners from
being confused. It charges payload plus a 384-byte entry allowance and extra
name storage. A content hit may cover a subrange and returns a new `Vec`.
Demanded base reads insert their returned range even when the file exceeds
8 KiB. Read hits do not update FIFO age; overlapping-range lookup scans
backwards within the file's entries. [Cache implementation][v-cache],
[read-plan execution][v-read-owner].

The Store SnapshotReader cache is a separate 8 MiB FIFO with a 96-byte entry
allowance. It intentionally skips CHUNK_MAGIC payload objects larger than
1 KiB. This says nothing about the daemon's range cache, kernel cache or OS
cache. The caches have different identities, contents, admission rules and
lifetimes; their capacities cannot be treated as one transferable budget.
[SnapshotReader cache][v-store-cache].

### 4.2 Round trips are demand-dependent

| Operation on the release live route | Possible backing demand | Fair new-plan comparison |
| --- | --- | --- |
| `lookup` | 0 with live/kernel/cached facts; otherwise one grouped or metadata-only lookup for the unresolved component | Current `Inspect::Stat` is path-based and returns no length; initial full regular-file attrs can need `Inspect::File` too |
| `stat` / `getattr` | 0 for known inode state; unresolved pathname components can acquire facts first | Count metadata lookup, content-length inspection, kernel cache hits and optional inode-stable API gaps separately |
| `readdir` / `readdirplus` | 0 for available facts; one request per needed host directory page, with batched inode facts and optional tiny payload | Current List returns names/serials, not all kinds/attrs/lengths; extra lookups must be paid or an agreed batched projection API supplied |
| `read` | Local pieces/zero/cache may use 0; each missing immutable Base piece/range can require a READ_BASE, bounded to 1 MiB per request | Current ReadFile is a bounded logical range, not a canonical-object RPC; fragmented mixed pieces can cause several such calls |
| `execve` | Kernel decides lookups, attrs, executable/interpreter/loader/library reads and cache reuse | No fixed callback/RPC count. Above 8 KiB avoids only the optional whole-file prefetch band |
| SDK edit batch | BEGIN + one PART per edit + END exchanges, plus any demanded metadata/cache-reconciliation work | New daemon control must preserve exact inputs, ordering and visibility; a service file-save call is not a replacement |

These are control-flow counts, not latency estimates. Count actual requests,
bytes, copies, authentication/decoding and retained memory for both arms in
the later mount experiment. A remote range response can still reconstruct
larger canonical objects on the service. C1 and C2 remain local together;
networking their individual object reads would be a different and prohibited
boundary. [Current public surface][c-request], [service inspection][c-inspect],
[current content read][c-read].

### 4.3 Kernel coherence is substantial behavior

The release requests ASYNC_READ, BIG_WRITES, PARALLEL_DIROPS, READDIRPLUS,
READDIRPLUS_AUTO and MAX_PAGES when available, with optional stateless-open
support. It does not request FUSE_WRITEBACK_CACHE in this negotiation. It does
use `FOPEN_KEEP_CACHE`; cached I/O, page-fault reads and mapped writes therefore
cannot be dismissed by observing the absent WRITEBACK_CACHE flag alone.
[Capability negotiation][v-fuse-init], [open flags][v-open].

Tiny-file kernel prefill is optional and uses already acquired immutable
bytes. It excludes writable-seen files and coordinates with per-inode edits.
SDK edits additionally reconcile queued kernel folios: stale bytes covering
SDK-modified ranges are replaced while unrelated mapped changes are retained.
These paths explain why removing kernel-cache code is not automatically a
safe simplification. [Prefill and owner state][v-owner], [SDK cut][v-sdk-cut],
[write reconciliation][v-write-owner].

The current Pair 1 initial profile adds no new userspace content cache or
prefetch. That reduces a policy and its implementation, but may increase
remote demand. The reference's 8 KiB threshold and 32 MiB daemon cache are
neither approved defaults for the replacement nor evidence of a speed defect.
Choosing a transport-aware prefetch policy requires matched tiny/large-file
read and repeated-exec measurements, including useless-prefetch bytes and
memory. Changing cache capacity still needs an owner decision.

Shared writable mmap is not qualified in the new plan. It must be enforceably
refused by the selected Linux/fuser capability combination or coherently
handled before writable admission. Documentation alone cannot prohibit a
mapping that the kernel allows. A cached readable/executable mount and its
required invalidation behavior remain part of the first-mount proof.

## 5. Writes, extents and byte-copy ownership

### 5.1 The release already avoids whole-file copy-up

```text
 before: immutable base B, length L

 overwrite [a,b) with replacement X
       |
       v
 Base(B,0..a) + Spool(segment,offset,len(X)) + Base(B,b..L)

 unchanged bytes are references, not copied into the spool
 append adds an extent; a gap is represented as Zero
 range reads visit the overlapping pieces and copy only returned bytes
```

`prepare_write` represents an unmodified Base file as a PieceTree with a base
reference. It prepares the range splice without I/O or visible live changes.
The piece tree shares its unchanged structure and the replacement backing
reference. `apply_edit` checks the prepared revision and data before changing
live state. This is a strength to preserve, not an innovation of the new
proposal. [File-edit preparation/apply][v-file-edit].

The normal kernel write path obtains per-inode order, excludes prefill,
reserves a private segment range, prepares the edit, writes the replacement
with `write_all_at`, then applies the prepared piece state. Only after success
does it acknowledge the write. No canonical construction, Store publication
or host acknowledgement is needed for that ordinary local write.
[Write path][v-write-owner], [physical reserve/write][v-spool].

Reserved high-water is advanced **before** the physical write. Despite a
nearby comment saying ranges below high-water are written, reserve alone is
not an initialization proof. The meaningful invariant is that a published
piece names successfully installed bytes. The new disk design must track
reserved/unpublished/failed ranges separately, reserve metadata and scratch
before visibility, and retain their charges through cleanup failure.

### 5.2 Copies that exist despite bounded I/O

| Route | Source-level ownership/copies | Replacement obligation |
| --- | --- | --- |
| Kernel write ingress | `submit_write` copies borrowed callback bytes to a `Vec` before asynchronous retention | Own any bytes retained past the borrow; direct local I/O within a valid callback borrow need not force an extra copy |
| Physical write job | `write_owned` creates another payload `Vec` for the `'static` blocking closure; SDK-folio reconciliation can allocate a patched vector | Transfer ownership when possible; budget all simultaneously live buffers; no blanket zero-copy claim |
| Local/immutable range read | ReadPlan retains references; output Vec is sized to the requested read; spool read returns a Vec copied into output | Bound output plus source/decode scratch and pins, not just the request size |
| Daemon immutable cache | Hit returns `to_vec`; miss can retain cache bytes alongside read output | New no-content-cache baseline avoids that retention but can incur more remote reads |
| Snapshot payload serving | Spool read returns Vec; SNAP_READ copies into response `out`; receiver allocates frame result | Stream/transfer existing owned buffers where public seams permit, while preserving limits and errors |
| Host frozen payload | RemoteWindows retains fetched segment coverage; requested slices return copied Vecs | A transfer window is one memory category, not complete host construction memory |
| Legacy frame send | Header and existing payload use `write_vectored`; no extra contiguous payload buffer is required by that frame helper | Preserve short-write handling; do not claim vectored send eliminates kernel, decode or consumer copies |

Sources: [write dispatch][v-write-dispatch], [physical write][v-write-owner],
[read execution][v-read-owner], [snapshot serving][v-snapshot-pages],
[host windows][v-input], [frame exchange/writev][v-exchange].

The current bridge also has real framing/encryption buffer ownership. Its
16 KiB payload frames and bounded coalescing are already implemented, so
reimplementing a bespoke “fast FUSE transport” would duplicate a solved
boundary. Reusing it does not make transport free; see section 8.

### 5.3 Disk and memory bounds have different scopes

| Release bound/mechanism | Exact value and scope | Required interpretation |
| --- | --- | --- |
| Packed segment capacity | 1 MiB per local segment | Limits dead-range retention per segment; many segments still accumulate |
| Hint window | 4 sealed segments repeatedly offered for eviction; current segment separate | Advisory behavior, not a hard resident-page ceiling |
| Workspace spool policy | Default 1 GiB logical spool bytes per Workspace | Does not cap whole process memory or all physical dead/pinned disk allocations |
| Final-delta policy | Default 8 MiB per Workspace | Separate from spool, caches, runtime reservations and host input |
| File piece count | 8,193 pieces per file | A fragmentation refusal bound, not a small-file-count bound |
| Inline edit/Workspace | 1 MiB per edit; 8 MiB inline bytes per Workspace | SDK inline state differs from normal kernel disk-write state |
| Piece allocation | 2 MiB limit in the file-edit resource accounting | Must not be mistaken for complete metadata/process memory |
| Logical file/zero bounds | 1 TiB result; 1 GiB logical zeros; 131,072 predicted zero extents | Historical source limits, not current service allowances |
| Host snapshot window | 8 MiB per frozen input, with at most 1 MiB per segment fetch | Several retained inputs and decoded metadata can exceed one window's footprint |
| Candidate object paths | 8 MiB candidate memory, 64 MiB candidate index, 1 MiB candidate spill buffer constants | Separate host construction ownership; not all necessarily simultaneously fully allocated |

Sources: [spool][v-spool], [Workspace resource policy][v-policy],
[file edit limits][v-file-edit], [host windows][v-input],
[candidate constants][v-object-bounds]. These are source settings, not memory
measurements, additive peak arithmetic or owner approval to reuse them.

One surviving byte can pin a full segment. A failed write can leave dead
reserved space. A removed directory entry can leave an open inode/read pin.
The release's retire path removes registry-only segments, attempts unlink
and decrements its logical physical counter even if unlink fails. The new
design must not copy that accounting shortcut: keep allocated, reserved,
dead, unlinked-but-open and failed-cleanup storage charged until the relevant
resource is actually released. [Retirement/abandon][v-spool-retire].

`POSIX_FADV_DONTNEED` is an optional hint. A filesystem can acknowledge writes
while kernel/cgroup file pages remain resident. `evict_served` offers recently
served ranges too, but a source comment is not a residency proof. The new
8 MiB consumer Workspace allocation target remains **unqualified** and is
not total RSS or a kernel-memory guarantee. Test append, dense overwrite,
read-after-write, open/close and mapped/cache behavior separately; successful
append evidence cannot erase a dense-rewrite memory failure.
The retained dense-rewrite result and the narrower append evidence are
distinguished in the [benchmark qualification map](06-benchmark-qualification-map.md);
this source analysis does not relabel either outcome.

## 6. Snapshot, host lowering and repeated Commit

### 6.1 What the release captures and what it materializes later

```text
 live dirty state at generation G
             |
             | CAPTURE: take dirty IDs + expand changed-directory child IDs
             v
 frozen frontier G                  live successor mutations
  IDs + original node ownership <--- copy original node on first mutation
  shared PieceTree/backing refs       new revision and dirty set
             |
             | SNAP_RECORDS(after): page encoded frozen nodes
             v
 host FrozenRemoteInput
  HashMap<NodeId,Node> + BTreeSet dirty + per-segment range metadata
             |
             | build_remote_candidate / canonical construction gate
             v
 host builder ---- on-demand SNAP_READ ----> execution-side disk ranges
             |
 canonical output / conditional Store publication
             |
 COMPLETE_BEGIN + COMPLETE_NODE pages + COMPLETE_END
             v
 only covered matching revisions change to canonical bases
 live newer revisions survive; frontier pins are released when settled
```

The release's first-touch preservation is real: `protect` retains a node's
capture-time state before later mutation; the persistent piece structure and
backing references retain immutable ranges. Rename has a broader path-retain
step because the reference rewrites descendant path records. Normal live
writes are not stopped for the entire host build. [Frozen ownership][v-frozen],
[remote Commit][v-commit].

A separate concurrency review item is visible in the source: SNAP_RECORDS holds
the state lock while obtaining the snapshot slot, whereas SNAP_CANCEL and
COMPLETE_NODE hold the snapshot slot before obtaining state. Normal host
sequencing limits which handlers overlap; this inspection is **not a demonstrated
deadlock or execution result**. The replacement must have a documented lock
order, avoid encoding under state, and exercise competing public control/data
transitions under the selected admission policy. [Record-page locking](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L2793),
[cancel locking](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L2584),
[completion locking](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L2629).

Three source costs are particularly relevant to the new design:

1. **Capture expands and clones the dirty frontier.** It avoids payload I/O
   and a complete namespace scan, but loops through dirty IDs and changed
   directory bindings. Maintaining the necessary persistent frontier before
   capture would remove work from the cut only if that maintenance is charged
   to each mutation, not hidden in setup.
2. **Every snapshot page recreates the whole ID list.** `frontier_ids()`
   allocates all frozen IDs, and the page loop skips entries at or before the
   cursor. Node cloning and encoding occur under the Workspace state lock.
   With D IDs and P pages this can revisit/copy D IDs per page; a page-size
   bound does not eliminate that repeated work. The new cursor must operate
   over the retained index directly, with bounded encoding outside the short
   state/capture lock.
3. **The host retains all decoded records.** `pull_frozen_input` accumulates
   a HashMap, dirty set and segment coverage; `build_remote_candidate` then
   builds a canonical-inode map. The 8 MiB payload window does not bound those
   allocations. For tiny-file workloads, the replacement needs paged/bounded
   metadata and completion associations, not only disk payload backing.

Sources: [capture/ID accessor][v-frozen], [snapshot pagination][v-snapshot-pages],
[input accumulation][v-input], [candidate input][v-build-remote].

`FACT_PAGE_BYTES=64 KiB` is a normal page target, not an absolute node envelope:
the first record may exceed it, and is rejected only by the frame-fit check.
The live frame maximum is 1 MiB + 64 KiB. A declared `MAX_NODE_BYTES=16 MiB` or
`MAX_FACT_MEMORY=96 MiB` constant elsewhere in `live_wire` does not prove those
limits are enforced on this current snapshot collection path. Audit the
call-site checks, not just the constants. [Wire constants][v-wire],
[snapshot sender][v-snapshot-pages].

### 6.2 Lowering remains host-side and can still do substantial work

The release materializes the changed frozen records and streams their payload
from execution-side backing as the existing builder demands it. Segment
coverage is learned while records are decoded; the host can fetch the bounded
covered span of a segment once and serve several small pieces from its
RemoteWindows cache. Eviction or later demand can require another fetch, so
“one round trip per segment” is a reuse strategy, not a lifetime guarantee.
[Frozen acquisition and windows][v-input].

`build_remote_candidate` feeds the existing CandidateInputs path. Complete
small-file construction, predecessor-aware file mutation, directory/inode
construction, reference bookkeeping, output authentication and Store
admission are still host work. Host journals/spills and codec buffers are
separate from the execution-side spool. This route does **not** first copy
the complete pending payload into a new host staging file.
[Candidate builder][v-build-remote], [candidate implementation][v-changes],
[object buffering/admission][v-objects].

The separate materialized/local capture-thread code in `capture.rs` remains
reference code, but it is not an additional third mandatory payload tier in
this exact container snapshot route. A fair comparison must trace the route
actually selected rather than sum all legacy implementations together.

Current Pair 1 likewise sends final file content or supported base-relative
edits to the shared file-save handlers, then one coherent prepared filesystem
change through the existing composite Commit or explicit StageChanges path.
It must not issue a redundant UpdatePreparedFilesystem save immediately before
StageChanges/Commit repeats that filesystem construction. C1/C2 algorithms,
SQL and packs remain service-local. [Current integration](03-commit-integration.md).

### 6.3 Completion is version-safe; recovery is narrower than the comments

Release completion checks the snapshot incarnation/attempt and each node's
captured revision. A matching version can replace its edited state with a
canonical base; a newer revision is skipped. `finish_covered_generation`
retains later mutations, scans edited nodes to recompute accounting and
updates the base root. This scan is outside capture, but is still real work
and can retain old backing through surviving versions/readers.
[Completion handlers][v-complete], [version guard and finalization][v-frozen].

The host retains a failed pre-publication attempt and may explicitly re-drive
that same attempt on a later Commit call. After known publication it retains
exact pending completion records, which a later Commit/End can redeliver.
However, COMPLETE_END removes the daemon snapshot slot before the final reply;
a lost final acknowledgement is not proven recoverable merely by comments
calling delivery “idempotent.” A later COMPLETE_BEGIN requires that slot.
Do not infer durable deduplication, indefinite byte-transfer resume or crash
recovery from this in-memory mechanism. [Host attempt/completion retention][v-commit],
[terminal completion][v-complete].

The current proposal keeps exact G and live D1 together until the existing
shared history outcome and local reconciliation are known. It does not import
legacy automatic attempt re-drive, refresh expected Branch context or replay
an uncertain mutation. An authoritative known Commit followed by failed
local reconciliation stays a known remote Commit with local completion work
remaining. A normal handle resolves its current inode state for each read;
each in-flight read pins its selected version. Old read pins can outlive G's
logical completion. [Overlay lifecycle](02-overlay-snapshot.md),
[completion and unknown outcomes](03-commit-integration.md).

### 6.4 Shutdown and connection failure remain observable work

Reference daemon teardown deactivates mount admission, prepares owner
shutdown, unmounts, destroys owned spool state, resolves control shutdown,
cleans the mount root and drains lifecycle ownership before emitting
WorkspaceClosed. Failure produces an infrastructure-lost result rather than
the successful close acknowledgement. The new API deliberately separates
unmount from `close_clean`: unmount must retain dirty/pending Workspace state,
and close_clean must refuse it. Copying the old destructive teardown directly
into the new unmount operation would violate that contract.
[Reference teardown][v-shutdown], [new lifecycle](01-workspace-fuse-contract.md).

The release reuses its established data/control/snapshot sockets across
operations; it does not perform a new connection handshake for every range.
The taken socket is restored after a complete successful exchange, while a
failed/cancelled partial exchange drops it. The control/snapshot request
wrappers have a historical 120-second timeout and map timeout/connection
failure to the broad port I/O error. These are neither a portable reconnect
protocol nor acceptable default FUSE deadlines for the replacement. The
new contract must preserve timeout/unreachable versus missing content and
unknown mutation outcomes, without retrying the mutation automatically.
[Persistent connection/failure handling][v-transport-failure].

## 7. Compatibility proxy and filesystem behavior inventory

The release also carries an older `ProxyClient`/`ProxyHost` route that forwards
filesystem operations over a single data connection. It has 1 MiB write
coalescing, read-ahead up to 2 MiB with four entries, deferred creates/closed
creates, up to 16,384 pending unlinks, and a protocol with 17 MiB frames,
16 MiB byte vectors and 16,384-entry limits. Fence/fsync responses acknowledge
deferred errors. `pause` drains callbacks and buffered operations before its
barrier; `resume` releases that pause. [Proxy client][v-proxy-client],
[proxy codec][v-proxy-protocol], [proxy host][v-proxy-host].

Those algorithms belong to that proxy route. They must not be credited to the
normal live-owner route merely because both live in `layerfs-fuse`. Removing
the remote filesystem proxy is a real boundary simplification because Pair 1
keeps filesystem algorithms with the daemon Workspace and uses existing
logical service operations. The remaining local ordering, admission and error
visibility requirements do not disappear with the proxy files.

The release implements these callback groups explicitly. The exhaustive
replacement policy and API gaps remain in [01](01-workspace-fuse-contract.md).

| Release callback group | Behavior to retain or deliberately qualify |
| --- | --- |
| `init`, `destroy`, `forget` | Negotiated capabilities, lookup/lifetime accounting and release under pressure |
| `lookup`, `getattr`, `readlink`, `access` | Path/type semantics, stable inode identity, synthetic versus stored attrs, permission enforcement and distinct missing/error outcomes |
| `open`, `read`, `release` | Current inode view, per-read version pins, rename/unlink survival and exactly-once handle cleanup |
| `opendir`, `readdir`, `readdirplus`, `releasedir` | Stable cookies and bounded directory views; names/kinds/attrs must not silently diverge |
| `create`, regular-file `mknod`, `mkdir`, `symlink`, `link` | Namespace creation, inode identity and hardlink sharing; current ReserveInodes alone does not implement all these operations |
| `write`, size-changing `setattr` | Write-before-visibility, checked ranges, append/truncate/zero gaps and failure behavior |
| metadata `setattr` | Supported modes/times and explicit unsupported attributes; new stored metadata construction is a prerequisite |
| `unlink`, `rmdir`, `rename` | Type/emptiness/cycle rules, overwrite behavior, surviving handles and both-directory atomicity |
| `flush`, `fsync`, `fsyncdir` | Distinguish volatile error/visibility validation from durability and logical Commit |
| `statfs` | Reference returns synthetic capacity; replacement should not expose invented disk availability as its actual admission budget |

Reference fsync validates volatile owned state, reports known write/allocation
errors and retires idle allocations. It performs no durability flush, fact
export, canonical construction or host acknowledgement. Thus returning
unsupported for fsync in the new initial profile is an explicit compatibility
reduction, not removal of an unnecessary host commit. The choice and
`O_SYNC`/`O_DSYNC` treatment must be enforced and tested as declared.
[Volatile fsync][v-fsync].

No new network lock manager is required simply to keep kernel-local advisory
locking. Conversely, omitting lock forwarding is not a distributed-locking
guarantee. Optional xattr, ioctl, fallocate, copy-range, special-device,
distributed-lock and mapping features need their declared support/refusal
behavior. Absent callbacks or unnegotiated flags do not prove that every
related syscall is rejected by the kernel/library combination.

The managed `/workspace-id` root is immutable in the current owner direction.
The parent namespace must enforce this for rename, replacement and removal;
the Workspace's descendant-rename handler alone cannot protect an external
parent. This policy is independent of the reference's general descendant
rename implementation and must not be billed as a faster rename algorithm.

## 8. Existing shared foundation: what is already reusable

At reviewed `152b9c3a2`, Pair 3 provides an implemented closed logical surface:
ReadFile, Inspect, ConstructFile, EditFile, UpdatePreparedFilesystem and the
merged history query/command surface. Direct and native delivery enter common
authorized handlers. The bridge has no C1/C2 dependency; service execution
keeps provider/consumer composition local. [Bridge exports][c-bridge],
[request surface][c-request], [service dispatch][c-owner].

| Implemented foundation | Value/scope or responsibility | Pair 1 consequence |
| --- | --- | --- |
| Logical read | ReadFile(root,start,end); Inspect File/Stat/List/Readlink | No per-canonical-object remote calls; inode-stable/full-attr projection gaps still need their agreed resolution |
| Logical save | ConstructFile; known EditFile; prepared filesystem update | Pending local edits are not saved or published by each FUSE write |
| History | Exact staged generation/selector, composite Commit, explicit AddLayer and Fork | Consume actual completion semantics from 03, not legacy Store/private history algorithms |
| Payload frame | 16 KiB | A framing bound, not callback, total input or memory bound |
| Metadata envelope | 32 KiB | Raising logical record counts alone does not solve the envelope/decoded memory limit |
| File limit | 4 GiB | Current shared limit, smaller than the legacy logical result ceiling |
| Known file edits | 256 edits and 8 MiB replacement input | Dense large-file edits need an agreed complete-file route or shared extension; no hidden split into intermediate Commits |
| Prepared filesystem inputs | Current 128 changed names, directories and inodes bounds | Full npm snapshots cannot be shrunk to fit; coherent large-input support remains a prerequisite |
| Service operation admission | 2 active complete operations across the Service, no waiting queue, including reads | Shared backend pressure affects neighbors even with separate kernel sessions |
| Sessions | 4 retained/handshaking/closing sessions plus 1 refusal slot | Distinct from operations and from proposed mount count |
| Operation/deadline bounds | Request maximum 600,000 ms; I/O-progress bound 5,000 ms | These are maxima in the shared profile, not automatically appropriate FUSE callback deadlines |
| Replayable input envelope | 8 MiB declared bound | “Replayable input” is not authorization to replay mutations |
| Native send coalescing | 256 KiB flush threshold after record append; a record may cross that threshold. Records shorter than half a 16 KiB frame are sent promptly | Already implemented transport optimization; threshold is not a strict allocated-memory ceiling and no new batching codec is needed |
| Native input delivery | Cooperative source and deadline; scoped upload thread configured with 2 MiB stack | No claim that reusing bridge removes every per-call thread or stack; do not add an extra thread per FUSE callback |
| Authentication | Native Noise authenticated peer; logical Store/operation grants in service | Do not port raw legacy capability framing, acquire SQLite credentials in FUSE, or treat hashes as authorization |

Sources: [contract bounds][c-request], [request validation][c-request-validation],
[native connection/batching][c-connection], [client lifetime][c-client],
[service admission][c-owner]. C2's two private saves per Store are a separate
storage constraint; neither number implies two permitted construction
producers or an approved higher consumer concurrency limit.

The current bridge owns plaintext/ciphertext frame buffers and batched sealed
records. Decode/authentication and output copying still cost work. Those
costs belong in matched service/transport measurements. It is fair to claim
shared implementation reuse and an existing bounded authenticated transport;
it is not fair to claim faster transport than v0.1.6 without measurements.

### 8.1 Exact current transfer/replay costs and possible regressions

```text
 consumer stable Source
      |
      | source read into upload buffer; Body owns bytes
      v
 Frame encode -> reusable plaintext -> Noise ciphertext -> contiguous batch
      |
      | persistent native connection; several records can share a socket write
      v
 receive sealed scratch -> plaintext scratch -> decoded Frame.bytes
      |
      +-- ConstructFile: Exact stream -> C1 construction -> C2 save
      |
      `-- EditFile: acquire all bounded Replacements -> C1 apply_edits -> C2 save
                   exact EOF before begin_save

 result direction: C1 sink -> ResultData-owned chunks -> encode/encrypt/send
                   -> receive/decode -> caller output
```

The exact shared implementation includes these costs:

| Source path | Already implemented behavior | Fair interpretation |
| --- | --- | --- |
| Sender/Receiver | Reused plaintext/ciphertext scratch, frame serialization and decryption; sealed records copied into a contiguous send batch | Buffer reuse and record coalescing exist now. They reduce allocation/syscall opportunities, but are not end-to-end zero-copy or a measured speedup |
| Frame codec and payload adapters | Frame decoding allocates body bytes; Input copies from the retained frame into consumer buffers; Output creates owned ResultData chunks | Small frame limits bound individual pieces, not all simultaneously retained decoded/body/codec state |
| Client call | Established connection reused; one scoped upload task and concurrent result delivery within an absolute deadline | Do not add a handshake per range or an extra callback thread. Account the existing upload stack and join/cancellation work |
| EditFile input | All replacement parts are read into `Replacements`, with exact end-of-input before `begin_save` | This is bounded in-operation input acquisition for C1's replayable source, not durable staging or automatic mutation replay |
| ConstructFile input | Validated sequential stream reaches C1 with exact EOF/length validation | Avoids requiring all complete-file bytes in service replacement vectors; canonical construction may still use its own bounded scratch |

Sources: [native buffers and send batching][c-connection-buffers],
[frame decode][c-frame], [payload adapters][c-payload], [client][c-client],
[service input/save][c-write]. The 8 MiB EditFile replacement limit is in
addition to frame/decode/metadata state and the execution-side frozen source;
it cannot be described as an 8 MiB total process bound.

The reference live protocol's writev helper already avoids an additional
contiguous frame-payload allocation, while current native transport performs
authenticated encryption and record batching. That is a security/portability
contract difference with computational and copying costs, not evidence that
the new transport is intrinsically faster. Compare the same authenticated
logical work and include connection setup where the workflow requires it.

Metadata fanout can also regress: a release grouped lookup/readdirplus can
return several inode facts and tiny contents together; the current service
profile offers one Inspect operation at a time, Stat omits file length and
List omits kind/attributes. Until an agreed full-attribute/batched projection
surface exists, additional calls are real work under the global two-operation
limit. Omitting readdirplus or prefetch is a declared first-profile choice,
not a batching improvement. Neither public ReadFile nor a root hash permits
inventing a private canonical-object call to work around those costs.

Likewise, C1's current 4,096-binding traversal ceiling and typed-error gap
remain user-visible namespace blockers. The prepared directory changes name
only final **changed-name bindings**, not every entry in each directory; that
improvement in caller input shape does not imply all service alias/cycle and
release traversal is proportional to the changed names. The current API and
metadata-scaling prerequisites are detailed in [01](01-workspace-fuse-contract.md)
and [03](03-commit-integration.md).

## 9. Fair comparison: keep, simplify, reduce, then measure

| Item | Classification | Concrete decision / required evidence |
| --- | --- | --- |
| One daemon Workspace owns mutable namespace and bytes | Preserved architectural strength | Keep FUSE and SDK edits on that owner; service and bridge do not own pending overlays |
| No whole-file copy-up on first edit | Preserved algorithmic strength | Keep base/piece/zero references; prove one-byte edit does not flatten or read all old bytes |
| Disk extents shared by live state, frozen G and readers | Preserved ownership strength | Keep immutable published ranges and version pins; bound dead-space and descriptor retention |
| Live writes during host save | Preserved behavior, proposed replacement implementation | Prove G/D1 consistency during actual save; delayed replies alone are insufficient |
| Existing Pair 3 auth/framing/common handlers | Implemented shared foundation | Consume it; no private canonical RPC, SQL or second remote implementation |
| Remove older syscall proxy implementation | Boundary simplification | Replace remote syscall forwarding with local Workspace + public logical calls; retain ordering/errors |
| Remove repeated full-frontier ID cloning/page scans | Proposed structural improvement | Maintained index and true bounded cursor; charge mutation maintenance and verify capture/paging work |
| Page pending metadata and completion associations | Proposed scalability improvement | Disk index design and resident/dirty-page bounds are still open; tiny-file count must not grow RAM without bound |
| Avoid redundant intermediate filesystem save before StageChanges/Commit | Proposed integration simplification | Use the existing composite path once; verify construction and storage outcomes |
| Reduce redundant payload/frame copies | Proposed ownership improvement | Preserve borrow lifetimes, admission, short writes and unknown outcomes; instrument actual copies |
| No extra userspace content cache/prefetch initially | Deliberate policy reduction | Can cost more round trips; measure matched read/exec workloads before any new cache decision |
| One retained frozen submission per consumer | Tighter admission / compatibility risk | Record cross-Workspace refusals and progress; do not change concurrency silently to pass a gate |
| Initial unsupported fsync/mmap/optional operations | Compatibility reductions where declared | Enforce refusals and retain full selected workload failures; fewer features are not an algorithmic win |
| 8 MiB consumer working target | Unqualified target | Account Workspace ownership and separately measure RSS/cgroup/kernel pages; no cache-hint proof |
| Faster mount, shell, Commit or repeated exec | Unmeasured | Requires real mount, matched semantics/identities/cache state and complete workload selection |

### 9.1 Minimal must-keep and must-not-port list

Keep the semantic invariants: accepted bytes are owned and readable; failed
pre-publication writes leave prior state intact; errors after publication
preserve the actual or unknown outcome; logical identity is distinct from
path, inode and mount; open/unlinked inodes and in-flight versions stay alive; hardlinks share
inode contents; directory operations are coherent; G/D1 completion matches
the exact capture and Branch context; cancellation and shutdown retain enough
state to report their actual outcome; release/control progress survives load.

Do not port a second filesystem proxy, canonical-object messages, host-owned
pending overlays, private Store algorithms, an unbounded dirty-record vector,
unconditional full-file reconstruction into RAM, an implicit background
Commit on close/pressure, or automatic mutation re-drive after an uncertain
reply. Do not remove required authentication, bounds, error distinctions or
kernel coherence to obtain fewer files.

### 9.2 Measurements and proofs that decide the proposed improvements

| Question | Later evidence required on both matched arms |
| --- | --- |
| Does lazy mounting help the complete workflow? | Actual mount plus first selected access, all work charged in its phase; no moved construction/setup work |
| Is current prefetch worth its cost across the bridge? | Tiny and large files, metadata-only versus grouped behavior, demanded and speculative bytes, actual RTT count, latency distribution and memory |
| Does repeated `execve` benefit? | Binary above 8 KiB with complete loader/library closure; declared cold/warm rows, callback and range-cache/kernel-cache observations |
| Does the new cut stay short as tiny-file count grows? | Cut work/allocations and dirty-frontier/index operations; no payload, full-ID scan, paging I/O or encoding under the cut |
| Are metadata and extents low-memory under npm? | Entire large+tiny-file installation and one coherent Commit, object/file counts, allocated capacities, backing/index/cache/FD/cgroup accounting |
| Do writes and reads progress during Commit? | Real overlapping file/metadata save and G1 writes, read pins, append/truncate/rename and failures; service admission pressure and separate lifecycle progress |
| Do repeated Commits remain incremental? | G-relative edits after own exact successful Commit; piece/index depth, normalized change count, retained graph/disk growth and cleanup backlog |
| Is multi-Workspace behavior competitive? | Same declared lanes, selection and admission semantics, explicit refusals, wall time and API-time attribution, per-consumer and per-process resources |
| Is disk-backed memory bounded during overwrite/open? | Separate append, dense rewrite, read-after-write, executable/cache and mapping cases; resident-page outcomes, not only heap or hint calls |
| Is completion safe under failure? | Partial input, peer loss, unknown publication, lost final response, stale context, retained exact stage and local reconciliation failure; no replay |

This audit ran no builds, tests, benchmarks, mount experiments or measurement
commands. No source/evidence receipts were changed. The open disk quota,
segment/window/FD policy, metadata index format, shared large-generation
surface and mounted cache/mapping proof are prerequisites, not implied
implementation details. [04](04-implementation-and-verification.md) owns the
ordered rounds and verification IDs; [03](03-commit-integration.md) owns
history's still-open qualification and exact completion interpretation.

[v-fuse-tree]: https://github.com/Ephemeral-AI-Lab/layerfs/tree/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src
[v-cache]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/immutable_read_cache.rs#L8
[v-open]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/filesystem.rs#L692
[v-mount]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-daemon/src/main.rs#L1337
[v-frozen]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/frozen.rs#L31
[v-snapshot-pages]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L2763
[v-input]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/snapshot_input.rs#L50
[v-workers]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/changes.rs#L572
[v-spool-hints]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/local_spool.rs#L71
[v-daemon-protocol]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-daemon/src/protocol.rs#L3
[v-remote]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/live_backing.rs#L950
[v-roles]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_transport.rs#L298
[v-sdk]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-sdk/src/client.rs#L275
[v-sdk-route]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/lifecycle.rs#L833
[v-sdk-wire]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/live_backing.rs#L1025
[v-sdk-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L2266
[v-splices]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/file_edit.rs#L181
[v-host-mount]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/host_mount.rs#L88
[v-runtime]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_runtime.rs#L11
[v-callback-admission]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1555
[v-listener]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_transport.rs#L45
[v-admission]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-daemon/src/main.rs#L1853
[v-worker]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/worker.rs#L4
[v-commit]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/remote_commit.rs#L35
[v-backing]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/live_backing.rs#L57
[v-read-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1161
[v-store-cache]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-layerstack-store/src/workspace.rs#L807
[v-fuse-init]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/filesystem.rs#L17
[v-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L420
[v-sdk-cut]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L2194
[v-write-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L996
[v-file-edit]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/file_edit.rs#L5
[v-spool]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/local_spool.rs#L142
[v-write-dispatch]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1644
[v-exchange]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_transport.rs#L647
[v-policy]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/limits.rs#L3
[v-object-bounds]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-layerstack-store/src/objects.rs#L33
[v-spool-retire]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/local_spool.rs#L308
[v-build-remote]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/changes.rs#L368
[v-wire]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_wire.rs#L11
[v-changes]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace/src/changes.rs#L1814
[v-objects]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-layerstack-store/src/objects.rs
[v-complete]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L2604
[v-shutdown]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-daemon/src/main.rs#L1495
[v-transport-failure]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_transport.rs#L175
[v-proxy-client]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/proxy_client.rs#L11
[v-proxy-protocol]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/protocol.rs#L4
[v-proxy-host]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/proxy_host.rs#L27
[v-fsync]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1908
[c-read]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/file/read.rs#L60
[c-request]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs#L8
[c-request-validation]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs#L198
[c-inspect]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/read.rs#L12
[c-bridge]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/lib.rs
[c-connection]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/connection.rs#L25
[c-connection-buffers]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/connection.rs#L162
[c-frame]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/protocol/frame.rs#L30
[c-payload]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/payload.rs#L27
[c-write]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/write.rs#L21
[c-client]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/client.rs#L63
[c-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/owner.rs#L90
