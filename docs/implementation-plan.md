# LayerStack cold implementation plan

> **Historical and superseded.** The binding V2 implementation order and
> terminal gates are in [`v2/spec.md`](v2/spec.md).

This plan replaces the existing storage topology with the architecture and
schemas in [model.md](model.md), public operation semantics in
[rule.md](rule.md), binding low-level Store/SQLite/transfer mechanics in
[db-transaction-transfer-model.md](db-transaction-transfer-model.md), and
production ownership in [source-tree.md](source-tree.md). It is not a migration,
compatibility project, or minimal diff.

The implementation order is therefore architecture -> operation contract ->
DB/transfer contract -> named production owner. This plan may stage those
requirements but may not weaken or fork them. If repeated low-level prose here
diverges, `db-transaction-transfer-model.md` wins and this plan must be amended;
the low-level document may not change the fourteen public operations or the
three-store architecture.

Architectural minimality applies to production crates, modules, LOC, types,
fields, dependencies, tables, columns, and runtime paths—not to documentation
length. Keep diagrams, rationale, failure rules, and raw proof requirements
when they prevent implementation drift.

## 1. Terminal target

| Dimension | Frozen target |
|---|---:|
| Public operations | 14 |
| Storage packages | 4 |
| Separate transfer package | 0 |
| Final workspace members | 10: nine crates plus `tools/layerfs-eval` |
| BranchStore schema | 3 tables / 9 columns |
| StackStore schema | 8 tables / 24 columns |
| LayerStore schema | 8 tables / 24 columns |
| Persistent topology records | 7 records + AddResult |
| New storage production files | 42 including SDK composition, five thin `lib.rs`, two named binaries |
| New/replacement storage production LOC | approximately 6,100 review estimate; not a terminal gate |
| File-size guidance | preserve cohesive files through 1,499 LOC; split on responsibility boundaries |
| Hard per-file cap | 1,500 production LOC, excluding `sql.rs` and `schema.rs` |

The current checkout contains approximately 89,488 non-test Rust lines and
112,294 total Rust lines across current crates/tools. Those lines and the ten
current workspace crates are replacement input, not a structure or LOC floor
to preserve.

Final non-test workspace review estimates:

```text
layerfs-core                         11,500
new storage packages + SDK           6,100
layerfs-materialization              8,000
layerfs-workspace                    1,000
layerfs-mount                        8,000
tools/layerfs-eval                      500
------------------------------------------
estimated total                      35,100 LOC
```

These package and aggregate figures are review guidance, not automatic
PASS/FAIL thresholds. Tests are budgeted by invariant and may exceed the
production estimate. The
plan does not preserve the current 35k-line evaluation tool or legacy topology
fixtures merely because they exist.

## 2. Cold-build rules

1. Freeze `model.md`, `rule.md`, `db-transaction-transfer-model.md`, and
   `source-tree.md` together; every low-level Store/SQL/transfer test maps to a
   binding DB/transfer requirement and one named owner.
2. Delete obsolete topology crates and workspace edges before implementing new
   behavior.
3. Reuse only proven `layerfs-core` filesystem, canonical object codec,
   ObjectId, sole FastCDC profile, authenticated COW extent splice, and
   ContentDigestWriter behind passing focused tests.
4. Implement one vertical slice at a time. Do not keep old and new paths alive
   in parallel.
5. No compatibility adapters, legacy schema readers, dual writes, feature-flag
   fallbacks, or aliases for retired Store names.
6. No generic repository, manager, factory, service layer, role superclass,
   optional-table framework, or one-implementation interface.
7. No HA, leader election, StackHistory authority transfer, GC, rollback,
   backup, migration, or offline queue work in this build.
8. No Workspace/tool-operation persistence. The retained `layerfs-workspace`
   is a zero-table transient runtime for COW, spool, staged mutations, direct
   commit/discard, and bounded resources only.
9. Do not optimize benchmark changes inside noise. Correctness, asymptotic
   bounds, query plans, and bytes moved gate the build before wall-clock tuning.
10. The experiment assumes no server/network failure. Incomplete frames are
    rejected without admission; automatic reconnect/resume, lost-ack handling,
    crash matrices, and recovery benchmarks are deferred.

```text
freeze four binding docs
      -> delete old topology
      -> shared storage core
      -> direct embedded slice
      -> stacked embedded slice
      -> byte protocol + optional remote binaries
      -> SDK/integration closure
      -> performance and workspace proof
```

## 3. Phase P0 — deletion-first workspace reset

P0 begins with a contract cross-check before deleting or writing code:

| Binding source | P0 extracts and freezes |
|---|---|
| `model.md` | Three stores, history shapes, identities, 3/8/8 schemas. |
| `rule.md` | Fourteen operation names, legal routes, outcomes, public invariants. |
| `db-transaction-transfer-model.md` | Store API mechanics, fixed search/membership SQL, batches, transactions/visibility, dedup, large transfers, resource bounds, and contention/order proofs. |
| `source-tree.md` | One production owner for every extracted requirement. |

P0 rejects any low-level requirement without an owner, any owner implementing a
second mechanical path, or any DB/transfer statement that changes public
semantics.

### Remove completely

```text
crates/layerfs-storage
crates/layerfs-working-store
crates/layerfs-durable-store
crates/layerfs-sync
crates/layerfs-service
crates/layerfs-server   # if present
```

Remove `layerfs-vfs` if it remains as an empty/non-member legacy directory.
Delete their tests, features, imports, and workspace dependencies rather than
moving old modules into new crates.

### Temporarily remove from the workspace build

Until the new SDK slice exists, temporarily exclude these consumers rather
than adding adapters:

```text
layerfs-materialization
layerfs-mount
layerfs-sdk
tools/layerfs-eval
```

Re-add each only after its new explicit dependency compiles. The workspace must
never compile by retaining old Store paths beside new ones.

### Preserve only proven primitives

`layerfs-core` and the rewritten zero-table `layerfs-workspace` remain as
primitives. Run Core's focused
canonical codec, ObjectId, filesystem tree, and CDC tests. Storage/history/
publication behavior found there is deleted or moved; the shared three-way and
merge-base rules must be implemented fresh in `layerfs-storage-core`.

Freeze COW semantics before storage work: full writes CDC the entire supplied
stream; range replacement CDCs replacement bytes only and reuses authenticated
old extent slices. Do not add localized normalization or full-file rechunking.
Equal final logical bytes reached through different edit histories may have
different FileState roots; P1's semantic-digest merge fallback handles that
without new persisted state.

P0 gate:

```text
test -s docs/db-transaction-transfer-model.md
cargo test -p layerfs-core
rg -n 'WorkingStore|DurableStore|DurableStoreCache|layerfs_sync' crates/layerfs-core
```

The grep must find no retained topology implementation. A manual/CI contract
matrix must also show all fourteen operations unchanged and every binding
DB/transfer requirement mapped to an exact source-tree owner before P1 begins.

## 4. Exact production artifacts and LOC review estimates

The exact production files are frozen in `source-tree.md`. Review estimates by
package:

