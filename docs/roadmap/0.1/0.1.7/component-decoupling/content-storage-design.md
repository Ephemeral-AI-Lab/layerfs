# Content-storage design

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Design issue: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
This is the integrated design for the [seven agreed components](cluster-1-2-components.md)
in clusters 1 and 2. The [policy and tables](content-storage-policy-and-tables.md)
define field semantics; the [source audit](cluster-1-2-source-audit.md) and
[co-design review](content-storage-co-design.md) retain detailed extraction evidence.

Three independent read-only reviews covered canonical edits/transitions, physical
encoding/packing/SQL, and Git/performance. Reference source:
`a8a1ba848429d5f2fbba83c2de22dada8c29def9`, reviewed 2026-09-16.
No replacement implementation, build or benchmark was run for this design.
At-least-existing performance is a mandatory qualification gate below, not an
established consequence of modularization.

Owner clarification: successful published versions are immutable. This design
adds no version rollback, history rewind, undo log or rollback service. Failure
handling concerns only incomplete, unpublished attempts and ordinary SQL atomicity.
Operations get one attempt. Conditions requiring retry return failure; no hidden
SQL/SDK retries, stale-proof refresh or write replay. Lost acknowledgement preserves
an unknown persistence outcome until a separately requested inspection resolves it.

Configuration scope is **configurable transparency**: preserve defaults, replace
hidden policy constants, derive necessary capacities and make supported overrides
work consistently. Finding optimal cutoff/depth values is not this refactor's task.

The active I/O work is [logical content plus database storage](content-io.md),
with [source-backed memory and call reductions](content-io-memory-audit.md).
Workspace-specific paths below are reference evidence or later integration
illustrations, not a required future Workspace mode. The core consumes neutral
stable inputs and must remain pluggable to other callers and environments.

## Reading map

The [file-content component design](file-content.md) owns detailed complete/edit/
read inputs, current-result coordinates, replayable replacement sources, decoded
COW boundaries and the file-specific cut list. This document retains the integrated
C1/C2 flow, physical storage policy and full-operation qualification context.
The [filesystem-tree component design](filesystem-tree.md) owns native checked
logical inputs, directory/inline-inode COW, attributes and compact reference
ordering. Existing Workspace code is behavioral evidence; its shape, lifecycle,
spool and checkpoint model are not requirements on either replacement cluster.
The [physical encoding and packing design](physical-encoding-and-packing.md) owns
C2 terminology, candidate selection, physical metadata, chosen pack lifetime and
the detailed cut list. Its single-attempt rule applies to all these paths.
The [save/persistence design](admission-and-persistence.md) completes C2 execution:
one owner, four tables, bounded transactions, read visibility, final composition
and indexed cleanup once. The [fuller review and implementation sequence](implementation-plan.md)
are recorded; next is implementation qualification rather than additional components.

