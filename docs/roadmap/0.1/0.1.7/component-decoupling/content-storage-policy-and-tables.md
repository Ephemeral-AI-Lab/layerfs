# Content representation, encoding policy and database tables

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Read with the [overall architecture](cluster-1-2-components.md),
[integrated content-storage design](content-storage-design.md),
[joint co-design](content-storage-co-design.md#simplified-payload-storage-model)
and [physical storage review](object-storage.md).
Tracking issue: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

This specifies the proposed meaning of policy inputs and database fields. It
does not implement a schema, allocate final enum codes, migrate a Store or
change the reference implementation. Exact DDL and format compatibility remain
implementation prerequisites.

Successful published versions are immutable and are never rolled back by this
design. Cleanup below applies only to failed unpublished attempts, preserving
all published objects and any existing physical bases they require.

Owner clarification: the objective is **configurable transparency**, not finding
optimal settings. Keep 128 KiB/8/4 defaults, expose their purpose and supported
range, and make every relevant path honor the selected values. Non-default cases
prove configuration works; parameter sweeps or tuning new defaults are outside
this refactor's scope.

## 1. Role, encoding and placement

| Property | Question answered | Examples |
| --- | --- | --- |
| Logical object role | What do the reconstructed canonical bytes represent? | WHOLE_FILE, CHUNK, FILE_STATE, EXTENT_NODE, directory/inode/metadata roles |
| Physical encoding | Does this object's reconstruction require a delta base? | FULL or DELTA for eligible file payloads |
| Compression and placement | How are the bytes compressed and located? | Prefix-capable codec, pack/group/record locator |

WHOLE_FILE is a descriptive name for the canonical whole-file SmallContent
role. CHUNK is the canonical file-payload chunk role. These proposed labels do
not rename wire tags or change canonical hashing. A CHUNK can contain only a
short replacement range; the role does not assert that every chunk was generated
by rescanning an entire file. The empty file keeps its existing file-state form.

Reserve FULL for the physical encoding: it reconstructs without a direct delta base
but may still be compressed or live in a compressed pack group. Use WHOLE_FILE
for a logical file payload. Avoid the ambiguous labels full_file and full_chunk.
FULL does not mean dependency-free: pooled metadata needs value groups, and
supported whole-owner/slice formats keep their own dependencies. The
[physical encoding glossary](physical-encoding-and-packing.md#2-terms-used-in-this-design)
and selection rules cover all C2 object families.

| Object role | Physical encoding | Meaning |
| --- | --- | --- |
| WHOLE_FILE | FULL | The entire file payload is stored without a delta base |
| WHOLE_FILE | DELTA | The entire file payload is reconstructed using an eligible whole-file base |
| CHUNK | FULL | This chunk is stored without a delta base |
| CHUNK | DELTA | This chunk is reconstructed using an eligible chunk base |

DELTA is not a canonical object role. ObjectId identifies complete reconstructed
canonical bytes, irrespective of FULL/DELTA or compression. REUSE is an admission
outcome, not a third physical encoding: it points to the object's existing row.

### WHOLE_FILE applies below the configured cutoff

In normal construction, WHOLE_FILE contains the complete payload of a nonempty
file whose final logical length is below the Store's configured cutoff T.
At or above T, construction produces file-state/extent objects referring to
chunks. It does not also produce a duplicate whole-file CAS payload for that
chunked representation. File metadata remains separately represented.

```text
0 < file size < T                    file size >= T
        |                                  |
    WHOLE_FILE                      FILE_STATE / extent tree
        |                                  |
    FULL or DELTA                    +-- CHUNK A: FULL
                                    +-- CHUNK B: DELTA
                                    +-- CHUNK C: FULL
```

A FULL chunk is an independently reconstructable chunk, not a complete large
file. FULL may still be compressed. The empty file uses its defined empty
mapping/file-state representation.

| Construction example | Logical representation |
| --- | --- |
| T = 128 KiB; file = 100 KiB | WHOLE_FILE |
| T = 128 KiB; file = 128 KiB | CHUNKED |
| T = 128 KiB; file = 900 KiB | CHUNKED |
| T = 1 MiB; file = 900 KiB, when that override is supported | WHOLE_FILE |

Thus small means below the selected cutoff, not a permanently fixed size
category. These rules select new construction; existing objects are decoded by
their recorded role/profile and are never reclassified from size alone.

The reference also has an authenticated whole-file physical owner for native
slices, explicitly documented as not a logical file root in
[content.rs](../../../../../crates/layerfs-content/src/file/content.rs#L13).
Do not classify that format as WHOLE_FILE SmallContent from its name alone.
It is a compatibility responsibility, not another default logical storage policy.
Supported physical slices and metadata pooling require their actual descriptors;
the four payload cases above do not redefine every historical storage format.
The proposed inventory view covers new ordinary FULL/PREFIX payload records.
Compatibility readers must represent physical slices explicitly. Importing such
records into the proposed schema requires an explicit representation discriminator
or qualified conversion; they cannot enter this view as CHUNK/FULL merely because
their delta base is NULL. Freeze that compatibility contract before implementation.

## 2. File representation and physical encoding policy

Cluster 1 chooses a representation from raw logical length and the persisted
construction policy:

```text
empty file          -> empty FILE_STATE / mapping
0 < length < cutoff -> WHOLE_FILE
length >= cutoff    -> FILE_STATE -> EXTENT_NODE tree -> CHUNK objects
```

The default cutoff is 128 KiB, exclusive. Exactly 128 KiB uses chunked
representation. Existing objects are read by their recorded role/profile; opening
a Store does not reinterpret or rewrite them according to a new cutoff.

Latest owner direction retains the reference defaults as configurable fields:

```text
small_file_threshold_bytes = 131072
whole_file_delta_max_depth = 8
chunk_delta_max_depth = 4
```

This supersedes the proposed 1 MiB default and one numerical depth cap for both
roles. The policy is shared data and implementation with role-specific limits.
Larger values such as 1 MiB/50 remain experiments requiring explicit supported
format/resource profiles; configurability does not make arbitrary values valid.

Cluster 2 uses one shared decision routine for eligible WHOLE_FILE and CHUNK
objects, supplied with the selected role's limits:

1. Check exact CAS membership and integrity. Existing objects reuse their row.
2. For a new object, obtain supported base candidates within a bounded search.
   An explicit predecessor is a hint, not a required dependency of the new object.
3. Authenticate a selected candidate and its chain depth and byte totals. Bases must have the
   compatible role/profile; this proposal does not enable cross-role deltas.
4. Compare complete FULL and DELTA record cost, including framing and the base
   reference. Delta must save space and satisfy prospective depth, reconstructed
   bytes, encoded acquisition, search and live-memory bounds.
5. Select FULL if there is no eligible beneficial delta. Unexpected codec, I/O,
   SQL or integrity errors propagate. Required stored dependencies cannot be
   treated as absent optional hints.
6. Pack the selected record and publish its descriptor and location through the
   existing admission transaction ownership. History publication remains external.

FULL has depth zero. DELTA has base depth plus one. Retain each role's byte/work
accounting and bounds: SmallContent caps canonical and encoded chain bytes;
native chunks cap raw closure and additional encoded/decoded work. Do not change
these units or budgets merely to consolidate code. The configured depth may be
preempted by a byte/work bound. The supported maximum size of an independently
stored FULL object remains a separate format constraint.

The reference 128 KiB/8/4 settings and byte bounds remain both the default policy
and comparison control. Fifty links is an experimental candidate. See the
[performance admission requirements](content-storage-co-design.md#performance-admission-before-changing-defaults).

### Making the whole-file cutoff genuinely configurable

The reference's SMALL_LIMIT is doing several different jobs. Merely replacing
the branch constant with a configuration lookup leaves validators, codecs, packs
and memory accounting hard-coded for below 128 KiB. Separate three categories:

| Category | Meaning | Treatment |
| --- | --- | --- |
| User policy | At what final logical size should new construction choose CDC? | small_file_threshold_bytes, default 131,072 |
| Derived capacity | Maximum canonical/frame size, codec scratch and single-record placement needed for the accepted policy | Compute with checked arithmetic and the pinned codec's sizing functions |
| Format/resource bound | What can the selected wire format represent and the declared operation budget safely process? | Named, documented, validated limits; independent of the construction cutoff |

All SMALL_LIMIT uses must be classified by purpose. In particular the legacy
physical whole-file owner's minimum length is a format rule, not the new
construction cutoff. Do not change legacy decoding by mechanically replacing
every occurrence of 131072 or SMALL_LIMIT with the user setting.

```text
persisted policy + supported format + declared resource limits
                            |
                  validate / resolve once
                            |
               explicit immutable runtime capacities
                   /        |          \
              construct    encode     pack / read / admission
```

This is one ordinary initialization calculation, not a plugin or configuration
framework. Invalid combinations return a specific create/open error naming the
limiting capacity before work begins. Valid configuration is never silently
clamped, changed to CDC, given more workers or allowed unlimited allocation.
One capacity-aware format should cover its documented configuration range; do
not introduce a new codec, format version or named profile for each cutoff value.
Any format change needed to escape the reference's fixed limits is made once
under an explicit compatibility contract, with policy values remaining data.

Required changes across the actual path:

| Area | Required behavior |
| --- | --- |
| Construction and transitions | Use the same T in Workspace planning, known-length/stream construction, capture, initialization and small/large transitions; eliminate stale 128-KiB routing checks |
| Canonical decoding | Select by explicit role/framing and supported format capacity; do not infer role or reject a stored object merely from its relationship to a construction cutoff |
| FULL/PREFIX codec | Derive frame bounds using the pinned codec's compress-bound function and scratch using estimates for the exact configured parameters; cover base and target; preserve current default parameters |
| Record grammar and reader | Validate actual raw/frame lengths before allocating, use checked length arithmetic, and support the same accepted capacities on write and read |
| Packing | Keep the ordinary pack target; plan a dedicated bounded singleton when a valid selected record exceeds it, retaining the selected FULL/PREFIX encoding |
| Batches and memory | Bound total bytes and live codec/base/output/pack ownership; reduce batch occupancy for larger objects and support a budget-checked singleton without changing producer count |
| Delta eligibility | Apply chain-work limits separately from valid FULL object size; over-budget optional bases select FULL, while corruption of required stored dependencies stays an error |

Sources of today's coupled assumptions include
[SmallContent framing and construction](../../../../../crates/layerfs-content/src/file/content.rs#L49),
[fixed codec sizes](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L385),
[static workspace estimate](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L1226),
[compact pack cap](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L576),
[chain scratch accounting](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L688),
[admission bounds](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L192)
and [Workspace batch sizing](../../../../../crates/layerfs-workspace/src/cow_tree.rs#L447).
Codec resource estimates must match the actual parameter sequence; changing
compression level/window to fit memory after a failure is not permitted.

#### Larger whole-file objects and packs

```text
T = 1 MiB, file = 900 KiB                 example override, not a new default
           |
       WHOLE_FILE
           |
     FULL or eligible PREFIX
           |
     selected record fits normal pack target?
               /                     \
              yes                    no
              |                       |
        ordinary placement       bounded singleton pack
```

The current compact-small grammar caps all packs at 256 KiB. It cannot express
the second branch for a larger incompressible record. Supporting larger cutoffs
requires an explicitly identified capacity-aware pack/record contract and a
reader for it; do not relabel old pack bytes or retry another format on error.
A singleton is the planned placement of the same selected encoding, not a
different logical object or a failed-compression escape path. Preserve old
supported format rules under their explicit compatibility profile.

Buffers may be sized from the accepted capacity or grow only within it; they
need not all allocate the format-wide maximum. The resolved plan must prove a
largest permitted FULL object can be constructed, stored and read within the
declared budgets. A target that cannot fit a normal multi-object batch can use
a planned singleton if it fits the total budget. If it cannot fit even that,
reject the configuration; do not silently enlarge memory limits. The supported
upper cutoff is therefore derived from format and resource capacity, not another
unexplained copy of the old 128-KiB branch constant.

The current 512-KiB canonical delta-chain bound may rule out a delta between
larger whole-file objects. This does not invalidate those objects as FULL.
Reject an ineligible prospective delta before expensive base reconstruction when
validated descriptor fields suffice; required reads must still authenticate their
actual representation. Do not automatically multiply chain budgets by T or depth.

#### Completion checks for configurable transparency

- Keep the 128-KiB default behavior and performance comparison unchanged.
- Qualify multiple non-default cutoffs, including an accepted value above
  128 KiB. A configuration option that can only repeat the old hard limit does
  not complete this requirement. 256 KiB and 1 MiB are useful capacity test
  targets, not proposed optimal/default values; exact support must be demonstrated.
- At each accepted T, test 0, 1, T-1, T and T+1, known-length and streaming input,
  compressible/incompressible FULL records, eligible/ineligible deltas, pack/batch
  boundaries and both size transitions. A large FULL must save/reopen/read when
  delta is ineligible. Validate defaults and reader/writer agreement for both
  configurable depth fields, including removal of fixed-depth rejection paths.
- Reopen using persisted policy and verify retained versions. Reject unsupported
  profiles/ranges and mismatched overrides before mutation; defaults from another
  process or host must not change the selected policy.
- Expose the resolved policy, supported bounds and limiting constraint in ordinary
  configuration inspection/errors; no new telemetry subsystem is needed. Resource
  and behavioral qualification of overrides is required, but does not authorize
  a campaign to search for the fastest cutoff or depth.

## 3. Current schema versus proposed descriptor

The current [schema-10 tables](../../../../../crates/layerfs-layerstack-store/sql/schema/v10.sql#L10)
contain:

```text
objects
  object_id (primary key)
  canonical_length
  pack_id, group_number, record_number

object_packs
  pack_id (primary key)
  data
```

Role comes from authenticated canonical framing. Pack headers select a physical
grammar; record tags and embedded base IDs distinguish FULL and PREFIX/DELTA.
See [native records](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L239)
and [SmallContent records](../../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L12).
Ordinary SQL over objects cannot currently enumerate the four payload cases.

Proposed content-storage tables:

The [save/persistence design](admission-and-persistence.md) selects four tables and
19 columns. It owns writer authority, bounded transactions, required indexes and
cleanup order; this document owns their meanings and configuration compatibility.
**As implemented, the schema has 20 columns and `user_version = 2`:** the review of
Stages 1–2 showed that bounded transactions release the write lock between
commits, so a failed save's early-committed packs stayed readable. The
`store_policy` row therefore also carries `retained_pack_ceiling`, the publication
watermark described below. `user_version = 1` is rejected, not migrated.

```text
store_policy                      one persisted policy per Store

objects                           one selected physical representation per ObjectId
  object_id ---------+            primary key
  object_role        |            canonical semantic role
  canonical_length   |
  base_object_id ----+            nullable self-reference for direct DELTA
  pack_id ------------------+
  group_number              |
  record_number             |
                            v
                       object_packs
                         pack_id
                         data

metadata_value_groups             first_ordinal, count, pack_id, group_number, digest
                                 catalogue only; value bytes remain in packs

object_inventory                  optional SQL view; no second copy of objects
  readable object_role + payload_encoding + base + location
```

These are the content-storage tables involved in this decision, not a list of
all Store tables. Namespace identity allocation and history retain their separately
reviewed responsibilities. A plain inspection query is sufficient initially;
object_inventory is optional presentation, not required persisted storage.

### Table ownership

| Owner | Tables / responsibility |
| --- | --- |
| Cluster 2: physical storage | store_policy, objects, object_packs and physical metadata_value_groups; optional inventory view |
| History/workflow, outside clusters 1 and 2 | layer_stacks, layers, branches, commits, and workspace_stages where workflow staging is retained |
| Namespace identity allocation contract | scope_allocator and its reservation effect; cluster 1 consumes supplied authorized identities, with exact allocation/persistence ownership settled by co-design |

LayerStack/Branch/logical Commit creation, parent relationships and head updates
are not cluster 2 APIs. History owns its SQL commands and schema families; the
lower-level connection/transaction facility may be shared to preserve required
atomicity. A SQL COMMIT completes database writes and does not itself create a
logical Commit. Storage-table initialization and object save/read must work
without creating any history entities or requiring their table families.

### store_policy

| Field | Meaning |
| --- | --- |
| id | Singleton key constrained to 1; create/open must additionally require the row |
| format_profile | Explicit supported canonical/physical capacities and decoding rules |
| small_file_threshold_bytes | Configurable Cluster 1 construction cutoff; default 131,072 bytes |
| whole_file_delta_max_depth | Configurable WHOLE_FILE dependency bound; default 8 links |
| chunk_delta_max_depth | Configurable CHUNK dependency bound; default 4 links |
| retained_pack_ceiling | Highest pack identifier belonging to a *completed* save; ordinary reads clamp to it and it is advanced only inside a save's final transaction |

Use typed fields, not an arbitrary key/value configuration system. Schema version
remains the SQLite schema identifier, separate from content policy.

`retained_pack_ceiling` is state, not configuration: it is the only persisted
value this slice mutates, it is not overridable at open, and it is not a second
durability claim. Opening requires `retained_pack_ceiling <= MAX(pack_id)`
(`Integrity` otherwise); a save acquires only when the watermark equals
`MAX(pack_id)`, and reports `UninspectedState` rather than treating another
attempt's live output as its own baseline. Cleanup deletes only rows above the
failing save's baseline, so a definite failure never moves the watermark. Validate the
profile and values before accepting work, load them at Store open, and pass the
policy explicitly. Do not query it once per object. Existing Store policy is
immutable in this first design; conflicting open-time overrides fail explicitly.
Hosts, containers and remote adapters receive the same policy.

The format profile fixes role-specific byte/work and memory ceilings and the
capacity-derivation rules. Resolve codec/frame/batch capacities from policy under
those ceilings; do not expose another knob for every internal derived number.
Validate all three configurable values against that profile on create/open.
Unsupported settings fail explicitly, without clamping or silent budget growth.
Writer and reader must agree on supported depths and sizes before larger values
can be accepted. A cutoff override controls new construction, not role decoding.
Persist policy and format identity as the source of truth; recompute deterministic
derived capacities at open rather than persisting a second, independently editable
set of frame and buffer constants.

### objects

| Field | Meaning and authority |
| --- | --- |
| object_id | Hash of the complete canonical object; unique CAS identity |
| object_role | Materialized semantic role derived from validated canonical construction; small stable code, readable label in the view |
| canonical_length | Complete canonical byte length, including framing; not compressed length or necessarily logical file size |
| base_object_id | Nullable direct physical delta-base reference, including supported metadata deltas; not a logical file/namespace reference |
| pack_id | Foreign key locating the pack |
| group_number, record_number | Record locator within that pack |

WHOLE_FILE/CHUNK encoding is derived from base_object_id:

```text
base_object_id IS NULL      -> FULL
base_object_id IS NOT NULL  -> DELTA
```

Do not also persist is_delta, is_full, is_chunk and is_whole booleans or four
separate combination tags. A readable payload_encoding field in the view derives
from the role and base reference. It is not FULL/DELTA classification for unrelated
roles or physical slice formats: those need their supported representation rules.

Materialize supported metadata direct-delta bases in the same base field; they
also constrain retention and cleanup. Payload depth settings do not control
metadata's separate program/limits. Pooled-value ordinals and physical-slice
owner/range dependencies remain explicit in their own supported descriptors.
The base column alone is not the complete reachability graph for all formats.

Do not add a second objects-to-storage table join or separate full-file, full-chunk
and delta tables. One CAS lookup returns identity fields, physical dependency and
location. Detailed logical file-to-chunk relationships remain in authenticated
file-state/extent objects, not duplicated into SQL file_chunks rows.

Use the existing validated producer/encoder results to populate the descriptor;
avoid serializing and then decoding merely to rediscover them. If the supported
pack grammar also carries role/base information, the row and record must agree.
On disagreement report corruption; never choose whichever copy can be decoded.
The final format must specify retained duplication explicitly. Moving the base
reference entirely out of packed records is not assumed by this proposal.

### object_packs

Keep pack payloads and their explicit format/group/codec framing. Pack format
selects the parser; it is not a substitute for the queryable semantic role or
payload encoding. A FULL object may be compressed. A delta may be encoded with
prefix compression in one codec call; the diagram does not require an extra
compression pass. Preserve the default profile's bounded pack behavior. Supporting
a future 1 MiB cutoff requires a separate pack-capacity/resource design.

Do not persist a per-object compressed-size claim when objects share compressed
groups. Exact pack/group allocation and per-record encoded cost are different
measurements and must remain labelled accordingly.

### Required indexes and statement sizing

The selected design adds UNIQUE objects(pack_id, group_number, record_number)
and a partial objects(base_object_id) index for non-NULL bases. The location index
serves cleanup order, pack-child lookup and locator uniqueness. The base index
serves dependency enforcement. Retain the existing metadata catalogue's unique
pack/group index; add no role index without a real query need.

Bulk object INSERT sizing now accounts for seven bindings per row rather than
five. Batch catalogue writes within actual SQL/parameter/byte limits. Exact role
codes, schema identifier and profile limits remain explicit format qualifications;
the canonical ObjectKind's broad tags are not sufficient semantic role codes.

## 4. Example inventory

IDs below are symbolic for readability; production ObjectIds remain full hashes.
Encoding is a view result, not a second stored flag.

| object_id | object_role | payload_encoding | base_object_id | locator |
| --- | --- | --- | --- | --- |
| W0 | WHOLE_FILE | FULL | NULL | pack 10, group 0, record 0 |
| W1 | WHOLE_FILE | DELTA | W0 | pack 11, group 0, record 0 |
| C0 | CHUNK | FULL | NULL | pack 12, group 0, record 0 |
| C1 | CHUNK | DELTA | C0 | pack 13, group 0, record 0 |

A file-state/extent tree can refer to C1 alongside other chunk IDs. Its logical
references describe file content. The C1 -> C0 reference describes physical
reconstruction; C0 need not be referenced by the current file's extent tree.
Multiple files or snapshots can share C1. The object row does not contain a path
or a single owning file ID.

The optional view supports an inspection query without scanning/decompressing
packed payloads. This is target-view SQL, not a command supported by schema 10:

```sql
SELECT object_role, payload_encoding, COUNT(*) AS object_count
FROM object_inventory
WHERE object_role IN ('WHOLE_FILE', 'CHUNK')
GROUP BY object_role, payload_encoding;
```

This counts currently retained physical objects. It does not measure per-operation
delta selection, exact reuse frequency, compression ratio or read speed. Use
admission outcomes and scoped telemetry for operation claims.

## 5. Database and integrity rules

- The primary key permits one selected representation per ObjectId. Changing
  physical representation later requires an explicit rewrite contract; it is
  not a side effect of reading, changing policy or opening a Store.
- Base IDs have the normal hash-length check and immediate NO ACTION self-FK. They
  cannot equal the target ID. A required base cannot be deleted while referenced;
  no cascade from base deletion to dependent content is allowed.
- A foreign key alone cannot prove role compatibility, acyclicity or work bounds.
  Admission verifies these against authenticated base data and chain measurements; new physical bases
  must already be admitted before dependents. Readers enforce the supported
  profile, cycle/depth/size bounds and canonical authentication.
- Dependency retention includes physical bases, even when they are absent from
  current logical file/namespace references. Any future reclamation must trace
  both kinds of references; this proposal does not add a reclamation subsystem.
- Insert the selected descriptor and valid pack location under the same admission
  ownership/transaction guarantee. An aborted attempt must not expose a row without its
  record or a delta without its base. Retain existing publication semantics.
- Unknown role/format codes and contradictory row/record metadata are errors.
  Do not infer the role from size, filename, current cutoff or failure to decode
  another format.

### Self-reference and failed-attempt cleanup

The existing [failed-admission cleanup loop](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2529)
deletes early-committed owned rows in ObjectId order, which is unsuitable for the
new self-FK. The [selected cleanup contract](admission-and-persistence.md#7-failure-ownership-and-bounded-cleanup)
requires earlier base locators for new writes, then pages owned objects in reverse
physical order through the location index. Use immediate NO ACTION for one bounded
DELETE per dependency-closed page; an IN list does not control row deletion order.
After all owned objects are removed, delete bounded owned pack pages and cascade
only their catalogue entries. Never cascade-delete canonical dependents, disable
constraints or load an operation-sized graph. Unknown ownership/outcome forbids cleanup.

```text
retained P0 <- unpublished U1 <- unpublished U2
failed attempt owns U1/U2: delete U2, then U1; keep P0
remove only unreferenced attempt-owned packs
```

After authoritative publication, keep the version even if its reply is lost.
Return lost acknowledgement as failure with unknown persistence outcome. Do not
retry, delete or republish. A separately requested authoritative inspection must
establish the outcome before classifying data as unpublished. This requires no
version rollback API, undo log or new rollback subsystem. The
[single-attempt contract](physical-encoding-and-packing.md#one-attempt-no-retries)
also forbids SQLite busy handlers, SDK retries and stale-state reprepare loops.

The base and location indexes are required parts of the selected cleanup design.
Qualify their query plans, page/write cost, bounded cleanup and failure behavior
together with the self-FK; indexes do not establish exclusive attempt ownership.

## 6. Cost and qualification

The proposed role and base fields increase the object index's size and write
work; a base reference costs hash bytes on delta rows even when compressed data
is tiny. Queryability is a concrete benefit, not a claim of free metadata.
Start with compact role codes and the required location/dependency indexes;
do not add inventory/role indexes without an actual query need. A WITHOUT ROWID
secondary index also carries primary-key information. Foreign-key enforcement,
larger insertion parameter counts and dependent-first cleanup must meet the same
latency/resource gates. The optional inventory view holds no copied records.

Chain depth and reconstructed-byte totals are derived values. Do not add persisted
summary columns solely to display them. If profiling shows validated summaries
are necessary to avoid costly repeated traversal, co-design their consistency
and measured storage cost before adding them. No unbounded recursive inventory
query belongs on the save/read critical path.

Before implementing the schema, settle the complete role-to-physical-format map,
exact column constraints, supported old-Store opening/conversion contract and
policy capacities. Qualify all four payload cases, reuse, missing/corrupt/cyclic
bases, row/record mismatch, atomic failure paths and unchanged canonical output
under matching profiles. Compare database bytes, SQL/page work, save/read/Commit
latency and scoped memory against the declared control. No benchmark or product
change was performed for this proposal.