| Package/module | LOC review estimate |
|---|---:|
| `layerfs-storage-core` total | 2,400 |
| IDs + records | 250 |
| Schema + typed SQL | 450 |
| Merge-base | 280 |
| Three-way + Merkle | 600 |
| CAS admission + private SQLite/deferred adapters | 320 |
| Contracts + wire framing | 400 |
| Re-exports | 100 |
| `layerfs-branch-store` total | 1,250 |
| Handle/open + layered read | 250 |
| Branch creation + Commit | 300 |
| Cross-base merge | 300 |
| Pull/Push Branch | 200 |
| Branch snapshot/layered view | 200 |
| `layerfs-stack-store` total | 1,150 |
| Handle/open + signer | 250 |
| History creation + pulls | 280 |
| `add_stack` | 220 |
| Branch/Stack transfers | 250 |
| Remote endpoint + binary | 150 |
| `layerfs-layer-store` total | 950 |
| Handle/open + genesis | 220 |
| `add_layer` | 260 |
| Pull/Push serving | 270 |
| Remote endpoint + binary | 200 |
| `layerfs-sdk` topology rewrite | 350 |
| `layerfs-workspace` transient runtime | 1,000 |

Consumer integration-delta review estimates, outside the approximate 6,100
storage/API figure:

```text
layerfs-materialization <= 150 LOC
layerfs-workspace       <= 1,000 LOC
layerfs-mount           <= 1,000 LOC
tools/layerfs-eval      <= 250 LOC
```

If a consumer needs more, delete old topology-facing code before adding a new
adapter layer.

These are estimates, not targets or terminal gates. Do not compress correct
code, create dense god files, move tests mechanically, or split/merge
responsibilities to hit them. First delete duplicate algorithms, low-level
public protocol, compatibility code, stubs, and wrong ownership. A current
storage-plus-SDK count around 7.9k is a review signal only; architecture,
canonical-model reuse, transaction/transfer correctness, measured speed/space,
SRP, public API minimality, and test evidence take priority.

Public-type budget across the four storage packages:

```text
8 persisted record structs
6 new typed storage IDs (ObjectId remains in layerfs-core)
3 Store handles
2 StackHistory handles (owned/read-only)
1 Stack signing capability
1 Parent endpoint value
1 AddLayerSource enum
1 shared operation command enum
1 shared operation outcome enum
1 shared error enum
1 three-way outcome
1 merge-base outcome
--------------------------------
Public-type counts are review signals, not numeric gates. Application-facing
access remains the fourteen domain operations; phase stats, counters,
membership bitmaps, frames, and endpoint request/response values are internal
implementation protocol and are not SDK surface.
```

Do not create one request/result struct per operation unless a value crosses the
wire and cannot be represented by the two shared contract enums.

Dependency budget:

| Dependency | Decision |
|---|---|
| `layerfs-core` | Reuse canonical filesystem/codec, untagged ObjectId, sole FastCDC profile, authenticated COW extent splicing, and ContentDigestWriter. |
| `rusqlite` | Reuse for all three schemas/WAL transactions; Store open requires SQLite >=3.35 for RETURNING. |
| `blake3` | Reuse existing domain-separated content hashing. |
| One audited public-signature crate | Allowed only for remotely verifiable StackHistory writer attestations. |
| `serde` / `serde_json` | Do not use in the four storage crates; `records.rs` owns the one manual bounded contract codec and `wire.rs` only frames bytes. |
| Async/network runtime | None initially; standard `Read`/`Write` loopback is sufficient. |
| UUID/random crate | None; `ids.rs` assembles UUIDv7 layout from `SystemTime` plus SQLite `randomblob` and tests the canonical bits. |

No other new production dependency is approved by this plan.

## 5. Exact schemas

### BranchStore: 3 tables / 9 columns

```text
objects(object_id PK, bytes)
commits(commit_id PK, root_id, parent_id NULL, merge_parent_id NULL)
branches(branch_id PK, head_commit_id, base_id)
```

### StackStore and LayerStore: 8 tables / 24 columns

```text
objects(object_id PK, bytes)
commits(commit_id PK, root_id, parent_id NULL, merge_parent_id NULL)
branches(branch_id PK, head_commit_id, base_id)
layer_histories(history_id PK, head_layer_id)
layers(layer_id PK, history_id, parent_id NULL, root_id)
stack_histories(history_id PK, base_layer_id, head_stack_id)
stacks(stack_id PK, history_id, parent_id NULL, root_id)
add_results(source_id PK, result_id)
```

No additional state, migration, metric, closure, Workspace, conflict, owner,
lease, session, request, or Receipt table is permitted.

Structural indexes:

```text
UNIQUE layers(history_id) WHERE parent_id IS NULL
UNIQUE layers(history_id, parent_id) WHERE parent_id IS NOT NULL
UNIQUE stacks(history_id) WHERE parent_id IS NULL
UNIQUE stacks(history_id, parent_id) WHERE parent_id IS NOT NULL
```

Reverse indexes exist only when used by a named query and confirmed by
`EXPLAIN QUERY PLAN`:

```text
commits(parent_id)
commits(merge_parent_id)
add_results(result_id)
```

Do not create duplicate Layer/Stack indexes when the unique indexes satisfy the
same query plan. Do not add indices solely to collect benchmark statistics.

The `objects` table keeps ordinary rowid layout. A 2026-08-29 local SQLite
comparison inserted 100,000 random 32-byte IDs with 256-byte payloads in one
FULL/WAL batch, five fresh databases per shape. Ordinary rowid had a 0.29 s
median and 36,159,488-byte median; `WITHOUT ROWID` had a 0.34 s median and
36,380,672-byte median. Exact-ID lookup was indexed in both (`sqlite_autoindex`
versus `PRIMARY KEY`). On this binding workload ordinary rowid was about 15%
faster and 0.6% smaller, so the current DDL is frozen; do not extrapolate that
choice to other tables without evidence.

Schema gate:

```text
fresh open -> exact manifest equality
wrong table -> reject
extra table -> reject
wrong column/order/constraint -> reject
BranchStore workspace/tool-op table count -> zero
```

## 6. Fourteen APIs

```rust
create_branch_from_layer(layer_history_id, layer_id)
create_branch_from_stack(stack_history_id, stack_id)
create_branch_from_commit(source_branch_id, source_commit_id)
commit(branch_id, expected_head, changes)
merge(source_branch_id, target_branch_id, expected_target_head)
pull_branch(source_branch_id, local_branch_id)
push_branch(branch_id)
pull_commit_history(branch_id)
create_stack_history_from_layer(layer_history_id, layer_id)
pull_layer_history(layer_history_id, through_layer_id)
pull_stack_history(stack_history_id, through_stack_id)
add_stack(stack_history_id, branch_id, commit_id)
push_stack(stack_id)
add_layer(layer_history_id, source)
```

`source` is only:

```text
BranchSource(branch_id, commit_id)
StackSource(stack_id)
```

A successful BranchSource AddResult freezes the receiving Store's same-ID
Branch row at the accepted Commit. Later identical Push may be UpToDate but may
not move that ref; `add_results[source_id]` plus `branches.head_commit_id`
derives the accepted mapping without another column. `push_stack` uses the
existing `add_results(result_id)` index to enumerate every suffix Stack's frozen
Branch/Commit/root provenance.

Use the shared outcomes and error enum from `rule.md`; do not create per-store
versions. Preserve generic `HeadMoved<I>`, `WrongHistory<H>`,
`WrongSourceRoute`, `ReadOnlyHistory<H>`, `NoCommonBase`,
`AmbiguousMergeBase`, `MissingBaseData`, `Conflict`, `Integrity`, and
`StoreBusy`.

### Atomic admission and visibility boundaries

