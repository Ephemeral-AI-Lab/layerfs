# LayerStack cold-build source tree

This is the smallest target tree that preserves the three-store model, fourteen
public operations, shared merge rules, exact schemas, CAS/CDC correctness, and
SRP. It is a clean replacement, not a shape-preserving refactor.

[model.md](model.md) fixes the architecture and schemas; [rule.md](rule.md)
fixes public operation semantics.
[db-transaction-transfer-model.md](db-transaction-transfer-model.md) is binding
for the low-level Store API execution contract, fixed membership/search SQL,
batching, transactions, visibility, deduplication, large-transfer streaming,
resource bounds, and verification. This file assigns each requirement one
production owner; no owner may introduce a second mechanical contract.

## 1. Package decision

Four storage packages are sufficient:

| Package | Sole responsibility | Production LOC review estimate |
|---|---|---:|
| `layerfs-storage-core` | Shared IDs/records, exact schemas/SQL, merge-base and three-way algorithms, CAS admission, contracts, and byte framing. | 2,400 |
| `layerfs-branch-store` | Branch/Commit persistence, layered snapshot reads, Branch operations, and Branch transfer orchestration. | 1,250 |
| `layerfs-stack-store` | Owned linear StackHistory creation/head CAS, history pulls, Stack transfer, signing, and optional remote endpoint. | 1,150 |
| `layerfs-layer-store` | Complete central persistence, LayerHistory provisioning/head CAS, transfer serving, and optional remote endpoint. | 950 |

Existing application integration remains in `layerfs-sdk`; it is not another
storage owner. `layerfs-workspace` is retained as a zero-table, non-Store
transient runtime between presentation and BranchStore. It owns generic COW,
dirty spool and the direct commit/discard lifecycle, but no database or
publication authority. The former SDK 350 LOC and storage-plus-SDK 6,100 LOC
figures are review estimates, not automatic PASS/FAIL thresholds.

Package and aggregate LOC estimates must never cause correct cohesive code to
be compressed, tests to be moved mechanically, or responsibilities to be
split/merged. Minimality means every surviving line has one necessary owner and
duplicate algorithms, types, round trips, compatibility code, and stubs are
deleted. Architecture, canonical-model reuse, transaction/transfer correctness,
measured speed/space, SRP, public API minimality, and evidence take priority.

```text
layerfs-mount -> layerfs-sdk -> layerfs-workspace -> layerfs-branch-store -> configured parent
application   -> layerfs-sdk       -> layerfs-branch-store -> configured parent

configured parent = layerfs-layer-store
                 or layerfs-stack-store -> layerfs-layer-store

all three Stores -> layerfs-storage-core -> layerfs-core
```

No store crate depends on another store crate.

### Why there is no `layerfs-transfer`

The cold design needs one manual contract codec, bounded framing, and
missing-object batches. The codec stays with `records.rs`, membership and
operation-scoped batching stay with `admission.rs` plus Store operations, and
the small byte-only carrier stays in `wire.rs` inside the already-shared
storage core. A separate crate would add a manifest,
dependency edges, error conversions, public types, and test surface without an
independent release boundary or consumer.

Remote store files use the same `wire.rs` over standard `Read`/`Write` streams.
They own socket/listener lifecycle; the shared module owns bytes only. If a
future second transport cannot use that interface without duplication, extract
it then. Do not preserve or rename `layerfs-sync` now.

## 2. Exact production tree

