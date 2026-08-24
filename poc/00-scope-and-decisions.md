# PoC scope, decisions, and acceptance contract

Status: **frozen design authority; historical preimplementation wording is not
current-source evidence**. Current implementation and closure are in `poc/13`
and `poc/17`.

This file freezes the smallest coherent implementation target. A change to a
canonical field, identity role, supported operation, durability boundary, or
verification oracle requires an explicit edit here before implementation.
Mechanical test/harness repairs do not create a new PoC version.

## 1. Problem statement

The current repository proves important core and benchmark mechanisms, but the
product path is incomplete:

```text
layerfs-core    substantial canonical/CAS/CDC/COW implementation
layerfs-engine  durable SQLite baseline plus large benchmark-private G5 code
layerfs-os      host probe, not a native projection adapter
layerfs-vfs     placeholder
layerfs-sdk     placeholder
```

The PoC must turn the surviving mechanisms into one reusable end-to-end system
without carrying Phase-4 benchmark architecture into production modules.

## 2. Frozen boundary

### 2.1 Included

```text
fresh versioned PoC store/profile
canonical immutable payload objects
fixed FastCDC profile
persistent byte-measured B+ extent rope
persistent byte-bounded B+ namespace trees
immutable parent/child roots plus Merkle root diff
SQLite object/ref publication; legacy root/delta rows are compatibility state
Verified and explicit TrustedLocalDev modes
ordinary APFS directory materialization
optional verified clone + same-size patch
managed exact-range edits
external-editor full-scan capture fallback
ordinary APFS files usable by Bash/editors/build tools
universal VFS ProjectionDriver boundary
Apple driver confined to layerfs-os
canonical inode table with hard-link topology
portable plus typed platform-extension metadata
reopen/reconstruction
retained root history
internal fork and rollback-by-ref tests
reachability enumeration/accounting
offline exclusive mark-copy-verify-swap compaction
minimal SDK workflow
compact deterministic real-workspace benchmark
```

### 2.2 Deferred

```text
legacy-store migration
transparent write interception
mounted filesystem frontend (FSKit/FUSE/kernel/File Provider)
multi-process writer pool
automatic retry
WAL
PostgreSQL/remote backend
branch/merge/rebase
automatic command checkpoints
online/background/in-place GC
compression/pack carrier
500 MiB qualification
production distribution/signing
Linux/Windows driver implementations
```

## 3. Architecture decisions

