# FUSE optimization for short-lived and persistent Workspaces

> **Status:** Research; informative and not a product contract.
> Investigation dated 2026-10-05. Product/source checkpoint
> `334fc743751b9a181e670d0601a24fb3169208f9`; its `core/crates` tree is
> `05c00c5d62889ae316bec9ea09dba16e93ba888e`, identical to the product baseline
> `f96d97651`. This report contains source analysis and engineering proposals,
> not implementation, new measurements or release qualification.

The owner requested a parallel investigation of highly optimized FUSE for the
smallest supported granularity: one tool call, potentially performing very many
mutations. Three investigations cover the kernel/request path, mutation engine
and complete lifecycle. This synthesis records their combined implications.
The [primary FUSE design](fuse.md), [engine](daemon-sqlite.md) and operation
documents remain the design authorities. Research alternatives below do not
silently change the promoted mount profile or persistence profile.

Owner clarification 2026-10-05: one-call orchestration is the minimum capability,
not a one-call-per-Workspace limit. A Workspace may serve many sequential or
concurrent calls for a long lifetime and Commit incrementally. Lifetime is
independent of any call/task; neither call completion nor Commit unmounts it.
The fresh-mount analysis below describes the smallest-lifetime case. Persistent
same-mount cache and ownership behavior is equally required.

Both per-tool-call and per-task modes are required; per-tool-call is the expected
common case. Short or long-lived Execs are possible in either. Native setup cost
matters for frequent fresh Workspaces, but its relevance must be attributed to
actual lifetime, never justified by an assumed short command. No mode or command
class implies an automatic timeout, Commit or teardown.

The most consequential new findings are:

| Finding | Consequence for per-call design |
| --- | --- |
| Unmodified pinned fuser supports deferred owned replies | Fair shared service can free dispatch workers without a dependency patch |
| Its INTERRUPT callback is not exposed as implemented cancellation | Request cancellation must have an honest supported boundary; a parked reply is not automatically cancellable by a Bash signal |
| Receive buffers are sized independently of 128 KiB negotiation | Per-mount allocations/workers need explicit accounting and a lifecycle floor diagnostic |
| Individual FORGET calls can repeatedly scan Workspace state | Long TTL can shift request savings into expensive disposal unless ownership release changes |
| Cached dirty mmap can emit FUSE_WRITE_CACHE without negotiated writeback | KEEP_CACHE requires correcting the current flag refusal and kernel-origin identity/handle handling |
| A stored non-file rebind can trigger whole-base alias validation | Even a small rename Commit can scale with N; streaming alone cannot remove this mandatory walk |
| Validity masks can fragment base READ demands | Bounding WRITE cells is insufficient without bounding the corresponding READ plan |
| SQLite UPDATE expires a BLOB handle even when another column changes | Data/mask metadata sequencing and handle closure must be transaction-local |

## 1. Conclusion and scope

The useful combination is the existing adapter's semantics and ownership,
the promoted cached profile, and a redesigned bounded mutable engine. Its
performance depends on reducing repeated requests and work per mutation, removing
unconditional whole-base scans from mount/capture, and removing repeated
whole-state collection during teardown. Localized Commit work should follow
affected paths plus necessary validated topology/release work; index depth,
base-page acquisition and boundary reconstruction still cost work. Native kernel
disposal can scale with visited state. Optimizing only Exec can move the cost to
Commit or unmount.

Fresh mount exposes the complete committed root; further calls on the same
Workspace use its complete current live view, including uncommitted changes.
Both include `.git` with index and objects, ignored files, dependencies, symlinks,
caches and build outputs. Ordinary Bash
receives no command-specific filesystem path, implicit Commit or automatic
runtime timeout. One daemon owns one overlay database, initialized once before
readiness. Terminal unmount includes logical close and automatic cleanup.
Construction retains the repository's single-producer rule. Kernel writeback
remains off. No optimization bypasses acknowledged-write capture or permissions.

```text
 one-time full-root acquisition + runtime/daemon readiness
                           |
                           v
 mount complete Rn -> ordinary Exec -> explicit Commit -> terminal unmount
        |                  |                 |                  |
 bounded root bind   many ordinary       captured final      fence actual
 native attach       filesystem ops      affected state      native owners
        |                  |                 |                  |
        +----- shared daemon services ------+----- bounded SQL reclamation
                           |
next call binds known complete Rn+1; no whole-root copy or reinstall
```

```text
 persistent W: mount R0 once
                  |
          calls A/B/C on current live view
                  |
          Commit C1 -> install R1, preserve later active changes
                  |
          further calls D/E + Commit C2 + more calls/Commits
                  |
          explicit final unmount; no per-call remount
```

