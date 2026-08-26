# LayerFS efficiency model and benchmark contract

Status: **cost model plus qualification plan**. Current measurements are quoted
only from accepted source-bound evidence. Equations and target complexity are
not latency claims.

Normative product model:

```text
version levels:  Layer in a LayerStack; OperationVersion in a Branch
commit action:   OperationCommit -> OperationDelta -> next Branch head
forks:           LayerBranchFork; ChildBranchFork
merges:          ChildBranchMerge -> immediate parent Branch head
                 LayerStackMerge  -> inherited originating LayerStack head
                                      from a Branch at any nesting depth
```

The one commit and two merge actions use exact expected-head state, preserve a
stale candidate on conflict, and perform one visibility-changing SQLite COMMIT
in the Store accepting that action. A Push that makes the already
WorkingRecorded Branch transition durable is not a fourth commit or merge.
WorkingRecorded and DurablyAccepted are separate transactions; there is no
distributed COMMIT. Merging an already canonical candidate is
reference/version work; moving a physical presentation afterward is separate.

Target ownership of measured work is:

```text
immutable snapshot read -> layerfs-core::logical + ObjectRead; no Operation/sync

WorkingStore begin -> layerfs-workspace + concrete driver
                   -> layerfs-core::logical candidate construction
                   -> layerfs-storage object admission
                   -> WorkingStore OperationCommit

explicit layerfs-sync -> accepted canonical/version records only
                      -> DurableStore independent authentication/acceptance
```

Current `layerfs-engine` and `layerfs-vfs` paths below are retained evidence;
they do not define final crate ownership.

`layerfs-core::logical` is generic over bounded `ObjectRead`/`ObjectStore`
access. Its counters cover exact-version stat/list/range/stream/readlink,
mutation, candidate RootId/RootTransition construction, root diff, and
three-root merge. It contains no SQLite, platform, workspace, or authority
policy, so presentation/storage costs remain separately attributable.

References:

- [architecture stack](../../architecture.html);
- [operation algorithms](../../docs/architecture/02-operation-algorithms.md);
- [performance and complexity](../../docs/architecture/03-performance-and-complexity.md);
- [PoC algorithms and frozen profile](../../poc/02-data-structures-and-algorithms.md);
- [correctness-first verification](../../poc/06-correctness-and-fast-verification.md);
- [mounted Stage 2 contract](../../poc/19-stage2-docker-linux-fuse.md);
- [mounted VFS implementation](../../crates/layerfs-vfs/src/mounted.rs);
- [persistent extent rope](../../crates/layerfs-core/src/content/rope.rs);
- [publication and dedup admission](../../crates/layerfs-engine/src/publication.rs); and
- [accepted Stage 2 metrics](../../poc/evidence/stage2-freeze-candidate-015/summary.json).

## 1. What “efficient” means

LayerFS has four different efficiency questions. They must never be collapsed
into one throughput number.

| Dimension | Question | Required evidence |
|---|---|---|
| Algorithmic work | Does work scale with the requested range/change or with the whole file/workspace? | Structural and byte counters |
| Host-recoverable latency | How long until WorkingStore records the accepted root/version? | Working lifecycle timers and WorkingStore restart oracle |
| System-durable latency | How long until DurableStore independently accepts and can recover the root/version on a fresh host? | Explicit sync/verification/publication timers and DurableStore restart oracle |
| Storage | Are unchanged payloads and persistent-tree nodes shared across roots? | Created/reused objects plus logical/apparent/allocated bytes |
| Runtime resources | Does memory, spool, CPU, FD, thread, and connection use stay bounded under active concurrency? | Owned-Q and external process/cgroup observations |

The product is efficient when the common path performs work proportional to
the request/change while exact fallbacks and lower bounds remain visible.

## 2. Cost variables

| Symbol | Unit | Meaning |
|---|---:|---|
| `F` | bytes | complete logical file length |
| `W` | bytes | complete logical workspace payload |
| `B` | bytes | bytes newly supplied or changed by the operation |
| `R` | bytes | bytes returned to the caller |
| `E` | extents | extent/chunk occurrences in one file |
| `C_R` | objects | payload objects intersecting a read |
| `H_f` | levels | persistent file extent-tree height |
| `H_n` | levels | namespace/inode-tree height along changed paths |
| `P_c` | paths | changed namespace paths |
| `U` | objects/bytes | unique canonical content across retained roots |
| `V` | roots | retained immutable revisions |
| `L` | refs/workspaces | logical branches or named heads |
| `A` | slots | concurrently active physical/mounted execution slots |
| `O` | operations | concurrent isolated `OperationWorkspace`s on one host |
| `Q` | bytes | LayerFS-owned logical userspace memory |
| `S_spool` | bytes | owned dirty bytes in the disk spool |
| `S_db` | bytes | SQLite Store size, qualified by logical/apparent/allocated class |