```text
crates/
├── layerfs-storage-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 # declarations/re-exports only; <= 80 LOC
│       ├── ids.rs                 # storage IDs; validate/delegate ObjectId to layerfs-core
│       ├── records.rs             # 7 records + AddResult + manual bounded endpoint codec
│       ├── schema.rs              # Store open/owner queue, exact DDL/indexes, fixed prepares
│       ├── sql.rs                 # typed Branch/Commit reads plus ref/history inserts and CAS
│       ├── merge_base.rs          # indexed typed ancestry/base reads and base resolution
│       ├── three_way.rs           # Clean or first lexicographic Conflict
│       ├── merkle.rs              # tree walk/pruning/result + rare logical-file digest equality
│       ├── admission.rs           # typed/Object membership, authenticated admission, inserts
│       ├── contract.rs            # 14-operation values, endpoint envelopes, shared errors
│       └── wire.rs                # bounded frames, checksum, byte I/O, backpressure only
│
├── layerfs-branch-store/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 # declarations/re-exports only; <= 60 LOC
│       ├── branch_store.rs        # handle/open/schema admission
│       ├── create_branch.rs       # create from Layer, Stack, or local Commit
│       ├── commit.rs              # transient changes -> Commit + Branch-head CAS
│       ├── merge.rs               # merge-base + three-way + target-head CAS
│       ├── branch_transfer.rs     # two-ID pull create/fast-forward + push orchestration
│       ├── layered_read.rs        # local-new/parent streaming adapter; no materialization
│       └── snapshot.rs            # Branch head/root view for Workspace and SDK callers
│
├── layerfs-stack-store/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 # declarations/re-exports only; <= 60 LOC
│       ├── stack_store.rs         # handle/open/schema admission
│       ├── writer.rs              # signer config and StackHistoryId/key match
│       ├── create_history.rs      # create_stack_history_from_layer
│       ├── add_stack.rs           # accepted-Branch freeze + three-way + one head CAS
│       ├── history_pull.rs        # pull_layer_history + pull_stack_history
│       ├── commit_pull.rs         # pull_commit_history pinned-head DAG pull
│       ├── branch_transfer.rs     # serve pull_branch / accept push_branch
│       ├── push_stack.rs          # signed suffix + Branch/Commit provenance transfer
│       ├── remote.rs              # decode/dispatch/encode; no domain algorithms
│       └── bin/
│           └── layerfs-stack-store.rs # process bootstrap only; <= 80 LOC
│
├── layerfs-layer-store/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 # declarations/re-exports only; <= 60 LOC
│       ├── layer_store.rs         # handle/open/schema admission
│       ├── provision.rs           # canonical-empty LayerHistory genesis
│       ├── add_layer.rs           # accepted-Branch freeze + route/three-way/head CAS
│       ├── transfer.rs            # serve pulls; accept Branch/Stack transfers
│       ├── remote.rs              # decode/dispatch/encode; no domain algorithms
│       └── bin/
│           └── layerfs-layer-store.rs # process bootstrap only; <= 80 LOC
│
├── layerfs-sdk/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                 # declarations/re-exports only
        ├── direct.rs              # BranchStore -> LayerStore composition
        ├── stacked.rs             # BranchStore -> StackStore -> LayerStore
        ├── endpoint.rs            # opaque endpoint re-export + Layer/Stack publication selection
        └── binding.rs             # Workspace/materialization caller -> BranchStore/Branch binding
│
├── layerfs-workspace/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                 # declarations/re-exports only
│       ├── model.rs               # transient node/data model; no Store records
│       ├── overlay.rs             # generic transient namespace/COW operations
│       ├── file_io.rs             # layered reads, dirty spool, bounded write/truncate/fsync
│       ├── changes.rs             # bounded final Change set and spool cleanup
│       ├── lifecycle.rs           # direct Workspace commit/discard; no Store authority
│       └── resource.rs            # bounded transient-memory/spool policy
│
└── layerfs-mount/
    └── src/
        ├── lib.rs                 # declarations/re-exports only
        ├── adapter.rs             # shared FUSE state + attribute/errno translation
        ├── filesystem.rs          # fuser callback implementation only
        ├── inode_table.rs         # kernel inode-number mapping only
        ├── handles.rs             # kernel open-handle mapping only
        ├── mount_session.rs       # Linux FUSE session/bootstrap only
        └── bin/layerfs-mount.rs   # process bootstrap only
```

The four storage packages plus SDK retain 42 production Rust files including
five `lib.rs` files and two named Store binaries. Workspace and mount are
non-Store consumers with the separate thin trees above; neither changes the
four-storage-package count or owns a database.

## 3. Fourteen operations and file ownership

Every public operation remains explicit as a function, but related short
operations share one cohesive file.

