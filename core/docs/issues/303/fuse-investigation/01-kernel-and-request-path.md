# FUSE investigation: kernel caching, request service and mutation bursts

> **Status:** Research; informative and not a product contract.
> Implementation proposals target LayerFS 0.1.7.
> Written 2026-10-05. No product change, build, mounted proof or measurement was
> performed. Source observations use LayerFS design HEAD
> `334fc743751b9a181e670d0601a24fb3169208f9` and unchanged product tree
> `05c00c5d62889ae316bec9ea09dba16e93ba888e` under `core/crates/`.
> Published dependency inspected: `fuser =0.18.0`, upstream source commit
> `9c957f74efe715112049298cdf1d601781829c8d`, as recorded in its downloaded
> `.cargo_vcs_info.json`. Kernel code analysis is pinned to Linux **v6.12**;
> that is a reference implementation, not a verified deployment-kernel identity.

The target supports per-tool-call and per-task Workspaces; per-tool-call is the
expected common mode. A Workspace can serve multiple sequential/concurrent calls
over a long lifetime and Commit incrementally. Execs may be short or long-lived
in either mode, without duration assumptions or automatic lifecycle actions.
One command can perform hundreds of thousands of namespace
changes and arbitrarily long streams of edits. Optimization must reduce service
cost while preserving ordinary Bash, permissions, open-file ownership and the
actual filesystem state captured into history. Kernel writeback remains disabled.
One daemon owns the local SQLite overlay. No mount-wide tree reconstruction,
dependency restoration or ignore filtering is permitted.

Read this with the primary [FUSE proposal](../fuse.md), the qualified
[experiment assessment](../05-fuse-assessment.md), the
[daemon engine](../daemon-sqlite.md) and the
[Commit contract](../workspace-api/commit.md). This investigation identifies
additional prerequisites and distinctions; it does not silently promote a new
profile or change a published API.

Owner lifetime clarification 2026-10-05: native caches persist across calls on
the same mount, while a genuinely fresh mount resets connection cache state.
Both cases require proof. Incremental install preserves the effective view,
valid cache entries and later active mutations; only final explicit unmount ends
the Workspace. Lifecycle overhead is attributed to actual native attach/detach,
not automatically charged as a new mount on every retained-Workspace call.

## 1. Findings that change the implementation plan

1. **Deferred replies are possible with the pinned dependency.** Reply objects
   own the channel and request identity, and can outlive the callback. Keep the
   existing synchronous trait and implement bounded owned jobs; adding an async
   runtime or patching `fuser` is unnecessary for this mechanism.
2. **A reply-send attempt is not an acknowledgement of delivery or application.**
   The pinned reply API returns `()`, logs send errors internally, and exposes
   no kernel-completion fence. Capture must order local publication and send
   attempts without claiming to know whether a process observed success.
3. **A cached write-through mount still has dirty shared-mmap writeback.**
   `FUSE_WRITE_CACHE` identifies a kernel-origin write route, not exclusively
   negotiated `FUSE_WRITEBACK_CACHE`. The current adapter rejects that route
   and ctime-bearing SETATTR. Switching to cached opens requires resolving both.
4. **FLUSH suppression has two materially different implementations.**
   `FLUSH -> ENOSYS` retains kernel-side flush checks before suppressing later
   daemon callbacks; `FOPEN_NOFLUSH` bypasses those checks in the analyzed kernel.
5. **Batch-forget exists in the trait but its argument type is not publicly
   nameable in this package.** The safe practical route is cheap individual
   decrements plus deferred collection, rather than assuming an external
   `batch_forget` override can be written with public types.
6. **Kernel passthrough APIs exist in `fuser 0.18.0`.** The blocker is the
   SQLite/capture architecture, backing-file custody and deployment requirements,
   rather than a missing flag. Experimental A2 ext4 forwarding was not kernel
   FUSE passthrough.
7. **Settings cannot eliminate the mutation count.** With kernel writeback off,
   separate small writes still reach userspace. CREATE, LINK, UNLINK and RENAME
   still need semantically correct changes. Cache policy removes redundant work
   around them; the daemon must make remaining work inexpensive and bounded.

