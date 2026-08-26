# Complete implementation plan from the current PoC to product-ready LayerFS

Status: **ordered executable target plan, not a completion claim**.

The migration preserves canonical bytes and proven mechanisms, moves each owner
once, and deletes compatibility forwarding after callers migrate. It never
creates a temporary portable-FS crate or maintains two semantic implementations.
The terminal scope is the complete [`poc-product-ready`](../README.md) package:
direct logical access, Linux mount/FUSE, Apple materialization/APFS, physically
distinct Working/Durable Stores, Fetch/Push, Branch/LayerStack history, and the
unchanged external Cloudflare `fs-bench.sh` campaign.

## 0. Mandatory pre-read, mental model, and execution law

An implementation owner must read the following completely before editing
product code. A commentary summary, plan, or historical benchmark is not a
substitute for the actual source and normative text.

### 0.1 Normative product package

Read in this order:

1. [package README](../README.md) — product model and reading order;
2. [glossary](../00-foundation/glossary.md) — exact public vocabulary;
3. [system architecture](../01-architecture/system-architecture.md) — target
   crates, dependencies, storage topology, and complete resulting file tree;
4. [algorithms and data model](../02-core/algorithms-and-data-model.md) —
   canonical algorithms, schema, authority, and Fetch/Push records;
5. [deduplication](../02-core/deduplication.md) — CAS/CDC/COW timing and physical
   accounting;
6. [operations](../02-core/operations.md) — one commit, two forks, two merges,
   two rollbacks, conflicts, and cross-tree rejection;
7. [OperationWorkspace lifecycle](../03-workspace/begin-end-lifecycle.md) —
   isolation, quiescence, custody, receipts, recovery, and cleanup;
8. [readiness contract](readiness-contract.md) — mandatory acceptance gates;
9. [efficiency and benchmarks](efficiency-and-benchmarks.md) — complexity,
   counters, timers, resources, evidence, and historical baselines; and
10. this implementation plan last.

If two product documents conflict, stop product edits, reconcile the normative
package first, then continue. Current source and historical PoC documents are
evidence, not permission to override this package.

### 0.2 Current source that must be traced before a move

Read the actual caller/callee flow, not only module names:

```text
workspace root
  Cargo.toml

canonical algorithms
  crates/layerfs-core/src/lib.rs
  crates/layerfs-core/src/object/{mod,codec,model}.rs
  crates/layerfs-core/src/content/{rope,extent,persistence}.rs
  crates/layerfs-core/src/{namespace,inode,metadata}.rs

current SQLite authority
  crates/layerfs-engine/src/{lib,integrity,publication,refs,scratch,generation}.rs

current portable/presentation overlap
  crates/layerfs-vfs/src/{lib,driver,resolver,workspace,managed_edit}.rs
  crates/layerfs-vfs/src/{mounted,materialize,capture,refresh}.rs

current Linux mounted route
  crates/layerfs-fuse/src/{lib,main}.rs

current Apple materialization route
  crates/layerfs-os/src/lib.rs
  crates/layerfs-os/src/apple/{mod,store,workspace,apfs,metadata,ffi}.rs

current public facade and product tests
  crates/layerfs-sdk/src/lib.rs
  crates/layerfs-sdk/tests/
```

Before changing one shared function, search every caller with `rg`. Preserve
existing user changes and unrelated evidence. Never use a mechanical file move
as an excuse to rewrite a proven algorithm at the same time.

### 0.3 Historical authority and source-bound evidence

Read only the evidence relevant to the owner being moved:

- [handoff freeze](../../poc/10-handoff-freeze.md) for canonical,
  authentication, publication, durability, Apple, and compaction authority;
- [Stage 1 implementation/complexity map](../../poc/13-stage1-implementation-and-complexity.md),
  [single-file campaign](../../poc/14-stage1-single-file-benchmark.md),
  [Apple edge campaign](../../poc/16-stage1-part1-apple-edge-benchmark.md), and
  [Stage 1 closure](../../poc/17-stage1-closure.md) before materialization/APFS
  work;
- [Stage 2 Linux/FUSE specification](../../poc/19-stage2-docker-linux-fuse.md),
  [Stage 2 optimization record](../../poc/23-stage2-fuse-performance-optimization.md),
  [Stage 2 implementation handoff](../../poc/24-stage2-docker-fuse-implementation-handoff.md),
  and [candidate 015 summary](../../poc/evidence/stage2-freeze-candidate-015/summary.json)
  before mount/FUSE work; and
- the unchanged external [Cloudflare `fs-bench.sh`](../../../cloudflare-computer-bench/upstream/script/fs-bench.sh)
  plus its [Docker/FUSE handoff](../../../cloudflare-computer-bench/cloudflare-docker-fuse-layerfs-handoff.md)
  before any benchmark implementation or measurement.

Historical rows stay bound to their exact source, executable/image, fixture,
Store profile, platform, and environment. They are regression references, not
proof that the target split is implemented.

### 0.4 Mental model

Keep this model in view during every phase:

```text
                         shared durable authority
                    +-----------------------------+
                    | DurableStore                |
                    | CAS + durable Branches      |
                    | Operation history + Layers  |
                    +-------------+---------------+
                                  ^
                         explicit | Fetch / Push
                                  v
                    +-------------+---------------+
                    | disk-backed WorkingStore    |
                    | local CAS + Working Branch  |
                    | OperationVersions/candidates|
                    +-------------+---------------+
                                  |
                       one private OperationWorkspace
                     /             |                 \
           direct logical      mount/FUSE       materialization/APFS
              no path       logical overlay        physical directory
                                  |
                         OperationCommit
                                  |
                         WorkingRecorded only
                                  |
                         optional explicit Push
```

Filesystem syscalls, native tool activity, close, `fsync`, workspace
finalization, and Working-only OperationCommit do not contact DurableStore.
Durable visibility occurs only through explicit Push and one exact DurableStore
head transaction. `LayerStackMerge` and `LayerStackRollback` head movement are
DurableStore-only; WorkingStore prepares candidates and requests.