| Public operation | Production owner |
|---|---|
| `create_branch_from_layer` | `layerfs-branch-store/create_branch.rs` |
| `create_branch_from_stack` | `layerfs-branch-store/create_branch.rs` |
| `create_branch_from_commit` | `layerfs-branch-store/create_branch.rs` |
| `commit` | `layerfs-branch-store/commit.rs` |
| `merge` | `layerfs-branch-store/merge.rs` |
| `pull_branch(source_branch_id, local_branch_id)` | `layerfs-branch-store/branch_transfer.rs`; parent handler serves pinned source |
| `push_branch` | `layerfs-branch-store/branch_transfer.rs`; parent handler is store transfer file |
| `pull_commit_history` | `layerfs-stack-store/commit_pull.rs` |
| `create_stack_history_from_layer` | `layerfs-stack-store/create_history.rs` |
| `pull_layer_history` | `layerfs-stack-store/history_pull.rs` |
| `pull_stack_history` | `layerfs-stack-store/history_pull.rs` |
| `add_stack` | `layerfs-stack-store/add_stack.rs` |
| `push_stack` | `layerfs-stack-store/push_stack.rs`; receiver is `layerfs-layer-store/transfer.rs` |
| `add_layer` | `layerfs-layer-store/add_layer.rs` |

The SDK route matrix is intentionally asymmetric: `Direct` exposes the seven
legal direct operations (`create_branch_from_layer`,
`create_branch_from_commit`, `commit`, `merge`, `pull_branch`, `push_branch`,
and `add_layer`); `Stacked` exposes the other thirteen operations and does not
expose `create_branch_from_layer`. Their union is the frozen fourteen, and the
API doctest compile-fails the forbidden Stacked route.

`contract.rs` owns domain values plus internal phase envelopes. It does not
execute operations. `records.rs` owns their manual bounded codec; `wire.rs`
frames opaque bytes and does not own record semantics.

## 4. One owner per invariant

| Invariant | Sole production owner |
|---|---|
| Tagged topology identities + untagged ObjectId delegation | `layerfs-storage-core/ids.rs` |
| Store ownership queue, exact table/column/index manifests, fixed prepared shapes | `layerfs-storage-core/schema.rs` |
| Typed Branch/Commit reads and ref/history insert/CAS primitives | `layerfs-storage-core/sql.rs` |
| Typed ancestry/base reads and cross-base merge-base selection | `layerfs-storage-core/merge_base.rs` |
| Entry-level Clean/first-conflict decision | `layerfs-storage-core/three_way.rs` |
| Merkle traversal/pruning/build and unequal-root file digest fallback | `layerfs-storage-core/merkle.rs` |
| Receiver membership, authenticated admission, immutable insert and race partitioning | `layerfs-storage-core/admission.rs` |
| Fixed bitmap/batch planning and operation-scoped missing-only frontier | `layerfs-storage-core/admission.rs` plus the calling Store operation |
| Framing, checksum, bounded byte I/O, and backpressure only | `layerfs-storage-core/wire.rs` |
| Domain operation values plus internal endpoint envelopes | `layerfs-storage-core/contract.rs` |
| SDK embedded/remote composition | `layerfs-sdk/endpoint.rs`; `Direct::from_parts` and `Stacked::from_parts` route Add Layer and Add/Push Stack through explicit opaque endpoints, while the reusable TCP client remains owned by `stack-store/remote.rs` |
| StackStore/LayerStore endpoint dispatch | each Store's `remote.rs` |
| Branch target-head transaction | `layerfs-branch-store/merge.rs` |
| Pull Branch fresh insert/ancestor-only local CAS | `layerfs-branch-store/branch_transfer.rs` |
| StackHistory ID-based writer-key check | `layerfs-stack-store/writer.rs` |
| Accepted Branch freeze + single StackHistory head transaction | `layerfs-stack-store/add_stack.rs` |
| Accepted Branch freeze + single LayerHistory head transaction | `layerfs-layer-store/add_layer.rs` |
| Read-only LayerStore copy of Stack head | `layerfs-layer-store/transfer.rs` |
| Signed Stack suffix provenance enumeration | `layerfs-stack-store/push_stack.rs` |
| Missing-only provenance validation/admission before copied head | `layerfs-layer-store/transfer.rs` |
| Generic transient COW, dirty spool, staged mutations, commit/discard | `layerfs-workspace/overlay.rs` + `lifecycle.rs` |
| Kernel inode/open-handle mappings and FUSE translation | `layerfs-mount/inode_table.rs` + `handles.rs` + `filesystem.rs` |

Store operations supply verified roots, own their transaction/CAS, and
interpret shared outcomes. No store may copy merge-base, three-way, Merkle,
identity, schema, or admission logic.

`layerfs-workspace` calls BranchStore snapshot/object/Commit APIs but never
opens SQLite or calls Push/Add. `layerfs-mount` calls Workspace only; it owns
kernel presentation state, not the generic overlay or spool algorithm.