Checked arithmetic is part of correctness. Overflow is an error before
allocation or publication, not a saturated efficiency counter.

## 3. Canonical data flow and exact deduplication point

### 3.1 Full import or full replacement

```text
caller byte stream (F_new)
  -> FastCDC scans the supplied stream
  -> canonical payload object for each chunk
  -> ObjectId = domain-separated hash(canonical bytes)
  -> incumbent lookup in SQLite
       equal authenticated incumbent -> reuse row
       absent incumbent              -> insert immutable row
       unequal/corrupt incumbent     -> integrity error
  -> persistent extent nodes built bottom-up
  -> inode/namespace paths copied
  -> layerfs-core::logical candidate
  -> layerfs-storage admission
  -> WorkingStore expected-head publication and one COMMIT
```

Chunking and payload deduplication happen while the new canonical file state is
constructed, before the new root is acknowledged. A full replacement must scan
the full supplied stream even when most resulting ObjectIds already exist:

```text
T_full_create = Theta(F_new) CDC/hash
              + Theta(E_new) extent construction
              + WorkingStore object/index/publication work
```

Deduplication reduces durable bytes; it does not erase the cost of determining
that the input is equal.

### 3.2 Mounted POSIX write and `OperationCommit`

The current mounted path deliberately separates live write acceptance from
canonical publication:

```text
write(offset, bytes)
  -> validate request and resource limits
  -> append bytes to bounded owned disk spool
  -> coalesce exact dirty ranges in mounted state
  -> read overlays accepted extents with spool ranges
  -> no accepted root change yet

OperationCommit
  -> close/freeze admitted dirty state
  -> feed each dirty range to persistent-rope replace
  -> FastCDC scans the replacement range supplied to that replace
  -> canonical payload IDs are reused/inserted
  -> path-copy affected extent/inode/namespace nodes
  -> one expected-head publication transaction and COMMIT
  -> clear/reset owned dirty state
```

This ordering explains two different latency classes:

- **live write acknowledgement**: spool/dirty-overlay work;
- **WorkingRecorded `OperationCommit` acknowledgement**:
  CDC/CAS/COW/WorkingStore SQLite publication.

They must be reported separately. Neither a fast `write(2)` nor WorkingRecorded
proves DurablyAccepted system persistence.

The current implementation still contains internal methods named
`checkpoint`. They are persistence mechanisms behind `OperationCommit`, not a
second public commit action. A mount may honor `fsync` for private operation
durability without advancing the shared Branch before `OperationCommit`.

### 3.3 Explicit logical splice

For an explicit replace/insert/delete operation, the byte-measured extent rope
splits by logical byte position, constructs new extents only for replacement
bytes, and rejoins the unchanged prefix/suffix by identity:

```text
old root
  -> split at start
  -> split after deleted range
  -> FastCDC + CAS(replacement bytes)
  -> concatenate shared prefix + replacement + shared suffix
  -> path-copy changed spines
```

The unchanged suffix payload is neither read nor rewritten on the direct
logical route. A POSIX `write(offset, bytes)` is overwrite/extend, not middle
insertion; an application that writes a complete temporary replacement still
supplies the whole file and pays `Theta(F_new)` input processing.

### 3.4 External native workspace capture

The Apple external-workspace fallback has no authoritative kernel change
journal. It freezes/cooperatively quiesces the workspace, walks the namespace,
and scans supported regular files. CDC/CAS may recover storage sharing after
the scan, but capture work remains worst-case `Theta(W)`. This route must not be
used as evidence for mounted/direct locality.

### 3.5 Canonical merge versus physical refresh

`ChildBranchMerge` compares a child candidate with the current immediate parent
Branch head and, if accepted, creates the next parent `OperationVersion`.
`LayerStackMerge` compares a Branch candidate with the current head of the
Branch's inherited originating LayerStack and, if accepted, makes the prepared
candidate the next visible `Layer`. This is valid at any Branch nesting depth;
the candidate closes over inherited state plus all accepted descendant changes.
Both merges move only their current destination head, preserve the source
Branch, and neither materializes nor captures files.