Mount/FUSE and materialization/APFS are equally required product presentations:

```text
mount/FUSE
  reads canonical extents directly
  keeps private dirty ranges in a bounded disk spool
  preserves count-changing logical locality
  never materializes or captures a backing workspace

materialization/APFS
  constructs ordinary physical files for native compatibility
  captures a quiescent physical result into the same canonical model
  refreshes changed paths where safe
  reports cold/full/count-changing physical linear work honestly
```

### 0.5 Continuation and fast-iteration law

- A compile, focused-test, readiness, performance, or implementation-caused
  REVISE is intermediate: preserve the failure, diagnose the shared root cause,
  make the smallest repair, prove it with one focused check, and continue.
- Do not stop at a plan, crate scaffold, compatibility forwarding layer,
  Working-only mount, zero-row readiness result, or partial presentation.
- A terminal PASS requires every phase and both presentation campaigns below.
- A terminal external impossibility requires concrete host/platform evidence and
  exhaustion of safe in-scope alternatives; difficulty or slow code is not an
  impossibility.
- During iteration use 1/10 MiB fixtures, one touched-crate check/test, and one
  focused causal screen. Use at most 100 MiB only for declared scaling rows.
- Run one complete workspace closure and one complete authoritative campaign
  only after source freeze. Never rerun unchanged passing populations for noise
  or weaken a threshold after observation.

## 1. Current source and exact target structure

Current source evidence:

```text
crates/
├── layerfs-core/
├── layerfs-engine/     SQLite, refs, publication, integrity, compaction
├── layerfs-vfs/        portable semantics + workspace/presentation overlap
├── layerfs-fuse/       current Linux adapter
├── layerfs-os/         current Apple adapter
└── layerfs-sdk/
```

Target ownership:

```text
crates/
├── layerfs-core/
│   └── src/
│       ├── identity/ object/ cdc/ content/ namespace/ inode/ metadata/
│       └── logical/
│           ├── mod.rs
│           ├── resolver.rs
│           ├── read.rs
│           ├── mutate.rs
│           ├── diff.rs
│           └── merge.rs
├── layerfs-storage/               one SQLite/object/schema/transaction kernel
├── layerfs-working-store/         host-recoverable Branch/Operation authority
├── layerfs-durable-store/         independent system-of-record acceptance
├── layerfs-workspace/
│   └── src/operation.rs workspace.rs direct.rs driver.rs
│          quiescence.rs leases.rs receipt.rs
├── layerfs-mount/
│   └── src/driver.rs workspace.rs handles.rs dirty.rs spool.rs fuse/
├── layerfs-materialization/
│   └── src/driver.rs workspace.rs materialize.rs capture.rs refresh.rs
│          provenance.rs apfs/
├── layerfs-sync/                  explicit Fetch/Push client + server
├── layerfs-sdk/                   snapshot/version/workspace facade
└── layerfs-service/               authenticated DurableStore service boundary
```

There is no `layerfs-fs`. The exact complete file tree is frozen in
[system-architecture.md](../01-architecture/system-architecture.md).

Target dependency direction:

```text
layerfs-storage          -> layerfs-core
layerfs-working-store    -> layerfs-storage + layerfs-core
layerfs-durable-store    -> layerfs-storage + layerfs-core
layerfs-workspace        -> layerfs-core + layerfs-working-store
layerfs-mount            -> layerfs-core + layerfs-workspace
layerfs-materialization  -> layerfs-core + layerfs-workspace
layerfs-sync             -> layerfs-storage + layerfs-working-store
                            + layerfs-durable-store
layerfs-service          -> layerfs-sync(server) + layerfs-durable-store
layerfs-sdk              -> working-store + workspace + selected driver + sync
```

`layerfs-core::logical` uses the narrow generic `ObjectRead`/`ObjectStore`
contracts from `core::object::access`; `layerfs-storage` implements them. Core
does not depend on Storage. Sync owns separate client/WorkingStore and
server/DurableStore orchestration over one protocol. Service authenticates and
transports requests, opens DurableStore locally, and delegates admitted work to
Sync server handlers; clients never open or call the service-owned DurableStore.
DurableStore may be on the same host, a LAN host, or a network service; network
distance is deployment, not storage semantics.

### 1.1 Exact current-to-target move map

Move ownership once, in dependency order:

| Current source | Target owner | Rule |
|---|---|---|
| `layerfs-core::{identity,object,cdc,content,namespace,inode,metadata}` | same modules in `layerfs-core` | Preserve canonical bytes/ObjectIds and split files only where the target tree requires it |
| portable resolver/read/mutate/diff/merge code in `layerfs-vfs` | `layerfs-core::logical` | One semantic move; generic over `ObjectRead`/`ObjectStore`; delete the old implementation after parity |
| `layerfs-engine` SQL/object/publication/integrity/generation machinery | `layerfs-storage` | One concrete schema/transaction implementation; no role switch or duplicate SQL |
| current ref/workspace history policy in Engine/VFS | `layerfs-working-store` | Working Branch/Operation/candidate/lease/recovery policy only |
| new independent durable admission/head/retention policy | `layerfs-durable-store` | Compose Storage; do not copy SQL or trust Working validation |
| lifecycle/quiescence/candidate/cleanup code in `layerfs-vfs::{workspace,managed_edit}` and FUSE main | `layerfs-workspace` | Universal OperationWorkspace lifecycle; no presentation-specific dirty state |
| `layerfs-vfs::mounted` mounted inode/handle/dirty/spool state | `layerfs-mount::{workspace,handles,dirty,spool}` | Mechanical extraction before optimization; preserve counters and roots |
| `layerfs-fuse::{lib,main}` | `layerfs-mount::fuse` plus thin mount binary | FUSE request translation/lifecycle only; no second filesystem model |
| `layerfs-vfs::{materialize,capture,refresh}` | `layerfs-materialization` | Preserve full-stream correctness and exact changed-path/fallback attribution |
| `layerfs-os::apple` | `layerfs-materialization::apfs` | Apple handles, clone, supported metadata, provenance filtering, and unsafe/libc FFI only |
| `layerfs-sdk` current Apple facade | thin target `layerfs-sdk` | Exact-version reads, Operation lifecycle, Branch/LayerStack, presentation selection, Fetch/Push |
| new bounded transfer orchestration | `layerfs-sync::{client,server}` | Hash/ObjectId-first accepted-state transfer only |
| new network/auth endpoint | `layerfs-service` | Transport/authentication and server delegation only |