Existing `layerfs-core` files remain the only large-file content owners; they
are reused, not duplicated into the new storage tree:

| Existing file | Sole retained responsibility |
|---|---|
| `layerfs-core/src/content/rope/build.rs` | Streaming full-write FastCDC and canonical extent construction. |
| `layerfs-core/src/content/rope/edit.rs` | Replacement-only CDC plus authenticated extent splice. |
| `layerfs-core/src/content/rope/read.rs` | Streaming logical reads and 64-entry payload-range batches. |
| `layerfs-core/src/object/access.rs` | Object read/store traits and in-memory defaults; SQLite bulk paths use storage-core adapters. |

## 5. Dependencies

| Package | May depend on | Must not depend on |
|---|---|---|
| `layerfs-storage-core` | `layerfs-core`, SQLite library, already-approved hash/codec dependencies | Any store crate, SDK, network runtime |
| `layerfs-branch-store` | `layerfs-storage-core`, `layerfs-core` | StackStore or LayerStore crate |
| `layerfs-stack-store` | `layerfs-storage-core`, `layerfs-core`, one audited signature crate | BranchStore or LayerStore crate |
| `layerfs-layer-store` | `layerfs-storage-core`, `layerfs-core`, the same signature crate | BranchStore or StackStore crate |
| `layerfs-sdk` | The three Store crates, `layerfs-storage-core` domain types/endpoints, `layerfs-workspace`, and `layerfs-materialization`; `layerfs-core` only in tests | Direct SQLite access, old storage crates |
| `layerfs-workspace` | `layerfs-branch-store`, `layerfs-storage-core`, `layerfs-core` | SQLite, StackStore/LayerStore mutation, Push/Add, transport |
| `layerfs-mount` | `layerfs-sdk`, `fuser`, signal handling | Store SQL, generic COW/change generation, Push/Add |

Remote endpoints exchange `layerfs-storage-core::contract` values through
`wire.rs`; stores communicate by local typed endpoint values or bytes, never by
linking another store crate.

Store operations call `sql.rs`/`admission.rs` for table-specific typed/Object
membership, separate missing bitmaps, authenticated bytes, and idempotent
inserts. They coalesce those results before calling `wire.rs`. The wire sees
only planned announcements/filtered frames and cannot choose a table, decide
deduplication, delete sender objects, or persist transfer state.

`layerfs-core` remains the sole FastCDC implementation and parameter source
(8/16/32 KiB plus hashed profile ID), the sole authenticated canonical object
codec, and the owner of transient file COW. Full writes CDC the full input;
incremental edits CDC only replacement bytes and splice authenticated old
extent ObjectIds without surrounding/full-file normalization. Equal logical
bytes reached by different edit histories may have different FileState roots.

There is one persisted content identity: the existing untagged 32-byte
`ObjectId` over canonical encoded bytes. Codec/context authenticates object
kind; the raw-byte `ChunkId` alias is not a storage API or transfer stage.
Pull/Push enumerate stored ObjectIds and canonical rows and invoke CDC zero
times.

The unequal-root regular-file slow path remains in `merkle.rs`: length check,
then at most three transient `ContentDigestWriter` values populated through
layered batched reads. It applies source/base, current/base, then source/current
Commit, layered read, Pull/Push, Add Stack, and Add Layer all pass through
storage-core admission/query functions; no Store may define another chunker,
content ID, object codec, or full-copy fallback.

## 6. Performance fit without new artifacts

The files below are the sole owners for the corresponding binding sections of
[db-transaction-transfer-model.md](db-transaction-transfer-model.md). A test
that proves a low-level clause must exercise these owners rather than a test-only
or endpoint-specific duplicate.

The fixed tree already has every owner needed for efficient multi-DB transfer:

| Requirement | Existing owner and review estimate |
|---|---|
| Set-based query/bitmap, SQLite batch adapter, deferred scratch, idempotent object/fact admission | `storage-core/admission.rs`, <= 320 LOC |
| Prepared typed SQL and final CAS statements | `storage-core/sql.rs`, <= 225 LOC |
| Count/byte bounds and coalesced typed/Object one-turn-ahead pipeline | `storage-core/admission.rs` plus Store operation owners |
| Canonical frame/checksum/backpressure | `storage-core/wire.rs` |
| Direct Branch Push/Pull stream reuse | `branch-store/branch_transfer.rs`; keep the two-ID pull and operation-scoped push flow cohesive rather than targeting a small-file number |
| Stack history/dependency Pulls | `stack-store/history_pull.rs` + `commit_pull.rs`, within 280 LOC combined |
| Stack Push, frozen Branch/Commit provenance, and LayerStore copy result | `stack-store/push_stack.rs` + `layer-store/transfer.rs`; split only if distinct surviving responsibilities emerge |
| One SQLite connection per Store handle | existing `*_store.rs` handle field; no connection class/pool |
| One TCP stream per operation/Push-Add sequence | existing `remote.rs`; no semantic session object/table |

Dedup negotiation stays in Store operation + storage-core admission. `wire.rs`
receives already-filtered frames and owns only bytes, backpressure, framing, and
checksum. Push never deletes sender objects.

`admission.rs` contains one private SQLite object adapter, not a public
abstraction. It buffers canonical puts to the deterministic 128-object/4 MiB
target (one valid object up to `MAX_OBJECT_BYTES` may be a singleton), flushes
with one prepared idempotent transaction per batch, and makes buffered objects
readable while the rope/tree builder constructs parents. It also implements
set-based authenticated reads. SQLite paths must not use `ObjectRead`'s
per-ID default batch loop or execute one `ObjectStore::put` transaction per
chunk on bulk paths; focused counters reject either fallback. The existing
rope walker may still issue counted individual indexed structural-node gets,
while its payload ranges use 64-entry batches. Do not claim a generic 512-ID
structural walker unless `merkle.rs` actually implements and tests one.
In-memory test stores may keep the simple defaults.

For local new bytes, `layerfs-core` canonical-encodes once and ObjectId-hashes
once into a trusted staged `(id, canonical_bytes)`; the SQLite batch adapter
does not hash it again. A remote sender streams an existing canonical row
unchanged after at most one stored-row authentication; the receiver
hashes/authenticates once and never re-encodes. Only a scratch-spill reread may
re-authenticate, with a separate counter. No new module/type is needed for
these counters; they are test instrumentation around existing codec/admission
calls.

`sql.rs` requires SQLite 3.35 or newer and prepares one 512-placeholder
ObjectId existence query plus the widest 128-row/four-column fact insert (512
binds) at Store open; any failure rejects open.
`admission.rs` sorts/deduplicates every input,
binds trailing placeholders to `NULL`, remaps unordered results, and emits a
fixed 512-bit missing bitmap. There are no 1..512 statement variants. Object
inserts use bounded shapes up to 128 rows/256 binds and the byte/singleton rule,
with `ON CONFLICT(object_id) DO NOTHING RETURNING object_id, length(bytes)`.
The returned IDs are the admitted set; set subtraction reports race-existing
IDs/bytes without a second query or metrics state.

Typed history, Branch/AddResult, Commit, Stack, and Layer ancestry/membership
use their exact indexed table queries/pages and separate position-preserving
bitmaps. They never bind typed
IDs to the fixed `objects` statement, and ObjectIds never enter a typed query.
Store operation files own coalescing those typed pages (`H`) and Object pages
(`P_o`) into dependency-ordered wire turns `P <= P_o + H`; `wire.rs` only
frames the already-planned announcements and bitmaps.

All store handles set WAL plus `synchronous=FULL`. `sql.rs` owns the one frozen
auto-checkpoint page constant selected by P1 measurement; an explicit PASSIVE
checkpoint, if retained, can run only between operations. The DB file is local
to its Store process; remote machines use endpoints and never raw SQLite over
NFS. A second owning process/handle fails `StoreBusy`; there is no owner/lease
table.

Store operation files borrow the writer gate only for a bounded object/fact
transaction or final/folded CAS, then release it. No writer gate or SQLite write
transaction spans network, CDC, encode/hash, signature, or three-way work. One
read-only recursive CTE cursor is the explicit exception to connection-release
wording: it may retain its read snapshot while streaming <=512-row ancestry
pages across the endpoint, but holds no writer gate or write transaction. The
source CTE is one statement; `H` counts emitted typed pages, so `2H` remains a
conservative source/destination SQL bound. A separate fair per-Store operation queue admits one active
working set before buffers/head preflight: Add reads the head once, evaluates
once, and exact-CASes once; queued callers later see the new head. An
injected/illegal CAS loss returns `HeadMoved` without internal retry. Independent
Store databases have independent queues. RETURNING race accounting does not
justify a pool. Test-only load instrumentation reports queue wait and per-stage
lock time without a metrics module/table.