If a live presentation must follow the accepted head:

- a mounted/FUSE view selects the new immutable root without importing any
  sibling operation's private dirty state;
- a materialized/APFS view runs a separate refresh: Merkle-diff roots, apply
  changed paths, clone/patch eligible same-length files, and report an explicit
  full fallback for ineligible or count-changing native files.

Refresh cost is presentation cost. It is never canonical merge work and cannot
change merge correctness.

## 4. Deduplication and storage equations

For one accepted mutation:

```text
Delta S_canonical_logical
  = sum(canonical bytes of unique new payload objects)
  + sum(canonical bytes of new extent nodes)
  + sum(canonical bytes of changed inode/namespace/metadata nodes)
  + new ref/retention/publication metadata
```

For retained roots sharing content:

```text
S_canonical_logical
  ~= S_unique_payload(U)
   + S_unique_tree_nodes(U, V)
   + O(V + L) reference/history metadata
```

It is **not** generally:

```text
S_canonical_logical = L * W
```

Unchanged ObjectIds and persistent subtrees are referenced from multiple roots
without payload copies. New canonical tree data is limited to changed nodes and
their ancestor spines on ordinary operations. However:

- genuinely new bytes consume new payload storage;
- similar bytes that chunk differently may reduce reuse;
- SQLite pages, indexes, freelists, journals, and allocation units add physical
  amplification;
- retained roots intentionally keep unique historical objects live;
- unreachable objects are not reclaimed until qualified compaction; and
- native exports/materializations are derived storage outside the canonical
  dedup equation.

### 4.1 Same-host multi-operation storage

One execution host/security domain normally uses one shared `WorkingStore` CAS
and one private `OperationWorkspace` per active operation:

```text
S_host
  = unique WorkingStore objects
  + sum(private dirty/spool bytes for active operations)
  + bounded per-operation handles and tree state
```

Operations share immutable payloads and COW subtrees but never dirty maps,
spools, handles, or expected Branch heads. Ten similar operations do not create
ten canonical base copies. They can still consume ten processes' working sets
and ten sets of genuinely unique dirty bytes.

`layerfs-working-store` and `layerfs-durable-store` are mandatory and compose
the same `layerfs-storage` SQLite/object/schema mechanisms without a runtime
policy selector or duplicate implementation. One disk-backed WorkingStore per
host shares its CAS across all local Branches and OperationWorkspaces. Multiple
WorkingStores synchronize accepted state only through DurableStore; no
WorkingStore is peer authority for another.

Fetch asks for objects and records absent from the destination WorkingStore.
Push asks which objects and records are absent from DurableStore, then may
create a durable Branch or advance one exact durable Branch head. Branch Push
is independent of `LayerStackMerge`; moving a LayerStack requires a separately
requested merge. Hash/ObjectId negotiation avoids known-present transfer and
duplicate rows within each Store, but concurrent races, interruption, or lost
responses may retransmit equal bytes and must be charged. The same ObjectId may
have one authenticated physical row in each Store; the architecture does not
claim one physical copy across hosts.

### 4.2 Storage versus sync costs

`layerfs-storage` owns concrete SQLite/object/schema/transaction mechanisms;
WorkingStore owns host-recoverable operation authority; DurableStore retains
pushed Branch/Operation history and LayerStack state as the system of record.
`layerfs-sync` coordinates explicit Fetch/Push without replicating SQLite
files, inventing another object model, or publishing merely by uploading
objects.

| Sync phase | Work | Transfer | Visibility |
|---|---:|---:|---|
| Fetch | read an exact durable receipt, negotiate hashes/ObjectIds, stream missing accepted canonical/version batches, authenticate in WorkingStore, verify closure | unique missing bytes + observed resumed/retransmitted bytes | records DurableTrackingRef; never merges or moves a dirty working Branch |
| Push transfer | read an already WorkingRecorded closure, negotiate hashes/ObjectIds, stream missing canonical/version batches; DurableStore independently authenticates/verifies | unique missing bytes + observed resumed/retransmitted bytes | none merely from transfer |
| Push durable Branch action | create a durable Branch or advance one exact durable Branch head | bounded metadata | one DurableStore transaction/COMMIT or exact `Conflict`; no LayerStack movement |
| Explicit merge action carried after Push | one expected-head `ChildBranchMerge` or `LayerStackMerge` request | bounded metadata | one separate DurableStore transaction/COMMIT or exact `Conflict` |