| Operation | Atomic visibility rule |
|---|---|
| Create Branch | Anchor Commit insert-if-absent + Branch insert |
| Commit | Commit insert + exact Branch-head CAS |
| Merge | Fast-forward CAS, or merge Commit insert + target-head CAS |
| Provision LayerHistory | Canonical-empty genesis Layer + LayerHistory/head |
| Create StackHistory | Seed Stack + StackHistory/head |
| Add Stack | One Stack per newly accepted Branch, including same-root Stack + AddResult + exact Stack-head CAS; repeated mapped source writes nothing |
| Add Layer | Layer or no-op result + AddResult + exact Layer-head CAS |
| Push Branch | After object closure and bounded immutable Commit admission, expose Branch ref/same-ID head in last bounded transaction |
| Push Stack | Admit the signed Stack suffix plus every mapped frozen Branch ref, accepted Commit DAG, and root closure missing from LayerStore; fold copied-head CAS only after complete provenance |
| Pull | Admit immutable facts in bounded unreachable batches; fold imported history/Branch ref visibility into the last bounded transaction |
| Pull Commit History | No mutable ref/head; terminal pinned Commit is the last fact and all-known performs zero writes |

CAS failure or conflict rolls back every locally authored candidate/AddResult
and head/ref from that attempt. Cross-store object and immutable
Commit/Stack/Layer/AddResult batches admitted before the final transaction may
remain unreachable; they are safe, PK-idempotent facts and are not
product-visible until reachable from an exposed ref/head. No recovery journal
or staging table is implemented; server/network recovery is deferred.

## 7. Shared CAS + CDC pipeline

This section schedules the binding mechanics in
[db-transaction-transfer-model.md](db-transaction-transfer-model.md); it does
not define an alternate local path, remote path, SQL shape, or transaction
boundary.

Reuse the existing, sole FastCDC profile from `layerfs-core`:

```text
minimum 8 KiB / target 16 KiB / maximum 32 KiB
canonical hashed profile ID
```

Freeze one persisted content identity:

| Identity | Meaning |
|---|---|
| `ObjectId` | Existing untagged 32-byte hash of authenticated canonical object bytes; SQLite key and transfer identity |

Canonical encodings authenticate object kind through codec/context, and
FileState records the mapping/CDC profile. The existing raw-byte `ChunkId` alias
is removed from the cold storage API; it is not persisted, referenced by an
extent, announced, or transferred.

One coherent CAS pipeline serves Commit, layered reads, Pull, Push, Add Stack,
and Add Layer. CDC runs only when new logical bytes enter a local CAS:

```text
new or rewritten byte stream
 -> sole FastCDC chunker/profile
 -> authenticated canonical type-domain encoding
 -> ObjectId over canonical bytes
 -> children-before-parent admission
 -> known-subtree pruning
 -> missing-ID batches
 -> idempotent INSERT by ObjectId
 -> metadata/ref/head last
```

Pull/Push instead enumerate stored roots/ObjectIds and transmit receiver-missing
stored canonical rows exactly. Sender CDC, re-encoding, and logical-file hashing
counts must remain zero during transfer.

No code path may store raw bytes under an ObjectId, derive object kind from the
untagged digest, use the raw ChunkId alias as storage identity, use a second CDC
parameter set, eagerly materialize a base, or fall back to a full closure copy.

### Incremental COW and merge equality

Keep the existing minimal edit shape:

```text
authenticated old prefix
    + FastCDC(new replacement byte stream only)
    + authenticated old suffix
    -> extent split/concat/rebalance
```

Normal edit work is `O(x + t)` for replacement bytes `x` and touched
extent-tree work `t`; `cdc_bytes_scanned == x`. It performs zero old-suffix
payload reads and no surrounding/full-file rescan. Equal final bytes produced
through different edit histories may have different FileState roots and
payload layouts.

ObjectId comparison remains the three-way fast path. For a regular-file leaf
that remains divergent, outside every write transaction:

1. compare logical lengths;
2. stream each distinct base/current/source root at most once through existing
   `read_all` + `ContentDigestWriter` using the layered set-based adapter;
3. cache at most three transient digests;
4. apply `semantic_eq(source, base) -> current`, then
   `semantic_eq(current, base) -> source`, then
   `semantic_eq(source, current) -> current`, else `Conflict`.

This rare path is worst-case `O(3*B_file)` bytes for maximum file length
`B_file`, `S` individual indexed structural
rope-node reads, and `O(sum(ceil(E_i/64)))` payload-range batches for the
distinct roots. It uses `O(1)` memory and no new persistent row, ID, or public
type. It intentionally uses whole-stream digests; a paired early-exit cursor or
new batched structural walker is deferred until measurement justifies its extra
code.

Admission ownership:

```text
Local Commit:
    CDC newly supplied bytes outside transaction
    -> canonical-encode once + ObjectId-hash once into trusted staged pair
    -> retain authenticated unchanged extent ObjectIds
    -> private SQLite adapter packs bounded batches
    -> prepared INSERT ... ON CONFLICT DO NOTHING
    -> no per-chunk pre-query

Cross-store transfer:
    operation announces bounded typed page and/or ObjectId page
    -> receiver performs table-specific typed/Object membership queries
    -> receiver returns separate typed/Object missing bitmaps
    -> sender authenticates each stored row at most once and sends it unchanged
    -> receiver hashes/authenticates each missing frame once; never re-encodes
    -> prepared INSERT ... ON CONFLICT DO NOTHING handles races
```

The private SQLite adapter in `admission.rs` buffers `ObjectStore::put` calls,
makes buffered objects readable to the builder, and flushes prepared batches.
Its set-based authenticated read implementation replaces `ObjectRead`'s
per-ID default loop. The default may remain for in-memory tests but is forbidden
on SQLite paths; focused counters prove there is no query or transaction per
chunk.

The local SQLite adapter trusts staged `(ObjectId, canonical_bytes)` pairs and
does not hash them again. Scratch-spill reread may re-authenticate for
corruption safety and reports that separately. Focused counters require one
normal encode/hash for each new local object, at most one sender
authentication, one receiver authentication, and zero transfer CDC/re-encode.

Use WAL. Admit payload/children before trees/parents, then immutable
Commit/Stack/Layer facts in bounded dependency order. Transferred signed
AddResults/frozen Branch provenance may be admitted in `F` batches but remain
unreachable until the copied head. Locally authored AddResult, Branch ref, and
history/copied-head visibility are last with exact CAS. No persistent transfer
or staging table is allowed.

Root row presence is the closure certificate because first admission validates
children before parent. Normal Add authenticates typed manifests/root IDs and
does not full-walk known descendants: repeated/no-op Add performs zero
descendant reads; divergence visits only unequal Merkle frontier nodes. Full
traversal belongs to first admission; scrub/recovery is deferred.

Three-way result building never writes speculative objects to SQLite. The
shared Merkle builder targets one private bounded `DeferredObjectStore` in
`admission.rs`: memory plus disposable scratch spill after the combined 8 MiB
three-way budget. On `Clean`, authenticate and admit its objects in `J` batches,
then execute the final metadata/head transaction. On `Conflict`, discard it and
assert every production table has zero row delta. Unexpected-process-failure
scratch cleanup is deferred. This adds no table, crate, public type, or durable
state.