Temporary compatibility re-exports may exist only while a current caller is
being moved. A phase cannot close while both old and target semantic
implementations are active. Terminal source contains no `layerfs-engine`,
`layerfs-vfs`, `layerfs-fuse`, or `layerfs-os` crate; their admitted mechanisms
live under the target owners above. Historical evidence keeps its old paths.

### 1.2 Resulting repository shape

The exact full tree in [system architecture](../01-architecture/system-architecture.md)
is binding. At terminal implementation, the workspace members are:

```text
crates/
├── layerfs-core/                 canonical + portable logical semantics
├── layerfs-storage/              shared SQLite/CAS/transaction kernel
├── layerfs-working-store/        Working Branch/Operation authority
├── layerfs-durable-store/        durable shared authority
├── layerfs-sync/                 bounded Fetch/Push client + server
├── layerfs-workspace/            universal OperationWorkspace lifecycle
├── layerfs-mount/                mounted driver + Linux FUSE adapter
├── layerfs-materialization/      native physical driver + Apple/APFS adapter
├── layerfs-sdk/                  thin public facade
└── layerfs-service/              authenticated DurableStore endpoint
```

Both target presentations are built from their owning crates:

```text
layerfs-mount
  src/{lib,driver,workspace,handles,dirty,spool}.rs
  src/fuse/{mod,session,callbacks,errno}.rs

layerfs-materialization
  src/{lib,driver,workspace,materialize,capture,refresh,provenance}.rs
  src/apfs/{mod,handles,clone,metadata,ffi}.rs
```

No benchmark-only product crate, alternative filesystem representation, Python
runner, watcher, or compatibility daemon is part of the resulting tree.

## 2. Fixed ownership laws

1. Core owns canonical formats, CDC/persistent trees, and all portable logical
   stat/list/read/range/stream/readlink, mutations, candidate
   RootId/RootTransition construction, root diff, and three-root merge.
2. `core::logical` is generic over object access and remains free of SQLite,
   platform, workspace, presentation, synchronization, Branch-head, and
   LayerStack authority policy.
3. Storage is the only SQL/object/schema/integrity/transaction/compaction
   mechanism. WorkingStore and DurableStore are distinct databases/policy crates
   composing it without runtime policy selectors or duplicate SQL.
4. WorkingStore persists Operation identity, exact BranchHead/base pin,
   base-version/origin leases, workspace recovery, candidates/conflicts, and
   WorkingRecorded OperationCommit.
5. Workspace is the universal runtime isolation/admission/quiescence/
   finalization contract. Direct, mount/FUSE, and materialization/APFS are
   concrete drivers, not modes or semantic owners.
6. One host/security domain normally uses one disk-backed WorkingStore CAS
   shared by its authorized Branches and OperationWorkspaces. A host may isolate
   security domains in separate WorkingStores. Multiple WorkingStores exchange
   accepted state only through DurableStore; none is peer authority for another.
7. Sync transfers already accepted canonical/version state only. It never
   transfers a path, marker, workspace, spool, dirty map, mount, process,
   descriptor, mapping, native file, or SQLite page.
8. DurableStore independently authenticates/validates/verifies, retains pushed
   Branch/Operation history plus LayerStack state, and applies exact heads;
   WorkingRecorded is never durable proof.
9. Branch Push may create a durable Branch or advance its exact head and is
   independent of `LayerStackMerge`; LayerStack movement is a separate request.
10. Every accepted head action uses one owning-Store transaction/COMMIT; there
   is no distributed transaction, implicit sync, merge, rebase, or retry.

## 3. Public surface boundaries

Immutable snapshot reads do not begin an Operation:

```text
stat / list / read_range / stream / readlink
    -> pin exact locally verified VersionRef
    -> layerfs-core::logical + ObjectRead
    -> no OperationWorkspace, head move, or sync
```

An absent durable version requires explicit Fetch first. Every
mutation uses one OperationWorkspace. The SDK exposes no shell/npm/Bash or
agent-tool execution taxonomy. External tools run in private FUSE/APFS views;
the direct driver exposes thin `core::logical` primitives. Generic process,
descriptor, writer, and mapping guards exist only to prove quiescence. One
arbitrary-tool workspace boundary produces one candidate RootTransition; an
accepted OperationCommit records one OperationDelta.

## 4. Phase 0 — Freeze model and move map

Deliver one glossary, target tree, schema, lifecycle, dependency graph,
request/receipt model, recovery/path custody, recursive child-origin lease law,
and current-to-target file map. Freeze:

- two versions, three deltas, one commit, two forks, two merges, two rollbacks;
- WorkingRecorded versus DurablyAccepted;
- Fetch/Push as the only sync actions; Branch Push independent of
  `LayerStackMerge`;
- immediate-parent-only `ChildBranchMerge`, plus direct `LayerStackMerge` from
  any Branch depth to its inherited originating stack with source preservation;
- no `layerfs-fs` and no generic Store/workspace runtime selectors; and
- exact current evidence labels without claiming target completion.

MCTS/search/rollout policy remains an external, non-normative consumer of
Branch/version primitives.

Exit: documentation consistency PASS; zero product-code changes.

## 5. Phase 1 — Extract Workspace and presentation drivers first

Before moving portable VFS semantics, separate runtime/presentation ownership
from current `layerfs-vfs`:

### 1A. `layerfs-workspace`

Move the universal OperationWorkspace state machine, admission, quiescence,
candidate routing, cleanup, and receipts. Initially use a thin compatibility
adapter to current Engine/VFS; do not copy semantic algorithms.