Verified closure traversal can be linear in the requested reachable closure;
it must not be mislabeled as transfer work. Object batches and verification
frontiers remain bounded, and received payloads stream into the Store. Fetch
and Push are resumable and use bounded identity/object/byte queues; neither
loads a complete object closure or workspace into memory. Neither transfers a
live workspace path, recovery marker, dirty map, spool,
mount, process, descriptor, mapping, or native file. Neither runs because of
snapshot reads, filesystem mutations, close/fsync, tool exit, workspace
finalization, or a WorkingStore-only OperationCommit; synchronization is
measured only at explicit durable version-control boundaries.

### 4.3 Three storage classes

| Class | Definition | Valid use |
|---|---|---|
| Logical | Sum of canonical object/record payload lengths according to the LayerFS/SQLite model | Data-model and dedup accounting |
| Apparent | File length reported for Store, journal, spool, or native output | Namespace/file-level storage observation |
| Allocated | Filesystem blocks physically allocated to those files | Physical storage cost on the observed filesystem |

These values may differ because SQLite and filesystems allocate pages/blocks,
maintain sparse regions, and clone/share extents. None may substitute for
physical read/write bytes. If block I/O or clone sharing is unavailable, report
`Unavailable(reason)`, not zero.

For a campaign:

```text
Delta S_store_apparent  = apparent_after  - apparent_before
Delta S_store_allocated = allocated_after - allocated_before
Delta S_native          = native_after    - native_before
```

Negative allocated deltas can occur through allocator reuse/reclamation and
are observations, not proof that an operation wrote no physical bytes.

## 5. Operation cost matrix

The table describes the current/required algorithmic class. Constants, storage
engine costs, and cache state still affect wall time.

| Operation | Expected/common work | Honest worst/lower bound | Dedup/chunk timing |
|---|---:|---:|---|
| Immutable snapshot `stat`/`readlink` | pinned VersionRef + path work | Integrity policy may authenticate fetched nodes | No Operation/workspace/head move/sync |
| Immutable snapshot `list` | pinned VersionRef + path work + returned entries | Linear in returned entries | No Operation/workspace/head move/sync |
| Path lookup | `O(H_n)` bounded tree descent | Directory listing remains linear in returned entries | No CDC; validate/authenticate fetched nodes according to the selected integrity mode |
| Point/range read `R` | `O(H_f + C_R + R)` | Full read `Theta(F)` | No new chunking; validate/authenticate fetched objects according to the selected integrity mode |
| Full create/import | `Theta(F + E)` | Cannot be sublinear in supplied bytes | CDC/hash all input before publication; reuse after ObjectId lookup |
| Same-size direct overwrite | `O(B + H_f + P_c*H_n)` plus durability | Full supplied replacement remains byte-linear | CDC replacement bytes during canonical mutation/`OperationCommit` |
| Explicit insert/delete | `O(B + H_f + P_c*H_n)` for selected rope route | Explicit repack/full replacement is linear; inserted input is `Omega(B)` | CDC inserted/replacement bytes; unchanged suffix extents shared |
| POSIX full-temp save | `Theta(F_new)` supplied stream | `Theta(F_new)` | CDC entire new file; dedup may reduce stored payload only |
| Append | `O(B + H_f + P_c*H_n)` | `Omega(B)` | CDC appended bytes |
| Truncate | `O(H_f + P_c*H_n)` | Boundary/path work; explicit repack is separate | No scan of removed suffix payload |
| Mounted live write | `O(B + dirty-range bookkeeping)` | Resource admission/spool I/O | No accepted-root dedup yet |
| Mounted WorkingRecorded `OperationCommit` | Sum of admitted dirty canonical mutations + one WorkingStore Branch-head publication | Full dirty replacement if application supplied it | CDC/dedup here for ordinary mounted writes |
| No-change `OperationCommit` | Operation/version history plus one head transaction while reusing the same `RootId` | Integrity policy may add verification | No CDC/new payload/tree objects |
| `LayerBranchFork` / `ChildBranchFork` | Indexed metadata; zero object copies | Authority and source-version validation | No CDC; source version root shared |
| `ChildBranchMerge` | Shared-spine comparison when ancestry is known, plus one parent-head transaction | Arbitrary/unrelated roots may require `Theta(nodes(A)+nodes(B))` comparison | Candidate canonical objects already exist |
| `LayerStackMerge` from any Branch depth | Candidate/inherited-closure verification plus one originating LayerStack-head transaction | Full candidate closure verification under Verified authority | Candidate canonical objects already exist; source Branch survives |
| Fetch | Hash/ObjectId negotiation plus missing accepted bytes/records and bounded verification frontier | Requested closure traversal may be linear; observed retransmission is charged | No CDC; authenticate every received identity |
| Push | Hash/ObjectId negotiation plus missing accepted bytes/records and one optional exact durable Branch action | Source closure traversal may be linear; observed retransmission is charged | No CDC; DurableStore authenticates independently |
| Rollback | Guarded ref move; zero object copies | Target authority/authentication policy | No CDC; retained root shared |
| Reopen TrustedLocalDev | Store/schema/ref admission, no Verified closure scrub | Explicit weaker mode only | No CDC |
| Reopen Verified | Mode/history dependent | Retained/reachable scrub may be linear | Authentication, not chunking |
| Cold native materialization/export | `Theta(W)` output | Must emit all destination bytes/paths | No new canonical dedup; creates derived native bytes |
| Warm exact native no-op | Exact-live/root verification | If provenance is unknown, fail/rebuild by policy | No CDC when exact |
| Changed-root native refresh | Changed paths/ranges where qualified | Ordinary middle length change can require suffix/full native rewrite | Canonical state already chunked; projection is derived |
| External workspace capture | Full namespace walk; worst `Theta(W)` bytes | `Theta(W)` without authoritative recorder | CDC while recapturing files; storage may dedup afterward |
| Offline compaction | `Theta(reachable objects + candidates)` | Full authenticated retained-union traversal/copy | No CDC; copy authenticated canonical objects |