All three stores set `journal_mode=WAL` and `synchronous=FULL`; no benchmark or
test configuration may weaken durability. A ref/head receiver performs
`max(1, J + F)` durable commits by folding visibility into its last admission
batch. `pull_commit_history` has no mutable visibility row and performs `J + F`
commits, including zero when everything is already known. P1 benchmarks a fixed
`WAL_AUTOCHECKPOINT_PAGES` threshold under these exact batches and freezes the
chosen constant. Include automatic-checkpoint spikes in p95. An explicit
checkpoint, if evidence later requires one, is `PASSIVE` and only between
operations—never while the final CAS transaction is open.

### Fixed batch and memory budgets

```text
ID_BATCH_COUNT       = 512
OBJECT_BATCH_COUNT   = 128
OBJECT_BATCH_BYTES   = 4 MiB
FACT_BATCH_COUNT     = 128
FACT_BATCH_BYTES     = 64 KiB
FINAL_METADATA_BYTES <= 64 KiB
FINAL_METADATA_STATEMENTS <= 8

O = max(OBJECT_BATCH_BYTES, MAX_OBJECT_BYTES) = 16 MiB currently
transfer buffers     < 34 MiB per active operation
three-way traversal + DeferredObjectStore memory <= 8 MiB
application working set < 42 MiB
SQLITE_PAGE_CACHE_BYTES = benchmark-frozen P1 bound
SQLite temp_store = FILE
bounded total < 42 MiB + SQLITE_PAGE_CACHE_BYTES + fixed SQLite overhead
```

These are SDK/storage-core constants in the initial build, not user settings or
database fields. A deterministic dependency-ordered greedy packer stops before
the next row crosses the count or byte bound. One valid canonical object larger
than 4 MiB and no larger than `layerfs-core::MAX_OBJECT_BYTES` (16 MiB) occupies
a singleton batch. No operation builds a complete closure, filesystem,
ancestor set, or conflict list in application memory. Tests record application
buffers, SQLite page-cache high-water, and transient-file bytes.

Store open requires SQLite 3.35 or newer and prepares one exact
512-placeholder ObjectId existence query plus the widest 128-row/four-column
fact insert (512 binds). Inputs are sorted/duplicate-free,
short pages bind trailing `NULL`s, unordered results are remapped, and the reply
is a fixed 512-bit bitmap. Do not generate 1..512 existence variants. Object
insert shapes are bounded to 128 rows/256 binds plus the byte/singleton rule and
use:

```sql
INSERT ... ON CONFLICT(object_id) DO NOTHING
RETURNING object_id, length(bytes)
```

The returned set proves newly admitted IDs/bytes; `sent_missing - returned`
proves race-existing IDs/bytes without a second existence query or metric table.

### SQL and transaction budgets

```text
A_t = typed ancestry/membership rows for exact typed table t after pruning,
      including history, Branch, Commit, Stack, Layer, and AddResult provenance
H   = emitted typed pages, sum_t ceil(A_t / 512) over nonempty table sets;
      a source recursive CTE may emit them from one read-only statement
P_o = actual 512-ObjectId membership pages after known-root pruning
P   = actual coalesced dependency-ordered wire turns after piggybacking typed
      and Object announcements; P <= P_o + H
J   = actual object insert batches emitted by the count+byte packer
F   = immutable typed-fact batches plus frozen provenance-Branch batches
L   = actual Store-endpoint layered-parent read turns during preflight
D   = final metadata statements, D <= 8
C   = merge-base recursive CTE statement count, 1 through 3
S   = actual indexed structural-node reads by existing logical walkers
E   = payload extents read across logical streams
G   = actual 64-entry payload-read batches
```

The fair per-Store operation queue admits one active working set. Add reads the
history head only after entering that queue, evaluates once, and exact-CASes
once. Queued callers later evaluate against the new head. An injected/illegal
head movement rolls back and returns `HeadMoved`; there is no internal retry.

Each `P_o` page has at most 512 ObjectIds. A max-of-ceilings expression is
only a lower bound for greedy byte packing and must not be used as `J`. `F` is
the actual deterministic packing count under fixed fact bounds.
Each `H` page is homogeneous to one exact typed table. Paged history,
Branch/AddResult, Commit, Stack, and Layer provenance belongs to `H`; point
current-head/scope/attestation preflights remain `operation_preflight` outside
`H`. `F` includes frozen provenance-Branch batches instead of hiding them in
`D <= 8`.

| Path | SQL statements | Write transactions |
|---|---:|---:|
| Cross-store ref/head receiver | `P_o + H + J + F + D` | `max(1, J + F)`; visibility folds into last batch |
| `pull_commit_history` receiver | `1 + 2H + P_o` indexed queries; writes `J + F` | `J + F`, including zero when all known |
| Cross-store end-to-end | `2H + 2P_o + 2J + 2F + D + operation_preflight`, plus `L` | receiver bounds above |
| Local Commit | `J + 2` | `max(J, 1)`; append metadata to final local batch |
| Add Stack/Add Layer with new objects | `J + 3` | `max(J, 1)`; fold candidate/AddResult/head into last object batch |
| UpToDate Add | constant lookup + one AddResult insert | 1 |
| Conflict | reads only | 0 |

One `P_o` page means one `objects` primary-key membership query and one Object
bitmap. A typed `H` page queries only its exact relevant typed table and has a
separate position-preserving typed bitmap. Typed IDs never use the `objects`
statement, and ObjectIds never use typed tables. Store orchestration coalesces
them into `P <= P_o + H` wire turns. One
object/fact/frozen-provenance Branch batch means one prepared multi-row
idempotent insert. Transferred Commit/Stack/Layer/AddResult facts and frozen
accepted Branch rows are dependency-ordered and closure-complete before the
final ref/head transaction. The final visibility transaction
contains at most eight statements and 64 KiB of metadata and is folded into the
last object/fact batch; with no batch, it is one small transaction.

For a clean Add, candidate/AddResult/head share the last `J` transaction; when
`J = 0`, they use one metadata transaction. CAS loss rolls back that last batch
and typed rows, while earlier closure-complete object batches remain unreachable
and reusable. Conflict never reaches admission and writes zero rows.

`L` includes actual layered-parent read turns used by Branch Commit/Merge
preflight: existing rope traversal contributes its counted individual indexed
structural reads, while payload ranges use 64-entry batches. Storage-core may
claim 512-ID Merkle frontier batches only where `merkle.rs` implements and tests
a real batched walker.
Embedded/already-local dependencies cost zero network RTT; otherwise preflight
costs at most `L` turns on the reused parent stream. Do not add per-object
payload requests or separately charge a synthetic `3 * digest_turns`. Final
writes have zero network I/O; unavailable data returns `MissingBaseData`.

The writer gate is acquired only for a bounded object/fact transaction or the
final/folded CAS transaction. No writer gate or SQLite write transaction spans
network wait, CDC, encoding, hashing, canonical authentication, signature
check, or Merkle traversal. One read-only recursive CTE cursor may retain its
SQLite read snapshot while <=512-row ancestry pages cross the endpoint; it
holds no writer gate or write transaction. The source CTE is one statement,
`H` counts emitted pages, and the `2H` SQL term remains conservative. Hard
lock-size bounds are
128 objects/4 MiB for an object transaction, except a valid object up to 16 MiB
may be a singleton; immutable facts are 128 rows/64 KiB; final visibility is
eight statements/64 KiB. Tests record lock duration; a transaction that grows
with history or closure size fails regardless of wall-clock noise.

Reference lock-duration budgets, measured over repeated warm WAL runs and
including automatic-checkpoint spikes:

```text
target object/fact-batch transaction p95 <= 25 ms
standalone visibility-only p95           <= 10 ms
```

Folded visibility uses its object/fact batch p95 class, not an additional 10 ms
wall bound. Its incremental work stays at `D <= 8`, 64 KiB, and at most `1.25x`
isolated CPU time for those prepared statements. An oversize singleton is
normalized only against an isolated FULL+WAL transaction of the same byte
count, not the 4 MiB absolute target. On a slower runner, the hard gate is at
most `1.25x` the matching isolated transaction. Use the later benchmark-noise
rule before diagnosing smaller differences.

### Connection and RTT budgets

Each Store process owns a local lock-safe SQLite file, reuses its admitted
connection, and serializes writes. Other machines use the Store endpoint and
never open the raw SQLite file through NFS/shared storage. A second owning
process/handle fails promptly with `StoreBusy`; no owner/lease table is added.
Each remote endpoint reuses one TCP stream for an operation and for immediate
Push-then-Add sequences. There is no pool, async runtime, session ID, session
table, or connection-bound semantic authority.

Each handle admits one active transfer/mutation buffer set; additional callers
enter one fair queue before allocation. Per-handle working memory is therefore below 42 MiB
at current `MAX_OBJECT_BYTES`, plus the benchmark-frozen SQLite page-cache bound
and fixed connection overhead, rather than scaling with connected clients.

The single owner should make negotiated raced-existing rows nearly zero;
RETURNING/set subtraction remains defensive PK-idempotence for test-injected
insert interleaving, not a reason to add writer connections or a pool. RTT and
p95 service-time formulas exclude queue wait. A ten-caller serialized-load gate reports queue wait,
throughput, peak memory, fairness/starvation, busy behavior, and maximum writer
lock time for each bounded stage.

```text
one transfer on a reused stream <= P + 1 RTT
Push then Add on that stream     <= P + 2 RTT
```

Pipelining is mandatory: sender frame `i` contains missing payload for turn
`i` plus typed and/or Object announcement `i+1`; receiver reply `i` contains
admission ack `i` plus separate typed/Object missing bitmaps `i+1`. The final
reply closes the transfer. Here `P` is coalesced wire turns, not SQL pages, and
`P <= P_o + H`. A cold TCP handshake is measured separately rather than hidden
inside the operation bound.

Direct remote route:

```text
BranchStore -> LayerStore Push Branch + Add Layer
    <= P_BL + 2 RTT
```

Stacked remote route:

```text
BranchStore -> StackStore Push Branch + Add Stack
    <= P_BS + 2 RTT

StackStore -> LayerStore Push Stack + Add Layer
    <= P_SL + 2 RTT

total stacked publication
    <= P_BS + P_SL + 4 RTT
```

The two stacked receivers and the Push-before-Add semantic barriers are
irreducible. Embedded hops cost zero network RTT. Remove connection creation per phase,
per-batch handshakes beyond the bounded window, per-object queries/acks,
duplicate head reads, full-closure announcements, and sender-delete handshakes.

| Cost | Direct | Stacked | Status |
|---|---|---|---|
| Physical receiver dedup query | LayerStore | StackStore, then LayerStore | Irreducible per DB because each owns a physical CAS copy. |
| Closure authentication | LayerStore | StackStore, then LayerStore | Irreducible trust boundary. |
| Add CAS | LayerHistory | StackHistory, then LayerHistory | Irreducible semantic boundary. |
| TCP connect | one link | two links | Removable after first use by connection reuse. |
| Per-object existence query/ack | none | none | Removed by set batches/bitmap. |
| Full-closure announcement | none | none | Removed by subtree pruning. |
| New connection between Push/Add | none | none | Removed by one stream per healthy sequence. |
| Duplicate payload at common receiver | none | none | Removed by ObjectId PK and missing-only send. |

Stacked mode can move the same new payload across two different physical links;
that is required when both StackStore and LayerStore must durably retain it. It
must not create two copies inside either receiving database.

### Byte budget

For ObjectId width `I_o`, typed-ID width `I_t`, announced ObjectId/typed counts
`N_o`/`N_t`, coalesced wire turns `P`, header bytes `h`, and metadata `M`:

```text
wire bytes <= missing canonical payload bytes
            + I_o*N_o
            + I_t*N_t
            + 64*(P_o + H)       # separate fixed position bitmaps
            + h*P
            + M
```

No Bloom filter, compression, global CAS, transfer table, connection pool, or
alternate transport is added until this exact-set path misses a measured gate.

## 8. Dedup matrix

Transfer flow:

```text
cross-store:
    announce table-specific typed/Object pages
    -> separate typed/Object missing bitmaps
    -> send missing canonical bytes/facts only
    -> authenticated idempotent admission
    -> closure verification
    -> expose metadata/ref/head last

local bytes: CDC -> canonical encode/hash once -> admit -> verify -> expose
local merge: canonical encode/hash once -> deferred -> clean admit -> verify -> expose
```

| Case | Exact flow | Required evidence |
|---|---|---|
| Repeated writes/Commits in one BranchStore | local byte flow | `COUNT(DISTINCT object_id)` grows only for new canonical objects; identical new streams from the same base/edit path add zero duplicate payload rows. |
| Multiple Branches in one BranchStore | local byte flow per Commit | Shared canonical ObjectIds have one row; history-dependent layouts are not claimed equal. |
| Pulled/pushed records in one StackStore | cross-store flow | Existing typed facts and ObjectIds are checked against their own tables; one complete closure is retained per physical DB. |
| All received histories in LayerStore | cross-store flow for every sender | All senders converge on the same typed PKs/ObjectId PK; history/ref rows do not duplicate payload. |
| BranchStore -> StackStore Push Branch | cross-store flow | StackStore computes separate typed/Object missing sets; Branch ref is written last. |
| BranchStore -> LayerStore Push Branch | cross-store flow | No Stack is synthesized and no authoritative head moves. |
| StackStore -> LayerStore Push Stack | cross-store flow | LayerStore skips stored suffix/provenance facts and objects, validates every mapped frozen Branch/accepted Commit/root, verifies the signed frontier, then CASes copied-head metadata only. |
| All reverse Pulls | cross-store flow with requester as receiver | Requested history/Commit prefix becomes visible only after closure verification. |
| Add Stack/Add Layer | local merge flow; missing source data first uses transfer flow | Result root is reused. A newly accepted equal-root Branch advances StackHistory with one same-root Stack; equal-current Add Layer may write only AddResult. Repeating a mapped source writes nothing. Conflict discards deferred objects and writes zero rows. |

Push never deletes sender data. GC is deferred.

Dedup scope is one physical SQLite store:

```text
10 identical installation byte streams using the same base and edit path in one BranchStore
    ~= one payload chunk set + O(10) small Commit/tree/ref metadata

10 separate offline BranchStore databases
    <= one private pre-push payload copy per database
```

The v1 recommendation for one-machine maximal dedup is one BranchStore with
many Branches. Do not add a shared-CAS subsystem or promise global cross-file
deduplication. The same final logical bytes reached through different edit
histories may retain different roots/layouts; measure that representation
overhead and require only that semantic-digest merge resolves them cleanly.

### Discard and bounded-resource contract

