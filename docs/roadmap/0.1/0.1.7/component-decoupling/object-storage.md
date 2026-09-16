# Cluster 2: physical object storage

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

The [integrated design](content-storage-design.md) covers compression/packing,
repeated payload deltas, indexed dependency cleanup and measured acceptance.
The detailed [physical encoding and packing proposal](physical-encoding-and-packing.md)
owns terms, exact selection/lifetime choices and source-backed cuts.
The [object save and persistence proposal](admission-and-persistence.md) owns
write authority, batching/visibility, four-table schema structure, indexes,
single-attempt cleanup and local/remote completion.
The [implementation review](implementation-plan.md) selects embedded SQLite first,
with no WAL/added durability, and records the remaining memory/placement proofs.

Successful published versions remain immutable. Failure cleanup applies only to
unpublished attempt-owned data. Lost acknowledgements fail with an unknown outcome;
a separately requested inspection can identify an existing result. No write replay,
automatic retry, version rollback or undo subsystem is part of this design.

Read with the [joint co-design review](content-storage-co-design.md),
[logical content review](canonical-content.md) and [shared proposal](proposal.md).
Tracking issue: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

## Purpose and review verdict

This component is the content-addressable store (CAS). The proposed implementation
folder is C2 cas/, previously labelled store/; encoding/, pack/ and sqlite/ remain
separate responsibilities. C1 object/ owns canonical identity and shared checks.
No additional CAS component, table or dependency layer is introduced.

Implement the canonical-object contract: membership and exact reuse,
authentication, physical base selection, delta/compression, packs, locators,
SQLite persistence and authenticated reconstruction. Cluster 1 owns canonical
meaning and identity. History owns publication semantics; storage owns the
transaction mechanism required to enforce them.

LayerStack, Branch and logical Commit are explicitly outside cluster 2. Its API
accepts canonical objects/IDs and storage resources, not history entities or
expected Branch heads. LayerStack creation, Branch management, Commit records,
parent links, head advancement and workflow completion belong to the separate
history/workflow owner. The existing layerfs-layerstack-store package name is
reference structure, not the target ownership boundary.