| ID | Decision | Reason | Consequence |
|---|---|---|---|
| D-01 | Use a fresh versioned file-state profile | K64/F64 cannot provide hard-local count-changing edits | PoC stores need not migrate legacy roots initially |
| D-02 | Use a persistent byte-measured B+ extent rope | Provides deterministic logarithmic structural operations independent of suffix length | Same bytes reached through different histories may have different operational roots |
| D-03 | Separate `FileStateRoot` from optional semantic `ContentDigest` | Avoid complete-file hashing/canonical rebuilding on every local edit | Semantic equality is lazy/secondary, not publication authority |
| D-04 | Extents are slices of immutable payload objects | Logical split must not copy unchanged payload | Validation must prove source range lies within authenticated payload |
| D-05 | Keep the existing FastCDC profile | Payload locality is already implemented and identity-sensitive | No simultaneous CDC redesign |
| D-06 | Use a persistent byte-bounded B+ tree for every directory | A small validation fixture must not force a disposable `Theta(D)` namespace algorithm | Lookup and mutation path-copy are logarithmic; full listing remains linear |
| D-07 | SQLite remains the only durable backend | Current durability/publication semantics already exist | No storage abstraction registry |
| D-08 | One writer transaction and one publication COMMIT | Preserves atomic root visibility | Every public capture converges on one engine path |
| D-09 | `Verified` is default; `TrustedLocalDev` is explicit and Store-lifetime | Preserves secure default and G5 policy boundary | Verified reopen after trusted history performs required verification |
| D-10 | APFS clone is an optional projection optimization | Clone can reduce same-volume copy work but is not canonical authority | Complete stream fallback remains mandatory |
| D-11 | Managed writes are the exact-range fast capture path | Ordinary filesystem notifications prove neither complete changed paths nor reliable byte ranges | External-editor capture walks the complete namespace and scans every supported regular file |
| D-12 | Ordinary native length-changing projection may be `Theta(F)` | A contiguous file must physically expose shifted suffix bytes | Logical rope edit can still remain local |
| D-13 | Fork and rollback move immutable references | Old roots remain authoritative and readable | No destructive in-place historical mutation |
| D-14 | PoC v1 implements one explicit offline mark-copy-verify-swap compactor | A durable object store needs a nonthrowaway reclamation path, but online deletion adds unnecessary concurrency risk | Compaction requires zero active readers/writers/workspaces, copies only the authenticated retained union to a sibling Store, verifies every retained root, then atomically swaps Store generation |
| D-15 | Native sibling names must be proven representable and distinct on the actual APFS destination | Case-/normalization-insensitive APFS may collapse distinct canonical names | Reject with typed `NativeNameCollision` before Complete live authority; canonical root remains valid |
| D-16 | Set SQLite `busy_timeout` to zero for PoC v1 | Deterministic one-writer behavior and exact Busy/Locked are simpler than hidden waiting | No SQLite/internal/application retry; changing this runtime policy does not change canonical profile identity |
| D-17 | Managed and external workspaces are distinct move-only types | A readable `Path` is also writable and would leak managed authority | Managed workspace has no path; caller-known materialization is External from creation; external capture requires cooperative quiescence/full scan |
| D-18 | Support directories, regular files, executable mode bits, symbolic links, hard links, xattrs/resource forks, supported ACLs and BSD flags in the Apple profile | Near-complete developer workspaces need inode topology and common Apple metadata | Namespace entries reference canonical `InodeId`; host inode numbers remain observations; unsupported device/FIFO/socket kinds fail typed |
| D-19 | Define ProjectionDriver in VFS and StoreGenerationDriver in engine; implement both only in `layerfs-os` | Adding a fitting platform must not change canonical/workspace/durability semantics | Core/engine/VFS contain no platform syscall or platform cfg; SDK wires `layerfs_os::native_platform()` |
| D-20 | Interpret Apple “99% complete” as 100% pass of a frozen supported profile | Percent-complete language must not weaken correctness | Remaining 1% is packaging/signing/mount/tuning and explicit exclusions, never a failing hard gate |
| D-21 | Product object reads use one-fetch/one-auth borrowed rows and ordered batches of at most 64 payload references | Current reusable reader repeats metadata queries and complete BLOB passes; G5 already exercised the bounded batch shape | SQLite BLOB remains the selected backend; packed carriers stay deferred |

## 4. Identity roles

```text
PayloadObjectId = H(canonical payload object)
MappingNodeId    = H(canonical extent leaf/internal node)
FileStateRoot    = H(canonical mode-free file record: profile + summaries + MappingNodeId)
DirectoryRoot    = identity of the canonical directory state
DeltaId          = H(canonical parent -> child operation record)
CommitId         = H(canonical publication transition, if separately modeled)
StoreId          = private durable store authority, never canonical content
WorkspaceId      = engine/VFS metadata, never canonical payload identity
ContentDigest    = optional/lazy logical-byte digest, not mandatory authority
```

Forbidden substitutions:

```text
native inode          != PayloadObjectId
SQLite row ID         != MappingNodeId
live projection authority != FileStateRoot
trusted assumption    != verified receipt
ContentDigest         != operational FileStateRoot
workspace generation  != canonical CommitId
```

## 5. Authoritative state machines

### 5.1 Durable root publication

```text
OperationAndEvidencePrepared
  -> WriterTransactionOpen
  -> ExpectedHeadMatched
  -> ObjectStreamAndTreeInserted
  -> RootAndRefInserted
  -> CommitDispatched
  -> Committed

CommitDispatched -> return ambiguous -> FreshReconciliation -> Old | New
failure before transaction -> NoTransaction
failure after transaction, before dispatch -> RollbackAttempted -> Prior | CleanupError
```

All current-profile SQLite object/ref rows are inserted inside that one writer
transaction. Large inputs stream through bounded buffers while the transaction
is open; no separate durable carrier or pre-transaction object COMMIT exists.
Only `Committed` or reconciled `New` may advance a workspace/reference head.