Required resource bounds for ordinary product operations:

```text
FastCDC profile                   = 8 / 16 / 32 KiB
largest product stream buffer    <= 1 MiB
payload reference batch          <= 64
owned logical userspace Q        < 8 MiB per admitted operation
terminal Q                       = 0
complete extent Vec              forbidden
complete workspace in memory     forbidden
in-memory workspace database     forbidden
whole-file mounted hydration     forbidden
whole-file mounted buffer        forbidden
complete namespace/object/version inventory forbidden
```

Dirty bytes beyond the memory envelope stream to an owned disk spool with an
explicit admission limit. Disk spool size may be `Theta(B_dirty)`; userspace
memory must not be. Clean object caching is likewise disk-backed when it exceeds
the bounded memory cache. Queue, frontier, batch, buffer, cache, and spool
high-water counters make the bound observable. Namespace traversal,
materialization, capture, Verified scrub, and compaction are streaming linear
exceptions, not permission to build a complete in-memory map. The memory bound
is independent of file, workspace, retained-version, and object-count size.

Mounted/FUSE reads and writes terminate at the nearby disk-backed WorkingStore.
They neither issue DurableStore RPCs nor hydrate a whole file. Only explicit
Fetch/Push crosses the WorkingStore/DurableStore boundary.

`layerfs-workspace` owns `Q` admission/high-water/terminal accounting. The
concrete driver owns spool/native allocation, handles, writers, mappings, and
process-quiescence observations. Workspace roots are `0700` under
`<working-root>/workspaces`; direct logical work may expose no path, FUSE uses a
private mountpoint with sibling spool, and APFS uses a private physical view on
the admitted volume. All path/spool/process bytes are host-local and excluded
from Sync transfer and DurableStore storage.

## 6. Expected-local versus worst-case claims

Use these exact claim classes:

| Claim | Allowed statement | Forbidden overclaim |
|---|---|---|
| Direct range read | Work follows tree height, intersecting objects, and returned bytes | “Every read is O(1)” |
| Direct explicit splice | Unchanged prefix/suffix extents and payload objects are shared; work follows replacement/touched spines | “Every editor save is delta-only” |
| Mounted POSIX overwrite | Exact dirty ranges are buffered/spooled; canonicalization occurs at `OperationCommit` | “Write acknowledgement is durable Branch publication” |
| Logical branch/fork | New reference metadata; zero payload copy | “A running physical workspace costs zero” |
| Native projection | Exact/sparse routes may avoid full work when provenance/capability proves them | “APFS middle insertion is logarithmic” |
| Full import/read/export | Byte-linear lower bound | Any sublinear complete-byte claim |
| Dedup | Durable unique payload is shared by exact ObjectId | “Duplicate input is free to read/hash” |

## 7. Versions, Branches, and active operation slots

The architecture in [architecture.html](../../architecture.html) separates an
immutable root/reference graph from workspace presentations. Let:

```text
L = total retained Layers and OperationVersions
A = simultaneously active mounted/native execution slots
```

Expected scaling:

```text
canonical payload/tree storage  ~ unique changes across L
reference metadata              ~ O(L)
live process/mount/dirty memory  ~ O(A)
spool/storage pressure           ~ active dirty bytes across A
```

Product design should maintain:

```text
A bounded by admitted concurrency
L allowed to be much larger than A
```

A system that creates a full native copy for every retained version loses
this property. A native workspace per **active slot** can be acceptable; a
native workspace per **retained version** is not the intended architecture.

The complete topology is:

```text
LayerBranchFork(Layer N) -> top-level Branch
ChildBranchFork(parent OperationRecordRef M) -> child Branch
OperationCommit -> next OperationVersion on that Branch
ChildBranchMerge -> next OperationVersion on the immediate parent Branch
LayerStackMerge -> next Layer on the inherited originating LayerStack
                   from any Branch nesting depth
```

Each active operation receives an isolated workspace pinned to its exact source
version. Sibling operations can run concurrently but cannot observe or mutate
one another's dirty state. The first matching expected-head action wins; a
stale candidate remains retained and returns `Conflict`.

Search, rollout selection, and MCTS may consume these Branch/version primitives,
but remain external, non-normative policy. They add no LayerFS schema, merge
rule, or benchmark shortcut.

The release scale test therefore varies these axes independently:

1. fixed `A=1`, increase `V/L` to test retained history and storage;
2. fixed small history, increase `A` to the declared concurrency ceiling;
3. fixed `A`, vary dirty bytes and file count to test Q/spool bounds.

## 8. Lifecycle and durability timer boundaries

Every mutation row records monotonic timestamps around these distinct owners:

```text
t0  WorkingStore begin_operation dispatch
t1  Operation identity/head pin/lease/recovery record durable
t2  concrete OperationWorkspace ready
t3  caller workload complete; dirty state may remain
t4  layerfs-workspace quiescence complete
t5  layerfs-core::logical candidate + WorkingStore recovery candidate ready
t6  WorkingStore OperationCommit dispatch
t7  WorkingRecorded COMMIT/reconciliation complete
t8  workspace cleanup receipt complete

s0  explicit Push dispatch                            // optional, s0 >= t7
s1  negotiated accepted canonical/version transfer complete
s2  DurableStore independent authentication/closure verification complete
s3  durable head action COMMIT/reconciliation complete
s4  sync receipt/cleanup complete
```

Derived durations:

```text
T_begin              = t2 - t0
T_live_work          = t3 - t2
T_quiesce            = t4 - t3
T_candidate          = t5 - t4
T_working_commit     = t7 - t6
T_workspace_cleanup  = t8 - t7
T_working_complete   = t8 - t0

T_sync_transfer      = s1 - s0
T_durable_verify     = s2 - s1
T_durable_publish    = s3 - s2
T_sync_cleanup       = s4 - s3
T_push_complete      = s4 - s0
T_request_to_durable = s4 - t0
```

All gaps remain charged, including `t6-t5` and `s0-t8`. WorkingRecorded is
host-recoverable at `t7`; only `s3` establishes DurablyAccepted system state.
Lost acknowledgement ends the owning COMMIT timer only after fresh
WorkingStore or DurableStore reconciliation.

For FUSE benchmark reporting, external tool/live time covers
application-visible filesystem operations; the SDK does not interpret the
tool. WorkingStore commit and explicit Push are reported
separately, never collapsed into “write latency.”

The accepted Stage 2 summary reports independently selected sums of per-scenario
medians for live, internal persistence, and durable durations. Because different samples
may supply each median, those aggregate sums are **not** asserted algebraically
additive. Per-row timer equations must still close exactly.

CPU time is separate from wall time. RSS, Q, storage allocation, and I/O are
observations, never inferred from elapsed time.

## 9. Required direct counters

At minimum, each operation class exposes or derives with an equation:

```text
layerfs-workspace: Q current/high-water/terminal, lifecycle state, cleanup
driver: dirty ranges, spool/native bytes, handles/writers/mappings/process guards
layerfs-core::logical: requested/returned/input bytes, CDC, tree/node/path counters
layerfs-storage: objects/bytes created/reused/fetched/authenticated/written,
                 SQLite statements/rows/transactions/COMMITs/connections
WorkingStore: begin pin/lease/recovery, OperationCommit, reconciliation
layerfs-sync: negotiated IDs, unique/retransmitted bytes, batches, receipts
DurableStore: independent object/closure authentication, head transaction,
              COMMIT/reconciliation, retention/backup/compaction
external: RSS/process/cgroup, CPU, threads, FDs
storage: logical/apparent/allocated DB/journal/spool/native bytes
terminal: owner/recovery/view/spool/mount/process/descriptor residue by owner
```