| Event | Required ownership-safe action |
|---|---|
| Present announcement ID | Receiver clears missing bit; sender sends no frame. |
| RETURNING omits raced-existing ID | Drop frame buffer immediately; no second query or duplicate row. |
| Invalid frame/order | Drop unadmitted bytes/scratch and return `Integrity`. |
| Three-way Conflict | Drop DeferredObjectStore memory/spill before DB admission; zero production-row delta. |
| Injected/illegal final CAS loss | Roll back final candidate/AddResult/ref/head; expose no partial new closure. |
| Successful Push | Never delete sender object/ref. |

GC/refcount/transfer-state tables remain forbidden. Resource gates reuse core
limits rather than adding configuration:

```text
MAX_OBJECT_BYTES          = 16 MiB
MAX_OBJECT_FIELD_BYTES    = 8 MiB
MAX_CHILD_REFERENCES      = 100,000
MAX_DECODE_NESTING_DEPTH  = 8
MAX_EXTENT_LEVEL          = 31
normal object batch       = 128 rows / 4 MiB
oversize valid singleton  <= 16 MiB
```

Streaming/backpressure, one active working set, first-conflict return, CTE
`temp_store=FILE`, and the benchmark-frozen page cache prevent unbounded
application memory. Do not full-materialize files/closures. Explicitly measure the
unavoidable rare `O(3*B_file)` semantic digest path and the empty-receiver
`O(closure objects + bytes)` transfer; neither is hidden as constant-time.

## 9. Phase P1 — shared storage core

Implement only the files under `layerfs-storage-core` from `source-tree.md`.
Treat the SQL, batching, transaction/visibility, deduplication, transfer,
large-stream, CPU/memory, and test clauses in
`db-transaction-transfer-model.md` as executable acceptance criteria, not
optional tuning notes.

Dependency order:

1. Tagged IDs and seven records plus AddResult.
2. Preserve the measured ordinary-rowid `objects` choice and freeze exact DDL,
   constraints, manifest admission, and typed SQL.
3. Closure-complete object admission.
4. Cross-base merge-base resolver: indexed recursive SQLite CTEs with `UNION`
   dedup/transient B-tree, paging only final maximal candidates; closest Commit,
   then Stack, then Layer; preserve history isolation and ambiguity/missing
   outcomes without a Rust ancestor set.
5. One lexicographic three-way/Merkle implementation returning Clean or the
   first `Conflict { path, base, current, candidate }` only.
6. Fourteen-operation values plus internal phase envelopes in `contract.rs`.
7. Manual bounded record/endpoint codec in `records.rs`; byte framing,
   checksums, and incomplete-frame rejection only in `wire.rs`; membership and
   batching remain in `admission.rs` and Store operations.

P1 focused tests:

```text
schema exactness and wrong-shape rejection
domain-separated ID fixtures
untagged ObjectId = hash(authenticated canonical bytes); no raw ChunkId in storage contracts/frames
one encode/hash counter for local new objects; transfer CDC/re-encode count zero
FastCDC from-scratch original/prefix-shifted deterministic suffix-reuse fixture
COW locality: replacement-only CDC, zero old-suffix payload reads, retained extent IDs
same logical bytes/different edit histories: allowed root/layout difference + clean digest merge
Layer/Stack strict-list uniqueness
merge-base recursive-CTE Commit -> Stack -> Layer ordering, index plan, deep-DAG temp/page bounds
cross-LayerHistory rejection
three-way clean/first-lexicographic-conflict/no-op, three-digest order, DeferredObjectStore discard/spill fixtures
children-before-parent rejection
known-root subtree pruning; repeated Add zero descendant reads
SQLite >=3.35 + 512-placeholder/four-column bind capability + fixed bitmap + trailing NULL mapping
typed IDs use typed tables/bitmap; ObjectIds use objects/bitmap; coalesced wire P <= P_o + H
SQLite batch adapter never uses per-ID default; 128-row/byte pack + 16 MiB singleton
two-connection RETURNING negotiation-race accounting
WAL + synchronous FULL + measured/frozen auto-checkpoint threshold
wire canonical round trip and batch bounds
```

P1 gate:

```text
cargo test -p layerfs-storage-core
cargo clippy -p layerfs-storage-core --all-targets -- -D warnings
```

P1 additionally fails if any binding DB/transfer scenario lacks a focused test,
if a test reaches a different owner than `source-tree.md`, or if local and
remote endpoints use different membership/admission/visibility algorithms.

## 10. Phase P2 — minimal direct vertical slice

Build `layerfs-branch-store`, `layerfs-layer-store`, and only `layerfs-sdk/direct.rs`.

End-to-end slice:

```text
provision canonical-empty LayerHistory
 -> create_branch_from_layer
 -> transient edit
 -> commit
 -> push_branch (local typed endpoint)
 -> add_layer(BranchSource)
 -> pull_branch(source_branch_id, fresh_local_branch_id) into a second BranchStore
 -> layered read verifies final bytes
```

Required transaction tests:

```text
anchor Commit + Branch atomicity
Commit insert + Branch-head CAS atomicity
add_layer Layer + AddResult + Layer-head CAS atomicity
conflict writes nothing
fair queued Add Layer callers evaluate serially against the newly visible head
injected/illegal Branch or Layer CAS loss rolls back once and returns HeadMoved
Stack-bound source returns WrongSourceRoute
fresh pull ID inherits exact source base/head
Pull Branch admits Commit metadata but verifies accepted roots through parent with zero accepted payload copy
same-ID Pull: absent/equal/local-ancestor/source-ancestor outcomes
true same-ID divergence -> HeadMoved and zero ref mutation
existing different local ID -> HeadMoved; fresh import + merge is required
```

Injected illegal head movement is a test-only exact-CAS defense and adds no
production retry, session, or journal state.

P2 gate:

```text
cargo test -p layerfs-branch-store
cargo test -p layerfs-layer-store
cargo test -p layerfs-sdk --test topology direct
```

## 11. Phase P3 — stacked vertical slice

Add `layerfs-stack-store` and `layerfs-sdk/stacked.rs`.

End-to-end slice:

```text
pull_layer_history
 -> create_stack_history_from_layer
 -> create_branch_from_stack
 -> commit
 -> push_branch
 -> add_stack
 -> push_stack
 -> add_layer(StackSource)
```

Required tests:

```text
creator signer survives reopen without user credential
pulled StackHistory handle is read-only
linear StackHistory; no sibling child insertion
stale retained Stack base three-way integrates against current head
content conflict writes no Stack/AddResult/head
fair queued Add Stack callers serialize and each evaluates once against current head
injected/illegal Stack CAS loss rolls back once and returns HeadMoved
push_stack equal/older/descendant/divergent LayerStore-copy cases
push_stack suffix includes every BranchId->StackId AddResult's frozen same-ID Branch ref, accepted Commit DAG, and root closure
signature covers ordered Stacks/AddResults/frozen Branch-head pairs plus typed/Object provenance frontiers
wrong mapping, moved same-ID Branch, wrong-history base, missing Commit parent/root -> Integrity and no copied-head movement
LayerStore can pull_commit_history for each accepted suffix Branch after creator StackStore is unavailable
pull_commit_history pins head and creates/moves no Branch ref
Stack-bound BranchSource cannot bypass add_stack
```

P3 gate:

```text
cargo test -p layerfs-stack-store
cargo test -p layerfs-layer-store
cargo test -p layerfs-sdk --test topology stacked
```

## 12. Phase P4 — cross-base Branch merge

Finish `merge` only after all ancestry pulls work.

Cases:

```text
same head / source already contained -> UpToDate
same tagged base and target ancestor -> target fast-forward CAS
closest common Commit -> divergent two-parent Commit
zero Commit candidates -> resolve closest common Stack
zero Stack candidates -> resolve closest common Layer
zero Layer candidate or different LayerHistory -> NoCommonBase
multiple incomparable maximal Commits -> AmbiguousMergeBase
missing ancestry/object -> MissingBaseData
first lexicographic path conflict -> one bounded Conflict value, no Commit/head movement
target CAS loss -> rollback + HeadMoved; no hidden retarget
```

Merge performs no hidden mutating Pull and never changes target `base_id`.
Preflight may perform up to `L` batched read-only turns through the configured
parent for zero-copy accepted bases, Merkle traversal, and semantic digests;
these occur before the write transaction and persist no parent payload.
Unavailable dependencies return `MissingBaseData`. The final local write
transaction performs zero network I/O.

A divergent tracking pull is resolved explicitly with existing operations:

```text
pull_branch(remote_source_id, fresh_local_id)
 -> merge(fresh_local_id, target_id, expected_target_head)
 -> push_branch(target_id)
```

`pull_branch` itself never overwrites, rebases, or merges the target ref.

## 13. Phase P5 — remote byte path

Remote support comes after embedded semantics pass.

1. Reuse the already-passing P1 `storage-core/wire.rs` over standard
   `Read`/`Write`; do not add a second codec, async runtime, or transfer crate.
2. Implement the two store `remote.rs` dispatchers.
3. Add named StackStore/LayerStore bootstrap binaries.
4. Run every Pull/Push scenario both embedded and loopback TCP against the same
   operation contract fixtures.

P5 must run the binding large-transfer and endpoint gates: self-contained
operation envelope, one reused stream, mandatory typed/Object page piggybacking,
separate bitmaps, `P <= P_o + H`, `P + 1` RTT, exact missing bytes, mid-frame
rejection without admission, empty-receiver streaming, 16 MiB singleton, bounded
buffers/backpressure, and byte-only wire ownership.
It must also prove that no route transfers a SQLite/WAL/SHM file or opens a
remote database path.

P5 gate:

```text
cargo test -p layerfs-storage-core --test wire
cargo test -p layerfs-branch-store --test branch
cargo test -p layerfs-stack-store --test transfer
cargo test -p layerfs-layer-store --test transfer
```

If blocking standard streams measurably prevent required concurrency, add one
existing runtime dependency to the binary crates only. Do not contaminate
storage core or create a transport abstraction beforehand.

## 14. Phase P6 — SDK and consumer closure

Rewrite SDK composition with explicit arguments only:

```text
direct(BranchStore, Layer publication endpoint)
stacked(BranchStore, StackStore, Stack publication endpoint, Layer publication endpoint)
```

No YAML, environment-driven hidden database, automatic local Store creation,
or runtime route guessing. `Direct::from_parts` and `Stacked::from_parts` make
embedded versus remote Add/Push publication explicit in code.

Retain the rewritten `layerfs-workspace` as the sole transient runtime, then
re-add the remaining consumers one at a time:

```text
layerfs-mount
tools/layerfs-eval
layerfs-materialization
```

The fixed presentation chain is
`layerfs-mount -> layerfs-sdk -> layerfs-workspace -> layerfs-branch-store -> configured parent`.
Mount owns only FUSE callbacks, kernel inode/open-handle mapping, errno/attribute
translation, and session bootstrap. Workspace owns generic transient COW,
change/spool state, direct commit/discard, and the spool bound. Neither
may Push or Add. Real Linux FUSE remains the priority: route the passing direct
slice through this chain immediately after P2, keep it green while P3 is built,
and route stacked mode through the same mount immediately
after P3. Close the real-FUSE functional oracle and focused `fs-bench` smoke for
both topologies before spending implementation time on materialization/APFS.
Materialization remains part of final workspace closure but may not delay or
replace the FUSE terminal path.

Each consumer uses SDK/public Store APIs and owns no schema, three-way,
history, or transfer behavior. Delete obsolete fixtures rather than adapting
them to old semantics.

## 15. Performance and storage gates

### Recorded cohesion review

The old 350-line review trigger and 500-line hard cap are superseded. The
handwritten production-file hard limit is 1,500 LOC, excluding `sql.rs` and
`schema.rs`. Cohesive files in the 350–1,499 range are not split merely for size; SRP and one owner per invariant
still require a split whenever unrelated responsibilities survive together.
Total production LOC and deletion of duplicate algorithms take precedence over
small-file targets.

The following files remain material architecture review points because of
their responsibilities, not their line counts:

| File | Review result |
|---|---|
| `storage-core/schema.rs` | Cohesive Store open/owner queue, exact schema verification, and fixed statement preparation. |
| `storage-core/sql.rs` | Cohesive typed Branch/Commit reads and ref/history transaction/CAS primitives. |
| `storage-core/merge_base.rs` | Cohesive typed ancestry/base reads and indexed Commit/Stack/Layer base selection. |
| `storage-core/merkle.rs` | Uses `layerfs-core` logical/Merkle/rope primitives, a bounded disposable object scratch, traversal/pruning, and logical equality support; no duplicate Snapshot/COW/codec/chunker remains. |
| `storage-core/admission.rs` | Cohesive receiver membership, authentication, insert/race accounting, and fact validation. |
| `storage-core/contract.rs` | Domain values/outcomes/errors plus internal operation-envelope values; no duplicate Operation model or public statistics return. |
| `storage-core/wire.rs` | Byte framing/checksum/backpressure only; admission, transfer planning, manual record codec, and Store dispatch use their precise existing owners. |
| `layer-store/transfer.rs` | Cohesive central endpoint serving plus signed Stack provenance verification. |
| `workspace/overlay.rs` | Cohesive generic transient namespace and inode graph; file bytes/lifecycle remain split out. |
| `mount/filesystem.rs` | One cohesive `fuser::Filesystem` callback implementation; state, mappings, translation, and session bootstrap remain split out. |

These are correctness gates, not optional tuning:

