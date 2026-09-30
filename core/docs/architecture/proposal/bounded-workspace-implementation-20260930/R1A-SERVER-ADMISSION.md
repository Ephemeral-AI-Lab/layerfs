# R1a typed Server admission

> **Status: Current general guide.**
> Source audited against parent
> `1d2fc8c2987a46acb906a1b9e720bb4cac116c7e` on
> `codex/issue287-implementation`, 2026-09-30. The owning source commit introduces
> this note and the admission implementation together; its actual identity is
> recorded by the [implementation log](../../../issues/287/IMPLEMENTATION-LOG.md).
> This is a partial R1a delivery. Covering checks are recorded in the checkpoint;
> no native protection, physical memory, benchmark or release PASS follows from
> its logical admission counters.

The [R0 resource audit](../../../issues/287/R0-RESOURCE-AUDIT.md) identifies pure
catalog operations competing with content requests for the Store's writer
allowance. This change separates that logical admission boundary. It addresses
SC-07 refusal/custody and SC-08 aggregate ownership/catalog progress, at the
existing direct authorized Service route. It introduces no canonical or persisted
format, dependency, writer setting, retry or worker change.

## Operation owners

[admission.rs](../../../../crates/layerfs-server/src/service/admission.rs) owns
three distinct RAII guards, selected after request validation and authorization
by the existing exhaustive Bridge predicates:

| Predicate | Guard and lifetime | Authority |
| --- | --- | --- |
| `content_mutation()` | `C2SavePermit`, for the complete legacy request | That StoreAccess's live request counter, bounded by the Store's persisted `max_concurrent_writes` read at assembly |
| `metadata_mutation()` | `C5CatalogPermit`, for the pure catalog request through its terminal construction | One live pure metadata mutation per exact shared catalog handle in this Service |
| `read_only()` | `ReadPermit`, for the complete read request | Existing process-wide `MAX_READ_OPERATIONS` count, independently of writers and catalog mutations |

The configured Store list remains bounded at four. Catalog budget construction
uses `Arc::ptr_eq` across that list: aliases of one `Arc<dyn HistoryCatalog>` share
one counter. Catalog IDs/incarnations identify persisted bindings, but do not prove
that two independently opened provider cells are the same continuing handle. They
therefore do not deduplicate distinct provider cells. The C5 provider's nonblocking
cell and SQLite transaction checks remain the actual provider arbitration,
including calls made through another Service or outside Service.

Admission attempts once and returns `Capacity` before input consumption or
dispatch when its selected allowance is occupied. The C2 persisted Save rows
remain the cross-process writer authority; a Service counter does not replace
them. The default persisted setting remains two. Read admission remains a count
allowance and does not establish the prospective 8 MiB read-window profile.

Pure `Fork`, `CommitStaged`, `AddLayer`, `DiscardStage` and `ReserveInodes` requests
acquire no C2 request credit. Their dispatch still requires exact body EOF,
identity/grant/deadline validation and the existing C5 semantic checks. None starts
a content Save, clones a C2 content index or acquires a codec arena. A full catalog
allowance refuses an alias before its first body read or metadata effect.

## Composite result-custody gate

Legacy `ImportNativeDirectory`, `StageChanges` and composite `Commit` remain
content mutations with their original whole-request C2 allowance and exact v1
failure bytes. Their C5 transactions still finish independently of content
construction; this delivery adds no catalog allowance across their input,
construction or output.

Splitting their C2 allowance into phases is not yet implemented. The current
`HistoryFailure` carries conflict and stage observations but no complete known
C2 result owner. Import may finish many file roots before it has a namespace
root, so a fabricated stage or a single-root failure extension cannot preserve
every completed result. Stage/Commit also need to distinguish a known Save with
no stage attempt from retained or Unknown stage custody. R3's versioned sealed
result/candidate ownership must be implemented before that phase change. Completed
C2 data, acknowledged stages and Unknown provider owners keep their existing
custody; this checkpoint performs no replay, guessed adoption or deletion.

Full R1a therefore remains PARTIAL even if the pure catalog subdelivery's checks
pass. Strict memory, protected native progress and concurrent Workspace enablement
remain disabled pending their own prerequisite composition.

## External owning proof

[catalog_admission.rs](../../../../crates/layerfs-server/tests/catalog_admission.rs)
uses public `Service::handle`, the real Store, the real writable/read-only SQLite
catalog providers, ordinary input/output and a separate SQLite process. It adds
no product hook, fake clock, allocator or command recognizer.

The occupied-slot case prepares two valid StageChanges requests with distinct
Workspace identities. Each source stops at body consumption, which follows
`Store::begin_save` on this route. External read-only SQL must observe two
non-null `saves.active_slot` rows before the allocator request begins. Initial
and refill RPCs use the proposed profile's full4,096-serial width: initial range
3..4,098 and refill4,099..8,194. Only serials3/4 enter this small prepared
namespace; unused reserved IDs remain burned. This is catalog RPC progress,
not an implemented Workspace range-lease/exhaustion proof. The successful refill
and exact committed high-water are required while both sources remain held.
The test checks that refill creates no content Save, and that a third
content request refuses before input or another Save row. Only afterwards are
the sources released.

Both returned stages must match the exact immutable captured context and
independently encoded expected namespace roots. The
[small v1 oracle](../../../../crates/layerfs-server/tests/support/catalog_oracle.rs)
encodes the common envelope, directory/inode leaf rows and v1 filesystem root
without using C1 builders or codecs. It shares only the existing BLAKE3 ObjectId
primitive. Immutable portable-metadata prerequisite roots are selected before the
candidate attempts; new file roots are checked against independently framed
whole-file bytes. The finite oracle covers these small leaf namespaces only.
It does not replace the R0 canonical vectors or certify broader partitions.

The test checks old and new exact file bytes, known Stage observations,
zero remaining active Save slots, exact token discard and the absence of serial
refund. Separate cases exercise shared-Arc alias refusal, distinct real provider
cells with equal catalog IDs, read/content progress while a pure catalog request
owns its logical allowance, actual externally held `BEGIN IMMEDIATE` contention,
finite allocator exhaustion, denied grants, read-only authority and wrong prepared
scope refusal before Save/input. Definite provider refusals preserve intended
roots/high-water/content. Existing C5 Unknown classification and composite custody
are unchanged; this checkpoint adds no real-provider Unknown induction proof.

Covering targets selected for the frozen owning check are locked Server
`catalog_admission`, `admission` and `history`. Existing history/provider checks
retain their original scope. Product-boundary and formatting/Clippy checks remain
required at the coordinated handoff; tests are not claimed passed by source
inspection or by this note.

## Remaining implementation and qualification

This change supplies separate logical request counts. It reserves no protected
native channel/thread, pending bytes, FD, terminal byte lease, SQLite heap
compartment or physical cache headroom. A free generic native session is not
protected catalog progress when ordinary sessions are saturated. Real engine
Unknown/NOMEM/cleanup composition, admitted byte/cache/wave owners, paged
construction metadata and actual physical-provider observation remain later R1
gates. The legacy composite result-custody/phase-separation gate remains explicit.

No benchmark was run. #288 receives the changed Service admission mechanism and
these scoped correctness proofs for later qualification, with existing campaign
receipts unchanged. R1a does not advertise `ProtectedCatalog` or
`StrictServerMemory`, does not enable multi-Workspace support, and does not close
any related issue.