Working-root custody is fixed:

```text
<working-root>/workspaces/<operation-id>-<nonce>/
├── owner
├── recovery
├── view/
└── spool/
```

The operation tree is `0700`, marker-validated without following links,
host-local, never a repository/global-temp default, and removed only by exact
ownership. Direct work may expose no path.

### 1B. `layerfs-mount`

Move mounted logical COW overlay, namespace changes, bounded spool,
handles/cursors/writer admission, and thin FUSE callbacks. `view/` is the
private mountpoint; sibling `spool/` is never visible inside it. Count-changing
edits retain logical extent locality. Every syscall terminates at the nearby
WorkingStore path: no per-syscall DurableStore RPC and no whole-file hydration
or buffer.

### 1C. `layerfs-materialization`

Move physical view, provenance, capture, refresh, exact verification,
process/writer/mapping quiescence, and Apple handles/clone/metadata/FFI. The
view is placed on the admitted APFS volume for qualified clone routes. Cold
materialization, arbitrary capture, and count-changing suffix/full fallback
remain explicitly charged.

Acceptance:

- all three drivers implement one Workspace contract without a runtime mode;
- begin/terminal receipts and exact cleanup agree;
- same-base Operations share immutable objects but not dirty state;
- mounted and materialized drivers still call current portable VFS semantics;
- memory remains independent of file/workspace/version size: disk-backed
  WorkingStore/cache/spool, <=1 MiB-style stream buffers, bounded queues and
  counters, and no in-memory workspace DB, whole-file mount hydration,
  all-extents vector, or complete namespace/object/version inventory;
- no platform/workspace code moves into Core in this phase; and
- real FUSE/APFS focused tests preserve current mechanism/counter behavior.

## 6. Phase 2 — Move remaining VFS semantics directly into Core

After Phase 1 leaves only portable semantics in current `layerfs-vfs`, move
them once into `layerfs-core/src/logical/`:

```text
resolver and exact-version stat/list/read_range/stream/readlink
create/replace/splice/truncate
namespace, symlink, hard-link, metadata rules
candidate RootId/RootTransition construction
Merkle root diff and three-root conflict/merge
portable counters/errors
```

Replace concrete Engine access with generic `ObjectRead`/`ObjectStore`. Core
must not import rusqlite, paths, libc, FUSE/APFS, workspace types, Branch/
LayerStack heads, Working/Durable policy, sync, SDK receipts, or processes.

Move direct, mount, materialization, and SDK callers to `core::logical`, prove
identical canonical roots/counters/faults, then delete old `layerfs-vfs`.
Never create a temporary FS crate or keep forwarding after all callers move.

Focused proof:

- exact snapshot-read pin with zero Operation/head/sync work;
- direct edit expected `O(B + log E + path)` and zero unaffected suffix I/O;
- bounded ObjectRead/ObjectStore batches, no all-extents/namespace collection;
- hard-link/metadata/merge models and malformed object rejection; and
- direct/FUSE/APFS supported final-state RootId equivalence.

## 7. Phase 3 — Extract `layerfs-storage`

Move current Engine substrate exactly once:

```text
SQLite connection/schema/migration
canonical object get/put and new/incumbent authentication
version/delta/transition/lease rows
bounded batches and scratch traversal
transaction/COMMIT/reconciliation primitives
generation install/reopen and compaction copy mechanism
```

Storage implements Core ObjectRead/ObjectStore and exposes typed low-level
records/transactions, not Working/Durable policy. Keep current Engine forwarding
only until source-equivalence tests pass, then remove it.

Proof: open/reopen/migrate, corruption/authentication, expected-head/lost-ack,
generation recovery/compaction, and zero canonical ObjectId/root change. Do not
add a second backend, trait factory, runtime role flag, WAL, pool, retry, or
background worker.

## 8. Phase 4 — Build `layerfs-working-store`

WorkingStore owns:

- fetched Layer/LayerStack refs and candidate policy, plus Working
  Branch/OperationVersion and scoped delta authority;
- LayerBranchFork and recursively nested ChildBranchFork;
- exact Working Branch heads, candidate Layers/conflicts, and transition request
  identities; WorkingStore never owns a LayerStack head transition;
- begin OperationId, exact head/base pin, version/origin leases, workspace
  recovery record and single-use WorkspaceTicket;
- WorkingRecorded OperationCommit/no-change/conflict/reconciliation;
- immediate-parent-only `ChildBranchMerge`, inherited originating LayerStack
  identity at every nesting depth, and candidate Layer/LayerDelta construction
  that closes over inherited state plus descendant changes;
- exact Branch Push planning that may create/advance a durable Branch without
  moving a LayerStack head, plus any-depth candidate/Push request planning for a
  DurableStore-owned `LayerStackMerge`;
- descendant-origin-lease rollback blocking/release; and
- working retention/compaction root enumeration for Storage.

Fast proof:

```text
L0 -> Branch A -> Operations A1/A2
   -> child B from A1 -> child C from B1
   -> C merges only to B; B merges only to A
   -> C may separately prepare its complete inherited candidate toward L0
   -> ancestor rollback blocked until explicit child Branch drop
   -> retained candidate Layer -> exact Push plan independent of stack merge
   -> repeated merge creates a new candidate while preserving source Branch
   -> reopen retained versions and compact
```

Integrate WorkspaceTicket/final candidate without letting WorkingStore own a
path, process, dirty map, mount, native view, or spool.

## 9. Phase 5 — Build `layerfs-durable-store`

DurableStore is a distinct database/crate composing Storage. It independently:

- authenticates every transferred new/incumbent object;
- validates version/delta/origin/request relationships;
- verifies complete requested closure;
- retains pushed Branch/Operation history and LayerStack state;
- creates or advances exact durable Branch heads independently of stack merge;
- performs exact immediate-parent `ChildBranchMerge` and inherited-origin
  any-depth `LayerStackMerge` head actions;