Transferred Commit/Stack/Layer/AddResult facts and frozen accepted Branch
provenance are admitted in bounded prepared batches counted by `F`. Only a Branch ref or
history/copied head is exposed in the last bounded transaction, or a standalone
small CAS when no admission batch exists. Public reads start at exposed
refs/heads, so mere row presence is not product visibility. Locally authored
Add Stack/Add Layer fold one result node, one AddResult, and one head movement
into their last object transaction or one metadata-only transaction.

`add_stack.rs`/`add_layer.rs` treat an accepted BranchId as single-publication:
the AddResult freezes the receiving Store's same-ID Branch row at the accepted
Commit without another column. `push_stack.rs` follows the existing
`add_results(result_id)` index for each Stack in the suffix and enumerates that
frozen Branch, its Commit DAG, and root closure into the signed provenance
frontier. `layer-store/transfer.rs` validates and missing-only admits that
closure before exposing the copied Stack head. This is also the central source
for later `pull_commit_history`; no new operation, record, or file is needed.

`admission.rs` establishes root-presence-as-closure-certificate during first
admission. `merkle.rs` trusts that certificate on normal Add, reads zero
descendants for a repeated mapping or equal-root result, and walks only unequal frontier nodes
for divergence. Full traversal belongs to first admission; scrub/recovery is
deferred.

`merkle.rs` builds only into a private `DeferredObjectStore` supplied by
`admission.rs`, with the shared 8 MiB in-memory budget and a disposable named
scratch spill beyond it. Clean results are authenticated/batch-admitted before
the folded/standalone final CAS; Conflict discards scratch and writes zero rows.
Unexpected-process-failure scratch cleanup is deferred. This is one private
helper inside existing files, not a crate, table, public type, or durable
journal.

Branch preflight uses the layered adapter for base/Merkle reads and the
at-most-three ContentDigestWriter streams. Tests count individual indexed
structural gets separately from 64-entry payload batches and reject only an
accidental per-payload default loop. The final write transaction performs no
endpoint read.

No new crate, production file, public class/struct, table, or column is needed
for the performance model. Bloom filters, compression, a global CAS, transfer
tables, connection pools, an async runtime, and a second local/remote algorithm
remain absent until evidence proves a stated budget cannot be met.

`merge_base.rs` uses indexed recursive SQLite CTEs with `UNION` deduplication
and a transient B-tree, paging only final maximal candidates. It never builds
an unbounded Rust ancestor set. `three_way.rs` follows canonical lexicographic
path order and returns one first `Conflict { path, base, current, candidate }`.
The existing files and public error enum cover both; no conflict collection or
new record type is added.

All stores freeze one page-cache bound from P1 evidence and use
`temp_store=FILE`, so deep CTE dedup spills rather than consuming unbounded RAM.
Tests count application buffers, SQLite page-cache high-water, and temp bytes.

## 7. Writer configuration and binaries

`layerfs-stack-store/writer.rs` generates/loads the configured signing key,
checks each supplied `StackHistoryId` against that key, and signs Stack suffix
pushes. There are no `OwnedStackHistory` or `ReadOnlyStackHistory` wrapper
types. The public verification digest is embedded in `StackHistoryId`; the
private key never enters SQLite or transfer payloads.

```text
embedded create -> transparent signer generation/persistence + IDs
local reopen    -> same signer reload
pull history    -> same public IDs, no signer transfer
clone writable DB -> unsupported
```

The two named binaries may only parse arguments, open one store, create a
listener, call `remote.rs`, and map startup errors to exit codes. No SQL,
history logic, merge, CAS traversal, or transfer planning belongs in a binary.

## 8. SRP and size gates

| Gate | Requirement |
|---|---|
| Cohesion | A file may group short functions only when they share one invariant and data flow. |
| Cohesion review | File size alone is not a split trigger; review and split only when distinct surviving responsibilities remain. |
| Hard cap | CI rejects any handwritten production `.rs` file above 1,000 lines. |
| Size guidance | Do not split a cohesive 350–999-line file merely to reduce its line count. |
| `lib.rs` | Declarations/re-exports only; no SQL, I/O, algorithms, transactions, or orchestration. |
| Module layout | No `mod.rs`; named module roots only when a directory is actually needed. |
| Binary layout | No `main.rs`; named `src/bin/*.rs` bootstrap files only. |
| Catchalls | No `product.rs`, `common.rs`, `utils.rs`, `manager.rs`, `repository.rs`, factory layer, or generic repository abstraction. |