Dependency evidence: [reply ownership/send semantics, lines 1–8 and 107–160](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/reply.rs#L107),
[request dispatch, lines 117–120](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/request.rs#L117),
[private ForgetOne import/module and trait method](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/lib.rs#L429).
Kernel distinctions are detailed in sections 4–7 below.

## 2. Current source and what should survive

The FUSE and Workspace packages are currently excluded from the active core
workspace. Source inspection describes dormant code; it does not establish a
built replacement or performance.

| Current mechanism | Exact LayerFS source | Disposition |
| --- | --- | --- |
| Thin argument checks and Workspace delegation | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 1–7, 136–157, 240–275 | Retain the boundary; replace expensive service beneath it |
| Stable canonical serial mapping, root swapped with inode 1 | [replies.rs](../../../../crates/layerfs-fuse/src/replies.rs), lines 61–75 | Retain; no sequential mount-local reassignment |
| Attribute/errno conversion | [replies.rs](../../../../crates/layerfs-fuse/src/replies.rs), lines 6–49, 77–110 | Retain checked conversion; document portable metadata limits |
| Permission and execution checks | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 45–59, 78–109, 213–216; [mount.rs](../../../../crates/layerfs-fuse/src/mount.rs), lines 209–225 | Retain enforcement; establish proper command identity |
| Zero TTL, writable DIRECT_IO | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 22, 228–234, 961–965 | Replace with promoted cached candidate after coherence prerequisites |
| 128 KiB, two receiver threads, background/congestion 1 | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 113–129; [mount.rs](../../../../crates/layerfs-fuse/src/mount.rs), lines 210–225 | Keep initial profile identity; investigate changes prospectively |
| Reply permits retained through send attempt | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 61–67, 597–605 | Keep ownership through send; do not call it a delivery fence |
| Busy refusal and two reply slots | [coherence.rs](../../../../crates/layerfs-workspace/src/runtime/coherence.rs), lines 518–587 | Replace contention refusal with fair deferred admission |
| Ten-second callback budget | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 21, 66, 556 | Do not carry into queued service as an operation-duration cap |
| Full cached-write rejection | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 565–572 | Distinguish authorized mmap-origin writes from forbidden writeback policy |
| Per-FORGET global collection | [namespace.rs](../../../../crates/layerfs-workspace/src/filesystem/namespace.rs), lines 201–210; [state.rs](../../../../crates/layerfs-workspace/src/runtime/state.rs), lines 429 onward | Replace with indexed ownership decrement and separately scheduled reclaim |
| Whole enumeration cookie maps | [directory.rs](../../../../crates/layerfs-workspace/src/filesystem/directory.rs), lines 99–138 | Preserve resumability, replace resident representation |
| Formatting before disabled trace check | [adapter.rs](../../../../crates/layerfs-fuse/src/adapter.rs), lines 251–255, 551–555; [trace.rs](../../../../crates/layerfs-fuse/src/trace.rs), lines 11–20 | Gate before formatting; count rather than print every callback |

The legacy node and handle limits are 256 and 128 in
[state.rs](../../../../crates/layerfs-workspace/src/runtime/state.rs), lines 15–16.
They cannot remain whole-workload limits. A bounded cache may evict; a live
inode/open/lookup owner must remain addressable through backing until its real
owner releases it. No count ceiling may replace that ownership obligation.

## 3. Request-service design with unmodified fuser

### 3.1 Ownership and runnable service

`Filesystem` is `Send + Sync + 'static`. Replies are owned; the request header,
names, targets and WRITE slice are borrowed from a receiver buffer reused after
dispatch. Copy only bounded arguments and required payload into an owned job,
along with mount incarnation, request ID, handle validation context and the
single-use reply. A WRITE buffer credit must be reserved before copying data.
Do not move a borrowed `Request`, `&OsStr` or `&[u8]` into deferred work.

```text
 FUSE receiver A                  daemon-owned service
 ----------------                --------------------
 read one kernel request
 validate routing/flags
 reserve bounded job credit
 copy borrowed inputs ----------> owned job + single-use reply
 callback returns                        |
 receiver reads again                    v
                                  per-Workspace runnable queue
 two jobs blocked on inode x ------------> wait list for x
 receiver remains available              |
                                  readiness event, no retry of failed SQL
                                         |
                                  fair job selection
                                         |
                                  short overlay transaction
                                         |
                                  publish actual state
                                         |
                                  reply-send attempt
                                         |
                                  release job/payload credit

 base fetch / notification / cleanup service have independent progress paths
 no queued job holds the SQL owner or a Workspace mutation lock
```

Use the normal trait, not one thread per request. The optional experimental
Tokio adapter is unnecessary and is not a bound for total spawned work.
Receiver threads, executor work and Commit construction are separate concerns;
adding FUSE workers does not authorize parallel Commit construction producers.

### 3.2 Bounded queues are backpressure, not unlimited parking

A reply object and owned payload remain resident until serviced. Therefore an
unbounded deferred queue recreates the old buffer problem. Maintain a daemon-wide
byte budget, per-mount fair shares and a bounded runnable window. A waiting request
must not keep a global state mutex, checked-out SQL connection, or reader that
prevents its prerequisite operation from finishing.

The dependency provides no public pause/resume hook for its receive loop. When
the owned-job window is exhausted, callback admission may have to block receiver
threads briefly for credit. This is acceptable resource backpressure only if
executor progress is independent of receiving later RELEASE/FORGET/control
requests. It cannot become a cycle in which queued jobs wait for a future callback
that both blocked receiver threads prevent from being dispatched.

Required design proof: once admitted, every blocked job has a prerequisite that
is already owned by independently runnable service, or it can be cancelled by
the daemon's explicit lifecycle mechanism. Reserve cleanup/control progress;
avoid making a foreground mutation wait for an unreceived last-FORGET to reclaim
space. If that cannot be established with the selected queue wiring, report the
integration blocker rather than return EBUSY or allocate an unbounded queue.

`max_background` concerns background requests such as readahead. It is not an
aggregate cap on synchronous foreground requests from many processes, so it
cannot be the proof for total deferred-job memory.

### 3.3 Cancellation and elapsed time

The actual `FUSE_INTERRUPT` dispatch is TODO and returns ENOSYS. A signal to an
application therefore does not yield a callable per-request cancellation hook in
this pinned crate. A queued daemon job can still be cancelled through explicit
control/lifecycle ownership. That is distinct from observing a kernel interrupt.
Do not claim exact signal-to-job cancellation support or add a dependency patch.

Replace the blanket ten-second callback deadline with explicit lifecycle state,
resource admission and progress/status accounting. A long Bash command and a
large mutation burst have no automatic runtime limit. An explicit caller deadline
is a separate requested contract; measurement watchdogs remain harness-only.
Cancellation before an attempt and cancellation after publication have different
outcomes. After local publication, retain the changed state even if the reply is
lost. Daemon disconnection drains or terminally abandons requests under retained
mount ownership; it must not invent successful Commit or silently kill Bash.

Source: [fuser request lifetime and dispatch](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/request.rs#L41),
[request-header accessors](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/request_param.rs#L14),
[Linux FUSE connection controls](https://docs.kernel.org/filesystems/fuse/fuse.html#control-filesystem).

## 4. Capture must use the actual published view

The phrase "acknowledged frontier" needs a precise meaning. Kernel successful
`write(2)` return implies that the daemon accepted the mutation first. Atomic
local publication does not conversely guarantee that the process received a
successful return. A request can mutate SQL and then lose its reply.

```text
 job state:

 QUEUED -> ATTEMPTED -> LOCALLY PUBLISHED -> REPLY ATTEMPTED -> RETIRED
                |             |
          definite failure    +---- actual bytes/names already changed
                |                        |
          error reply                   capture must preserve them

 kernel delivery / kernel inode update / process return
                    are not exposed as fuser reply completion events

 Workspace operation order:

 [ admitted mutations before boundary ] [ CAPTURE BARRIER ] [ later mutations ]
            |
      finish/resolve attempts
      order reply-send attempts
            |
      freeze generation G, existing owned rows
            |
      later mutations publish into G+1
```

Proposed contract: capture the **locally published mutation frontier**, ordered
with preceding admitted mutation attempts and reply-send attempts. Include every
actual publication belonging to G, including a publication whose reply could
have been lost. Retain unresolved publication outcomes rather than guessing
that an error reply means no effect. The exact operation-order barrier belongs
to the Workspace scheduler, not the kernel TTL or a callback timeout.

Reads already using stable roots remain valid. A barrier must not stop replies
needed to release kernel locks or prevent independently runnable service. This
terminology preserves all successful acknowledged writes while avoiding an
unimplementable promise to include exactly those writes whose success the
application saw. Capture itself changes ownership, not names, attributes or bytes;
it needs no kernel-cache invalidation if that equivalence is proved.

`reply.written()` and `reply.ok()` consuming the object are evidence of a send
attempt in this API, not evidence of later kernel processing. The current adapter
already documents that distinction at lines 597–598. No requirement should
reintroduce a delivery-controlled rollback or temporary whole-overlay copy.

## 5. Cached write-through and coherence

### 5.1 What the kernel already does

Cached write-through sends each ordinary write to userspace and maintains cached
file pages. Keep that route and remove redundant per-WRITE reverse invalidation
once its integration is proved. It does not combine ten thousand distinct tiny
write syscalls into a single daemon mutation.

The analyzed kernel protects existing inode attributes from certain old replies.
GETATTR and LOOKUP capture an `attr_version`; ordinary write completion increments
the inode version, and reverse inode invalidation does too. Applying an older
attribute response is rejected when the inode has a newer version. Do not infer a
stale-attribute bug merely because a reply arrives late. Entry bindings, new inode
creation and page-cache data have different ordering paths and need their own
proofs.

Sources: [kernel I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html),
[GETATTR version capture/application](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L1254),
[attribute version checks and reverse invalidation](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/inode.c#L298),
[write completion attribute update](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L1111).

### 5.2 FUSE-origin mutations

```text
 process         kernel                 daemon
 -------         ------                 ------
 write() -> lock/cache request -> bounded WRITE job
                                         |
                                  atomic local publication
                                         |
              <------ WRITE reply attempt
              update cached bytes / size / invalid attribute masks
              unlock / return

 daemon does not throw away this page with a reverse invalidate per WRITE
```

For CREATE/LINK/UNLINK/RENAME/SETATTR, preserve existing semantic validation and
kernel-managed entry/attribute updates. Stable serials give hard-link aliases a
common inode identity. Atomic rename must change both bindings under one short
transaction, preserving source/destination ownership and whiteouts. The kernel
invalidates parent attributes after directory changes even with long TTL;
permissions-on GETATTR traffic around mutations therefore cannot all be removed
by increasing the lifetime.

The [kernel namespace mutation paths](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L983)
and the existing adapter's rename comment at lines 843–845 support avoiding
reverse entry notifications from the same related callback. Long TTL is not a
reason to add one notification to every namespace mutation.

### 5.3 Non-FUSE changes: practical ordering proposal

A non-FUSE mutation is acknowledged only after required invalidation completes.
Use an independently runnable notification service. Never call entry invalidation
while holding locks needed by a related callback or waiting for that callback's
reply. Kernel reverse entry invalidation takes the parent inode lock; stopping
all affected callbacks while synchronously notifying creates a deadlock.

```text
 stale read/lookup jobs            non-FUSE mutation         notification service
 ---------------------            -----------------         --------------------
 snapshot epoch E
 prepare bounded result
      |
 short affected-key send gate
 validate E / serialize send  ---> wait for older send attempts
 reply attempt
 gate released
                                  publish epoch E+1
                                  enqueue exact notifications ------>
                                  release SQL + key gates             |
 new jobs see E+1 and can reply                              kernel locks/pages
                                                                       |
 kernel cached old readers may observe old state             complete invalidation
 while control operation remains in progress                            |
                                  <-------------------------- result
                                  acknowledge control operation
```

This is a proposed algorithm, not a proved implementation. It prevents an old
prepared reply from being intentionally sent after the mutation's notification.
Keep the final generation check and send attempt in one short serialized gate;
avoid blocking base fetch or SQL under that gate. A late result that was never
published can be reconstructed from current state through a declared scheduling
path; do not replay a mutation whose outcome failed or became uncertain.

The kernel's attribute versions, VFS parent locking and locked-page invalidation
then participate in completing the ordering. A send attempt alone is insufficient
to claim that those kernel steps finished. Verify positive/negative lookup races,
new inode installation, delayed READ replies, alias writes and page-tail truncation
on the actual supported kernel. Entry invalidation and inode invalidation differ:
use the former for names and the latter for attributes/data; do not assume one
automatically covers every alias and cached directory enumeration.

The pinned `Notifier` returns `io::Result<()>` and treats missing cached entries
as harmless. Notification failure retains the published state and a failed/pending
coherence owner; no SQL rollback on a guess. Dirty mapped pages make successful
notification an especially insufficient proof of replacement semantics: the
reference reverse-inode path does not return the page invalidation function's
result to userspace. Destructive out-of-band data replacement while writable
mappings remain is unresolved; refuse that operation in the first supported
slice rather than discard dirty data.

Sources: [libfuse notification deadlock rules](https://libfuse.github.io/doxygen/fuse__lowlevel_8h.html),
[kernel reverse entry invalidation](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L1360),
[kernel reverse inode invalidation](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/inode.c#L517),
[pinned Notifier](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/notify.rs#L62).

### 5.4 Capture and install are not out-of-band file replacement

Known-success install must preserve the effective union view, including later
G+1 changes, inode identity and metadata. Changing which immutable root/overlay
rows supply identical bytes does not require invalidating cached data. Prove
that equivalence instead of flushing every touched or visited inode. Failure,
conflict and uncertainty must also preserve the live view; a full remount or
whole-tree cache sweep is not a hidden recovery shortcut.

## 6. Shared mmap is a separate compatibility route

Kernel writeback **policy off** does not mean no dirty kernel pages can exist.
Cached MAP_SHARED stores dirty pages and can send WRITE_CACHE requests later.
The current adapter rejects those requests at lines 565–572. Kernel-origin
requests can use non-user credentials, so the blanket request-uid guard also
needs a mount/handle-owned authorization path. Verify inode/handle routing and
retained writer custody; do not simply permit every uid-zero request.

The analyzed writepage path marks `FUSE_WRITE_CACHE`, retains an open-file
reference, and submits background writes with `nocreds`. It also flushes times
through a SETATTR including ctime. Existing `setattr` rejects ctime because it
has no persisted representation. Accepting the kernel's synchronization route
while retaining the declared portable metadata contract needs an explicit design.

```text
 ordinary write()                   shared mapped store
 ----------------                   -------------------
 WRITE request -> daemon publish     CPU changes cached page
 WRITE reply -> syscall returns      no daemon request at that instant
                                     |
                              msync/fsync/reclaim/last-map close
                                     |
                              kernel-origin WRITE_CACHE + time updates
                                     |
                              daemon publish / reply

 capture sees local publications, never unreported dirty bytes in kernel memory
```

Last mapping close calls `write_inode_now(WB_SYNC_ALL)`, but FUSE copies original
folios to temporary buffers and ends original page writeback after queueing.
RELEASE can be asynchronous and delayed until outstanding requests finish.
Therefore process exit or stdio EOF alone is not established here as an exact
daemon-publication fence. FSYNC's reference path explicitly waits for writes
before its daemon callback. Any stronger generic Exec-to-Commit drain requires
an actual mounted proof and explicit owner/handle accounting. No command-specific
preparation hook or implicit Commit is introduced.

Sources: [kernel mapped-page route and VMA close](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L2492),
[writepage WRITE_CACHE/no-credentials route](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L2067),
[original-page writeback completion](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L2175),
[time-flush SETATTR](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L1868),
[generic synchronous inode writeback](https://github.com/torvalds/linux/blob/v6.12/fs/fs-writeback.c#L2816).

## 7. Request-removal candidates and their real limits

### 7.1 Long positive TTL and KEEP_CACHE

The owner-promoted 60-second TTL/cached-open candidate is the starting point.
It removes repeated lookup/stat/read work inside a call. A fresh mount has fresh
kernel dentries, attributes and file page-cache identity; this cache cannot be
assumed to survive each tool call. Across calls preserve canonical serials,
unchanged stat identity, the actual `.git/index` and complete dependency/build
caches. Bounded daemon caches can share authenticated immutable inputs across
mounts; keys include root/policy/authority context. Overlay state has a separate
generation/version identity and must not enter an immutable cache by accident.

`ReplyEntry::entry_with_ttls` can split entry and attribute lifetimes. CREATE and
DirectoryPlus APIs in this pin use one TTL argument for both; do not advertise
independent TTL control on every reply kind. All cached page/slab residency is
accounted alongside daemon/SQLite memory. No bounded heap claim excuses kernel
cache proportional to a file or a visited namespace.

### 7.2 Negative entries and symlinks

A missing name can be answered as a successful entry with node ID zero and a
positive entry TTL. ENOENT alone does not install that lifetime. Use normal
permission and overlay-before-base resolution, no lookup reference for node zero,
and invalidate names for authorized non-FUSE creation. Cache misses for immutable
bases with their root identity; a miss from one Workspace must not hide another's
new file. Negative caching helps repeated dependency-probing; it does not suppress
exclusive-create validation.

`FUSE_CACHE_SYMLINKS` is another available, currently disabled flag. Stable
immutable symlink targets per never-reused serial make it a plausible candidate
for the complete dependency tree. Replacement changes the name-to-inode binding;
non-FUSE change still needs coherence. This is an investigation with no measured
speed claim, not permission to filter symlinks from Init.

Sources: [node-zero timeout handling](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/dir.c#L383),
[published initialization flags](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/ll/flags/init_flags.rs).

### 7.3 Adaptive READDIRPLUS and cached directory listings

Negotiate both DO_READDIRPLUS and READDIRPLUS_AUTO if the deployed kernel supports
them. The reference adaptive rule starts with plus and can use later lookup
activity as advice; it does not guarantee plus for every page. Current READDIR
already obtains attributes then discards them, making reuse attractive, but a
larger plus entry reduces names per reply and creates lookup ownership.

Increment lookup ownership only for entries actually inserted into a successful
reply attempt, excluding dot entries and zero-node entries as appropriate.
`ReplyDirectoryPlus::add` reporting a full buffer means that candidate entry was
not inserted. Keep ownership for a possibly delivered reply; failed-send logging
does not permit decrement-on-guess. Batch SQL bookkeeping for the bounded reply
does not require resident bookkeeping for the full directory.

`FOPEN_CACHE_DIR` can reuse enumeration within the same call. Directory mutations
advance kernel directory version; non-FUSE changes require explicit coherent
invalidation. It can help a multi-stage command that revisits a directory, so its
usefulness is not limited to mounts lasting multiple tool calls. It still adds
kernel memory and cannot warm a fresh mount. Keep exact resumed offsets with one
reply/cursor window per handle; old-offset seeking may require deliberate replay
or backed cookie records, rather than retaining every listed name in RAM.

Sources: [adaptive rule](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/readdir.c#L16),
[directory version cache checks](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/readdir.c#L473),
[DirectoryPlus reply](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/reply.rs#L742).

### 7.4 FLUSH and fsync

Retain OPEN/RELEASE initially. In a route with no close-deferred errors, no
userspace lock cleanup and already-published writes, `FLUSH -> ENOSYS` is a
candidate to eliminate repeated callbacks. Kernel file flush still runs its own
checks before learning the no-flush state. This preserves more of the kernel's
behavior than setting `FOPEN_NOFLUSH`, which returns early before those checks.
The latter needs a separate mmap/error-lifetime proof and is not the first choice.

Promoted no-op FSYNC/FSYNCDIR compatibility means no extra daemon durability work.
It must not mean skipping the kernel's ordinary mapped-page/write synchronization.
The reference FSYNC path does that synchronization before calling the daemon,
even after it learns an ENOSYS no-fsync state. There is no Workspace crash-durability
claim or backing `fsync` added. A caller demanding persistence needs the actual
published Commit/storage profile contract.

Source: [kernel FLUSH and FSYNC paths](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L501).

### 7.5 No-open/no-opendir and FORGET

Handle-free modes remove request traffic but also remove daemon open/release
signals, per-open validation and directory cursor state. They are a later
candidate, conditional on lookup-based inode custody, append/execute validation,
CREATE handle behavior, open-unlinked survival and terminal drain. A first FORGET
is not last ownership: subtract its actual nlookup count, preserve hard-link
aliases and active operations, and never recycle a serial.

The trait's default batch-forget loops individual forget calls. At this dependency
pin, `ForgetOne` is public inside a private module and privately imported into
`lib.rs`; it is not reexported. Thus an external adapter cannot name the advertised
override argument through a public path. Use an O(log n) indexed per-inode
decrement and coalesced deferred collector behind the default loop. Never scan
all live nodes for each decrement. Direct override needs a usable published API;
third-party patching is forbidden.

Not every referenced inode is guaranteed a FORGET on unmount. Terminal namespace
retirement must wait for callback/native-session/handle fences and then retire
remaining mount lookup owners as a group. No-open does not remove this rule.

Sources: [kernel open/release state](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L126),
[ForgetOne visibility](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/lib.rs#L34),
[forget-count/unmount contract](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/lib.rs#L418).

## 8. Request size and concurrency require actual negotiation

| Knob | Pinned API/implementation | Correct interpretation |
| --- | --- | --- |
| max_write | `KernelConfig::set_max_write` | Request byte window; crate upper bound 16 MiB, not the deployed kernel limit |
| max_read | `MountOption::CUSTOM("max_read=...")` | Linux mount option; no public KernelConfig max_read setter in this pin |
| max_readahead | `set_max_readahead` | Limited by the value offered by kernel INIT |
| max_pages | Private calculation from max(max_write,max_readahead)/host page size | No public independent max_pages setter; kernel clamps negotiated pages |
| max_background/congestion | Public nonzero setters | Background admission; not total foreground request count |
| n_threads/clone_fd | `Config` fields, Linux support | Receiver concurrency and device contention; not SQLite multiwriter support |
| ASYNC_READ/BIG_WRITES/MAX_PAGES | Defaults, conditional on support | Availability is negotiated; no writeback implied |
| PARALLEL_DIROPS | Explicit capability | Removes additional FUSE directory serialization; ordering moves into daemon |
| ATOMIC_O_TRUNC | Explicit capability | Can merge open/truncate protocol work only after atomic semantics are implemented |
| io_uring/request-timeout bits | Constants exist | No actual io_uring request transport or timeout configuration path found in the selected normal Session |

The v6.12 reference kernel caps negotiated max_pages at 256; with 4 KiB pages that
is at most 1 MiB of pages per such request. Record actual kernel version, page
size, offered/selected flags and final limits. Do not infer that fuser's 16 MiB
setter permits 16 MiB requests on this kernel, or that changing max_write alone
raises max_read. The LayerFS 128 KiB validation and Workspace windows must change
together if a prospective larger profile is selected.

Larger requests help large sequential I/O, not one-byte syscalls. Their SQL work
must remain bounded even after dense fragmentation; total file/edit history
cannot determine one request's transaction size. More receiver threads help only
when daemon service and backing resources can run productively. `clone_fd=true`
is available but requires kernel/device support and is not intrinsically faster.

The pinned receiver allocates `vec![0; 16 MiB + 4096]` per event-loop buffer,
independent of the smaller negotiated request size. Two loops have two such
buffers, plus a transient handshake buffer. Actual resident initialization and
per-call allocation cost need attribution; do not describe this as only a
128 KiB per-thread cost or patch the dependency to shrink it. Many concurrent
mounts multiply that cost. Compare a prospective one-receiver-plus-deferred-service
profile only after count-driven evidence, preserving the promoted profile as
the recorded starting point.

Sources: [KernelConfig implementation](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/lib.rs#L299),
[Session receiver threading](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/session.rs#L257),
[fixed receive buffer](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/read_buf.rs#L7),
[kernel max-pages clamp](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/inode.c#L1301),
[kernel page limit](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/fuse_i.h#L39).

## 9. Copy acceleration, passthrough and unsupported shortcuts

### 9.1 CopyFileRange is a serious candidate

The ordinary `Filesystem::copy_file_range` callback exists; LayerFS currently
inherits ENOSYS. Implementing it could remove payload transfer through the
application for same-mount copies. Do not equate it with a hard link: the
destination needs an independent inode and later writes must not modify source.

```text
 ordinary copy_file_range syscall, same mount
                  |
        validate handles/ranges/permissions
                  |
        retain stable source revision/root
                  |
       +----------+------------------------------+
       |                                         |
 full immutable file + suitable destination       other source/range
       |                                         |
 attach same canonical file root                  bounded copy-plan/byte windows
 to independent destination inode                 stable backing + atomic publication
       |                                         |
 later writes create independent overlay          no recursive live-source dependency
       +-------------------+---------------------+
                           |
                publish destination size/mtime/data
                           |
                      reply byte count
                           |
                kernel invalidates destination pages
```

Whole immutable-file mapping reuse can avoid rereading the payload when the
source is a proven unchanged canonical file root. Mutable and partial copies
require stable source ownership, overlap rules, holes, destination truncation/
extension, O_APPEND constraints and cancellation semantics. A bounded copy plan
may need backing plus a short pointer publication; not a single giant SQL
transaction proportional to the file. Copy chains must not grow with successive
operations or Commits.

The callback accepts a u64 length but `ReplyWrite::written` is u32. Return a
valid bounded positive partial count as required and let the caller continue;
this is a per-request representation limit, not a 4 GiB file limit. Same mount
is significant: the reference kernel checks matching superblocks. Cross-mount
copy can take the kernel's normal byte route; no extra command interception.

Kernel v6.12 itself documents weaker partial-page behavior when COPY races with
shared-mmap writes. Identify supported-kernel behavior in qualification; daemon
correctness cannot repair every kernel cache race through metadata design.
Current content `apply_edits` reads replacement bytes; it does not expose a
general public zero-I/O immutable slice-splice constructor. An overlay copy plan
does not automatically establish payload-free Commit construction for partial
copies. Use the current public content contracts or report the required API work.

The JuiceFS C10 observation in #306 permits copy acceleration but has no syscall
trace proving that route. It motivates investigation, not a claimed LayerFS
speedup or copy bandwidth. [Qualified observation](https://github.com/Ephemeral-AI-Lab/layerfs/issues/306).

Sources: [fuser CopyFileRange callback](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/lib.rs#L986),
[kernel copy/cache handling](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L3190),
[public content contracts](../../../../../cluster_one_handbook.md#32-construction-read-and-filesystem-apis).

### 9.2 Reflink is not automatically exposed by generic ioctl

The analyzed FUSE file operations expose CopyFileRange, not remap_file_range.
Linux handles FICLONE/FICLONERANGE through the VFS clone path before the ordinary
filesystem ioctl callback. Merely adding `Filesystem::ioctl` is not a verified
reflink implementation. Do not advertise native reflink from the current pin.
[Kernel FICLONE dispatch](https://github.com/torvalds/linux/blob/v6.12/fs/ioctl.c#L850).

### 9.3 Kernel passthrough and splice

The crate has safe backing registration and opened-passthrough APIs. Linux
passthrough requires a suitable backing file and kernel/config/capability
negotiation. Current kernel documentation requires CAP_SYS_ADMIN for registration.
Canonical compressed/delta-encoded objects and local SQLite payload cells are
not ordinary per-inode byte files to hand to the kernel. Mutable passthrough
would bypass the daemon mutation frontier, making capture incomplete.

Even read-only opens in a writable inode are not automatically safe: an existing
passthrough mapping can conflict with later overlay writes and requires a stable
whole-byte backing file, exact lifetime custody and cache-mode transitions.
Creating those files at mount would violate fast bootstrap. Keep kernel
passthrough out of the first architecture; do not claim unavailable crate support.

Splice flag constants likewise do not establish a splice transport. The normal
pinned Session reads `/dev/fuse` into owned memory and sends iovecs with writev;
there is no public FD-backed ReplyData/send-splice method in that route. Reduce
avoidable LayerFS copies and disabled-trace allocations first. SQLite reads,
authentication and decompression still require buffers. io_uring/DAX also need
actual supported request/transport paths, not enabling a named bit.

Sources: [kernel passthrough requirements](https://docs.kernel.org/filesystems/fuse/fuse-passthrough.html),
[fuser safe backing/open APIs](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/reply.rs#L367),
[actual read/writev channel](https://github.com/cberner/fuser/blob/9c957f74efe715112049298cdf1d601781829c8d/src/channel.rs#L29).

## 10. Disposition, evidence and proof matrix

"Candidate" means a proposed product mechanism. "Available" means source/API
support, never deployed or measured qualification. A2 is forwarding ext4,
with no overlay/capture/Commit; all historical numeric comparisons retain their
INELIGIBLE cache qualification. Coupled A2 settings do not isolate causal gains.

| Mechanism | Expected work removed | Evidence | Disposition | Required proof |
| --- | --- | --- | --- | --- |
| 60 s TTL + KEEP_CACHE, writeback off | Repeated LOOKUP/GETATTR/READ within call | Owner promotion; qualified A1/A2 request observations | Starting candidate | Full mutation/attribute/page coherence; mmap route; aggregate memory |
| Stable serial/stat and retained Git/dependency caches | Repeated per-call content scans/preparation | Existing serial source; qualified A2 identity counts | Retain | Two mounts and post-Commit identity; all ignored files present |
| Bounded shared immutable base cache | Repeated authenticated base acquisition | Natural immutable keying; not measured in A2 | Candidate | Authority/root keys, eviction, shared memory attribution |
| Deferred owned jobs | Occupied receivers, lock-held waits, contention EBUSY | Available Send replies | Required service change | Two inode waiters + unrelated I/O; queue saturation; no dependency cycles |
| No global scan on FORGET/RELEASE | Collection amplification during bursts/teardown | Existing State.collect source | Required bookkeeping change | Counts and scanned owners; last-reference correctness; terminal retirement |
| Negative entries | Repeated missing-name lookups | Qualified experiment request counts; node-zero protocol | Candidate | Create/rename after negative hit; permission checks; no lookup count |
| Adaptive READDIRPLUS | LOOKUP after enumeration | API available; historical always-plus results mixed | Investigate | Names/reply, total calls, ownership, concurrent mutation/cookies |
| CACHE_DIR / CACHE_SYMLINKS | Repeated enumeration/readlink within call | APIs available; no product proof | Investigate | Directory mutation/version; target replacement; kernel memory |
| FLUSH -> ENOSYS | Repeated daemon close callbacks | Qualified count experiment | Investigate first | Locks/deferred errors absent; mmap/close/release handling |
| FOPEN_NOFLUSH | Same plus kernel flush checks | API available; no equivalent proof | Defer | Lost synchronization/error-report behavior explicitly resolved |
| ATOMIC_O_TRUNC | Separate truncate request | API available; current flags reject it | Investigate | Permission-before-effect, atomic size/content, failure and cached tails |
| Larger request window | Sequential I/O round trips | API available; no isolated experiment | Investigate | Effective negotiation, per-request memory/SQL/journal bound, request counts |
| More background/receiver depth, cloned fd, PARALLEL_DIROPS | Idle I/O service gaps/device serialization | APIs available; concurrency Stage C unrun | Investigate after service rewrite | Fairness, SQL ownership, locks, total buffers, deployment support |
| Whole immutable CopyFileRange reuse | Application payload reads/writes | API available; #306 diagnostic motivation | High-value candidate | Independent destination, source stability, ranges, sparse/cached/Commit behavior |
| No-open/no-opendir | OPEN/FLUSH/RELEASE | Qualified handle-free experiment with permission caveats | Defer | Exact FORGET lifetime, retained orphans, append/exec/cursor/drain semantics |
| Kernel writeback | Small write batching | Conflicts with accepted-write capture contract | Reject | No enabled route in target profile |
| Mutable kernel passthrough | Most data callbacks | API available; architecture conflict | Reject for first architecture | Would require different mutation capture/backing design |
| Remove default_permissions | Parent attr checks | Experiment allowed reading mode-000 file | Reject | Enforcement remains required |
| Fixed CPU pinning | VM wake-up overhead | Mixed/confounded VM diagnostics | Reject as policy | Target host attribution before any placement choice |

## 11. Why large bursts remain load-bearing engine work

Let M be actual namespace mutations, W be daemon WRITE requests, and B be
changed payload bytes. TTL can reduce repeated inspection; it cannot make M
or W vanish. A load-bearing target is bounded foreground work per request,
indexed namespace access, and streaming total work proportional to the changes
and required tree reconstruction. One call producing a million files legitimately
does more work than one producing one file. No vector, reply table, cookie map,
edit buffer or transport packet must cap that total.

```text
 one Bash call with huge mutation burst
                 |
    eliminate repeated metadata/read requests
                 |
    remaining M creates/links/removes + W writes
                 |
    bounded owned window -> short indexed SQL publication
                 |
    overwrite cells / names in active generation
    no per-mutation whole-tree copy or full owner scan
                 |
    capture existing ownership at explicit boundary
                 |
    single producer streams changed roots/bytes to Save/history
                 |
    install equivalent live view; automatic batched reclaim
```

Hard-link replay is especially instructive: it already avoids copying payload,
yet was slow in A2. Remaining namespace and permission/kernel costs matter.
CopyFileRange can reduce byte work in a copy replay but still creates independent
inodes and bindings. No request optimization fixes a daemon slot held across
Exec, SQL connection held across a whole Save, unbounded edit normalization,
whole-directory resident sorting, or full-tree work at each mount.

## 12. Implementation slices and required diagnostics

1. Establish actual deployment kernel/INIT identity, ordinary permission/flag
   behavior and complete-root mount. Record lifecycle fixed work without scans.
2. Implement request ownership/scheduling, local-publication capture terminology,
   cheap lifecycle bookkeeping and disabled-diagnostic allocation removal.
3. Implement cached write-through coherently, including mmap-origin writes/time
   updates or an explicitly supported compatibility boundary. Retain permissions.
4. Add negative caching and investigate adaptive enumeration/symlink/directory
   cache with exact references and bounded cursor representation.
5. Add immutable copy reuse with bounded source custody; then investigate larger
   windows/concurrency based on request counts and resource attribution.
6. Consider handle-free or transport alternatives only when their missing
   lifetime/API/deployment proofs are resolved through supported published code.

Collect counts by callback class, bytes/request, queue bytes, parked/runnable
jobs, receiver occupancy, inode/dentry rows touched, owner rows scanned, metadata
and payload BLOB bytes, reply copies, notification waits/failures, kernel file/slab
cache, SQLite pager/journal/scratch and reclaim debt. Count both foreground and
deferred work; fast Exec followed by slow Commit/unmount is visible in the
per-call lifecycle. Do not print every callback during a timing phase.

Mounted proofs must cover delayed reply races; cached truncation/regrowth;
hard-link/rename replacement; negative creation; partial enumeration resumes;
overlapping writes/O_APPEND; dirty MAP_SHARED with retained descendants; failed
reply/disconnect after local publication; open-unlinked files over repeated
Commits; two guarded-inode jobs plus unrelated work; saturated queues; simultaneous
Workspaces and Commits; and terminal mount ownership. These are proposed proof
cases, not a newly run or frozen performance selection.

Before future measurements, follow the repository's one-sample, complete-command,
cache-state and evidence rules. Historical #305/#306 observations remain
diagnostics with their original limits and failures. No new measurement,
third-party edit, source build, branch, commit or push accompanies this report.