SQL transaction COMMIT remains a persistence mechanism. Sharing a SQLite database
and composing object/history writes atomically where required does not transfer
history semantics or commands into this component. Exact session composition is
a [selected execution contract](admission-and-persistence.md#5-transactions-visibility-and-finish),
with concrete signatures and backend qualification still required.
The isolation proof is object save/read/authentication using storage tables
without creating a LayerStack, Branch, logical Commit, Workspace or mount.

Read-only review at `8357b1e336d1e16eae349a3313c5f3dbdc777b82` found no Cargo
dependency on FUSE, daemon, Docker or the Workspace service. Storage isolation
from their runtimes is feasible. The current package still contains history
and Workspace responsibilities, and prepare/persist stages are not independent
data artifacts. No extraction, build, test or measurement was executed.

The [diff/conflict deferral](diff-conflict-deferral.md) removes reconciliation
builders, candidate unions and reconciliation-only snapshot overlays from the
replacement target. It does not remove physical delta, authentication, exact reuse,
ordinary staging ownership, rollback or conditional history publication.

Those workflow obligations remain with their outer owner; the storage cluster
provides the effects and transaction guarantees it needs. Failure cleanup concerns
only unpublished attempt-owned data, never successful version rollback.

## Existing foundations

- [Store dependencies](../../../../../crates/layerfs-layerstack-store/Cargo.toml)
  are content, BLAKE3, rusqlite and zstd-sys.
- [NativeEncoder::compress, pack.rs:456](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L456)
  takes raw bytes and an optional prefix without SQLite.
- [Delta reconstruction, delta.rs:109](../../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L109)
  takes a record and supplied base and reconstructs canonical content.
- [Native admission tests](../../../../../crates/layerfs-layerstack-store/src/objects/admission/native_tests.rs)
  exercise a real SQLite Store and canonical objects without a live Workspace.
- [Read authentication, read.rs:2040](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L2040)
  reconstructs physical content and applies the content library's identity rules.

## Extraction obstacles

| Existing coupling | Source | Required separation |
| --- | --- | --- |
| Construction creates WorkspaceAdmission and streams directly into it | [objects.rs:4492](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4492) | Supply the bounded output consumer explicitly; keep one construction algorithm |
| Store package exports history, snapshots, WorkspaceAdmission and construction alongside physical storage | [lib.rs:19](../../../../../crates/layerfs-layerstack-store/src/lib.rs#L19) | Separate responsibilities internally; retain only externally required compatibility facades |
| PreparedAdmission owns an admission session and validates Store identity/activity | [admission.rs:162](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L162) | Distinguish encoded data from effectful same-Store session ownership |
| Preparation performs predecessor/base/membership access under schema-selected policies | [admission.rs:415](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L415) | Explicit profile and lookup/base providers; count actual dependency reads |
| Publication assembles retained pack tails and insertion can merge into an open pack | [admission.rs:1268](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1268), [1474](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1474) | Attribute pack placement/assembly honestly or relocate computation without duplicate work |

Do not turn a same-Store admission handle into a generic serializable plan merely
to benchmark it. A portable pack artifact would need format, base closure,
provenance, logical locators and revalidation contracts beyond this requirement.

## Internal responsibility sequence

```text
canonical batches and optional hints
  -> membership, collision and dependency checks
  -> base selection and encoding
  -> bounded pack groups
  -> pack placement and SQLite mutation
  -> storage acknowledgement; external workflow owns publication
```

These can remain ordinary functions/modules. Pure codecs can run without a DB;
storage preparation can use declared lookup/base providers; persistence uses
real SQLite. Keep finality, batches and base dependencies explicit. Delta and
compression may share a codec invocation and need not become artificial passes.

Consume finalized objects directly under the
[handoff contract](finalized-object-handoff.md). Share bounded batches/packs/cohorts across
files; do not flush per file or replace removed payload spill with temporary SQL
payload staging. Constructor completion and final storage acknowledgement remain
separate, with exact ownership of failed unpublished attempts.

Acceptance transfers bounded ownership; it may leave work queued or in an open
transaction. Storage completion accounts for every accepted object as exact reuse
or acknowledged storage, closes final tails/cohorts and establishes dependencies.
Use incremental dependency checks and bounded pending state, with no finish-time
whole-graph walk or operation-sized transaction. Allocation release follows the
last required consumer, independently of transaction acknowledgement. Detailed
[ownership](finalized-object-handoff.md#4-allocation-ownership-and-release) and
[completion rules](finalized-object-handoff.md#6-completion-and-failure) live in the handoff document.

Standalone completion can commit its final object cohort. Integrated workflows
can compose their required writes in that same final transaction before success;
the completion barrier must not force an extra commit or split existing atomicity.
After a completed standalone save, later history failure does not silently invoke
failed-admission cleanup on the retained result. History/retention policy stays external.

The database boundary must support embedded SQLite and a selected remote SQLite
service through bounded grouped reads and atomic writes. Keep connection/BLOB
handles internal; do not translate each local call into an RPC. Writer authority,
read-after-write behavior, unknown outcomes and provider limits need explicit
qualification. The selected target has one enforced writer across transactions;
unavailable ownership fails without queuing. Local epochs and watermark cleanup
cannot assume remote exclusivity. A definitely failed save uses indexed reverse-
location cleanup once; unknown outcome prevents destructive cleanup.
See [placement requirements](content-io.md#6-pluggable-to-later-environments).

Packing keeps bounded compatible append. Select exact placement before materializing
and assemble only the selected write once from borrowed groups. Preserve grouping,
small-operation density, visibility and acknowledgement. Seal-once insertion is a
separately qualified future replacement, not a second runtime mode. No measured
optimization is claimed.
The existing RAW singleton pack path still stages through a temporary file. Its
replacement and the producer/admission size-limit mismatch are explicit
[audit targets IO13/IO14](content-io-memory-audit.md#2-core-optimization-ledger),
with supported encoding/capacity, collision operands and memory coexistence preserved.

## Logical attributes and physical inode values

Logical [filesystem attributes and inode values](filesystem-tree.md#6-attributes-and-physical-metadata-are-different-responsibilities)
remain C1 data. C2 first pools compact inode values, then evaluates eligible delta
encoding for pooled leaves; it stores new value groups in packs and reconstructs
the original canonical leaf identity. The metadata_value_groups table catalogues
ordinal ranges, pack/group locations and digests; it is not a separate payload BLOB
table and does not contain arbitrary xattrs. Keep metadata-chain limits distinct
from payload 8/4. FULL pooled leaves still depend on their value groups.

Replace the private temporary SQLite fingerprint index with a bounded Store-owned
standard-library BTreeSet, preserving the same retained window, authenticated full
equality and minimum matching ordinal. This selected proposal still needs actual
memory/speed/reuse proof; removal is not implemented. Build/hash each exact value-
group body before compression to remove compress/clone/decompress/hash work.
C2 remains callable with supplied canonical objects and a backend, without a
Workspace-shaped coordinator, filesystem workflow or history entities.

## Simplified payload delta policy

The [policy and table proposal](content-storage-policy-and-tables.md) defines
WHOLE_FILE/CHUNK as logical roles and FULL/DELTA as physical encoding. It proposes
queryable object-role/base fields in the existing object index, a derived inventory
view and one persisted Store policy, with schema/format and performance gates.

The [joint payload model](content-storage-co-design.md#simplified-payload-storage-model)
consolidates whole-file and CDC-chunk delta selection into shared functions with
explicit role-specific limits. Defaults remain a 128 KiB construction cutoff,
8 whole-file links and 4 chunk links; these three settings are configurable within
a supported profile. Existing byte/work and memory bounds remain enforced.
Larger settings such as 1 MiB/50 are experimental candidates, not new defaults.

The [file-content contract](file-content.md#7-bounded-reads-and-delta-cooperation)
supplies whole-file predecessors and bounded chunk correspondence with finalized
output. Preserve actual candidate ordering and regular-file provenance; current-
result edit positions are not automatically original-base correspondence. Reuse
existing batched small-base lookup and qualify batching native locator demands
without widening search, dropping hints or concealing required base reads.
WHOLE_FILE may use the existing bounded admitted-FULL cache when no explicit anchor
is usable; CHUNK keeps its first supplied candidate only. A completed nonwinning
trial selects FULL normally. A codec/acquisition failure fails the operation.

Use the [single-attempt contract](physical-encoding-and-packing.md#one-attempt-no-retries):
no stale-proof refresh/reprepare, busy handler, transaction retry or SDK replay.
Known own writes may preserve valid authority; unexpected invalidation fails.
Ordinary bounded backpressure remains allowed before any failed attempt.

Remove duplicate decision implementations, repeated base acquisition within an operation,
and broad error-to-FULL paths. Reuse the existing shared prefix codec; consolidate
chain traversal/accounting while retaining required role/framing validation,
authentication and bounded codec resources. Missing optional base hints may
select FULL; missing dependencies during required reconstruction remain errors.
Do not generalize this change into cross-role delta bases or a metadata-format
rewrite. The proposal adds ordinary policy data/functions, not another component
or plugin framework, and leaves admission, packing and SQL responsibilities intact.

## Invariants across the boundary

- [Dependency closure, objects.rs:4071](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4071)
  and [collision checks, objects.rs:4089](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4089)
  remain required. Presence alone cannot replace an integrity check.
- [Publication, admission.rs:1295](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1295)
  currently refreshes stale membership. Target: preserve valid authority and planned
  exact-CAS checks; invalidation fails without refresh/reprepare.
  [Base chronology, admission.rs:1447](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1447)
  still constrains physical references. Stale preparation never bypasses validation.
- [AdmissionSession, objects.rs:2238](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2238)
  owns writer serialization and rollback scope; [resolve/rollback, objects.rs:2493](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2493)
  can quarantine a Store after failed cleanup. Any simpler replacement must carry
  the same required safety obligations.
- [Direct candidate publication, workspace.rs:326](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L326)
  combines final admission and history update in a transaction callback.
  [Workspace publication, workspace.rs:450](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L450)
  finishes admission, stages/retains the root, then performs a separate
  conditional history transaction. Preserve each route's observable contract;
  a common internal implementation must account for their different ownership.

Private state machines and wrappers may be deleted or replaced. Review them by
the guarantees above, not by their existing names or source locations.

## Save coordination must not become another expensive pass

Owner requirement: admission must not introduce a substantial performance penalty.
Treat admission as the object store's save coordination, not another service or
generic validation pipeline. It orchestrates the existing bounded work; independent
measurement must not duplicate preparation or force additional transactions.

```text
bounded finalized objects + ID / role / references
                  |
      batch membership / required checks
         /                       \
   exact reuse                missing objects
                                  |
                       existing encode / pack / SQL
         \                       /
            acknowledged result
```

Required design constraints:

- Carry established ID, role, length and direct references from checked local
  construction. Do not rehash/reparse solely because ownership moves between
  internal functions. Untrusted/imported bytes still cross checked decoding and
  authentication; caller-supplied labels are not proof. SQL integrity and exact
  collision checks remain required.
- Batch membership and location acquisition together. Resolve only dependencies
  not already proven available under valid ownership, using bounded lookup pages.
  Membership is not one SQL call per object. Required collision/base payload reads
  are separate real work; do not promise that one query completes an entire save.
- Reuse a valid membership/absence result through the selected write. Unexpected
  authority/state invalidation fails without refreshing/repreparing. Do not add
  unconditional before/after membership queries, retry loops or per-object locks.
- Reuse proven private batch uniqueness and established references. The reference
  [temporary uniqueness set](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L201)
  and [reference reparsing](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4071)
  are specific removal candidates when the checked handoff provides those guarantees and references.
  Keep validation on external inputs and duplicate-byte collision comparison.
- Transfer payload ownership without a coordination-only clone, encode/decode
  cycle, queue hop or temporary staging pass. Charge every buffer needed by actual
  encoding, collision comparison, packing or the selected database adapter.
- Validate dependencies incrementally. Add no finish-time full graph/Store scan,
  operation-sized object map, or retained payload set. Required existing index
  initialization/synchronization work remains attributed where it occurs; this is
  not a claim that every cold-open operation is independent of Store size.
- Share batches, packs and transactions across files. Apply backpressure at the
  bounded consumer, not a flush/acknowledgement per object. Use coarse timer scopes
  around real calls, without per-object report growth or a new monitoring system.

Qualify this with matched v0.1.6 successful complete saves/reads and isolated real
membership/dependency/encoding/packing/SQL work. Include fresh objects, exact reuse,
many small objects, large-file small edits, supported threshold transitions and
slow consumers. Record actual queries, acquired/written bytes, collision reads,
payload copies/hashes, assemblies, transactions and peak simultaneous ownership.
Preserve declared cache state, worker count, storage quality and acknowledgement
semantics. Existing-or-better performance remains the gate, not an invented allowed
overhead percentage. A quicker failure, omitted integrity work or warm setup cannot
earn a passing result. No performance qualification has been run for this proposal.

## Independent measurement and proof

The [persistence cut list and measurement contract](admission-and-persistence.md#9-cut-list-and-measurement)
specify the actual query/cleanup reductions. Distinct-reuse receipt changes are
conditional compatibility work, not permission to relabel metrics silently.

- Codec: target/base bytes to actual encoded result; verify decode and identity
  separately. Include the declared codec initialization/reuse policy.
- Pack construction: encoded records/groups to framed packs and valid locations.
- Storage preparation: canonical input plus readers to bounded prepared output;
  include lookup, base reads, comparisons, encoding and packing.
- Object save: real writable Store plus canonical input through admission and
  final acknowledgement; separately reopen/read/authenticate and check reuse.
- Persistence phase: valid prepared/session state through actual remaining pack
  work, checks, SQL/index work and commit. Do not label it SQL-only prematurely.
- Read: IDs through lookup, base reconstruction, decompression and authentication
  under a declared cache state.

A one-object diagnostic must finish its real transaction/acknowledgement. Bulk
production retains shared packs and transactions. Dividing batch time by count
is an average, not observed latency for each individual object; do not force one
transaction per object to produce convenient trace spans.

Prove isolated/integrated canonical equivalence, full/delta/duplicate paths,
missing/corrupt bases, stale membership, wrong session/Store, final transaction
failure, abandonment and cleanup. Verify bounded working state/batches across
many files, slow consumers and uncertain remote acknowledgements, with unchanged
worker policy. The joint review records remaining isolation and measurement gates.