The earlier canonical-delta proposal is superseded for FileStateV3 by
`poc/13` section I11: immutable roots and refs are durable authority, Merkle
root diff derives transitions, and a new canonical delta format is deferred.
Existing legacy root/delta tables remain compatibility state and must not be
silently discarded by maintenance.

### 5.2 Workspace

```text
Active
  -> Capturing
       -> Captured
       -> ActiveAfterRecoverableFailure
       -> FailedTerminal
  -> Discarded
```

`Captured` and `Discarded` are mutually exclusive terminal outcomes.

### 5.3 Native projection

```text
Unknown
  -> FullStreamPreparing
  -> ExactPublished

ExactParent
  -> NoOp                         when requested root is exact
  -> ClonePatchPreparing          when same-size patch proof is valid
  -> FullStreamPreparing          otherwise

Preparing -> failure -> PriorFileOrIncompleteDerived
Preparing -> sync + atomic rename -> ExactPublished
```

The transition above is one-file atomicity. A multi-file ordinary directory
may contain a mixture of individually complete old/new files after interruption.
It remains `IncompleteDerived` and must not receive or reuse `Complete` live
authority until the entire tree is freshly verified. PoC v1 does not persist
that authority across process reopen.

## 6. Correctness invariants

| Boundary | Required invariant |
|---|---|
| Canonical decode | Exact domain/version/role/length/order/EOF validation |
| CAS get/put/reuse | Supplied canonical bytes always authenticate to requested object identity |
| Extent | `source_offset + length` checked and within authenticated payload length |
| B+ leaf | Ordered nonempty slices except canonical empty-file form; bounded encoded node size |
| B+ internal | Child measures positive where required; checked sums equal parent measures |
| Root | Total bytes/extents and height match complete reachable structure |
| Edit | Old root unchanged; new root streams exactly the reference-model bytes |
| History | Every retained root remains independently readable after later edits |
| Delta | Exact before identity, after identity, operation order, and replay semantics |
| Publication | Expected head, one transaction, one COMMIT, old-or-new reconciliation |
| Projection | Each published file is complete old/new; only a freshly verified complete tree may receive process-lifetime `Complete` live authority |
| Capture | Frozen evidence revalidated before content admission/publication |
| Fork | Two refs may share one immutable root without copying payload/tree objects |
| Rollback | Ref moves only to an existing verified retained root with expected-head protection |
| Reachability | Read-only enumeration never deletes or weakens retained-root authority |

## 7. Complexity contract

These are implementation targets to be enforced first with structural counters,
then sanity-checked with one small benchmark.

| Operation | Time target | New live memory target | New durable data target |
|---|---:|---:|---:|
| Point read `R` bytes | `O(log E + R)` | `O(H + R_buffer)` | `0` |
| Range read | `O(log E + C_R + R)` | `O(H + bounded stream buffer)` | `0` |
| Full stream/reconstruction | `Theta(F)` | bounded stream + `O(H)` | `0` unless projecting |
| Same-size overwrite | `O(B + K + log E)` | `O(BoundedCDC + H * node_size)` | new payload + replacement-tree nodes + `O(H)` path nodes |
| Insert | `O(B + K + log E)` | same | new payload + replacement-tree + split/path nodes |
| Delete | `O(log E)` plus boundary/node work | `O(H * node_size)` | merge/path nodes |
| Append/truncate | `O(B + K + log E)` | bounded | replacement-tree + right-spine/path nodes |
| Snapshot/fork ref | zero object-byte copies; `O(log refs)` indexed DB operation | `O(1)` | `O(1)` ref metadata |
| Rollback ref | zero object-byte copies; `O(log refs)` indexed DB operation plus authority check | `O(1)` | one ref publication |
| External workspace capture | complete namespace walk; worst `Theta(total workspace bytes)` without authoritative watcher/journal | bounded stream | unique payload/nodes after comparison |
| Cold native materialize | `Theta(F)` | bounded stream | `Theta(F)` native output |
| Native length-changing refresh | `Theta(F)` in ordinary-file PoC | bounded stream | `Theta(F)` native output |