Structural constants such as a fixed request limit are `Invariant`, not an
observed zero. Unsupported block-I/O counters are `Unavailable` with API/source
reason. A missing counter cannot silently become zero.

## 10. Control and candidate equality

Any comparative performance claim freezes this table before rows:

| Equality | Required check |
|---|---|
| Source/product path | Each arm's source, diff, executable/image hash; both call shipped entry points |
| Input | Byte-identical fixture and mutation schedule |
| Initial state | Equal Store/root/ref/generation and equal retained-history shape |
| Integrity | Same mode, or comparison explicitly asks the cost of different modes and labels it |
| Cache/preconditioning | Same warm/cold class and balanced adjacent order |
| Operation semantics | Equal paths, offsets, lengths, outputs, final RootId/inventory |
| Durability | Equal sync endpoint, transaction count, COMMIT count, reconciliation policy, storage class |
| Resources | Equal CPU/memory/IO limits and no competing campaign |
| Timer | Same boundaries and complete-wall equation |
| Cleanup | Equal terminal residue and process/mount lifecycle |

Historical subtraction, candidate-only warmups, control-only extra work,
post-observation schedule changes, and selective row deletion invalidate a
comparative claim.

## 11. Fast iteration ladder

The shortest valid ladder is:

```text
source change
  -> one touched deterministic test
  -> touched crate test/check/Clippy
  -> zero-row schedule/custody assertion
  -> one complete mechanism screen under a predeclared short total budget
  -> stop unless counters show the intended mechanism
```

Rules:

- no performance campaign for a correctness-only repair;
- build/preflight once and reuse prepared deterministic inputs;
- one source change, one focused regression at the shared owner;
- use 1/10 MiB for semantic/fault screens and 100 MiB only when needed to
  expose a causal scaling owner;
- do not run 500 MiB in the normal iteration loop;
- do not rerun unchanged bytes for a favorable number; and
- preserve failure output without creating recursive manifest/version churn
  for pre-row mechanical harness fixes.

The existing Apple PoC small campaign uses an approximately 3 MiB fixture and
one three-repetition complete sequence with a 30-second gross diagnostic stop
([verification section 12](../../poc/06-correctness-and-fast-verification.md)).
That stop is not a production SLO.

## 12. Release qualification ladder

Run once on frozen release bytes after all correctness gates pass:

### 12.1 Static and deterministic closure

```text
format check
workspace tests
Clippy -D warnings
diff/whitespace check
canonical/model/corruption/history/fault suites
Storage/WorkingStore/DurableStore ownership and independent-admission suites
universal Workspace contract with direct/mount/materialization drivers
real mount operation oracle
```

### 12.2 Functional release matrix

Use the smallest population covering distinct mechanisms:

| Axis | Required cells |
|---|---|
| File size | tiny, 1 MiB, 10 MiB, 100 MiB causal rows |
| File shape | repeated/deduplicable, high entropy, many small files, multi-level namespace |
| Read | point, cross-extent, range, full, old retained root |
| Snapshot-read isolation | `stat`/`list`/`read_range`/stream/`readlink` pin exact version with zero Operation/workspace/head transition/sync |
| Mutation | overwrite, extend, explicit insert/delete, append, truncate, full-temp replace, rename/link/unlink/metadata |
| Durability | no-change and dirty WorkingStore `OperationCommit`; Branch Push/create-or-advance independent of `LayerStackMerge`; WorkingStore and DurableStore kill/reopen; lost durable acknowledgement |
| History | retained Layers/OperationVersions, both forks, divergent edit, both merges, rollback, post-rollback edit, compaction |
| Parallel operations | one shared `WorkingStore`, `O=1/2/declared maximum`, private dirty state, same-base commit race with one WorkingRecorded and one preserved conflict |
| Workspace ownership | direct no-path; FUSE private mountpoint+sibling spool; APFS private same-volume view; `0700` markers, crash recovery, safe cleanup, never synced |
| Nested Branches | depth >1; parent/child parallel three-root merge; stale parent; immediate-parent-only `ChildBranchMerge`; forbidden cross-tree destinations; direct any-depth `LayerStackMerge` to the inherited originating stack with complete inherited closure; repeated merge; source preservation; origin-lease ancestor rollback rejection and release only after explicit Branch drop |
| Presentation | mounted/FUSE logical operations; materialized/APFS cold output, capture, eligible refresh, and explicit full fallback qualified separately |
| Sync | Push a durable Branch, Fetch it into a second disk-backed WorkingStore, continue/edit/Push; same-durable-head conflict from multiple WorkingStores; hash-first missing-object negotiation; bounded resumable transfer and retransmission accounting; accepted canonical/version records only; DurableStore independent verification; no peer authority and zero DurableStore RPCs from workspace/mount syscalls |
| Integrity | Verified primary; explicit TrustedLocalDev separately labeled |
| Resource | many-Branch CAS sharing in one WorkingStore; cross-store missing-byte transfer; mounted small/count-changing edit with no whole-file memory/work; Q/cache/spool/queue/RSS/FD/thread/connection limits and one-over-limit rejection |