Every incremental Commit captures the shared Workspace frontier. Changes from
concurrent calls can be included; per-call isolation/attribution is not supplied
by sharing a mount. Later calls see the live view, including uncommitted changes.

## 2. The cost model

Track separate quantities rather than treating Workspace size as the cost of
every operation:

| Quantity | Meaning | Relevant work |
| --- | --- | --- |
| N | Entries in the complete committed namespace | Backed base size; whole-tree walks necessarily visit their output |
| M | Filesystem mutations actually requested by the command | FUSE round trips, validation, atomic overlay updates |
| B | Bytes actually supplied to writes | Data ingestion; repeatedly overwritten bytes still cost ingestion |
| K | Distinct captured namespace/inode keys affected | Captured enumeration, final metadata construction and reference checks |
| D | Final replacement bytes/runs supplied to construction | Edit normalization, canonical construction and Save |
| U | Distinct immutable tree pages that must be read/rebuilt | Changed paths, boundary rebuilds, release/topology work |
| W | Live Workspace owners | Mount sessions, descriptor custody and scheduling state |
| Q | Bounded admitted jobs/bytes | Transient resident work, backpressure and service fairness |
| G | Unreachable local rows/pages awaiting reclamation | Sustained disk capacity and background service |

M can greatly exceed K: rewriting one file many times need not produce one
canonical version per write. K can still be enormous: creating a million files
requires representing the final million files. A million create/delete pairs
can have little final namespace delta while still requiring every requested
operation to be executed correctly. Cancellation of final bindings must preserve
open descriptors, inode identity, hard-link counts and any metadata changes.

```text
 raw ordinary mutations                         captured final-state inputs
 write A, write A, write A -----+                final bytes/runs for A
 create X, rename X to Y ------+--> active view --> final name Y + inode value
 create Z, unlink Z -----------+                no binding Z; orphan if open
                                                    |
                                          bounded canonical construction
```

This is final-state normalization, not replaying a chronological syscall log as
`EditSequence`. Current-result edit coordinates and replacement applicability
must be obeyed. Exact CAS reuse is not free work: lookup, authentication and
comparison can still be necessary. Unchanged canonical subtrees can be retained
only where the construction algorithm proves they are unaffected.

Per-call latency follows the actual critical path through root binding, native
attach, execution/output completion, capture, construction/Save, publication,
install and native teardown. Callback spans and overlapping service spans must
not be summed as a measured decomposition. No latency number is claimed here.

## 3. Caches across and within calls

A fresh native FUSE connection has fresh kernel cache state. Stable `st_ino`
does not transplant the previous connection's inode/page caches. Long entry and
attribute lifetimes therefore remove repeated requests within a call; daemon
immutable-object/metadata caches and persisted application caches provide the
cross-call mechanisms.

When calls retain the same Workspace, they retain its native connection and
valid kernel cache entries. Long TTL, directory/symlink caches and KEEP_CACHE
can therefore help across those calls too. Incremental install is not a new
mount: preserve cache coherence and visible identity as the base advances.
Old-version/orphan/reclaim state still needs bounded sustained progress, and
cache expiry must never become a Workspace lifetime limit.

```text
 call n kernel cache          daemon: bounded immutable cache       global root
 [dentries/attrs/pages] -----> [Store/profile/authority + object ID] ---> Rn
         |                                   |
    native unmount                     eligible reuse/eviction
         |                                   |
 call n+1 kernel cache -----------------------+---------------------> Rn+1
 [initially new]               only exact compatible entries reused

 committed .git/index / dependencies / caches are filesystem data in Rn+1
```

Positive and negative base lookup results must include the immutable directory
or tree identity in their keys. An unchanged inode serial alone is insufficient
for a name lookup or a file's current content. An exact base-cache hit does not
authorize bypassing the Workspace's overlay, current root or caller authority.
Mutable results require incarnation/version validation and atomic cache update;
the first implementation can keep this cache limited to immutable base state.

KEEP_CACHE and long TTL require event-specific coherence proofs. Kernel writes,
non-FUSE mutations, rename aliases, truncation tails and racing attribute replies
have different rules. Capture does not change visible contents; known install
can preserve caches only if it preserves the complete visible identity/value.
Speculatively caching more state is not a correctness mechanism.

The kernel has attribute-version protection for several stale GETATTR/LOOKUP
cases; a reply-order race must be evaluated against those actual paths rather
than assumed to be a cache bug. Notification calls have kernel inode/page-lock
ordering constraints. Calling an entry invalidation while a related kernel
request waits for its reply can deadlock. An independent notification service
must not hold the SQL/Workspace owner or block the reply it needs to progress.