- returns DurablyAccepted/Conflict/Indeterminate with fresh reconciliation;
- owns durable leases/retention, backup/restore, and compaction root set; and
- rejects TrustedLocalDev and WorkingStore trust claims.

Corrupt both new/incumbent transfer rows, use stale heads, lose acknowledgement,
restart, and restore to a fresh host. Also reject cross-tree merge destinations,
stale parent heads, and two WorkingStores racing the same durable Branch head.
This is not a Storage mode or second SQL implementation.

## 10. Phase 6 — Add Fetch/Push Sync and service

Sync implements explicit bounded Fetch/Push with separate
client/WorkingStore and server/DurableStore modules over one protocol. Service
authenticates and transports requests, opens DurableStore locally, and delegates
admitted work to the Sync server module. WorkingStores never synchronize
directly with peers.

Allowed transfer:

```text
canonical objects and RootTransitions
WorkingRecorded Operation/OperationVersion/scoped deltas
accepted Branch/Layer candidates and exact head/request receipts
```

Forbidden transfer:

```text
OperationWorkspace/recovery path/owner marker/view/spool/dirty map
mount/session/handle/process/descriptor/mapping/native files
SQLite pages or database files
```

Fetch and Push negotiate hashes/ObjectIds first, stream only missing accepted
objects/history through resumable <=1 MiB-style buffers and bounded queues, and
charge unique, resumed, and retransmitted bytes. They never construct a complete
closure inventory in memory. A Branch Push may create or advance a durable
Branch; `ChildBranchMerge` and `LayerStackMerge` remain separate exact-head
actions.

Snapshot reads, mutations, mount syscalls, close/fsync, tool exit, workspace
finalization, and WorkingStore-only OperationCommit never trigger Sync or a
DurableStore RPC. Exit proof covers Push->Fetch->continue/edit/Push across two
disk-backed WorkingStores, same-durable-head conflict, bounded
negotiation/retransmission, interruption without false completeness,
independent Store authentication, transfer-without-visibility, stale-head
candidate preservation, lost-ack reconciliation, and terminal cleanup.

## 11. Phase 7 — Integrate the SDK and retire current crates

Replace the Apple-only facade with one thin product surface over exact target
owners. The SDK exposes:

```text
exact-version stat/list/read_range/stream/readlink
begin_operation / end_operation(discard | preserve | OperationCommit)
direct logical workspace primitives
mount workspace request/receipt
materialization workspace request/receipt
LayerBranchFork / ChildBranchFork
ChildBranchMerge / candidate Layer preparation / LayerStackMerge request
BranchRollback / LayerStackRollback request
Fetch / Push
exact WorkingRecorded / DurablyAccepted / Conflict / Indeterminate receipts
```

It exposes no shell, Bash, npm, editor, agent-tool, watcher, FUSE syscall, APFS
FFI, SQLite connection, raw path-cleanup, or second filesystem implementation.
External processes receive only the admitted private FUSE/APFS view path and are
supervised by the caller/runtime under the Workspace quiescence contract.

Migrate direct, mount, materialization, branch/version, and Sync callers to the
target SDK. Then remove compatibility forwarding and delete current
`layerfs-engine`, `layerfs-vfs`, `layerfs-fuse`, and `layerfs-os` from workspace
members. The phase exits only after:

- no target crate depends on a removed current crate;
- `rg` finds no active product import of an old owner;
- exact canonical roots and retained historical formats remain readable;
- SDK tests use the shipped target routes rather than private crate shortcuts;
- both presentation binaries start through `layerfs-workspace`; and
- workspace format/check/test/Clippy plus release builds pass once at source
  freeze.

## 12. Phase 8 — Release qualification

Qualify:

```text
WorkingStore + Workspace/direct + DurableStore
WorkingStore + Workspace/mount/FUSE + DurableStore
WorkingStore + Workspace/materialization/APFS + DurableStore
```

Fast iteration remains:

```text
one ownership move/root cause
-> one focused deterministic proof
-> touched-crate format/check/test/Clippy
-> zero-row custody/schedule assertion
-> short mechanism screen
-> source freeze
-> one complete release closure and exact campaign
```

Use 1/10 MiB routine fixtures and at most 100 MiB only for a declared causal
scaling row. Do not run 300/500 MiB routine workspaces, rerun unchanged source
for noise, or repeat passing full closure during iteration.

Mandatory qualification cases include:

- Push a durable Branch, Fetch it into another WorkingStore, continue/edit, and
  Push again; race two WorkingStores at the same durable head and preserve the
  loser as an exact conflict;
- parent/child parallel three-root merge, stale parent, forbidden cross-tree
  merges, direct any-depth LayerStack merge with complete inherited closure,
  source preservation, and repeated merges;
- many Branches sharing one host WorkingStore CAS, while cross-store transfer
  charges only negotiated missing plus resumed/retransmitted bytes;
- mounted small and count-changing edits with delta-sized logical work, bounded
  memory, no whole-file hydration/buffer, and zero DurableStore RPCs per
  syscall; and
- materialization/APFS capture and changed presentation refresh reported
  separately with honest full/changed-path/count-changing linear fallbacks.

Existing Part-1/Stage 1.1 APFS and Stage 2 FUSE evidence remains historical for
its exact source/artifact. It does not qualify the split, Core logical move,
universal Workspace lifecycle, or Working/Durable sync path. Preserve the
plans/rows without relabeling them.

## 13. Complete implementation ledger

Every item below is required. “Implemented” means shipped target source plus a
focused deterministic proof; “qualified” additionally means exact-source release
evidence from the applicable campaign below.