Hard resource requirements:

```text
no source-sized userspace buffer
no unbounded pending request collection
no recursive history replay for an ordinary root read
no cache required for correctness
no SQLite writer pool
no retained private temp after terminal success/failure/restart cleanup
all counters use checked arithmetic
```

## 8. CPU, memory, and storage ownership

| Resource | Owning layer | Bound/measurement |
|---|---|---|
| CDC scan CPU | core | bytes scanned; bounded context for managed edit |
| Identity hashing CPU | core | exact canonical bytes hashed; no hidden complete-file hash on local edit |
| Tree traversal CPU | core | nodes decoded/validated; proportional to height/path plus touched extents |
| SQLite CPU/I/O | engine | statements, rows, transaction count, COMMIT count |
| Native copying/writes | OS/VFS | bytes cloned logically, bytes patched, bytes streamed, sync/rename count |
| Logical owned memory | owning module | current/high-water exact counter with terminal zero |
| Process RSS | final executable | external observation; not a substitute for ownership accounting |
| Durable store growth | engine | canonical/apparent/allocated bytes by operation/history |
| Native temp growth | OS/VFS | apparent/allocated bytes and terminal residue |

## 9. Required operation scenarios

The implementation and verification documents must cover every row before PoC
closure:

| Group | Scenarios |
|---|---|
| Create | empty, tiny, multi-chunk, multi-level file, directory tree |
| Read | point, cross-extent range, EOF, complete stream, retained historical root |
| Edit | equal overwrite, shorter/longer replace, insert start/middle/end, delete, append, truncate |
| Native projection | cold, exact no-op, same-size patch, missing seed, replaced destination, length-changing fallback |
| Capture | managed exact ranges, external full-workspace scan, stale evidence, ambiguous native identity |
| Shell/native tools | `/bin/bash` performs the frozen operations; capture waits for registered children and relies on caller cooperative quiescence; unregistered writers are excluded |
| Reopen | clean reopen, post-trusted verified reopen, ambiguous COMMIT, interrupted projection |
| History | 1/10/100/1,000 edits in correctness model; direct old-root read |
| Fork | two refs from one root, divergent edits, unchanged shared object identity |
| Rollback | expected-head rollback to old root, stale rollback conflict, post-rollback new edit |
| Reachability | current ref, forked ref, rolled-back-from root, abandoned unreachable root report |
| Compaction | retained-root mark, abandoned-object exclusion, crash-safe sibling Store copy/swap, every retained root exact afterward |
| Fault | malformed object/node/delta/receipt, missing object, wrong role, no-space/busy, crash boundaries |

## 10. Minimal acceptance gate

PoC v1 passes only when all are true:

1. The new core file representation passes deterministic differential operation
   sequences against a plain byte-vector model.
2. Canonical goldens and corruption/failure precedence are frozen.
3. Structural counters demonstrate local paths do not touch a suffix-linear
   number of mapping nodes.
4. SQLite publication is one expected-head writer transaction and one COMMIT.
5. Managed capture rematerializes exact bytes after close/reopen.
6. A real Bash process operates on ordinary materialized files; after process
   quiescence, external capture is correct and honestly labeled full scan.
7. Cold, no-op, same-size patch, and full-fallback native routes preserve exact
   old-or-new visibility and clean private temps.
8. Old roots, forked roots, and rollback targets remain directly readable.
9. Offline compaction rejects active pins, preserves every retained root, removes
   only authenticated-unreachable objects, and recovers correctly at each swap
   boundary.
10. Persistent namespace split/borrow/merge/root-collapse and path-copy bounds
    pass independently of the small fixture size.
11. The compact real-workspace/Bash operation sequence and benchmark complete once
    on frozen source without retrying for noise.

## 11. Change control

The following require editing this decision file before code changes:

- changing the node codec, size bound, or occupancy invariant;
- changing identity roles or hash preimages;
- changing the supported operation population;
- adding migration, online/in-place GC, transparent write interception, or another
  backend;
- weakening an exact error, integrity, durability, resource, or oracle rule;
- changing the final benchmark population after observing results.

Ordinary implementation defects, test repairs, formatting, and report wording
do not create new architecture versions.