Redundant rows should be replaced, not appended indefinitely. Correctness
coverage lives primarily in deterministic tests; release performance rows
exist to measure causal owners.

### 12.3 Performance campaign

- one warmup plus a small fixed measured population;
- adjacent/balanced A/B only when comparing products/candidates;
- one complete total budget declared before execution;
- real mount, persistent Store, exact release image;
- live and persistence-inclusive results separate;
- raw rows append-only; one independent recomputation; and
- no threshold changes after observation.

Numeric SLOs are not invented here. They must be selected from product
requirements before the release campaign. Existing candidate 015 values are
the local ARM64 baseline, not universal future thresholds.

### 12.4 Final storage/resource campaign

Record:

1. clean WorkingStore, DurableStore, service, and runtime baseline;
2. after full working import/OperationCommit but before Push;
3. after explicit Push and independent DurableStore Branch acceptance;
4. after small same-content and high-entropy edits;
5. after long retained history and recursive child Branches;
6. after fork/rollback and origin-lease rejection/release;
7. after per-Store offline compaction; and
8. terminal owner/recovery/view/spool/mount/process/journal cleanup.

Report unique canonical bytes/objects and Store logical/apparent/allocated
growth separately. For cloned/native outputs, report apparent and allocated
bytes without assuming allocated bytes equal private bytes.

## 13. Accepted local baseline, correctly scoped

Candidate 015 directly observed for its exact ARM64 Docker/FUSE artifact:

| Metric | Accepted observation |
|---|---:|
| Durable campaign | 12 warmups + 36 measured fresh Stores; 48 samples total |
| Sum of live per-scenario medians | 3.898 s |
| Sum of historical internal-checkpoint per-scenario medians | 4.337 s |
| Sum of command-to-durable per-scenario medians | 8.229 s |
| High-entropy 64 MiB to durable | 926.499 ms, exact restart-visible bytes |
| Max daemon RSS upper-bound increase | 9,469,952 B |
| Max whole-cgroup peak | 285,024,256 B |
| Threads / max FD growth | 7 / 6 |
| OOM/OOM kill | 0 / 0 |
| Terminal Q / Store connections / residue | 0 / 0 / 0 |

The aggregate median note above applies. These values prove a strong local
baseline. Historical labels such as “durable campaign” and
“command-to-durable” mean the then-current local SQLite/restart boundary, not
target `DurableStore` acceptance. They do not prove deployed-cloud durability, hardware power-loss,
AMD64, hostile multi-user isolation, or all future workloads.

## 14. Honest limitations

LayerFS cannot remove these lower bounds:

- full input must be read/chunked/hashed before exact dedup is known;
- full read and cold full export/materialization are byte-linear;
- a conventional native file may need suffix/full reconstruction for a middle
  length change;
- an opaque editor that rewrites the full file supplies full-file work;
- Verified closure scrub is proportional to the closure authenticated;
- retained unique history consumes storage until retention/compaction removes
  unreachable objects; and
- active processes and mounted slots consume real CPU/memory/FD resources even
  when their immutable base is shared.

The efficiency claim is therefore precise:

> Exact canonical objects and persistent trees share unchanged payload and
> structure across roots; direct range reads and explicit local mutations avoid
> mandatory whole-file/workspace work; mounted dirty bytes are bounded and
> canonicalized at `OperationCommit`; full-byte operations remain full-byte
> operations.