| Owner | Required implementation | Phase exit |
|---|---|---|
| `layerfs-core` | exact current canonical codecs; `ObjectRead`/`ObjectStore`; persistent extent/namespace/inode/metadata validation; logical resolver/read/mutate/diff/three-root merge | direct, mount, and materialization produce identical supported RootIds; malformed/wrong-role objects fail closed |
| `layerfs-storage` | one SQLite schema/migration; object admission; all version/delta/head/lease/receipt rows; one-writer transactions; reconciliation; generation compaction | current Store migration/reopen parity; new/incumbent corruption tests; one-COMMIT equations; no Working/Durable role field |
| `layerfs-working-store` | Working Branch/OperationVersion/OperationDelta authority; forks; immediate-parent merge; rollback; exact begin pins; origin leases; candidate Layers; recovery; tracking/outbox | restart-recoverable WorkingRecorded history, conflicts/candidates, nested branches, source preservation, compaction roots |
| `layerfs-durable-store` | independent verified admission; durable Branch create/advance; durable child merge; LayerStack merge/rollback; retention; backup/restore | fresh database Fetch recovery, stale/conflict/indeterminate proofs, no trust inheritance, fresh-host restore |
| `layerfs-workspace` | one Operation state machine; driver ticket; path custody; process/writer/mapping quiescence; candidate disposition; terminal cleanup | direct/FUSE/APFS share one lifecycle and receipt schema; siblings share no dirty state; crash recovery exact |
| `layerfs-mount` | mounted logical overlay; inode/handle/cursor state; namespace changes; bounded disk spool; Linux FUSE callbacks/session/invalidation; explicit splice control | complete Linux profile, real FUSE, no backing tree/materialization/capture, no whole-file memory, current benchmark parity |
| `layerfs-materialization` | physical materialize/capture/refresh; provenance; hard-link closure; process/writer/mapping guards; APFS handle/clone/metadata/FFI | exact A/B RootId round trips, native Bash/mmap, supported metadata, fail-closed unsupported state, honest fallback counters |
| `layerfs-sync` | one typed protocol; bounded resumable Fetch/Push; hash negotiation; missing-object streams; receipts/reconciliation | interrupted/resumed transfer, retransmission accounting, transfer-without-visibility, no complete inventory in memory |
| `layerfs-service` | authentication/transport; request limits; server delegation; DurableStore process ownership | client cannot open Durable SQLite; authenticated same-host service and fresh-process restart; no duplicated sync policy |
| `layerfs-sdk` | thin exact reads; Operation lifecycle; Branch/LayerStack APIs; presentation requests; Fetch/Push; typed receipts | public tests use only shipped facade; no private crate/benchmark bypass; old facade removed |

Canonical and product rules are implemented once. Tests, evaluators, Docker
entrypoints, and evidence finalizers may observe product counters but may not
copy algorithms, recognize benchmark paths/names, or provide a faster product
route unavailable to callers.

## 14. Mount/FUSE completion and external `fs-bench`

### 14.1 Scope and preserved baseline

The current source already has a green real-FUSE mechanism. The target task is
to preserve it while moving ownership, not to rebuild a new daemon. Candidate
015 is the regression reference only:

```text
current admitted source path
  layerfs-fuse -> layerfs-vfs::mounted -> layerfs-engine/core -> SQLite

target measured source path
  layerfs-mount::fuse -> layerfs-workspace -> layerfs-core::logical
  -> layerfs-working-store -> layerfs-storage
```

Candidate 015 observed live LayerFS median sums `3.361 s` (`/var/tmp`) and
`3.299 s` (`/tmp`), versus matched Cloudflare FUSE `7.260 s` and `7.449 s`.
These values are source-bound regression references, not evidence for the new
path and not permission to weaken the gates below.

### 14.2 Fast mount iteration

Do not use the full campaign as a debugger:

```text
changed shared function
  -> focused unit/model test
  -> target mount crate check/test
  -> real-FUSE functional oracle
  -> one causal fs-bench scenario
  -> three-scenario smoke
  -> continue implementation
```

Use these causal screens:

| Changed owner | Focused scenario |
|---|---|
| resolver/inode cache | `stat 1000 files` |
| create/publication | `create 1000 files` |
| unlink/namespace cleanup | `rm 1000 files` |
| directory tree/cursors | `mkdir tree (10x10x10)`, `find tree` |
| range/payload read | `pure read 64 MiB` |
| dirty spool/write | `write 64 MiB`, `overwrite 64 MiB` |
| combined read/write | `pure copy 64 MiB` |
| metadata/integration | `git init + commit 100 files` |

The compatibility smoke is exactly:

```text
REPS=1
WARMUP=1
RANDOMIZE_TARGETS=0
SCENARIOS=create 1000 files,stat 1000 files,pure read 64 MiB
```

It requires six rows, one sample each, zero FAIL markers, correct bytes, and
terminal mount health. Passing smoke is not a performance disposition.

### 14.3 Real-FUSE functional admission

Before timing, prove:

- native Linux ARM64 product executable and libraries, with no emulation;
- explicit LayerFS FUSE mode and startup receipt;
- `/workspace` is a dedicated kernel FUSE mount sourced through `/dev/fuse`;
- Docker inspection shows no bind or volume target at `/workspace`;
- nested files/directories, exact digests, append, truncate, rename,
  symlink/readlink, hard-link inode identity, unlink-open, directory enumeration,
  supported metadata, `mmap`, and `fsync` private recovery;
- explicit OperationCommit followed by fresh WorkingStore reopen and exact
  RootId/bytes/history;
- forced death before acceptance loses no accepted state and forced death after
  acknowledgement reopens the accepted state;
- materializations and capture scans remain literal zero; and
- handles, dirty ranges, directory cursors/changes, spool live/dead/physical,
  operation Q, Store connections, mounts, processes, journals, scratch, and
  owned containers/volumes reach their specified terminal baseline/zero.

### 14.4 Authoritative external workload

The benchmark source is immutable:

```text
path
  /Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/upstream/script/fs-bench.sh

SHA-256
  0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef
```

Do not patch, copy, or recognize it in product code. The raw script labels the
mount target `computerd`; retain that raw output and normalize the product label
to `layerfs` only in the outer verified report.

Exact complete filter:

```text
create 1000 files
stat 1000 files
rm 1000 files
mkdir tree (10x10x10)
find tree
write 64 MiB
copy 64 MiB
read 64 MiB
pure read 64 MiB
pure copy 64 MiB
overwrite 64 MiB
git init + commit 100 files
```