There is also a concrete compatibility gap: dirty shared mmap writeback can set
the per-request FUSE_WRITE_CACHE flag without negotiating mount-wide
FUSE_WRITEBACK_CACHE. The current adapter refuses that flag. Enabling cached
opens requires accepting and authenticating the supported kernel-origin path
with correct file-handle rules; it must not accidentally enable early syscall
acknowledgement through mount-wide writeback. Capture still includes only the
locally published mutation frontier, not unflushed mapping stores.

Resource accounting includes kernel dentries/inodes/pages and daemon ownership
records, not merely Rust heap or one buffer. Physical resources still limit
simultaneous residency. Removing an artificial file-count cap does not imply
zero per-file ownership cost or unlimited kernel memory.

## 4. Request dispatch and mutation service

The pinned unmodified `fuser` reply types permit deferred asynchronous replies.
Borrowed callback inputs must be converted into bounded owned jobs before return.
Retaining a reply frees its dispatch worker; it does not itself provide fair
scheduling, cancellation, bounded queued bytes or a safe ordering frontier.

```text
 FUSE receive/dispatch worker
          |
 validate + bind Workspace incarnation + own required request bytes/reply
          |
 enqueue bounded operation ---> return worker to receive other requests
          |
 fair runnable service / park without locks while prerequisite unavailable
          |
 plan exact mutation --> short atomic SQL attempt --> update visible state
          |
exact reply send attempt; retain proper lifecycle/outcome accounting
```

The default must remain one atomic transaction per mutating request. Prepared
statements, indexed keys and elimination of read-only BEGIN/COMMIT framing reduce
avoidable SQL overhead. Do not keep the SQL owner across a whole Exec, Commit,
base fetch or object upload. A single database still has one writer at a time.

Pending group commit is a research alternative, not the baseline. Every included
reply must wait for the outer COMMIT, capture must fence the entire group, and
failure coupling must be specified. It cannot amortize a process that issues one
synchronous tiny write and waits before issuing the next. SAVEPOINT RELEASE is
not an independent commit/acknowledgement boundary.

Deferred request retention needs explicit admitted-byte accounting. A queue of
owned WRITE buffers can otherwise recreate file-size-proportional heap growth.
Demand reads, mutations, Commit service and reclamation need fair progress;
strict read priority can starve Commit, while unbounded Commit service can stall
interactive reads. Readiness waits before an attempt are not automatic retries
of failed operations. A measurement watchdog is distinct from Bash semantics.

Capture is an explicit ordering/admission barrier over local mutation publication
and prior reply-send attempts. A successful syscall requires prior publication;
publication can still exist when reply delivery is lost. Pinned reply APIs expose
no delivery/kernel-application receipt, so the snapshot includes the actual live
published state and retains unresolved attempts. It must not define membership
by an acknowledgement the daemon cannot observe. Queue/drain wait is part of the
capture critical path; the metadata flip alone is not its latency.

The current fixed ten-second callback budget is separate from Bash runtime, but
must not become an automatic queue-wait ceiling in the replacement. Pinned fuser
does not implement a public INTERRUPT cancellation callback. Explicit daemon
cancellation, process signals, connection abort and an operation's exact attempt
state therefore need distinct outcomes. Do not promise transparent per-request
interrupt handling the pinned dependency does not expose.

## 5. Payload and metadata amplification

The payload design must survive dense one-byte fragmentation. A 128 KiB request
does not bound the number of historical variable extents it intersects. A fixed
payload-cell/validity candidate bounds spatial work by touched cells, but its
first-touch allocation, bitmap work, generation ownership and copy amplification
must be explicit. A one-byte write is not automatically a one-byte SQL update.

The READ plan must be bounded too: an alternating byte-validity mask must not
issue a separate upstream read for every invalid byte. Bounded mixed-range base
acquisition followed by overlaying valid local bytes is one candidate. It trades
extra base bytes/copies for bounded demand count, and must expose both costs.
This is read-path work, not base copy-up on a write.

For many tiny files, allocating a full cell per nonempty file can produce large
physical amplification. Inline small payloads are an alternative only with a
bounded conversion and exact snapshot/read-lifetime model. No cell size or inline
threshold is selected by this research. Shrink/regrow must use logical visibility
cutoffs so discarded old bytes do not reappear or require foreground bulk delete.

SQLite incremental BLOB handles cannot be used as long-lived append caches:
updating any column in their row expires them, and open handles can keep a
statement unfinished. Close/sequence handles inside the attempted transaction,
preserve data/validity/size atomicity, and close them before COMMIT.