- [Components and contracts](#2-components-and-ownership)
- [Small edits to large files](#3-small-edits-to-large-files)
- [Small/large/empty transitions](#4-smalllargeempty-transitions)
- [Repeated chunk deltas and storage accounting](#5-repeated-chunk-delta-encoding-and-storage-savings)
- [Compression, groups and packs](#6-compression-and-packing)
- [Database dependencies and cleanup](#7-database-descriptors-dependencies-and-cleanup)
- [Reads, deployment and timing](#8-read-path-environment-independence-and-timing)
- [Git comparison](#9-fair-comparison-with-git)
- [Required qualification](#10-performance-and-correctness-acceptance)
- [Implementation and review](#11-implementation-and-review-sequence)

## 1. Decisions and terminology

Keep these configurable Store-creation defaults:

```text
small_file_threshold_bytes = 131072      T = 128 KiB, exclusive
whole_file_delta_max_depth = 8
chunk_delta_max_depth = 4
```

Store policy is persisted and passed explicitly. Opening uses the recorded
policy; unsupported values or conflicting overrides fail. Frame, chain-byte,
read-work, search, scratch and batch capacities remain supported-profile bounds,
not budgets that grow automatically with a depth setting. The 1 MiB/50 proposals
are non-default capacity cases, not proposed tuned defaults. Ordinary CDC retains its frozen 8/16/32 KiB
minimum/target/maximum profile, including a possibly shorter final chunk.

| Logical role | Physical payload encoding | Meaning |
| --- | --- | --- |
| WHOLE_FILE | FULL | Whole file payload without a delta base |
| WHOLE_FILE | DELTA | Whole file payload using an eligible whole-file base |
| CHUNK | FULL | Chunk payload without a delta base |
| CHUNK | DELTA | Chunk payload using an eligible chunk base |

FULL may be compressed. DELTA is a physical representation of an entire
authenticated canonical object, not a canonical patch object. Exact CAS reuse
retains an existing representation without extending its chain. File-state,
extent, directory, inode and metadata roles remain distinct from payload roles.
The empty file retains its empty mapping/file-state representation.

For normal construction, WHOLE_FILE is only the complete payload of a nonempty
file below the configured cutoff T. Files at or above T use chunks and mappings,
without an additional whole-file CAS payload. FULL can describe either a
whole-file record or one chunk; it means no delta base is needed, not no
compression. See the [representation examples](content-storage-policy-and-tables.md#whole_file-applies-below-the-configured-cutoff).
Historical whole-file physical owners/chunk slices are separate compatibility
formats, not another default logical-file policy.

The [configurable-cutoff design](content-storage-policy-and-tables.md#making-the-whole-file-cutoff-genuinely-configurable)
separates routing policy, derived codec/frame/batch capacities and format/resource
ceilings. Supporting a cutoff above 128 KiB requires coherent encoding, decoding,
bounded singleton packing and FULL-object admission; changing only the routing
constant is incomplete. Configuration testing proves this behavior and preserves
the default performance baseline; it does not select new optimal values.

## 2. Components and ownership

LayerStack, Branch and logical Commit are outside both clusters. History/workflow
owns those entities, expected-head policy, history records and publication. The
name of the reference layerfs-layerstack-store crate does not carry into the new
boundary. Cluster 2 owns object persistence and SQL transaction mechanics;
its SQL COMMIT must not be confused with a logical Commit operation.

```text
SDK / CLI / agent adapters / FUSE / selected runtime
                         |
              workflow and Workspace owner
        freeze edits / reserve identities / publish history
                         |
             stable inputs + explicit resources
                         v
 CLUSTER 1: CANONICAL CONTENT
 +------------------------------------------------------------+
 | file content                 filesystem tree and metadata  |
 | whole payload / CDC          paths / directories / inodes   |
 | extent COW / range reads     reference accounting / COW     |
 |                  \           /                             |
 |                 canonical objects                          |
 |          framing / identity / references / validation       |
 +---------------------------+--------------------------------+
                             |
      bounded canonical batches + optional predecessor hints
      authenticated reads; unfinished boundaries stay in C1
                             |
 CLUSTER 2: PHYSICAL STORAGE  v
 +------------------------------------------------------------+
 | object store and admission: membership / session / bases    |
 |          |                       |                 |        |
 | physical encoding             packing           SQLite     |
 | FULL/delta + compression      records/groups    rows/query  |
 | reconstruction               placement         transactions|
 +------------------------------------------------------------+
```

These are responsibility boundaries, not seven new crates. Reuse ordinary
functions and existing narrow capabilities. Cluster 1 does not import Workspace,
FUSE, a concrete Store or a writable canonical SQL connection. Supplied readers
can perform declared I/O. The target removes generic candidate payload spill and
scratch storage through finalized output; multi-edit boundaries and namespace
reference ordering remain proof gaps. Canonical-Store-write-free construction
is a separate claim from construction with no SQLite provider at all.

Cluster 2 must save/read/authenticate canonical objects without creating history
entities, a Workspace or a mount. History and storage may share one SQLite
database and the existing required atomic transaction; their schema/command
ownership stays separate. No history commands belong in the cluster 2 public
object API. The exact shared-session composition remains to be specified.

### Boundary contracts

The [finalized-object handoff](finalized-object-handoff.md) owns detailed object fields,
finality, allocation release, backpressure and completion semantics. This table
summarizes each component's place in the integrated operation.

| Boundary | Required data and ownership |
| --- | --- |
| Construction input | Immutable base root/role/length, ordered known edits or explicit complete input, exact range reader, format/policy, reserved namespace identities |
| Construction output | Immutable owned bytes, ID, established role and direct references; derive canonical length from bytes; return root/logical summaries separately. Acceptance is not persistence or publication |
| Physical hints | Optional predecessor roots/ranges/IDs; missing hints affect encoding opportunities, not canonical content or required integrity |
| Codec input/output | Supplied target/base bytes and role limits -> selected framed encoding; no live Workspace or SQL ownership |
| Packing | Selected records + placement context -> pack bytes and valid locators; no whole-candidate materialization |
| Persistence | Same-Store admission/session ownership and lookup results, rows and selected packs; preserve race checks, unpublished-attempt cleanup and publication atomicity |

The production path can stream bounded batches and overlap work. Independent
measurement must not introduce a second constructor, a no-op destination that
breaks read-your-writes, a serialized portable SQL plan, or extra transactions.
Bounded private object batches/cohorts may be admitted before construction
finishes under the existing unpublished-attempt ownership; no whole-candidate buffer is
required to delay authoritative publication.

The [I/O contract](content-io.md) applies this budget across many files/directories,
with shared batches/packs and incremental namespace construction. Avoid retaining
all file results and avoid SQL transactions or pack flushes per file. Bounded
working memory stays with its algorithm; no buffer manager or generic spill
fallback is added. Filesystem construction retains a narrow compact inode-effects
ordering mechanism with explicit resource ownership. Its costs remain visible;
checked retained bindings may arrive incrementally without an all-files plan.

## 3. Small edits to large files

The [file contract](file-content.md#3-edit-coordinates-and-normalization) uses
normalized edits in current-result coordinates, preserving existing operation
segmentation and checked arithmetic. Required no-op comparisons may replay stable
replacement ranges; their acquisition and base reads remain measured work.

For a known edit, retain old extent references and chunk only replacement bytes:

```text
old mapping:  [A] [------------- B -------------] [C] [D]
                              edit range
                                  |
                                  v
new mapping:  [A] [B prefix slice] [X] [B suffix slice] [C] [D]
               ^        ^                 ^           ^   ^
                    existing payload IDs and offsets reused

X = newly constructed replacement payload(s)
Only affected mapping paths are rebuilt through immutable COW.
```

This is stronger than re-running CDC over the whole file and hoping to recover
unchanged chunks. The existing [mutation planner](../../../../../crates/layerfs-workspace/src/changes.rs#L1916)
and [replacement scan/split/concat](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L69)
already do this. Extent splitting retains the same payload ID with adjusted
source offsets. Preserve it through extraction.

For R replacement bytes, canonical payload work follows R plus touched mapping
work, required equality reads and physical dependency work. This is not a claim
that the entire Commit is O(R): dirty namespace/frontier handling, fragmented
pieces, allocation, scratch, admission and acknowledgement also cost work.

```text
known range edit                         first import / complete new stream
      |                                                |
reuse unaffected extents                         consume all supplied bytes
      |                                                |
CDC replacement bytes only                       streaming full build
      |                                                |
update touched mappings                          bounded mapping builder
      +---------------- same canonical/physical output boundary ------------+
```

First imports and complete replacements pay O(B) for B supplied bytes. Unknown
edit provenance cannot be treated as trusted ranges. The current
[file_matches path](../../../../../crates/layerfs-workspace/src/changes.rs#L2034)
can compare full final/base digests; do not route a known edit through it merely
because an adapter lost provenance. Choose known-edit, complete-input or
representation-conversion work explicitly before execution. Remove error-driven
full rebuilds; retain deliberate complete construction and genuine no-op checks.

Exact root equality is required for the same operation/profile/base/edit order.
Different valid extent layouts or edit histories can represent identical logical
bytes with different roots; do not promise universal file-byte-to-root equality.

The structural simplification preserves split/concat/partition choices while
replacing encoded draft maps with owned unfinished nodes. FileMutationBatch already
streams payloads and sealed nodes; its structural budget is not the whole-operation
memory bound. The [finality counterexample and proof gates](file-content.md#5-immutable-file-cow-and-final-construction)
explain why simply emitting every valid node or rebuilding the entire mapping
would not preserve the required roots and cost.

## 4. Small/large/empty transitions

For one normalized known-edit operation, select the final representation from its
declared final length and verify actual lengths before success. The detailed
[small/large paths](file-content.md#4-empty-small-large-and-transition-paths) remove
classification-only probes for known lengths and separate retained head/tail
allocations where one final buffer or streaming composition suffices. Unknown-length
complete input retains bounded probing. No-op root reuse must remain compatible
with the selected profile or use an explicit qualified conversion.

```text
                   final raw logical length
                    /          |          \
                   0        0 < n < T     n >= T
                   |           |            |
                 EMPTY     WHOLE_FILE     CHUNKED

WHOLE_FILE -- grow across T --> stream final result through CDC --> CHUNKED
CHUNKED    -- shrink below T -> read retained result ------------> WHOLE_FILE
either     -- result empty --> empty mapping/file-state --------> EMPTY
```

| Transition | Required algorithm and cost |
| --- | --- |
| Whole -> whole | Construct/hash complete resulting payload, bounded by T; apply eligible whole-file FULL/delta policy |
| Chunked -> chunked, known edits | Preserve unaffected extent slices; scan replacement bytes and update touched mappings |
| Whole -> chunked | Stream final content once through CDC; retained old whole payload is below T, but all new bytes still cost work |
| Chunked -> whole | Read only retained ranges needed for the final result plus replacement bytes; build one whole-file object |
| Any -> empty | Construct defined empty state; do not read discarded payload solely to delete it |
| Empty -> nonempty | Use final size and ordinary constructor |

The [Workspace routing](../../../../../crates/layerfs-workspace/src/changes.rs#L1844)
and [content replacement](../../../../../crates/layerfs-content/src/file/content.rs#L245)
already support conversion. Preserve the distinction between retained and
discarded content:

```text
large -> small, deleting the middle

old: [keep head] [................ discard ................] [keep tail]
           |                                                     |
           +--------------------+--------------------------------+
                                v
                   construct new whole-file result

Do not acquire discarded logical ranges solely to perform the deletion.
```

Less than T retained logical bytes does not imply less than T physical I/O.
Selected ranges can be slices of larger chunks, compressed groups and delta
chains. Charge selected mapping traversal and full required dependency work,
especially after many fragmented edits. No file-size-proportional whole-old-file
scan may be introduced just because the old file was large.
Selected physical records/groups/dependencies can unavoidably include bytes
outside the retained logical ranges; this work is included, not concealed.

At `T-1 -> T -> T-1`, each crossing performs roughly O(T) logical conversion
under the existing formats, even for a one-byte edit. Test this oscillation
explicitly; do not introduce hysteresis, sticky roles or duplicate representations
silently. A small-to-500-MiB append necessarily consumes the appended bytes with
bounded live memory; crossing the threshold is not a constant-time operation.

Physical delta chains do not have to continue across roles. The current
[SmallContent predecessor path](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3318)
does not create a CHUNK predecessor cursor. A first whole-to-chunked conversion
can reuse identical existing chunks but otherwise stores new chunks as FULL
without a suitable chunk base. A chunked-to-whole conversion needs an eligible
whole-file base or selects FULL. Retained snapshots must remain readable across
every conversion; no cross-role delta trick is required.

## 5. Repeated chunk delta encoding and storage savings

```text
newly constructed payload
           |
      exact CAS match? -- yes --> reuse existing ID/encoding
           |
           no
           v
  bounded delta-base candidates
           |
  authenticated eligible base?
      /                 \
     no                 yes
     |                   |
   FULL          compare FULL and PREFIX alternatives
                         |
             savings + role depth/work/memory bounds
                   /                 \
                  yes                no
                   |                  |
                 DELTA               FULL
```

In the current CHUNK path, a cursor supplies up to four previous-file overlap
hints, but admission tries only the first. It does not perform global similarity
search. Preserve bounded predecessor/range information through the new boundary;
adding another search strategy is a separate measured change. Sources:
[cursor](../../../../../crates/layerfs-content/src/file/rope/read.rs#L395),
[selection](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L649).
WHOLE_FILE additionally retains the bounded admitted-FULL candidate cache when no
explicit predecessor anchor is usable. It does not try another candidate after a
completed losing trial. The diagram's choices are planned eligibility or completed
cost comparisons; actual codec/acquisition failures fail the operation. See the
[detailed selection rules](physical-encoding-and-packing.md#4-payload-selection-and-delta-base-candidate-quality).

Repeated version example, assuming each changed chunk selects its predecessor:

```text
version       retained mapping             selected new physical record
v0            A, B0, C                    B0 FULL      depth 0
v1            A, B1, C                    B1 -> B0     depth 1
v2            A, B2, C                    B2 -> B1     depth 2
v3            A, B3, C                    B3 -> B2     depth 3
v4            A, B4, C                    B4 -> B3     depth 4
v5            A, B5, C                    B5 FULL      depth 0
```

A and C are reused, not repeatedly encoded. At v5 the illustrated immediate-base
delta is ineligible; FULL creates an independent starting point. Another
eligible base, a byte/work bound or lack of savings can change this sequence.
Append-only data with no overlap hints, unrelated content and representation
transitions need not produce deltas. Unchanged objects keep their existing depth.
This is an ongoing opportunity on new admissions, not a promise that every edit
produces another delta or that background compaction is required.

For native chunks the actual [record comparison](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L750)
is `37 + prefix_frame_bytes < 5 + full_frame_bytes`. Record savings do not alone
prove Store savings or speed. Account for the graph chronologically:

```text
retained Store cost
  = all unique retained FULL and DELTA records,
    including dependency-only bases, counted once
  + group/pack directories and framing
  + objects/policy/dependency indexes + SQLite page allocation
```

An early base choice can force a later FULL reset. Compare complete retained
history and exact readback, not a sum of individually best pairs. Low delta
selection can be healthy when exact reuse already avoids new records. Report
reuse, admitted FULL/PREFIX records, actual depth distribution and reconstruction
work separately. Historical [CDC analysis](../../0.1.5/issue100/experiments40/full157/cdc/report.md)
is informative about these tradeoffs, not current-source speed qualification.

## 6. Compression and packing

### Codec alternatives, not mandatory separate passes

```text
target canonical payload             authenticated base payload, if eligible
              |                                     |
              +--> Zstd without prefix --> FULL     |
              |                                     |
              +--> same codec with base prefix <----+
                                     |
                                  PREFIX
                                     |
                 select record using complete cost and limits
                                     |
                              packing component
```

The [NativeEncoder](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L378)
already shares the codec with explicit profiles. DELTA and compression can be a
single prefix-codec invocation. Keep the pinned parameters and bounded scratch;
do not create a second generic compressor layer or concatenate base+target into
an unbounded temporary buffer. The existing comparison computes a FULL candidate
and may compute a prefix candidate; count both, plus base acquisition and codec
initialization. Smaller selected output is not proof of lower CPU cost.

Native chunk and whole-file payload frames are compressed per record. Native
groups contain RAW framing and do not apply a second group-compression pass.
Ordinary and metadata groups may use group-level compression under their existing
format rules. Preserve these distinctions when sharing codec plumbing.

### Placement and group ownership

```text
bounded canonical batch
        |
membership / dependencies -> selected FULL or DELTA records
                                      |
                        group plan (roles/formats/capacity)
                                      |
                       choose placement BEFORE assembly
                         /                       \
             append to compatible open pack     new pack / supported singleton
                         \                       /
                            assemble once
                                  |
                    packed bytes + final record locators
                                  |
                         bounded SQL writes
```

Ordinary reference group/pack ceilings are 64 KiB / 256 KiB with explicit
special-role and oversized-singleton rules. Native grouping is planned using
FULL-alternative sizes before selected PREFIX sizes are known
([admission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L632)).
Preserve this planning/order/budget behavior first. Packing selected deltas more
densely by changing group membership is a separate performance/format decision.

Eliminate the current retained-tail/merged-pack double assembly where possible.
Calculate append fit from lengths/counts before copying. Capacity exhaustion is
a planned new-pack result; structural assembly errors propagate. The current
[append path](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2401)
clones groups and assembles before deciding, and catches assembly errors as a
new-pack outcome. Avoid replacing that with another hidden retry.

The selected target retains compatible append and assembles the selected write
once from borrowed groups. Fill -> seal -> insert once is deferred as a separate
qualified replacement: it changes visibility, candidate availability and short-pack
density. It is not an alternate runtime mode. No speed/storage gain is proved.

The current RAW singleton pack path also stages header/object bytes through a
temporary file before allocating and rereading the assembled pack. Remove that
round trip only with bounded in-memory assembly that preserves supported formats,
sizes and collision operands. The output writer's smaller object limit must be
reconciled with admission and codec limits for accepted profiles. These are
[IO13/IO14](content-io-memory-audit.md#2-core-optimization-ledger), not evidence that
all large objects already have a no-spill route or should use RAW encoding.

Keep partial reads possible: locate only required groups/records, validate their
bounded framing, and reconstruct selected dependencies. Do not decompress every
pack to answer a range read. Also do not add a codec setup, SQL transaction or
network request for every tiny record merely to separate component labels.

### Formats that remain real responsibilities

For physical metadata, pooling compact inode values precedes delta selection on
the pooled leaf. New pooled value groups and leaf representations have distinct
encoding/dependency work. The [filesystem/metadata review](filesystem-tree.md#c2-physical-inode-value-pooling)
separates these C2 effects from logical attribute trees. The detailed C2 proposal
selects a Store-owned bounded BTreeSet instead of the temporary SQLite fingerprint
index, preserving the window and exact-equality/minimum-ordinal semantics. It also
builds/hashes each value-group body before compression, removing the digest round
trip. Allocation, speed and storage-quality equivalence remain implementation gates.
Value-group bytes live in packs; metadata_value_groups catalogues locations/digests.

| Current writer family | Required treatment |
| --- | --- |
| Ordinary pack 1 | Preserve ordinary and supported oversized object handling |
| Native pack 2 | CHUNK FULL/PREFIX records and bounded reconstruction |
| Compact-small pack 4 | WHOLE_FILE FULL/PREFIX records and compact framing |
| Pooled-metadata pack 6 | Metadata values, inode-leaf encodings and required dependency/ordinal handling |

These are active families, not a list where the highest number replaces all the
others. Metadata deltas use a distinct copy/literal program with current
[16-edge / 128-KiB canonical closure limits](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L148).
The payload 8/4 configuration does not change that algorithm. Preserve its
semantics during extraction; do not silently classify every nonpayload as FULL.
Historical formats and physical whole-file owners/chunk slices require explicit
reader descriptors. No old reader is removed solely because of its version number.

## 7. Database descriptors, dependencies and cleanup

The [table proposal](content-storage-policy-and-tables.md) owns exact field
meanings. Logical object role and physical encoding remain independent.

```text
store_policy -- loaded once --> content / admission / encoding

objects
  object_id <--------------------- base_object_id (direct delta dependency)
  object_role
  canonical_length
  pack_id -----------------------> object_packs(pack_id, data)
  group_number, record_number

object_inventory = optional readable view, no duplicate object rows
metadata_value_groups = separate pooled-value locations and ordinal ownership
```

For ordinary WHOLE_FILE/CHUNK FULL/PREFIX records, a NULL base means FULL and a
non-NULL base means DELTA. Metadata direct delta bases must also be retained and
represented consistently; physical slices and pooled-value dependencies need
their actual ownership descriptors. An inventory view cannot label a supported
slice as FULL just because its delta-base column is NULL. Complete the role/format
map before freezing the schema; the four payload examples are not a complete
compatibility schema.

The new role/base metadata costs row bytes, index bytes and SQL work. Populate
it from validated construction/encoding results, not encode/decode round trips.
Pack/index metadata must agree where duplicated. No performance claim follows
from making these fields queryable.
The [selected indexes and failure contract](admission-and-persistence.md#6-four-tables-and-the-necessary-indexes)
use a unique physical locator index, non-NULL direct-base index and immediate
NO ACTION self-FK. Earlier-base placement enables bounded reverse-location cleanup;
no operation-sized dependency graph or reliance on DELETE row order is introduced.

### Successful versions stay; failed unpublished attempts may be cleaned up

```text
prepare attempt ----> authoritative publication ----> version retained
      |                         |
      | fails beforehand        | reply/completion acknowledgement lost
      v                         v
abort open SQL transaction      return failure / unknown persistence outcome
clean owned unpublished data    never undo or republish that version
```

Publication is the boundary, not receipt of a reply. If the outcome is uncertain,
return the failure without replay or automatic polling. A separately requested
authoritative inspection must establish ownership/outcome before cleanup. Keep
only required identity checks; introduce no retry or general undo system.
An early SQLite cohort COMMIT stores private preparation data; it does not
publish a successful version.

The [handoff completion contract](finalized-object-handoff.md#6-completion-and-failure)
distinguishes accepted bytes, successful construction and acknowledged storage.
Successful standalone storage completion retains its output and ends failed-
admission cleanup ownership; later history failure does not silently undo that
completed save. Any unused-content retention decision belongs to its separate owner.

The foreign-key issue below applies to failure cleanup already performed by the
reference, not deletion of successful versions:

```text
P0 (existing retained base) <- U1 (unpublished) <- U2 (unpublished)

failed attempt owns only U1 and U2:
    delete U2 -> delete U1; keep P0 and all published versions
    remove only now-unreferenced attempt-owned packs
```

Current [failed-admission cleanup](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2529)
deletes early-committed owned objects in ObjectId order. The selected
[save cleanup](admission-and-persistence.md#7-failure-ownership-and-bounded-cleanup)
uses exclusive pack-range ownership, earlier-base placement for new writes and
bounded reverse-location pages. The direct-base FK is immediate NO ACTION so a
whole page can be deleted without depending on SQL's internal row order. Remove
owned packs/catalogues only after all owned objects; no unbounded cleanup graph,
foreign-key disablement or cascade into retained canonical dependents.

Location and base indexes are selected requirements. Their query plans, write
cost and cleanup behavior must meet the same actual-schema performance gates.
If they regress those gates, revise the implementation before accepting it.

Preserve bounded SQL transactions, writer serialization, membership/race checks and
safe handling of failed unpublished attempts. Final publication remains conditional on the
captured head/base. SQLite executes transactions; the outer workflow decides
which history may advance. No one-object-per-transaction rule is introduced.
Where required, final object writes and the workflow's publication writes remain
in the same atomic final transaction, as the existing owner callback allows.
The storage-completion barrier must not split that transaction or add a separate
commit. It establishes acknowledgement under the backend's actual contract, with
no stronger crash-durability promise inferred from the word persisted.

## 8. Read path, environment independence and timing

The [generic I/O contract](content-io.md#3-three-small-capabilities) owns the
minimal core input/read/output requirements; this is not a separate reader or
buffer component. Transport acquisition and lifecycle stay outside these clusters.

```text
requested file range
        |
decode root role / select extent slices
        |
batch object locations + explicit physical descriptors
        |
acquire bounded selected records and required base chains
        |
FULL base -> reconstruct next -> authenticate -> ... -> target
        |
return requested slices
```

Use bounded iterative reconstruction. Small requested slices can still require
complete chunk/whole-file reconstruction and group reads; record that amplification.
An ObjectId check authenticates canonical bytes after reconstruction. Missing or
corrupt required bases are errors, not permission to return partial content.

```text
local adapter ------------------+
                                +--> same component bodies
existing bounded transport -----+       explicit input / Store / resource owner
                                        |
                                  embedded SQLite OR
                                  bounded remote DB adapter
```

Ordinary local composition uses direct calls; component boundaries do not imply
per-object RPC. Later callers select placement, establish stable inputs/policy
and preserve edit-range provenance. No particular Workspace mode is required.
Host and daemon remain supported roles. The
[remote database contract](content-io.md#6-pluggable-to-later-environments) requires
grouped reads, atomic bounded writes, writer authority and explicit uncertain
outcomes; local BLOB handles and cleanup watermarks are not portable assumptions.
Deployment-specific integration comparisons must separately obey the approved
benchmark topology; this core contract does not change it.

The implemented [timer](../../../../../core/crates/layerfs-telemetry/README.md)
can observe coarse real calls:

```text
content.operation
  content.construct
    plan
    whole_file.build OR chunked.build
    mapping.finish
  storage.save
    membership
    base.acquire
    physical.encode
    pack.build
    persistence
      connection.wait / sql.begin / sql.statements / sql.commit
```

This is a responsibility map; actual nesting follows real invocation. Streaming
construction may invoke or wait on admission, so phase times can overlap and
must not be summed or subtracted into fictional CPU time. Transaction lifetime
can include construction/encoding between BEGIN and COMMIT; measure it separately
from SQL execution without changing the transaction.

Select Timing::record or Timing::disabled before execution. Inject child scopes
within the same thread. Existing workers return owned completed reports for
attachment before propagating their Result; live scopes are not Send/Sync. Keep
coarse batch scopes within the recorder's 1,024-node/32-level limits. No node per
chunk, syscall or delta edge. The outer caller renders/saves JSON after the
operation; report-output failures must not replace the product result. Depth,
work and memory evidence remain component/harness data, not new timer features.

## 9. Fair comparison with Git

The useful comparison is update granularity, not the claim that Git always scans
every file's bytes. Git's index/stat cache, fsmonitor and untracked cache can
avoid repeated unchanged-path discovery. Ordinary commit records the index;
changed-file staging occurs in add or automatic staging. Sources:
[git-update-index](https://git-scm.com/docs/git-update-index),
[git-add](https://git-scm.com/docs/git-add),
[git-commit](https://git-scm.com/docs/git-commit).

```text
Git, ordinary actually changed file       LayerFS, known range edit
----------------------------------       -------------------------
discover changed path                    record affected ranges
stage complete changed blob              freeze dirty input
read/hash its content                    retain unchanged extents
store object if needed                   construct replacement chunks
commit index/tree                        update touched mappings
pack/repack may delta whole blobs         bounded online FULL/PREFIX + packs
```

Git already compresses objects and delta-compresses packed blobs. Current source
can stream large input directly into a non-delta pack, so not every add first
creates a loose object. See [Git object-file.c](https://github.com/git/git/blob/master/object-file.c)
and [Git packfiles](https://git-scm.com/book/en/v2/Git-Internals-Packfiles).
Its [pack-depth documentation](https://git-scm.com/docs/git-pack-objects) also
explains deeper-chain unpacking cost; depth 50 is not a latency qualification for
LayerFS.

LayerFS can avoid rereading unchanged payload on known small edits and can read
selected chunks rather than reconstructing a complete large-file blob. That
makes its mechanism suitable for frequent agent checkpoints. It is not proof
that every workload is faster: imports, whole replacements, cutoff conversions,
fragmented edits, SQL overhead and cold dependency chains still need measurement.

Compare equivalent complete workflows and bytes. Do not compare LayerFS Commit
against git commit while omitting git add, or compare an SDK range edit against
an unrelated full-file rewrite and call the ratio a storage-engine speedup.
Git comparison is a separate study; existing LayerFS is the refactor acceptance
baseline. Offline repacked size and live checkpoint latency are separate claims.

## 10. Performance and correctness acceptance

### Required gate

For the current core work, use the [isolated v0.1.6 comparator contract](content-io.md#7-measurement-and-completion).
Workspace/public Commit rows below remain later integration obligations; their
mode and topology are not assumed by core design or credited to core speedups.

Under unchanged default policy and matched inputs, operation surface,
acknowledgement, cache state, topology and workers:

```text
candidate time(case) <= reference time(case) + predeclared allowance(case)
```

The default allowance is zero. The benchmark rules permit a preregistered
absolute noise allowance for small local phases; it cannot be invented after
results, generalized into a broad slowdown allowance, or waive absolute limits.
Keep existing resource/latency ceilings and no-amplification requirements. Storage
savings or a faster average cannot compensate for a failed read, transition,
Commit, cleanup or memory case. Noisy/incomplete evidence cannot establish this
claim; retain it explicitly instead of rerunning for a nicer number.

Use one construction producer except namespace initialization, with identical
declared reference/candidate wiring. The source still has multiworker defaults;
do not silently compare an unrestricted old arm with a one-worker new arm or
claim to beat the old unrestricted default. Preserve the Init exception. Record
the matched comparator and all source/product/harness identities before collection.

### Required case coverage

| Case group | Cases that must be represented |
| --- | --- |
| Boundary construction | Empty, 1 byte, T-1, T, T+1; supported overrides and rejected settings |
| Known large edits | Fixed overwrite/insert/delete/append at head/middle/tail across declared increasing base sizes |
| Full input | First import and unrelated complete replacement; streaming memory and EOF validation |
| Whole -> chunked | Append and insertion across T, large append and zero extension |
| Chunked -> whole | Truncate, retain tiny suffix, delete middle retaining head+tail, complete replacement, zero result |
| Oscillation/fragmentation | Repeated T-1 <-> T with retained versions; fragmented retained ranges and subsequent conversion/read |
| Physical history | Exact reuse, useful/useless deltas, default maximum depths and next-link FULL selection, byte/work exhaustion |
| Reads | Declared cold small-range/full reads, current and retained roots, deepest actual dependencies |
| Packing/DB | Group/pack boundaries, supported oversized records, append/new placement, descriptor/index cost, batch lookup |
| Failure/no-op | No-op edits, malformed length/EOF, missing/cyclic/corrupt bases, contradictory descriptors, pre-publication failures and bounded owned-data cleanup; lost replies must preserve published versions |
| Integration | Public edit+Commit/snapshot acknowledgement, hardlink/reference correctness, reopen and all retained byte oracles |

Known large edits must show work following changed spans/touched mappings rather
than old-file bytes. Large-to-small work must exclude discarded ranges while
charging selected physical dependencies. Full-input and transition work remain
inside operation time. Qualify component calls independently and the same bodies
through the complete public workflow. Test output/identity separately from timing.

New configuration/schema rejection cases may have no identical historical path.
Qualify their correctness and resource bounds without inventing paired speed
ratios. Unchanged-default public operations retain the strict matched speed gate.

Record required logical/CDC bytes, mapping accesses, emitted/reused objects,
admitted FULL/PREFIX records, actual depth, encoded/decoded base work, pack bytes,
SQL statements/transactions/page allocation, complete operation latency and
phase-scoped live memory/backing/cache. Byte counters describe their actual scope;
requested-range bytes alone do not report physical reconstruction cost.

### Execution discipline

Freeze registered cases, identities, thresholds, exact commands and verification
oracles before measurement. Follow [benchmark rules](../../../../general/benchmark_rules.md),
[benchmark instructions](../../../../../benchmark/AGENTS.md),
[runner mechanics](../../../../../benchmark/fs-bench-pro/QUICKSTART.md) and
[repository rules](../../../../../AGENTS.md): one sample per case/arm; append-only
receipts; independent verification; post-init setup clone without cache credit;
equal declared cold-state enforcement; no concurrent resource-sensitive work.
No new worker, timeout extension, reduced workload or warmed base may rescue a
failure. Respect complete-command 15-second budgets and only the declared
25-second exceptions; report unrun cases and qualification gaps. These tables
are design coverage, not registered benchmark IDs or executed performance proof.

## 11. Implementation and review sequence

The [ordered co-design decision list](content-storage-co-design.md#remaining-co-design-decisions)
separates settled architecture from the concrete contracts below. It is the
discussion checklist; this section is the subsequent implementation sequence.

1. Freeze neutral input/output ownership, role/policy contracts and exact
   supported-format/old-Store compatibility. Keep default algorithms/parameters.
2. Extract the existing complete-file finalized-output path through real storage
   and authenticated readback, including backpressure and late EOF failure. Extend
   to sorted namespace operations, range edits/transitions and the proven multi-edit
   boundary rewrite. Resolve reference ordering; qualify operation-wide bounds and
   source-matched behavior before changing physical policy or SQL schema.
3. Consolidate payload codec selection and chain accounting, retaining necessary
   role parameters and distinct metadata algorithms. Keep base acquisition visible.
4. Remove duplicate pack assembly through placement-before-copy; preserve grouping,
   occupancy rules, locators, chronology and failure behavior.
5. Add queryable descriptors only with dependency-index and bounded cleanup proof;
   qualify their storage/SQL cost. Preserve cohort and history publication semantics.
6. Wire existing timer scopes and independent diagnostics into the same bodies;
   qualify disabled/enabled observation overhead and full public cases.
7. Retire reference code only after correctness, compatibility and at-least-speed
   gates pass. Production source stays under core/ with external tests and thin
   <=200-line lib.rs/mod.rs entry files; crate names remain an implementation decision.

### Review outcome and remaining proof

| Review | Finding incorporated |
| --- | --- |
| Canonical/edit/transition | Preserve replacement-only CDC and extent slicing; count necessary conversion and fragmented physical-read costs; no chain continuity promise across roles |
| Physical storage | Reuse prefix codec; preserve FULL-based grouping and active metadata formats; calculate placement before assembly; base FK requires indexed dependent-first cleanup |
| Git/performance | Correct whole-scan premise; compare changed-blob staging with known-range work; freeze per-case gates and matched single-worker baseline |

No reviewer established equal-or-better runtime speed from source inspection.
Exact supported config ranges, full format/descriptor mapping, detailed cleanup
algorithm and matched benchmark specification remain proof prerequisites before
implementation admission. They cannot be replaced by architectural diagrams or
historical storage-saving figures.