Authoritative settings:

```text
REPS=3
WARMUP=1
RANDOMIZE_TARGETS=1
MOUNT=/workspace
SCENARIOS=<exact filter above>
OUTPUT_JSON=<unmeasured path outside both targets>
```

Run two separate complete populations after source freeze:

```text
population A: BASE=/var/tmp   # observed Docker Linux storage class
population B: BASE=/tmp       # explicit verified tmpfs
```

Each population is one unchanged script invocation and requires exactly 24
unique rows (`12 scenarios * 2 targets`), three samples per row, zero omitted
or duplicate rows, zero ANSI-stripped FAIL markers, and independently
recomputed mean/median/p95/min/max and ratios. Script exit zero alone is not
proof because the harness intentionally does not use `set -e`.

### 14.5 Frozen Docker measurement environment

Use the candidate-015 comparison class unless a new environment is explicitly
declared:

```text
Docker context              desktop-linux
container platform          linux/arm64
container architecture      aarch64, native
init                        enabled
CPU                         --cpus 1
memory                      --memory 512m
PIDs                        --pids-limit 512
FUSE                        --device /dev/fuse --cap-add SYS_ADMIN
privileged                  false
network                     none
/workspace                  LayerFS FUSE only; no bind/volume
/var/tmp                    Docker Linux-VM native/overlay control, observed
/tmp                        tmpfs rw,nosuid,nodev,size=1g,mode=1777
benchmark tooling           coreutils, findutils, util-linux, git
tracing/sampling            disabled during authoritative rows
```

The build may use network only before the image is sealed. The measured
container uses no network. Evidence is written to an unmeasured in-container
path and extracted after the run. No measured regular file exceeds 100 MiB.

### 14.6 Live mount gates

For `/var/tmp`:

```text
sum of LayerFS medians (SL)     <= 4.5 s
sum ratio (Rsum)                <= 2.85
geometric mean ratio (G)        <= 7.0
population spread               <= 1.15
```

For `/tmp`:

```text
SL                              <= 4.5 s
Rsum                            <= 3.10
G                               <= 7.75
population spread               <= 1.15
```

Resource/lifecycle gates:

```text
OOM / OOM-kill deltas           0
population CPU throttle ratio   <= 5%
mount lock-wait ratio            <= 10%
whole-cgroup peak               <= 512 MiB
largest admitted request        <= 1 MiB
materializations/captures       0 / 0
operation Q terminal            0
Store connections terminal      0
complete population wall        preferred <60 s; hard <=120 s
```

The full campaign measures live WorkingStore mount behavior only. Report three
separate boundaries for durable product claims:

```text
T_live              unchanged fs-bench/tool wall
T_working_recorded  quiescence + candidate + Working OperationCommit
T_durably_accepted  explicit Push + Durable verification/head transaction
```

Never add network/Durable latency to each syscall or relabel `T_live` as durable.
A separate persisted operation workload must record all three boundaries plus a
fresh WorkingStore Fetch/remount verification.

## 15. Materialization/APFS completion and qualification

### 15.1 Native environment

Run materialization qualification natively on Apple Silicon macOS:

```text
architecture                 arm64
filesystem                   admitted APFS data volume
WorkingStore database/CAS    outside the Operation view, on admitted storage
Operation view               private 0700 same-volume directory when clone is claimed
network during measurement   none
largest regular file         <=100 MiB
routine fixture              1/10 MiB
FUSE/Docker                  absent from APFS rows
```

Bind every result to macOS build, APFS volume/device/UUID, source tree, release
executable, Store profile, integrity policy, fixture digest, and environmental
metadata policy. Exact `com.apple.provenance` is ignored only by the Apple
adapter as host-regenerated environmental metadata. Supported xattrs round-trip
exactly; other retained unsupported/protected metadata fails closed. Never
wildcard `com.apple.*`.

### 15.2 Required native operation chain

Qualify both `Verified` and explicitly labeled `TrustedLocalDev` populations;
never combine them:

```text
fresh WorkingStore
  -> accepted Branch/root A
  -> begin materialized OperationWorkspace
  -> cold materialize A to private APFS view
  -> real native reads, Bash, mmap, rename/link/symlink/metadata activity
  -> process/writer/mapping quiescence
  -> capture exact candidate B
  -> Working OperationCommit B
  -> WorkingStore close/reopen exact B/history
  -> refresh retained physical A view toward B where requested
  -> explicit Push B
  -> fresh WorkingStore Fetch B
  -> rematerialize and verify exact RootId/bytes/namespace/metadata
```

Required cases:

- cold 100 MiB materialization and reconstruction;
- same-size overwrite, append, truncate, count-changing insert/delete, and full
  temporary replacement at beginning/middle/end/random locations;
- exact no-op capture/refresh with literal zero payload/native/CDC/publication
  work where the normalized operation is intentionally a no-op;
- changed-root refresh with Merkle changed paths and safe clone/patch where
  admitted;
- explicit different-length/full fallback, without claiming APFS suffix shift
  locality;
- nested directories, non-UTF-8 names where admitted, symlinks, stable logical
  hard-link closure, modes, mtime, supported Apple xattrs, root metadata, and
  environmental provenance filtering;
- Bash/process tree, native editor-like temp-file rename, writable `mmap`, open
  descriptor, escaped writer, and quiescence refusal paths;
- exact old A and new B reads, fork/divergence/rollback/history, compaction, and
  fresh reopen; and
- incomplete physical refresh/capture marked unusable for continued authority,
  with discard/rebuild only and ownership-safe cleanup.

### 15.3 APFS regression gates

Use the retained Stage 1 targets until new prospective product requirements are
approved before measurement:

| Operation | Gate |
|---|---:|
| 100 adjacent 1 MiB canonical ranges | `>=250 MiB/s` |
| 100 MiB streamed import | `>=150 MiB/s` |
| 100 MiB replace existing | `>=150 MiB/s` |
| 100 MiB reconstruction | `>=200 MiB/s` |
| Trusted 4 KiB logical edit | p50 `<=15 ms` |
| native 4 KiB edit + Working commit | p50 `<=20 ms` |
| reopen/head ready | p50 `<=4 ms` |
| cold managed 100 MiB materialization | `>=150 MiB/s` |
| exact normalized no-op | p50 `<=5 ms`, literal zero work |
| locally derived 4 KiB A->B refresh | p50 `<=25 ms` |
| complete focused campaign | preferred `<60 s`, hard `<=120 s` |

Report live native edit, capture, candidate construction, Working commit,
optional Push, and fresh Fetch/rematerialization separately. Cold
materialization, arbitrary external capture, Verified scrub, and explicit
full/count-changing fallback remain honestly byte-linear. Passing APFS cannot
substitute for FUSE, and passing FUSE cannot substitute for APFS.

## 16. Working/Durable distributed qualification

Use physically distinct databases and `StorageId`s even when the first service
campaign runs on one host:

```text
WorkingStore A SQLite/CAS
WorkingStore B SQLite/CAS
DurableStore SQLite/CAS opened only by layerfs-service
```

Qualify:

1. Push a newly created top-level Branch and its ordered Operation chain;
2. Fetch it into WorkingStore B, mount and materialize the same exact version,
   continue/edit/OperationCommit, and Push the next head;
3. publish/fetch recursively nested child Branches from exact durable fork refs;
4. race A and B from one durable Branch head: one Push wins, one returns exact
   Conflict with its Working history preserved;
5. perform child-to-immediate-parent three-root merge after both advanced;
6. reject sibling/cousin/grandparent-skipping/unrelated/cross-LayerStack merge;
7. prepare an any-depth complete inherited candidate and perform a
   DurableStore-owned LayerStackMerge, preserving the source Branch;
8. repeat the merge after later source/destination work;
9. lose Push acknowledgement and reconcile by request identity without duplicate
   history/head movement;
10. interrupt/resume Fetch and Push, counting missing/resumed/retransmitted
    objects and bytes with no false complete receipt;
11. corrupt transferred new and incumbent objects and fail closed;
12. back up/restore DurableStore, create a fresh WorkingStore, Fetch, and verify
    exact Branch/LayerStack roots, history, leases, mount view, and APFS view; and
13. compact each Store independently from its complete retained-root set.

Every transfer reports:

```text
hash/ref negotiation time
closure traversal time
IDs checked / present / missing
unique missing objects and bytes
resumed/retransmitted objects and bytes
receiver authentication/verification work
head transaction/COMMIT/reconciliation
terminal queues/buffers/connections/residue
```

No filesystem syscall, close, `fsync`, tool exit, or Working OperationCommit may
produce a DurableStore RPC. Fetch/Push latency is reported separately from live
presentation and WorkingRecorded latency.

## 17. Terminal completion contract

The package is fully implemented only when one frozen source satisfies all of
the following without compatibility-path or benchmark-only exceptions:

- the exact target crate/file/dependency tree exists and old owner crates are
  removed from active workspace members;
- canonical bytes/ObjectIds and legacy reads remain unchanged unless an
  explicitly approved versioned migration says otherwise;
- direct, mount/FUSE, and materialization/APFS all use
  `layerfs-core::logical`, one Storage substrate, one Working policy, and one
  OperationWorkspace contract;
- real FUSE passes functional/restart/resource closure and the two authoritative
  unchanged external `fs-bench` populations;
- native APFS passes the complete materialize/edit/capture/refresh/reopen,
  metadata/hard-link/quiescence, performance, and cleanup campaign;
- both presentations complete Working OperationCommit, explicit Push, fresh
  WorkingStore Fetch, and exact reconstruction from DurableStore;
- durable Branch create/advance, nested branch continuation, parent merge,
  any-depth originating LayerStack merge, conflicts, rollback, leases,
  reconciliation, backup/restore, and compaction pass;
- all required algorithmic counters prove no unaffected suffix work on the
  logical route, bounded memory/queues, no complete inventories, and no hidden
  materialization/capture or per-syscall Durable RPC;
- one workspace fmt/check/test/Clippy/release closure and platform-specific
  Linux/macOS tests pass on the exact release artifacts; and
- source manifests, environment receipts, raw rows, independent verification,
  failure ledger, resource observations, cleanup, and checksums are complete and
  terminal residue is at baseline/zero.

Required terminal result classes remain separate:

```text
PASS_WORKING_MOUNT
PASS_WORKING_MATERIALIZATION
PASS_WORKING_OPERATION_COMMIT
PASS_DURABLE_BRANCH_PUSH_FETCH
PASS_DURABLE_MERGE_HISTORY
PASS_END_TO_END_FRESH_RECOVERY
PASS_PRODUCT_READY
```

`PASS_PRODUCT_READY` requires every preceding class on the same frozen target
architecture source. A REVISE in either presentation or the distributed path is
not a reason to delete passing evidence, weaken a gate, stop, or call the other
presentation sufficient; repair the shared or presentation-specific cause and
continue to terminal PASS.

## 18. Work deliberately not added

- `layerfs-fs` or another temporary/parallel semantic crate;
- another storage backend or duplicate SQL/schema implementation;
- runtime Store/workspace policy selectors;
- SQLite page replication or network-filesystem SQLite;
- WorkingStore peer authority or per-syscall DurableStore RPCs;
- in-memory workspace databases, whole-file mount hydration/buffers, or complete
  namespace/extent/object/version inventories;
- WAL, writer pools, implicit retry/sync, background workers;
- online/destructive in-place GC;
- platform/workspace/authority types in Core;
- another canonical filesystem per platform;
- per-LayerStack SQLite files, pack/compression redesign;
- SDK shell/tool taxonomy or agent orchestration; or
- routine large benchmark farms.

The shortest correct path is presentation/runtime extraction first, then one
direct semantic move into Core logical, one Storage substrate, explicit
Working/Durable policies, and accepted-state synchronization.