Do not split a 40-line cohesive operation into request, validator, executor,
outcome, and adapter files. Do split when one file owns unrelated invariants,
even when it remains below the 1,000-line cap. Total production LOC and
deletion of duplicate algorithms matter more than small files.

## 9. Test tree

Keep tests broad enough to cover invariants but fewer than production modules:

```text
crates/layerfs-storage-core/tests/
├── schema.rs             # exact 3/9 + 8/24 + wrong-shape rejection
├── merge.rs              # recursive-CTE merge base + first-conflict + history isolation
├── cas_pipeline.rs       # ObjectId/CDC/COW, batch adapters, pruning, dedup matrix
└── wire.rs               # frame/checksum/incomplete-frame rejection only

crates/layerfs-branch-store/tests/
├── branch.rs             # anchors, two-ID Pull outcomes, commit, zero-copy reads
└── merge.rs              # same/cross-base, unequal-root logical equality, CAS race

crates/layerfs-workspace/tests/
└── lifecycle.rs          # direct commit/discard over the transient overlay; zero DB state

crates/layerfs-stack-store/tests/
├── history.rs            # seed, linear CAS, stale-base merge, conflicts
└── transfer.rs           # Pulls, signer, signed suffix + accepted Branch provenance

crates/layerfs-layer-store/tests/
├── layer.rs              # genesis, both sources, WrongSourceRoute, CAS race
└── transfer.rs           # central completeness, provenance/copy validation, no Add

crates/layerfs-sdk/tests/
└── topology.rs            # direct/stacked local/remote and caller binding
```

Every non-trivial algorithm keeps a focused runnable test; duplicating the
production file tree one test file at a time is not required.

## 10. Workspace replacement map

Treat all ten current workspace crates as replacement input, not target
architecture.

| Existing member | Cold-build action |
|---|---|
| `layerfs-core` | Keep only proven filesystem/CAS/CDC primitives; remove storage-topology logic. |
| `layerfs-storage` | Delete; replace with new `layerfs-storage-core`. |
| `layerfs-working-store` | Delete; replace with new `layerfs-branch-store`. |
| `layerfs-durable-store` | Delete; replace with new `layerfs-layer-store`. |
| `layerfs-sync` | Delete; `wire.rs` replaces its only justified byte behavior. |
| `layerfs-service` | Delete; named store binaries replace it. |
| `layerfs-server` | Delete if present; no standalone server package exists. |
| `layerfs-workspace` | Retain and rewrite as the sole zero-table transient COW/spool/lifecycle owner; remove every Store/database authority. |
| `layerfs-materialization` | Keep only the minimal new SDK integration; no topology ownership. |
| `layerfs-mount` | Keep only the minimal new SDK integration and named binary. |
| `layerfs-sdk` | Rewrite composition to explicit direct/stacked constructors. |

`tools/layerfs-eval` is adapted after the vertical slices pass; it is not a
source architecture to preserve.

## 11. Final structural proof

```text
cargo metadata --no-deps
rg --files crates/layerfs-{storage-core,branch-store,stack-store,layer-store,sdk,workspace,mount} \
  | rg '(^|/)(mod|main|product|common|utils|manager|repository)\.rs$'
find crates/layerfs-{storage-core,branch-store,stack-store,layer-store,sdk,workspace,mount} \
  -name '*.rs' -print0 | xargs -0 wc -l
cargo fmt --all -- --check
cargo test -p layerfs-storage-core
cargo test -p layerfs-branch-store
cargo test -p layerfs-stack-store
cargo test -p layerfs-layer-store
cargo test -p layerfs-sdk
cargo test -p layerfs-workspace
cargo test -p layerfs-mount
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The forbidden-file search must return nothing for the four storage packages,
SDK, Workspace, or mount. No handwritten production file may exceed 1,000
lines. Files below that cap still fail SRP when they own unrelated
responsibilities; cohesive 350–999-line files are not split for size alone.
Workspace metadata must
contain no old storage, sync, or service package. `layerfs-workspace` remains
as the sole non-Store transient runtime and must have no SQLite dependency,
database file, or production table.
