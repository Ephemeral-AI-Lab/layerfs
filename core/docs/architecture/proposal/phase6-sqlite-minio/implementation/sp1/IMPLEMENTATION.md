# SP1 strict SQLite metadata / MinIO file-content implementation

Status: Proposal; target LayerFS 0.1.7; not a released contract.

Source audit: published `8c926b9392f3636ae156236dc26d0510ee069d8d`, branch
`codex/phase6-metadata-experiments`, 2026-10-02. The strict split below follows
owner direction and updated root/core AGENTS; it is local/unpublished until its
exact revision commit is recorded. **Strict SP1 is unimplemented and NOT_RUN.**
S2 changes remain a separate uncommitted/unverified checkpoint.
Strict SP1 starts from a clean published source independently of that WIP.
[#295](https://github.com/Ephemeral-AI-Lab/layerfs/issues/295) owns SP1 under #293;
[scope](../STORAGE-PARITY-SPEC.md), [test plan](test.md), and
[S2](https://github.com/Ephemeral-AI-Lab/layerfs/issues/296) are owning references.

**Dated supersession note, 2026-10-02:** commit `8c926b9392f3636ae156236dc26d0510ee069d8d`
published an unreleased SP1 proposal with ordinary/pooled metadata packs in
MinIO and a metadata-only catalog. This owner decision supersedes that placement.
Its source/history and receipts remain intact; no earlier receipt is relabeled
as strict-split proof. The old `34 MiB - 2` simultaneous-buffer subtotal is a
legacy conservative envelope, not the revised topology's measured memory.

## 1. Ownership and retained algorithms

| Owner | Authoritative data |
| --- | --- |
| One persistent daemon SQLite engine | Live mutable Workspace inode/name/extent/edit rows in shared Workspace/incarnation-scoped tables; S2 ownership. No DB/table set per W, Commit or Exec. |
| Global SQLite | Immutable canonical directory/inode/attribute/file-mapping/root metadata, metadata records/groups and pooled values, both-domain locators, save visibility, Commit/history/Branch references and conditional publication. |
| MinIO | **File-content packs only:** regular-file whole payloads and CDC chunks, with existing FULL/PREFIX/STORED representations. No mapping, attribute, filesystem tree or pooled metadata pack. |

Canonical metadata is immutable and keyed by its existing ObjectId: reuse old
objects, insert only newly changed objects, retain prior roots. Encoded/pool
storage is a representation of those exact canonical bytes, not a conflicting
mutable namespace. SQL metadata BLOBs/groups are permitted. SQL storage of
file-payload bytes or file-content pack shadows is forbidden.

Retain C1 canonical formats/IDs, CDC, `construct_stream`, `apply_edits` and roots;
retain C2 exact CAS, selector, FULL/PREFIX/STORED codec, framed-cost comparison,
metadata compression/pooling and depth/work policies. Change placement/access,
not algorithms. One coordinated storage/Bridge owner owns this boundary; global
services never run C1 construction or recertify namespace/provider contents.
No universal manager, second selector/codec, new dependency, patched dependency,
new worker, fallback backend or custom metadata pager is selected.

| Source evidence | Consequence for this implementation |
| --- | --- |
| [C2 selector](../../../../../../crates/layerfs-storage/src/encoding/delta/select.rs), [resolver](../../../../../../crates/layerfs-storage/src/encoding/delta/read.rs) | SQLite-coupled access must be extracted; retain one eligible trial, packed-record authoritative base ID, strict chronology and payload-chain intermediate/final hashes; pooled authentication has the separate scope below. |
| [C2 private placement](../../../../../../crates/layerfs-storage/src/cas/selection.rs) | `pending_base_for` seals/places a group before a same-save base is acquired; retain this without one PUT per base. |
| [C2 pool](../../../../../../crates/layerfs-storage/src/cas/pool_lane.rs), [pool reader](../../../../../../crates/layerfs-storage/src/encoding/pool/read.rs) | Retain ordinals, exact value matching, COPY/INSERT leaves and work limits, with SQL metadata group access. |
| [Attribute values](../../../../../../crates/layerfs-content/src/filesystem/attributes/value.rs) | `emit_value` emits **Chunk + ExtentLeaf + FileState** for metadata. ObjectRole alone cannot classify placement. |
| [Finalized output](../../../../../../crates/layerfs-content/src/object/output.rs), [filesystem objects](../../../../../../crates/layerfs-content/src/filesystem/objects.rs) | Existing objects carry role/bytes/references/predecessors, not placement domain; consumer/provider must bind logical producer provenance. |
| [Prototype packing](../../../../../../benchmark/phase6-live/src/packing.rs), [reader](../../../../../../benchmark/phase6-live/src/read_window.rs), [catalog](../../../../../../benchmark/phase6-live/src/metadata_catalog.rs) | FULL-only all-role packing, base `None`, unpooled inode leaves and two-table catalog do not implement the selected split. |

## 2. Placement provenance and exact-ID dual use

Use two concrete operation-scoped adapter pairs, bound at logical C1 entry points:

- `MetadataConsumer`/`MetadataReader` for FilesystemObjects, directories, inodes,
  attributes, symlinks and their complete value/mapping graphs. Every emitted or
  acquired object in that scope is global SQL metadata, even WholeFile/Chunk.
- `FileConsumer`/`FileReader` for regular-file `construct_stream`, `apply_edits`
  and content reads reached from a regular-file inode content reference. In this
  explicit file scope WholeFile/Chunk are MinIO payload; extent pages/FileState
  are SQL metadata. Role refines known provenance; it never creates provenance.

Attribute `read_value` descends existing file-mapping grammar through its
**MetadataReader**, so its Chunk read remains SQL. Regular-file FileView/mapping
reads use the **FileReader** with SQL mappings and MinIO payloads. These wrappers
implement existing C1 FinalizedConsumer/AuthenticatedObjects; no hash/grammar
change is required. SP1.0 audits every producer and filesystem/content caller,
including mixed read batches and unchanged references; an unbound producer or
ambiguous caller fails explicitly, never guesses from filename/command or probes
one backend after the other fails.

Maintain canonical identity facts once and physical placement eligibility per
`(PlacementDomain, ObjectId)`. A byte-identical Chunk used both as an attribute
value and regular-file data has **the same ID**, and must have both a SQL metadata
representation and a MinIO payload representation. Validate equal role, length
and exact authenticated bytes; dedup within each domain, never count a SQL copy
as a completed file-payload upload or move metadata into MinIO because its ID
already has a payload locator. Shared canonical mapping IDs can be reused in SQL;
edge-use/reader scope distinguishes attribute-chunk from file-chunk references.
Registration preserves `LogicalUse::{RegularFileGraph, MetadataGraph}` beside
physical domain, including reference-use facts for a deduplicated mapping. The
same SQL mapping body may need both use facts; its child placement is checked
for each use, not frozen once by the first writer of its canonical ID.
Candidate/base and pooled-value lookup are domain-scoped; no delta dependency
crosses SQL/MinIO storage domains. Global identity equality does not authorize a
missing required placement. First valid locator wins **within its domain**.

## 3. Frozen construction/encoding profile

[C1 policy](../../../../../../crates/layerfs-content/src/policy.rs),
[CDC](../../../../../../crates/layerfs-content/src/file/cdc/gear.rs),
[C2 policy](../../../../../../crates/layerfs-storage/src/policy.rs), and
[codec](../../../../../../crates/layerfs-storage/src/encoding/codec.rs) establish:

| Item | Retained setting |
| --- | --- |
| Regular-file length | 0 empty; `0 < length < 128 KiB` whole; `>=128 KiB` chunked. Attribute extent-only grammar remains unchanged. |
| CDC | Minimum/target/maximum 8/16/32 KiB, existing seed/profile. |
| Codec | Payload Zstd 3; ordinary/pooled groups Zstd 1; existing STORED probe and complete-cost choice. Metadata Chunk uses its existing codec in SQL. |
| Depth | Whole-file 8, chunk 4, pooled metadata 8. |
| Payload-grammar dependency work | Canonical 512 KiB, encoded 256 KiB, whether a Chunk is file payload or metadata value. |
| Arenas | Encode 16 MiB per admitted writer; decode 1 MiB per read owner. |
| Placement/pool | Group framed target 48 KiB, canonical group cap 512 KiB; current lane capacities, 165 values/group, 100 rows/leaf, 131072-entry pool index. |

Ordinary requested-object size is bounded separately from dependency work;
`resolve_dependency` charges the acquired object too. Keep pooled chains' separate
canonical/encoded and 32 MiB decoded-work limits. Small/large file transitions
retain canonical and byte semantics; cross-role delta hints remain deferred #185.
C1 deferred mapping draft `8 MiB - 1` is a separate accumulation gate, unchanged.

## 4. One neutral reader/selector access seam

These are **proposed**, source-reviewable signatures, not already exported APIs:

```rust
enum PlacementDomain { FilePayload, Metadata }
struct CatalogScope { publication: u64, captured_pack_ceiling: i64,
                      own_save: Option<SaveId> }
struct LocatedObject { location: ObjectLocation, domain: PlacementDomain,
                       body: BodyRef }
enum BodyRef { PayloadPack { order: i64, digest: [u8; 32] },
               MetadataPack { order: i64, row: MetadataPackId },
               MetadataValueGroup { ordinal: u32, row: ValueGroupId },
               Private { owner: SaveId, domain: PlacementDomain,
                         order: i64, generation: u64 } }
trait CatalogRead {
    fn locations(&self, scope: CatalogScope, domain: PlacementDomain,
                 ids: &[ObjectId]) -> StorageResult<Vec<Option<LocatedObject>>>;
    fn value_group(&self, scope: CatalogScope, ordinal: u32)
        -> StorageResult<Option<LocatedValueGroup>>;
    fn group_page(&self, scope: CatalogScope, from: u32, limit: usize)
        -> StorageResult<GroupPage>;
}
trait BodyRead {
    fn read_into(&self, scope: CatalogScope, body: BodyRef,
                 out: &mut Vec<u8>) -> StorageResult<()>;
}
fn resolve_at(access: &ReadAccess<'_>, root: LocatedObject,
              state: &mut ResolveState<'_>) -> StorageResult<(Vec<u8>, ObjectId)>;
fn resolve_dependency_at(access: &ReadAccess<'_>, root: LocatedObject,
                         state: &mut ResolveState<'_>)
    -> StorageResult<(Vec<u8>, ObjectId)>;
fn select(input: &mut SelectInput<'_>, id: ObjectId, canonical: &[u8],
          role: ObjectRole, advisory: &[ObjectId],
          encode: &mut CompressionWorkspace) -> StorageResult<EncodedRecord>;
```

`ReadAccess` borrows captured scope, domain, catalog and body reader; SelectInput
borrows this seam instead of Connection and retains existing C2 cache/counter/
capacity fields. Both resolve functions call one existing bounded chain algorithm.
Move/re-export ObjectLocation from SQLite-specific access without semantic change.
Concrete current SQLite adapter preserves existing standalone Store behavior.
Phase6 body access dispatches a typed PayloadPack to MinIO, MetadataPack/MetadataValueGroup to
bounded global SQL pages, and Private only to its matching owner. Metadata native
encoded records, pooled leaves and value-group reads **never issue a MinIO GET**.
SQL `MetadataPack` stores the original complete bounded metadata-pack envelope
(header, directory and framed records), because the existing canonical decoder
and stored-base lookup parse that envelope. It contains only metadata-domain
records. SQL pooled value groups retain their existing separate value-group bytes
and checked lane/codec/decoded-length/digest descriptors through
`MetadataValueGroup` and the existing value-group decoder/authenticator; those
standalone bytes do not enter the PoolReader pack-header path.
Do not feed bare group bytes into a pack decoder or store a file-payload pack in
SQL. SP1.0 freezes concrete row IDs, bounded transfer assembly and wire allocation.
Keep existing record/group bytes/lane versions; physical placement is not a new
codec. Pack digest authenticates MinIO bytes; SQL group/record authentication
retains existing digests and payload-grammar intermediate/final canonical IDs.
Existing pooled-leaf reconstruction authenticates value-group digests/edge IDs
and the requested final canonical leaf; it does not independently reconstruct/
hash every intermediate leaf during one dependent read. Read each retained leaf
individually against its sealed canonical ID in the pool witness. Stronger
per-step pooled canonical hashing is not claimed as existing behavior.

Qualify body/group caches by domain plus immutable body/order/group identity;
private keys include save/generation. Equal numeric SQL and payload orders must
never collide. Check required `(domain, ID)` placement, logical-use references,
captured publication/ceiling and save ownership before any provider/cache
answer, including a canonical-ID cache hit. Shared-owner reads in both domain
orders must refuse a missing placement even when equal canonical bytes are cached. Private snapshots require matching generation; immutable SQL rows do not
become visible because their bytes are cached. Tail append invalidates its pack
snapshot; returned canonical bytes are caller-owned. ResolveState borrows caches
and codec for a declared wave/operation. No SQL mutex/statement spans C1/codec/
provider calls, and no daemon-lifetime file-payload cache is introduced.

## 5. Bounded writer chronology and publication

1. Domain-scoped exact CAS checks role/length/authenticated canonical bytes.
   Pending finalized bytes are visible to their own C1 construction, but are not
   physical delta bases. Preserve seal_pending/private placed representation and
   chronological `(order, group, record)` checks before acquiring a same-save
   candidate. Keep one bounded tail per relevant domain/lane; no input-wide packs.
2. File payload lanes retain framed-byte grouping, including multi-record Native
   groups. A private group seal need not upload/close a payload pack. At existing
   capacity/completion seals, hash and PUT complete immutable content packs;
   base precedes dependent in the same pack or its prior uploaded pack. ACK, or
   definite 412 plus exact-byte validation, establishes complete payload custody.
   Do not upload metadata lane tails or issue a tiny PUT for every private base.
3. After referenced file-payload ACKs, stage newly changed metadata objects,
   encoded groups, pooled value groups/leaves and normalized payload locators in
   bounded global SQL transactions. Previously staged same-save SQL metadata is
   a private eligible base; foreign/private metadata is hidden. Value groups are
   staged before leaves using their ordinals; base metadata precedes its delta.
   Reuse old immutable SQL objects; no whole namespace copy, transaction or scan.
4. Bounded registration pages normalize selected `(domain, ID)` locators. Base
   ID remains authoritative in physical record; daemon-derived edge metadata
   must agree. Check normalized base-before-dependent chronology/availability
   and value-group coverage incrementally. Alternate valid payload packs keep
   first valid locator; existing selected dependents retain their recorded edge.
   Invalid chronology/identity fails the page atomically, with no re-encoding.
5. `finish_storage` checks incremental completion/closure markers, not the whole
   namespace. Mark completed SQL metadata/locators/candidates ready, then perform
   the short existing C5 conditional root/head publication. No SQL/ownership lock
   spans Exec, construction or upload; no cross-provider atomicity promise.
   Global authority checks row shape, ownership and readiness only, without
   content/tree/provider recertification. Keep runtime SQL atomicity, not new
   fsync/cloud durability. Retained old roots still reference unchanged objects.

Concrete mutation client operations: begin_save, bounded metadata-body staging,
payload-locator registration, reserve_ordinals, paged candidate/group access,
finish_storage, abandon and quarantine. Source-reviewed schema/transport carries
these capabilities; it does not become a construction/upload/namespace manager.
S2 owns live mutable metadata and daemon engine lifetime. Current prototype is
serialized single-W with one pending Commit per W; non-pausing successor writes,
multi-Exec and multi-W concurrency remain separate gates.

## 6. Global SQL catalog, index custody and failure

Global SQL stores canonical metadata descriptors keyed by ID and immutable
physical metadata bodies/groups sufficient to reconstruct their exact bytes.
Compression/pooling may avoid a duplicate uncompressed canonical copy; this is
still authoritative canonical metadata storage. Include scoped save/publication,
chronological body order, both-domain selected locators, pooled spans/decoded-body
digests, ordinal cursor/window, and existing bounded FULL-winner stamps/signatures.
Use SQLite B-trees/keyset paging, not a new custom metadata index/pager.

Keep existing Candidates/PoolIndex algorithms as bounded disposable daemon
derivations over authoritative SQL. Declare domain activation/replica counts;
never accidentally double a 704 KiB candidate allowance. Private admissions are
save-scoped, selected ready updates persist, failures invalidate derivations.
Whole-file content candidates and first advisory chunk candidate retain current
policy; metadata fingerprint matches still acquire SQL groups and compare full
value bytes. Pool values exist in SQL metadata only, with current first-encounter
ordinal reservations and whole-window eviction. No index lock spans provider I/O.

Missing/ineligible/unprofitable candidate chooses FULL by policy. Acquired-base
I/O, codec, allocation or integrity failure is terminal. Unknown PUT, SQL staging/
registration or C5 ACK quarantines custody: no resend, refresh or guessed delete.
Definite failure aborts owned private SQL rows once; complete shared SQL objects,
payload packs and live bases remain retained. Deleting the originating Commit
cannot reclaim a base needed by another retained dependent. Runtime GC/migration
remains unsupported; report orphan/alternate bytes and unresolved custody.

## 7. Source-first allocation and simultaneous resources

SP1.0 must record actual schema/wire/action allocation before dependent edits.
Audit [catalog](../../../../../../benchmark/phase6-live/src/metadata_catalog.rs),
[wire](../../../../../../benchmark/phase6-live/src/wire.rs),
[dispatch](../../../../../../benchmark/phase6-live/src/metadata.rs),
[session](../../../../../../benchmark/phase6-live/src/metadata_session.rs) and
[statistics](../../../../../../benchmark/phase6-live/src/transport_stats.rs): current
experimental `P6L2` schema 2, `P6META7`, actions0–7, 16 KiB requests, 8-wide arrays;
independent C2 schema10/profile1. Allocate domain/body/save fields and bounded SQL
metadata-body transfer against real sources, not guessed Bridge Kind/lane tags.
Reject incompatible old catalogs/wire explicitly; no silent migration/profile
change. Page by encoded bytes within declared wire/128-row bounds; split large
metadata group transfer into ordered bounded pages without a whole-load buffer.

Retain MEMORY journal, synchronous OFF, temp_store MEMORY, mmap disabled, existing
pager allowance, zero busy timeout, one attempted operation; no WAL/retry/fsync.

| Domain/owner | Retained bound and lifetime |
| --- | --- |
| Shared construction admission | Pending canonical/result windows each `<4 MiB`,512 pending objects; move allocations, do not duplicate whole windows per domain. |
| File payload read/write | Existing bounded dependency pack cache up to4 MiB, payload lane tails and queued framing; release before crossing allowance/private generation change. No pooled MinIO cache. |
| SQL metadata read/write | Existing pooled body cache up to4 MiB, value cache512 KiB, decoded ordinary groups512 KiB, bounded native/ordinary/pool body pages; SQL pager/journal/temp memory charged separately. |
| Codec | Existing encode16 MiB and decode1 MiB arenas; declare whether serialized daemon-shared or per operation, never a copy per object/domain. |
| Derived indexes | Candidates704 KiB allowance; PoolIndex charge `131072*(size_of::<(i64,u32)>()+8)`; domain overlays/replicas and invalidation explicitly charged. |
| Engine/catalog/transport | One S2 daemon and global SQL pager allowances remain separate; bounded request/reply/group staging, current+base canonical buffers and assembly copies are additional terms. |

The 512 pending objects and byte/result limits are temporary operation windows,
reused with backpressure; they are not a total namespace/file/edit/population cap.
The old all-MinIO `34 MiB-2` subtotal is not evidence of revised domain overlap
or a newly fitted budget. Derive a new simultaneous live ledger from actual owner
lifetimes in SP1.0, including SQL metadata transfers and codec/base copies.
Keep Workspace16 MiB, backing disk1 GiB and container512 MiB unchanged. If actual
admission/physical ownership cannot fit, report that open failure; do not raise
quotas or silently exclude shared daemon/SQL/cache terms. Current MinIO1 MiB HTTP
body capability does not imply the entire configurable16 MiB singleton profile.

## 8. Four implementation steps and focused folder ownership

Start clean strict-source SP1 independently; importing or completing uncommitted
S2 is not a prerequisite. One storage/Bridge owner executes:

| Step | Source responsibility | Exit gate, currently NOT_RUN |
| --- | --- | --- |
| SP1.0 provenance/access | C2 `access/{catalog,body,location}` and concrete SQLite adapter; phase6 domain consumers/readers, actual catalog/wire allocation | Every producer/caller classified; exact-ID dual placement works; frozen source allocation/owner ledger; old standalone C2 path preserved. |
| SP1.1 reader | Existing C2 resolver/decode/pool read through seam; phase6 payload MinIO and metadata SQL body adapters | FULL/PREFIX and metadata pool reconstruction, payload intermediate IDs, each retained pool leaf final ID, scope/chronology/work refusals; metadata-only read GET=0; no discarded SQLite file-payload shadow. |
| SP1.2 writer | Existing selector/candidates/pool/index/placement; phase6 private state, content transport and canonical SQL staging | Same-save private bases without per-base PUT; actual payload PREFIX/STORED/dedup; SQL metadata compression/pooling; domain inventory, incremental closure/normalization, Unknown/base custody. |
| SP1.3 public route | Construction/provider wiring, Bridge/metadata/C5 publication/install | Generic SDK Exec/FUSE -> C1/C2 -> payload ACK + SQL metadata -> short C5 CAS -> retained old/new reads/cleanup. No command recognizer; unchanged roots/byte semantics. |

Keep focused existing-runtime folders: `storage/{domain,reader,consumer,private,
placement}`; `canonical_metadata/{schema,objects,bodies,saves,pool,signatures}`;
reuse existing `minio.rs` payload transport and metadata/wire/session/publication
owners. Core `access/` stays provider-neutral; no MinIO library/new crate in C2.
Only thin module entry files; <=999 physical lines/file and <=200 entry lines.
S2 retains Engine/live inode/name/edit/admission/lifetime/paged-verifier files.

Dated source pointer, 2026-10-02: old task published fixture-only
`81f2cf22dd52dd17d2977c12f06d80d37d82654f` from `8c926b9`; its runtime remains
PARTIAL. Reuse unaffected independent fixtures only by exact source/hash and
matching oracle scope; do not regenerate expectations from the strict candidate
or import runtime WIP as a dependency.

[test.md](test.md) owns four compact external witnesses and subsequent distinct
stride10/3/1 selections. Verify once at frozen identity with relevant locked core/
adapter tests/examples/fmt/Clippy and boundary/self-tests; record exact seals,
checks/gaps and production LOC per commit. No CI/preflight claim or runs here.
Reuse unaffected canonical/Phase B proof scopes without relabeling. Strict storage
accounting includes MinIO file packs **plus SQL immutable metadata** and required
SQL/history allocation; historical all-object pack ratios are not MinIO-only
comparators. Large S2/repository metrics, C1 draft accumulation, concurrency,
physical-memory/cold-cache qualification and cloud durability remain separate.
