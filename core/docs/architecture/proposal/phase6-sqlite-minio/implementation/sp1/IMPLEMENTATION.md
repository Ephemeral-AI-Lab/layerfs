# SP1 implementation specification

Status: Proposal; target LayerFS 0.1.7; not a released contract.

Source audit: published parent `bfbf48ea7694e8450bd47a3bf2c297fdac36badc`,
branch `codex/phase6-metadata-experiments`, 2026-10-02. The publication commit for this prospective specification is recorded on #295.
SP1 is unimplemented
and unrun; existing uncommitted S2 runtime/documentation changes are a separate,
unverified checkpoint. [#295](https://github.com/Ephemeral-AI-Lab/layerfs/issues/295)
owns SP1 under #293. [Storage parity scope](../STORAGE-PARITY-SPEC.md),
[S2 ownership](https://github.com/Ephemeral-AI-Lab/layerfs/issues/296), and [test plan](test.md) govern the handoff.

## 1. Selected change and source evidence

Retain C1 canonical objects, CDC, `construct_stream` and `apply_edits`; retain
C2's exact CAS, existing FULL/PREFIX/STORED encoding, selector, pooled inode
metadata, group/pack placement and chain policies. Refactor their actual storage
access so the daemon can use MinIO immutable packs with a metadata-only global
SQLite catalog. One coordinated storage/Bridge owner owns that contract.

The audit identifies these concrete gaps; they are not implemented capabilities:

| Source | Existing mechanism or gap |
| --- | --- |
| [C2 selector](../../../../../../crates/layerfs-storage/src/encoding/delta/select.rs) | `SelectInput`, depth walk and base acquisition depend on `Connection`; one eligible trial compares complete framed record cost. |
| [C2 resolver](../../../../../../crates/layerfs-storage/src/encoding/delta/read.rs) | Reads base ID from packed record; checks strict locator chronology; authenticates every intermediate and final canonical object. |
| [C2 placement/selection](../../../../../../crates/layerfs-storage/src/cas/selection.rs) | `pending_base_for` seals/places a pending group before a candidate is used; this is a private placed row, not ordinary publication. |
| [C2 pool reader](../../../../../../crates/layerfs-storage/src/encoding/pool/read.rs), [pool owner](../../../../../../crates/layerfs-storage/src/cas/pool_lane.rs) | Ordinal/value-group catalog, exact value checks, COPY/INSERT leaf deltas and reconstruction also depend on SQLite access. |
| [Prototype packing](../../../../../../benchmark/phase6-live/src/packing.rs) | Calls `encode_full`; inode leaves bypass pooling; Native is incorrectly sealed per record relative to current C2 grouping. |
| [Prototype reader](../../../../../../benchmark/phase6-live/src/read_window.rs) | Calls `decode_canonical` with base `None`; cannot reconstruct PREFIX. |
| [Prototype catalog](../../../../../../benchmark/phase6-live/src/metadata_catalog.rs) | Application `P6L2`, schema 2, only `packs`/`objects`; first registered exact identity keeps its locator. No pool/signature/save catalog. |
| [Prototype construction](../../../../../../benchmark/phase6-live/src/construction.rs) | Already uses C1 fresh construction and inherited edits; adapt its consumer/provider, preserve these algorithms. |

No second selector, codec, fingerprint algorithm, host constructor, SQLite
BLOB-pack shadow store, third-party patch, new worker or tuned profile is selected.
Physical pack digests may differ with immutable placement; canonical IDs must not.

## 2. Frozen profile

[C1 policy](../../../../../../crates/layerfs-content/src/policy.rs),
[CDC](../../../../../../crates/layerfs-content/src/file/cdc/gear.rs),
[C2 policy](../../../../../../crates/layerfs-storage/src/policy.rs), and
[codec](../../../../../../crates/layerfs-storage/src/encoding/codec.rs) establish:

| Item | Retained setting |
| --- | --- |
| Logical file length | 0 = empty; `0 < length < 128 KiB` whole-file; `>=128 KiB` chunked. Cutoff is exclusive. |
| CDC | Minimum/target/maximum 8/16/32 KiB; existing seed/profile unchanged. |
| Encoding | Payload Zstd level 3; ordinary/pooled group level 1; existing STORED probe/selection unchanged. |
| Delta depth | Whole-file 8, chunk 4, pooled metadata 8. |
| Payload dependency work | Canonical 512 KiB; encoded 256 KiB. |
| Codec arenas | Encode 16 MiB per writer; decode 1 MiB per read owner. |
| Placement | Framed group target 48 KiB; group canonical ceiling 512 KiB; ordinary/Native/whole packs 256 KiB; new pooled packs 64 KiB. |
| Pool | 165 values/group; 100 rows/leaf; value index 131,072 entries; current ordinal reservation/window policy. |

Keep the existing distinction: an ordinary requested object is bounded by object
and read-wave capacity; chain work charges its **dependencies**. Acquiring an
object as a dependency charges that object too. Do not make the requested root
consume a dependency budget. Pooled chains retain their separate canonical,
encoded and 32 MiB decoded-work limits from C2 policy.

Small/large transitions retain logical byte/canonical semantics. Cross-role
whole/chunk delta hints remain deferred to #185. The C1 deferred mapping draft
guard (`8 MiB - 1`) is a separate accumulation gate; SP1 neither changes it nor
claims arbitrary accumulated edit capacity.

## 3. Narrow access contract

These are **proposed signatures**, to be implemented once in C2 and re-used by
both existing SQLite and MinIO adapters. Types live in focused access files;
`lib.rs`/`mod.rs` only declare/re-export/delegate. Avoid a universal Store manager.

```rust
struct CatalogScope { publication: u64, captured_pack_ceiling: i64,
                      own_save: Option<SaveId> }
struct LocatedObject { location: ObjectLocation, pack: PackRef }
enum PackRef { Registered { order: i64, digest: [u8; 32] },
               Private { owner: SaveId, order: i64, generation: u64 } }
trait CatalogRead {
    fn locations(&self, scope: CatalogScope, ids: &[ObjectId])
        -> StorageResult<Vec<Option<LocatedObject>>>;
    fn value_group(&self, scope: CatalogScope, ordinal: u32)
        -> StorageResult<Option<LocatedValueGroup>>;
    fn group_page(&self, scope: CatalogScope, from: u32, limit: usize)
        -> StorageResult<GroupPage>;
}
trait PackRead {
    fn read_into(&self, pack: PackRef, out: &mut Vec<u8>) -> StorageResult<()>;
}
fn resolve_at(catalog: &dyn CatalogRead, packs: &dyn PackRead,
              scope: CatalogScope, root: LocatedObject,
              state: &mut ResolveState<'_>) -> StorageResult<(Vec<u8>, ObjectId)>;
fn resolve_dependency_at(catalog: &dyn CatalogRead, packs: &dyn PackRead,
                         scope: CatalogScope, root: LocatedObject,
                         state: &mut ResolveState<'_>)
    -> StorageResult<(Vec<u8>, ObjectId)>;
fn select(input: &mut SelectInput<'_>, id: ObjectId, canonical: &[u8],
          role: ObjectRole, advisory: &[ObjectId],
          encode: &mut CompressionWorkspace) -> StorageResult<EncodedRecord>;
```

`SelectInput` borrows catalog/pack access plus scope instead of a `Connection`;
keep its existing capacities, Candidates, DepthCache, codec/cache/counter fields.
Writer base acquisition calls `resolve_dependency_at`: it charges the acquired
base itself and its chain, matching existing `resolve_dependency`. Ordinary
`resolve_at` exempts only its requested object from dependency work. Both call
one shared bounded resolver; the adapter must not substitute an uncharged read.
`ResolveState<'a>` borrows the caller's C2 caches/workspace for one wave; returned
canonical bytes are caller-owned. No SQLite mutex/statement or pending mutex
survives a provider/codec call. Pack bytes remain in caller-owned bounded storage;
private pack access copies into that cache at most once per generation, never
returns a reference that outlives the mutable lane tail. Appending a tail changes
its generation and invalidates its cached pack body; closed group/value content
remains immutable. There is no daemon-lifetime payload cache.

`ObjectLocation` moves/re-exports from its SQLite-specific module into neutral
access without changing its role/length/group/record meaning. The current SQLite
adapter preserves read-scope/ceiling SQL and pack access. The MinIO adapter maps
registered pack order to digest, verifies GET digest and invokes the same
resolver/pool reader. Enforce both publication/save visibility and captured
registered-pack ceiling before **every** lookup/provider/cache answer. Private
locations additionally require the matching save and tail generation; they do
not bypass captured public visibility. Advancing a view is an explicit new
selection, not a cache refresh or retry. A pack
digest is identity, not chronological order.

Metadata mutation has a separate concrete client with bounded methods:
`begin_save(owner) -> SaveScope`; `reserve_ordinals(save, demand) -> Range`;
`register_page(save, completed_packs, objects, value_groups) -> SelectedPage`;
`candidate_page(scope, cursor) -> CandidatePage`;
`finish_storage(save) -> ReadyStorage`; `abandon(save)` / `quarantine(save)`.
Do not put upload, reconstruction, C1 construction or C5 history into this client.
`SelectedPage` returns normalized locators in input order and exact cardinality;
it also returns registration order for value groups and a continuation cursor.

## 4. Private placement, chronology and publication

1. Admit finalized objects into the existing bounded operation window. Exact CAS
   checks role, canonical length and authenticated canonical bytes, including a
   competing selected representation. C1 may read same-operation finalized bytes
   while constructing references; those raw bytes alone are not a physical base.
2. Preserve C2 `pending_base_for`/`seal_pending`: close and place a needed group
   into the operation's bounded lane tail, create an owner-private locator, then
   let the existing selector acquire that representation. Private packed records
   and bases are visible only to that save; another Workspace sees neither them
   nor private candidate admissions. Do not PUT one tiny pack per candidate.
3. Preserve framed-byte grouping for Ordinary, Native and WholeFile. Native
   payload records carry their own codec frame but may share a raw group. Only
   actual C2 singleton/pooled grammar and existing capacity/seal events force
   their relevant boundaries. Retain one tail/lane, bounded queued groups and
   backpressure; never return a `Vec<Pack>` for an entire input.
4. Private chronology is `(owner's pack order, group, record)`; a base is strictly
   earlier than its dependent. Closing a group need not close its pack. When an
   immutable pack closes, hash/upload its final complete bytes under digest key.
   Every dependency's pack is uploaded no later than its dependent's; bases in
   the same pack appear earlier within that pack. PUT success, or the current
   definite 412 plus exact-byte validation, establishes complete storage.
5. Register uploaded packs and trusted complete record/value-group metadata in
   bounded save-private pages. Assign final catalog pack order in dependency
   order; translate private locations to normalized selected locations. A record's
   base ObjectId remains authoritative in its bytes; any dependency manifest is
   derived by the daemon's C2 parser and must agree. The global service validates
   row shape/ownership, selected reference availability and chronology only;
   it never GETs packs, rebuilds trees or recertifies the provider.
6. For distinct valid alternate packs of the same exact object, keep the first
   valid selected locator and return it. A dependent refers to base **identity**,
   not the discarded base's proposed pack. Existing selected dependents retain
   their own recorded edge. Check newly selected dependency chronology against
   the normalized base; a conflict or malformed/forward edge fails that bounded
   registration transaction. Never silently pick another locator or re-encode.
7. Each registration page records completion and resolves outstanding selected
   dependency/value-group edges incrementally. `finish_storage` checks those
   counters/markers and closure, without rescanning a namespace, then atomically marks its catalog
   rows and candidate updates eligible for ordinary storage reads. Storage
   readiness precedes C5 stage/commit; these are distinct acknowledgments, not a
   cross-provider atomicity or crash-durability promise. C5 receives only the
   ready root/profile and trusted provenance; retain its thin publication path.
   An older read scope never gains newly private rows through cache reuse.

Global packs, complete canonical locators and storage publication are not
Workspace dirty metadata. S2's one daemon-lifetime SQLite database has
Workspace/incarnation-scoped mutable rows; SP1 does not introduce a DB per W.
Current prototype is serialized single-W; retain one pending Commit per W.
Non-pausing Exec/successor writes, multi-Exec and multi-W isolation require later
owning gates rather than being inferred from new access traits.

## 5. Pool and candidate ownership

Global SQLite persists C2 policy/profile, save ownership/publication, pack order
and digest, selected canonical locators, pooled value-group ordinal span/location/
decoded-body digest, ordinal reservation cursor and metadata window start, and
the existing bounded FULL-winner signature rows/stamps. No canonical or encoded
payload BLOB column is permitted. Pooled value bytes live only in MinIO packs.
Use existing first-encounter ordinals, reservation block rules and whole-window
eviction; do not add a permanent full-value lookup table to improve deduplication.

The daemon uses existing `Candidates` (8,192 slots, declared 704 KiB) and
`PoolIndex` (131,072 entries, source charge per entry) as bounded derived indexes.
Global metadata is authoritative; load/synchronize it in pages. A save's private
FULL admissions/pooled groups form its own eligible overlay. Persist only the
selected ready updates; failure invalidates affected derived index state. Do not
make failed/private admissions eligible to another save or hold index locks
across MinIO I/O. The content index proposes whole-file candidates; chunks keep
the existing first advisory candidate rule. Pool fingerprints propose ordinals;
existing C2 authenticates groups and compares full value bytes.

## 6. Allocation, error disposition and resident accounting

Freeze the catalog and transport allocation in SP1.0 before edits depend on it.
Current independent identities are C2 schema 10/format profile 1; experimental
global catalog `P6L2`/version 2; wire `P6META7`, actions 0–7, 16 KiB request limit,
and [statistics arrays](../../../../../../benchmark/phase6-live/src/transport_stats.rs)
of width 8. The [wire](../../../../../../benchmark/phase6-live/src/wire.rs),
[dispatch](../../../../../../benchmark/phase6-live/src/metadata.rs), and
[session](../../../../../../benchmark/phase6-live/src/metadata_session.rs) are the
allocation sources. Record the exact new schema/wire/action table in the same
SP1.0 patch after checking this source; no inferred unused native Bridge `Kind`
values, invented record tags, profile change or silent migration. Old incompatible
experimental catalogs/wire are rejected explicitly; old receipts keep their pins.
Each operation page fits existing 128-row/16 KiB wire bounds by encoded bytes,
not just row count; pooled group/candidate pages need their own checked row shape.

Retain MEMORY journal, synchronous OFF, temp_store MEMORY, mmap disabled,
the existing pager-cache allowance, zero busy timeout and one attempted operation
for new/refactored catalog connections. No WAL, busy handler/retry or
fsync/fdatasync/sync_data/sync_all is introduced. SQLite COMMIT is runtime
atomicity, not a new crash/cloud durability promise.

Use typed definite/Unknown outcomes across MinIO, registration and publication.
Missing/ineligible/unprofitable candidate is a FULL policy outcome. I/O, codec,
allocation, integrity or acquired-base failure is terminal, never FULL fallback.
Unknown PUT/registration/C5 acknowledgment quarantines operation and ownership:
no resend, refresh, guessed retry or delete. Definite failure aborts private
metadata once; complete shared/selected packs are retained. Bases/value groups
stay in custody while any retained selected dependent or unresolved save needs
them, even if the originating Commit is deleted. Keep runtime GC/migration
unsupported; this milestone retains complete packs and reports orphan/alternate
bytes rather than making an unproved reclamation claim.

Account simultaneous owners, not separate per-buffer maxima:

| Live owner | Charge/release |
| --- | --- |
| Pending canonical + caller read results | Each `4 MiB - 1`; 512 pending objects; result IDs/pages retain current bounds. Drain/move objects; no second full pending copy. |
| Ordinary + pooled pack caches | Each 4 MiB; one copy per distinct pack/generation; wholesale release before next insertion crosses bound. |
| Decode group + pooled value caches | Each 512 KiB; wave/operation ownership as existing C2; private append invalidates pack snapshots. |
| Codec | Writer encode 16 MiB + decode 1 MiB; read-only owner decode 1 MiB; no duplicate arena per object. |
| Placement | Sum of one tail per C2 lane: `3*256 KiB + 64 KiB + (16 MiB+4096)` upper envelope; queued groups at most 256 KiB. Preserve smaller actual frozen-profile admissions. |
| Candidate/pool indexes | 704 KiB candidate bound plus `131072*(size_of::<(i64,u32)>()+8)` reported PoolIndex charge; one declared replica/owner, not per file. |
| Metadata/transport | Global and S2 daemon pager caches each remain separately charged; bounded page/request/reply and SQL journal/temp memory are additional live costs. |

The first four writer rows can coexist: `2*(4 MiB-1)+2*4 MiB+2*512 KiB+17 MiB`
= `34 MiB-2` before tails, indexes, chain/current-object buffers, SQL and transport.
This already exceeds the current Workspace 16 MiB memory budget; current 1 GiB
backing-disk and 512 MiB container limits do not resolve that admission mismatch.
The encode/decode/cache owners above are per construction/read operation; only
explicitly serialized reuse can make an arena daemon-shared, and it must not
multiply once per Workspace or imply future overlap is supported. Freeze actual
Workspace-vs-daemon charging and mutually exclusive lifetimes in SP1.0. Admission
and physical proof remain open if the unchanged budgets cannot fit; do not raise
quotas or call these independent maxima automatically compatible.
List those extra terms with actual multiplicity in implementation; a bounded heap
subtotal is not a physical-memory PASS. Admission checks cover temporary old+new
canonical buffers and group/pack assembly copies, transfer ownership of a closed
tail into upload, and release caches on error. Keep current MinIO 1 MiB HTTP-body
limit and explicit unsupported larger-pack failure; do not imply SP1 opens the
entire configurable 16 MiB singleton profile. No total-load quota/worker expansion.

## 7. Submilestones, file ownership and exit gates

Finish S2a atomicity/source checkpoint first. Then these small coherent patches,
with one storage/Bridge contract owner; tests are external and paired with their
owning patch. All gates below are prospective, **NOT_RUN** at this specification.

| Patch | Exclusive implementation responsibility | Required exit |
| --- | --- | --- |
| SP1.0 | C2 neutral `access/{catalog,pack,location}.rs`; existing SQLite adapters; phase6 catalog/wire/session/statistics contracts | Exact type/schema/action allocation reviewed; old SQLite behavior retained; no second codec/selector and no payload catalog storage. |
| SP1.1 reader | C2 `delta/read.rs`, `decode.rs`, `pool/read.rs`; phase6 `read_window.rs`, Reader portion of `objects.rs` | Smallest complete authenticated FULL/PREFIX/pool reader on real MinIO; old/new bytes, intermediate IDs, scope/chronology/depth/work refusals and bounded base GET accounting. |
| SP1.2 writer | C2 `delta/select.rs`, `candidates.rs`, `pool/index.rs`, `cas/{selection,pool_lane,placement}`; phase6 `packing.rs`, Consumer/pending and catalog registration | Same selector and packing policies; same-save placed candidate works without per-base PUT; exact dedup, profitable PREFIX, FULL policy outcomes, native multi-record groups, pooled values/deltas, closure/normalization and failure custody. |
| SP1.3 integration | phase6 construction/provider wiring, metadata/publication/install contract; associated SDK/Bridge adapter paths | Ordinary public generic SDK Exec/FUSE mutation -> C1 -> C2 -> MinIO ready -> C5 -> read retained old/new heads -> cleanup. No command recognizer; inherited parent/provenance/bytes and explicit Unknown custody. |

S2 retains Engine/inode/name/extent/admission/schema/lifetime files, normalized
edit SQL and paged verifier. Coordinate call-site signatures with that owner;
do not rewrite its mutable metadata under SP1. Keep product files <=999 physical
lines, entry files <=200 declaration/delegation lines; update affected architecture
alongside product boundaries. No product-source fixtures/hooks or legacy includes.

Follow [test.md](test.md) once per frozen owning checkpoint with locked C2/adapter
tests, owning core tests/examples/fmt/Clippy and boundary/self-tests. Record exact
checks, source/compilation/dependency/flags seals and production LOC per commit;
no CI/preflight claim. Reuse unaffected canonical/Phase B proofs in original scope.
After SP1.3, stride10/3/1 are three separately declared storage selections using
[historical baseline](../HISTORY-STORAGE-BASELINE.md) and prospective provider
allocation accounting; they are not a compression tuning loop or permission to
rerun unchanged arms. Full repository import, large S2 measurements, non-pausing
successors, C1 draft accumulation removal, physical-memory/cold-cache/storage
qualification and cloud durability remain separate, explicit gates.