Metadata indexes must serve exact point/range queries without selecting payload
BLOBs. Listing k entries costs output-sized work, not O(log N) total work.
Rename of a directory should change its bindings and required topology metadata
without rewriting every descendant path. An actual recursive removal still
executes many unlink/rmdir requests and may require captured reference/release
work. These costs must be reported rather than hidden in background phases.

## 6. Capture, construction and reclamation

```text
 before capture           after capture              known install
 [active G over R]        [active G+1 over G over R]   [active G+1 over R']
       |                        |       |                   |
 same existing rows             |       +-- frozen input    +-- same live view
 become captured input          +-- later changed keys
                                        |
                                  bounded scratch/stream
                                        |
                               Save -> Stage -> Commit

 obsolete captured/scratch ownership -- last owner release --> reclaim queue
                                                        --> bounded SQL deletes
```

Capture establishes a stable domain and ownership frontier without copying all
dirty rows or bytes. Subsequent mutations copy/create only the touched state
required by the selected ownership algorithm. Orphans and failed captures need
bounded composition; a chain of generations per Commit is not acceptable.

The integration audit found broader cluster-one prerequisites than a directory
Vec or new-parent map: total touched-serial refusal, declared row/name/demand
limits, full demand/addition/parent collections, full touched/zero collections
and released-descendant state. These must become backed/streamed structures with
bounded resident windows. Increasing `ordering_bytes` does not satisfy the
requirement. Validation, topology safety and reference accounting remain required.

More fundamentally, `check_parent_aliases` can traverse the complete base
namespace after rebinding a stored directory/symlink. Its visit refusal is
derived from `ordering_bytes / 1024`. Backing the traversal would bound memory
but still charge a tiny rename Commit for the full namespace. Incremental checked
parent/membership evidence or an authenticated reverse-binding index needs an
explicit API/format design to remove that mandatory walk while retaining alias
and cycle validation. Mount must not hide construction of that index.

Automatic reclamation requires sustained capacity, not just deferred deletion:

```text
 reclaim debt after service = previous debt + newly unreachable work - deleted work

 continued mutation traffic + idle intervals
          |
 reserved capacity + fair reclamation service
          |
 delete only state with no captured/read/orphan/uncertain owner
```

Logical retirement does not free SQLite pages immediately. Batched SQL deletion
allows reuse inside the daemon's shared file; it does not imply shrinking the
file or collecting global immutable objects. No whole-Workspace DELETE/VACUUM is
charged to a tiny foreground mutation or terminal-unmount reply. Slow reclamation
must remain visible as debt and pressure, with safe admission before capacity is
exhausted. There is no artificial total operation count/size cap.

## 7. Complete-call correctness boundaries

Bash exit, stdout/stderr EOF, acknowledged FUSE mutations, descendant lifetime,
dirty shared mappings and native detach are separate observations. Exec does not
implicitly stop descendants, flush every mapping, Commit or unmount. The
controller must use the explicit API contracts and capture only the frontier it
can establish. Dirty mappings cannot be declared included from process exit alone.

The actual mount-session owner is retained through native teardown. A pathname
disappearing from mountinfo is not a universal proof that all native references
are gone, particularly for lazy detach. Normal privileged unmount and helper
fallback paths must be distinguished. Cleanup uses an incarnation fence, not
guessing that every inode produced FORGET.

The pinned library creates receive workers per Session, rather than using the
daemon's shared job pool. Its worker receive buffer is based on a 16 MiB maximum
plus 4 KiB, independent of the negotiated 128 KiB request window. Two live loops
thus request more than 32 MiB of buffer storage per mount. Allocation size is
not proof of resident pages or initialization wall time; OS/allocator behavior
requires direct observation. A transient handshake buffer and LayerFS's outer
session thread are additional lifecycle work. Deferred shared jobs do not remove
these receive-session costs.

The existing individual FORGET path calls a whole-state collector. Repeating
that scan for many kernel references can amplify teardown work quadratically
under the relevant node population. Targeted per-inode release and efficient
batch handling must preserve lookup/open/orphan custody; actual detach then
retires namespace ownership in bounded service rather than waiting for a FORGET
event for every inode.

The pinned default `batch_forget` loops over individual callbacks, but its public
trait signature names a privately imported type with no public reexport found.
Do not promise a custom external batch override through this package's current
API. Cheap individual reference decrements plus coalesced bounded disposal can
work through the existing callback; no third-party patch is required for that
direction.