| Gate | Required evidence |
|---|---|
| Canonical byte identity | Untagged ObjectId equals the one hash of authenticated canonical bytes; object kind comes from codec/context; raw ChunkId appears in no storage row/contract/frame. |
| One encode/hash | Local new object encodes/hashes once; normal admission does not repeat it; sender authenticates stored row at most once; receiver authenticates once; scratch reread is counted separately. |
| One CDC profile | All paths report the same hashed 8/16/32 KiB profile ID. |
| COW edit locality | `cdc_bytes_scanned == replacement.len`, zero old-suffix payload reads, unchanged extent ObjectIds retained; no claim of whole-file root convergence. |
| Standalone FastCDC quality | From-scratch original/prefix-shifted deterministic streams freeze exact canonical payload-ID/byte reuse; fixed-block oracle fails; worst-case full churn remains allowed. |
| Edit-history difference | Same final logical bytes via full write/one edit/multiple edits may have different roots/layouts; record overhead and prove exact three-digest semantic rules merge clean. |
| Closure admission | Parent/root metadata is never visible before all children; known root presence certifies closure. |
| Add pruning | Repeated mapped Add and equal-root Add perform zero descendant reads; divergent Add visits only unequal frontier. Equal-root Add Stack still writes its required provenance node. |
| Bounded conflict | Canonical lexicographic walk returns the first conflict only and stops; no Vec/truncation state or DB row. |
| Merge-base memory | Indexed recursive CTE + UNION/transient B-tree finds maximal common candidates; no Rust ancestor set; page-cache/temp bytes remain within frozen bounds. |
| Missing-only transfer | Announced, receiver-missing, sent, RETURNING-admitted, and raced-existing sets/bytes balance exactly; transfer CDC/re-encode counters are zero. |
| Cross-store dedup | Common StackStore/LayerStore receiver retains one row per ObjectId across multiple senders. |
| Repeated install | Ten identical installation streams from the same base/edit path in one BranchStore approach one payload set plus `O(10)` small metadata. |
| No full-copy fallback | BranchStore contains zero accepted base payload rows after create/pull/read. |
| SQLite batch path | SQLite >=3.35; one 512-placeholder existence statement and widest 512-bind fact insert prepare; fixed bitmap; no per-ID/default-loop query; 128-row/byte inserts; valid 16 MiB singleton. |
| Typed/Object membership separation | Typed IDs query only typed tables; ObjectIds query only `objects`; bitmaps remain separate while wire turns satisfy `P <= P_o + H`. |
| Stack provenance completeness | Every pushed suffix AddResult resolves to the exact frozen same-ID Branch/accepted Commit DAG/root; signed frontier verifies and copied head stays hidden until all missing provenance is admitted. |
| Two-ID Pull Branch | Fresh local ID inherits exact base/head; same-ID updates only by ancestor CAS; local-ahead never rewinds; divergence/different occupied ID returns HeadMoved with zero ref mutation. |
| Query plans | Head CAS and exact ID lookup are indexed; no full object/history scan in normal paths. |
| Durability/checkpoint | WAL + FULL remains enabled; frozen auto-checkpoint bound includes spikes and WAL growth. |
| PK idempotence | Repeated identical object/fact inputs create no duplicate row or payload. |
| Serialized Store order | Fair queue admits one active working set; Add reads/evaluates/CASes once; ten callers produce correct heads with no starvation or partial-visible closure. |

Record raw SQL plans, typed-fact/ObjectId row counts, separate typed/Object
announced/missing bitmaps, `P_o`/`H` SQL pages, coalesced `P` turns, sent bytes,
and chunk reuse. Do not create metric tables; test code computes evidence from
canonical rows and transfer traces.

Required query-plan assertions:

```text
one 512-placeholder objects IN lookup -> primary-key/index SEARCH, never SCAN
short page -> trailing NULLs + fixed 512-bit bitmap + exact input remap
typed history/Branch/AddResult/Commit/Stack/Layer membership -> matching typed PK/index + separate bitmap, never objects
object INSERT RETURNING -> admitted set, no follow-up existence query
Branch/LayerHistory/StackHistory head CAS -> primary-key SEARCH
Commit parent and merge-parent reverse lookup -> declared parent index
merge-base recursive CTE -> parent indexes + transient dedup B-tree, no full Commit table scan
add_results source/reverse lookup -> source PK / result index
normal Pull/Push -> no full objects, Commit, Stack, Layer, or history scan
```

Required contention/order points:

```text
two SQLite connections negotiate the same missing batch, then race INSERT RETURNING
valid MAX_OBJECT_BYTES singleton admission
conflict after DeferredObjectStore spill -> zero production-row delta
incomplete wire frame -> error and zero admission for that frame
injected/illegal final CAS movement -> rollback + HeadMoved, no internal retry
push_stack missing/moved Branch provenance or tampered provenance frontier -> Integrity, copied head unchanged
WAL growth across repeated batches + automatic checkpoint spike in p95
explicit PASSIVE checkpoint only between operations
concurrent/busy second-process raw-file open is rejected/does not bypass owner
10 serialized callers -> child objects, parents, immutable facts, AddResult/ref/head, exact CAS last
10 serialized callers -> queue wait, throughput, max memory, fairness/no starvation, correct final heads, zero partial-visible closure
independent Store databases -> independent queues and parallel progress
```

Every admitted path preserves child-before-parent order and exposes
AddResult/ref/head only after complete immutable facts, with exact CAS last.
Conflict and injected CAS failure leave zero partial-visible result. Test-only
traces count statements, transactions, durable syncs, RTT-equivalent frame
turns, layered turns `L`, bytes, queue wait, insert order, and maximum buffered
memory without production metrics.

Benchmark noise policy:

```text
do not tune or fail on a single result below 5% regression
or below 3 median-absolute-deviations of repeated samples
```

An asymptotic regression, unexpected full scan, full-copy fallback, or extra
bytes is not noise even when wall-clock change is small. The earlier
9.4045 ms versus 9.2618 ms difference is noise and must not consume critical
path time.

## 16. Explicitly deferred

```text
legacy database migration
compatibility aliases/adapters
parallel old/new execution
HA or multi-writer StackHistory
signing-authority transfer
GC and rollback
backup/restore
offline outbound queue
historical error/session persistence
automatic reconnect/resume and lost-acknowledgement recovery
server/network failure injection and recovery benchmarks
crash/kill-point matrices and Store-open scratch recovery
alternative CDC profiles
shared physical CAS across separate BranchStore databases
additional transports
```

Add one only after the terminal cold implementation passes and a concrete
requirement proves the existing design insufficient.

## 17. Terminal proof

Structure:

```text
cargo metadata --no-deps
rg -n 'name = "layerfs-storage"|layerfs-(working-store|durable-store|sync|service)' \
  Cargo.toml crates tools
rg --files crates/layerfs-{storage-core,branch-store,stack-store,layer-store,sdk,workspace,mount} \
  | rg '(^|/)(mod|main|product|common|utils|manager|repository)\.rs$'
find crates/layerfs-{storage-core,branch-store,stack-store,layer-store,sdk,workspace,mount} \
  -name '*.rs' -print0 | xargs -0 wc -l
```

Expected:

```text
no obsolete package/import hit
no forbidden filename in target packages/SDK
no handwritten production file above 1,500 LOC, excluding sql.rs and schema.rs
no cohesive 350–1,499-line file split only to satisfy a size target
storage/package/workspace LOC estimates reviewed for unnecessary surviving lines
aggregate LOC is not an automatic PASS/FAIL threshold
```

Verification:

```text
cargo fmt --all -- --check
cargo test -p layerfs-core
cargo test -p layerfs-storage-core
cargo test -p layerfs-branch-store
cargo test -p layerfs-stack-store
cargo test -p layerfs-layer-store
cargo test -p layerfs-sdk
cargo test -p layerfs-workspace
cargo test -p layerfs-materialization
cargo test -p layerfs-mount
cargo test -p layerfs-eval
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Terminal pass additionally requires raw evidence for:

```text
active-scope db-transaction-transfer-model.md matrix: binding clause -> owner -> focused test -> raw result
3/9 and 8/24 schema manifests
all 14 operations in direct/stacked legal routes
cross-base Merge outcomes
Stack/Layer conflict and injected exact-CAS rollback
writer attestation and read-only LayerStore copies
push_stack accepted-Branch/Commit/root provenance completeness and central pull_commit_history
canonical CAS/CDC identity pipeline
dedup matrix and byte counts
no-copy BranchStore
embedded/loopback contract equivalence
fixed membership/search SQL and query plans
bounded object/fact admission + folded visibility transactions
DeferredObjectStore normal conflict discard and bounded spill
large-file and empty-receiver streaming without materialization
CPU/memory/lock/RTT/byte/queue bounds from the binding low-level spec
```

Do not declare completion from compilation alone.