Concurrent Workspaces have separate views and roots while sharing bounded
service. Concurrent Commits on one Branch still require exact conditional
publication; they cannot both assume their expected head won. Unknown Save/stage/
transition outcomes retain exact custody. Immutable objects do not replace the
mutable history/authority protocol or supply an implemented distributed provider.

## 8. Implementation priorities and qualification gates

The retained promoted profile remains the initial candidate: 60 s entry/attribute
TTL, KEEP_CACHE, 128 KiB requests, two dispatch loops, background/congestion 1,
writeback off and permissions retained. The following order is an engineering
recommendation, not a new measured selection.

| Priority | Work | Exit evidence |
| --- | --- | --- |
| 1 | Deferred exact replies, fair queues, explicit frontiers and efficient ownership release | Unrelated operations progress with parked requests; cancellation/teardown retain exact custody |
| 1 | Fragmentation-safe payload, cutoff truncation, selective captured indexes | Visited-row/cell/page counts stay bounded per window under adversarial history |
| 1 | Backed construction, validation, reference and orphan state | Full affected-state Commit succeeds with fixed resident windows and no total-input refusal |
| 2 | Coherent promoted cache profile and exact immutable base caching | Alias/rename/truncate/attribute/install proofs; bounded whole-system cache residency |
| 2 | Event-driven mount/Exec/teardown and targeted FORGET disposal | Lifecycle counts and actual native resource closure; no repeated whole-state scans |
| 3 | Negative entries, adaptive READDIRPLUS, FLUSH elision | Permission-preserving single-mechanism request counts and mutation correctness |
| 3 | Whole immutable-file CopyFileRange reuse | Independent destination inode, stable authorized source, byte count/cache/metadata and later-write proofs |
| 3 | Request-size/in-flight tuning, receive-buffer/session alternatives | Actual negotiated capabilities, bytes/copies/residency and target-host scheduling evidence |
| 4 | Mutable/partial copy plans or handle-free opens | Exact snapshot/orphan/alias/capture semantics; bounded composition and required public APIs |

Qualification covers both fast fresh mount/teardown and persistent multi-call
Workspaces. For the latter, native setup is amortized over its actual lifetime;
price per-call filesystem work and each incremental Commit separately. Repeated
walk/status/build calls, overlapping writers/readers, long-lived descriptors,
Commit success/failure cycles and reclamation during continued activity are
required. Do not hide fresh-mount costs or force persistent sessions to remount.

Do not infer that configuring 128 KiB requests configures `fuser`'s receive-buffer
allocation to 128 KiB. Pinned session/worker implementation costs require their
own analysis. An existing public API is preferable; if a dependency cannot meet
a demonstrated requirement, report the limitation without patching/forking it.
Replacing a first-party adapter would require full ABI/lifecycle qualification.

CopyFileRange is an ordinary filesystem callback, not a Bash-command shortcut.
For a proven unchanged whole canonical source file and suitable destination,
sharing its immutable content root between independent inodes can avoid payload
transfer. Partial/mutable copies need stable owned ranges and bounded composition;
current content APIs do not provide a generic zero-I/O slice-splice constructor.
Copy count, metadata and destination cache behavior remain real work. Generic
ioctl does not establish reflink support. See the
[copy investigation](fuse-investigation/01-kernel-and-request-path.md#91-copyfilerange-is-a-serious-candidate).

## 9. Investigation reports and evidence

The supporting reports provide exact sources, alternatives and proof matrices:

- [Kernel and request path](fuse-investigation/01-kernel-and-request-path.md).
- [Mutation engine](fuse-investigation/02-mutation-engine.md).
- [Per-call lifecycle](fuse-investigation/03-per-call-lifecycle.md).

Historical evidence remains in [05](05-fuse-assessment.md) and issues
[#305](https://github.com/Ephemeral-AI-Lab/layerfs/issues/305) and
[#306](https://github.com/Ephemeral-AI-Lab/layerfs/issues/306). A2 is an ext4
passthrough prototype, not the LayerFS overlay or kernel FUSE passthrough.
Numerical observations remain INELIGIBLE without residency proof; B's required
survival proof failed and concurrent Stage C was NOT_RUN. No integrated speed,
memory or durability conclusion follows from them.

Future diagnostics should count requests, statements, transactions, visited
rows/pages/cells, constructed/reused objects, owned queue bytes, copies, lookup
references and reclaimed work. Qualification covers the complete per-call path
and sustained concurrent service, including physical reclamation. Cache states,
identities and budgets must be declared prospectively under repository rules;
reuse qualifying unaffected evidence and never rerun a passing sample for a more
convenient number. No workloads, builds or runtime tests were run for this report.
