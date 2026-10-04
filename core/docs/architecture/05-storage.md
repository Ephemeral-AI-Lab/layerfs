# Storage (C2, `layerfs-storage`)

> **Status:** Research; informative and not a product contract.

The pending issue #192 schema 7 changes are described in
[save ownership and publication](15-multi-writer-storage.md), based on
`0819f3f39833d477d9ed6d878a50691c3c046a83`. That description supersedes the
older exclusive-save, prefix-publication, shared-private-cache and cleanup rules
below, and adds the returned-read byte bound and bounded ordinal window. The
older source-pinned sections remain historical descriptions; they do not qualify
the pending implementation or a performance change.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

---

## 6. Storage (C2 — `layerfs-storage`)

### #302 two-port ordinary writes (2026-10-03)

Described against parent `366046097` and the step 4a implementation. `Storage`
shares a fixed candidate index across saves; each handle admits one producer,
while separate handles may write concurrently through the ports. `Save` holds
the existing 512-object/4 MiB−1 pending batch, bounded lane groups, immutable
wave packs and the existing codec/decode workspaces. It is also C1's authenticated
same-operation provider; `SaveSink` is its consumer adapter and preserves the
original typed error for `finish`.

Wave membership includes offered ids, direct references and declared physical
candidates. Known dependency chains are prefetched by level. Exact reuse checks
role, length and reconstructed bytes. The existing selector, group builder and
pack assembler supply the physical representation; queue flushes keep the old
non-pooled placement boundaries. A registered object is immediately valid for
other readers, so each registration is closed under direct references. Pending
lanes referenced by ready members are sealed to achieve that closure. Payload
PUT acknowledgements precede registration; a failed or uncertain operation is
terminal and causes neither resend nor guessed cleanup. A later failed save can
leave previously acknowledged independent batches and unreferenced payloads.

A first-wins physical base must be acknowledged before a dependent's PREFIX
selection: its winning chain may differ in depth from this producer's private
copy. Candidate-triggered seals therefore register their closed prerequisites
before selection reads the winner. This can add registrations within a wave;
operation counts expose the cost. Returned lost ids are batch-located and
compared through authenticated canonical reconstruction. A race clears the
producer's derived depth cache.

The old path still checks allocation chronology. The port path checks explicit
logical cycles, because a winning base's pack id can exceed its dependent's
reserved id. Both paths retain the existing role/depth/work/identity checks.
No canonical identity, CDC, deduplication, record grammar or pack framing changes.
Pooled write admission is described below; the old API remains available to the
server and SDK.

### #302 pooled writes and namespace parity (2026-10-03)

Described against parent `10954a167` and the step 4b implementation. `Save`
admits canonical inode leaves using the existing pooled value encoding and
COPY/INSERT selector. The handle shares a bounded `PoolIndex`; the operation
synchronizes its catalogue/window before first use. Each leaf assigns unknown
values in encounter order with a leaf-sized memo. The first four ordinal blocks
are exact; later blocks cover sixteen leaves of the current fresh-value width.
A completed save conditionally releases its unused final allocator tail, while a
failed operation neither retries nor guesses that an unacknowledged tail is free.

The pooled tail retains at most one bounded pack, with the existing group grammar,
compression rule, fit predicate and directory width. Same-operation group reads
can assemble that private tail; appends release pack caches while decoded values
remain immutable. Full packs, finish and ordinal-reference closure seal it.
Catalogue rows and complete metadata pack bodies enter atomic registration before
or with the leaves that use them. Window advancement accompanies its group rows;
the metadata provider applies the existing value-count recurrence atomically.
The shared index remains bounded even while other handles allocate ordinals.

`WriteOutcome.pool` and `Save::pool_counters` expose real assignment and selection
counts. Port diagnostics record acknowledged metadata bytes, pooled packs/groups,
reserved pooled directory bytes and ordinal reservation calls. These are counts,
not performance acceptance. The private pooled tail's population can be closed by
D12; counts expose that placement cost. Paired single-leaf, reopened-chain and
100/1,000-entry filesystem fixtures compare exact canonical objects, roots and
complete sealed bytes at their stated inputs.

An absent ordinal is negatively cached within the current demand, preventing a
second catalogue request for the same absence. The next demand clears those
negative facts. A maximum-depth chain is accepted when prefetch reaches an empty
frontier on its final permitted iteration; the resolver retains every existing
role, depth, work and identity bound. No record or pack framing changes.

### #302 two-port authenticated reader (2026-10-03)

Described against parent `6db678f3a`. `Storage` reads its persisted policy once
and holds bounded locator/catalogue indexes. `Reader` owns the existing decode
arena, ordinary pack/group caches and pooled reader. Dependency locators are
prefetched by chain level; metadata pack demands are batched up to 4 MiB and
payload bodies are fetched whole. SHA-256 covers every sealed pack and the
existing resolver checks each reconstructed BLAKE3 object identity, role and
chain budgets, with source-specific cycle/chronology validation described above. Routing is derived from the pack lane and checked against
the returned descriptor. There is no service bootstrap, retry or alternate route.

`ObjectStore` is opaque immutable-body I/O; `MetadataStore` has policy, locate,
read_packs, value_groups, signatures, reserve and register units. They own engine
I/O and return definite or uncertain outcomes; uncertainty maps to the existing
`UnknownOutcome` class. Typed port originals use the existing `Io` error carrier
during coexistence because the server exhaustively matches the old enum. Creating
the carrier performs no I/O. The old provider error mapping is shared from `error.rs`
by both readers. Source cache-hit notifications count actual consults in the
unchanged encoding caches; they do not select another algorithm. The already
locked SHA-256 dependency is added directly to C2 without a package/version move.
No write-path or engine implementation is claimed by this slice.

### #302 encoding access seam (2026-10-03)

This change is described against parent `15155598b`. `location.rs` owns engine-independent object, pooled catalogue and
signature rows. `encoding/` now consumes `source::Source`; `sqlite/source.rs`
implements the seam for the legacy connection. It preserves the old publication
ceiling, streamed catalogue traversal, bounded signature ring and caller-owned
transactions. The old mutex helper relocates into the seam and is re-exported
by the SQLite owner. These are relocations with adaptation during coexistence;
canonical identities, encoding, packs, cache bounds and authentication are
unchanged. This seam is not yet the Phase 7 two-port reader or writer.


### #286 grouped whole-file dependency work (2026-09-30)

This update describes the change against parent `8baf47e45` in the same
implementation commit. Version17 whole-file groups contain independently framed
records. `encoding::delta::read::record_width` charges the selected raw record,
using the same validated `record_number` boundaries as canonical reconstruction;
unrelated sibling records no longer consume that object's dependency budget.
Compressed ordinary groups still charge their full decoded body. The256KiB
encoded-work ceiling, canonical/depth bounds, pack/group validation, chronology,
visibility and final identity authentication stay enforced. No format, policy,
cache size, worker count or write behavior changes.

### #190 group compression profile (2026-09-20)

This addition describes the working tree based on
`605f6efc6a095a1b6335cbc5549dc0fda78ed9ab`; older sections retain their pins.
`encoding::codec::GROUP_LEVEL` is 1 for ordinary and pooled value-group bodies.
Payload records remain at level 3. The group window cap, content-size and
checksum fields, dictionary policy, decode validation and canonical identities
are unchanged. This is an encoding-effort choice within the existing format,
not a schema or canonical-grammar migration.

The caller-owned encode arena remains 16 MiB; this change does not resize the
workspace, alter cache or transaction bounds, add workers, or expose a new
configuration knob. Physical group/pack sizes and byte-based transaction cadence
may change. Existing pooled physical-group reuse and catalogue statement reuse
remain in place. Diagnostic time/space evidence and qualifications belong to
[ledger L40](../../../docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md#l40--190-group-compression-level-1-live-pair-2026-09-20),
not this architecture description.


### #190 pooled physical-group reads (2026-09-20)

This addition describes the working tree based on merged parent-batching commit
`9d82685f39460fabec6c810184d87adcccd53dc5`; the older sections retain their source
pin. No on-disk format, writer behavior, resource limit or release claim changes.

A Store read's `Resolver` borrows the existing `ReadSession` decoded `GroupCache`
when reconstructing pooled inode leaves. Physical leaf groups live in Ordinary
packs. Their compressed bodies are now decoded once while retained, then reused
across both chain passes and subsequent requests. The cache remains bounded by
`DECODED_GROUP_CACHE_BYTES` (512 KiB) and clears wholesale on overflow; no second
physical-group cache or broader canonical-object cache is introduced.

Every extraction still fetches its current bounded pack view and checks lane,
group boundaries, decoded length, record framing and record length. The cached
entry refuses roots above the current publication ceiling before consulting the
cache; dependency locations remain constrained by that same ceiling. Final
canonical identity authentication and per-chain canonical/encoded work charges
remain unchanged.

`PoolReader` itself remains fresh per requested leaf: its value-group cache,
pack cache and decoded-work accounting retain their previous lifetime and bounds.
Public `leaf_body`, `leaf_canonical` and `stored_base` still take the uncached
physical-group route, so writer-owned readers and pack invalidation are unchanged.
Only successful Zstandard decompressions avoided by the borrowed cache count as
`physical_group_cache_hits`; raw groups do not produce such hits.

### #190 catalogue statement reuse (2026-09-20)

This addition describes the working tree based on `81f4f1fef`, after the pooled
physical-group reuse change. `sqlite::pool::group_for` now borrows its fixed
catalogue lookup statement through the connection's existing `prepare_cached`
mechanism. The SQL and parameters are unchanged; every call still executes the
query and validates the returned row and ordinal coverage. Query results and
value groups are not cached by this change. The pinned rusqlite default bound of
16 prepared statements remains unchanged, and dropped statements release their
bindings before returning to that cache. Missing rows, invalid ordinals, damaged
rows and engine errors retain their existing handling. Pack BLOB acquisition and
all cache ownership, resource policies and formats are unchanged.

### #190 pooled leaf ordinal-ordered resolution (2026-09-20)

This addition describes the working tree based on the catalogue statement-reuse
change above. It changes the **order** in which one pooled leaf's rows are
resolved, not the statement, the grammar, the caches or any bound.

A pooled leaf's rows are ordered by serial, and their ordinals are scattered
across the value-group catalogue. The covering-group memo in
`PoolReader::leaf_canonical_with_groups` therefore only helped when two
consecutive serial-ordered rows happened to share a group, and a leaf issued
several catalogue statements per distinct group it touched. The rows are now
visited in ascending ordinal order, with each resolved value written back at its
own row's index. Ordinals inside one group are consecutive, so the memo answers
one `sqlite::pool::group_for` statement per distinct covering group.

`group_for`, its `prepare_cached` statement, its ordinal-coverage validation and
its error handling are unchanged. The decoded-value cache, the per-chain decoded
work charge, the pack cache, the visibility ceiling, the leaf's rebuilt canonical
bytes and the `values` slice handed to `rebuild_leaf` are all unchanged; only the
visit order and the index at which each value is stored differ. The sort is over
at most `MAXIMUM_LEAF_ROWS` positions. No format, schema, cache size, limit or
public API changes.

### 6.1 The Store handle

`core/crates/layerfs-storage/src/cas/store.rs`

```text
   Store { path       : PathBuf,
           policy     : StoragePolicy,       ← the PERSISTED policy row
           capacities : StorageCapacities,   ← derived, checked arithmetic
           pool_index : Arc<Mutex<PoolIndex>> }  ← disposable derivation
```

- `Store::create` creates a **fresh** Store; an existing Store is **never
  migrated**. An unsupported policy is rejected **before the database file
  exists**.
- `Store::open` validates the schema identity, the four table shapes, the two
  indexes and the persisted policy **before any object work**.
- `begin_save` acquires exclusive write ownership **once**.
- Reads are independent bounded waves that capture their retained-pack ceiling
  **once**.

### 6.2 The four tables

`core/crates/layerfs-storage/sql/schema.sql` — 21 declared columns.

```sql
PRAGMA application_id = 1279677261;
PRAGMA user_version   = 4;

store_policy            ( id = 1, format_profile, small_file_threshold_bytes,
                          whole_file_delta_max_depth, chunk_delta_max_depth,
                          metadata_delta_max_depth, retained_pack_ceiling )
object_packs            ( pack_id PK, data BLOB )
metadata_value_groups   ( first_ordinal PK, count, pack_id, group_number, digest )
objects                 ( object_id PK, object_role, canonical_length,
                          base_object_id, pack_id, group_number, record_number )

UNIQUE INDEX objects_locations ON objects(pack_id, group_number, record_number);
INDEX        objects_bases     ON objects(base_object_id) WHERE base_object_id IS NOT NULL;
```

Every table is `STRICT`. The `objects` table is `WITHOUT ROWID`, which suits a
32-byte primary key.

Notable column constraints:

| Column | Constraint |
| --- | --- |
| `store_policy.format_profile` | `= 1` |
| `small_file_threshold_bytes` | `BETWEEN 131072 AND 1048576` |
| three depth columns | each `BETWEEN 0 AND 50` |
| `retained_pack_ceiling` | `>= 0` |
| `objects.object_role` | `BETWEEN 1 AND 13` |
| `objects.canonical_length` | `> 0 AND <= 16777216` |
| `objects.base_object_id` | `NULL` or (32 bytes **and** `<> object_id`) — a self-base is impossible |
| `objects.group_number` | `>= 0 AND < 256` |
| `objects.record_number` | `>= 0 AND < 8191` |
| `metadata_value_groups.count` | `BETWEEN 1 AND 165` |

`base_object_id` is a self-referencing foreign key, so a prefix record's physical
base is a declared dependency of the store, and `objects_bases` is a partial index
over exactly the rows that have one.

### 6.3 What `validate` refuses

`core/crates/layerfs-storage/src/sqlite/schema.rs`. Validation is strict in ways
that matter:

1. Schema identity — `application_id` and `user_version` must match **exactly**.
   "A schema that merely resembles another product's version is rejected."
2. All four table **column shapes**, in declaration order.
3. Both required indexes present.
4. **Any unexpected table** → `Integrity("unexpected table in Store")`. The check
   enumerates the four expected names and refuses anything else that is not a
   `sqlite_%` internal.
5. The literal constraint text `CHECK (object_role BETWEEN 1 AND 13)` must be
   **present in the DDL**.
6. The table DDL must contain `STRICT`.
7. Invariant **I1**: `retained_pack_ceiling <= highest_pack_id`.

Points 5 and 7 carry source comments explaining why they exist, and both are good
examples of a validation placed where the caller can see the failure:

> The role ceiling is the one that changes without changing the column shape, so a
> Store written by an older same-version build would otherwise **open** and then
> **refuse the first tree-role INSERT**. Requiring the text at open makes the
> refusal happen where the caller can see it: the Store is refused rather than
> accepted and failed later.

```text
   I1:  watermark  <=  highest pack id
        └── Store::open refuses "publication watermark is ahead of storage"
            ("A Store whose watermark exceeds its highest pack id has been
              corrupted or written out of band.")
```

### 6.4 The publication watermark — visibility without WAL

This is C2's answer to "how does a reader see committed data without WAL", and the
mechanism is a single integer that only moves in the last transaction.

```text
   begin_save()                       ── exclusive write ownership, acquired ONCE
        │
        │  accept(FinalizedObject) × N         bounded batches:
        │       BATCH_OBJECT_LIMIT               =    512 objects
        │       BATCH_CANONICAL_BYTES_LIMIT      =  512 KiB
        │       TRANSACTION_ROW_LIMIT            =  8_191 rows
        │       TRANSACTION_CANONICAL_BYTES_LIMIT = 4 MiB − 1
        │
        │  early transactions MAY commit ──── but the packs they wrote stay
        │                                       INVISIBLE, because the watermark
        │                                       has not advanced
        ▼
   finish()                           ── final transaction advances
        │                                 store_policy.retained_pack_ceiling
        ▼
   SaveOutcome { reused, inserted, packs_created, pack_appends, commits,
                 statements, full_records, prefix_records, delta, chain, pool }
```

> A pack is visible to an ordinary reader **exactly when** the save that created it
> was acknowledged.

**Holding a `SaveOutcome` at all *is* the acknowledgement.** `finish` returns one
only after the watermark transaction committed, and every other outcome is a typed
error. The source records a deliberate removal here:

> There is deliberately no boolean field for it — **a field that can only ever hold
> one value cannot fail an assertion**, and one used to sit here doing exactly that.

That is a small thing and a good one: an `acknowledged: bool` that is always `true`
is not evidence, and its presence would invite a test that appears to check
durability while checking nothing.

Counters in `SaveOutcome` describe work actually done: `reused` (exact existing row
served the occurrence) vs `inserted` (newly written), `full_records` vs
`prefix_records`, `statements` (the `INSERT` statements issued for object rows —
statements, not rows; added at #178 **V5**), plus `DeltaCounters` for selection
outcomes and `ChainCounters` for base acquisition.

### 6.5 The persistence profile

| Setting | Value |
| --- | --- |
| journal mode | `MEMORY` |
| `synchronous` | `OFF` |
| temporary storage | memory |
| busy timeout | zero |
| `COMMIT` | **still required** |
| WAL | **absent** |
| `fsync` / `fdatasync` / `sync_all` / `sync_data` | **nowhere in the product path** |
| retries / resends / silent route changes | **none — one attempt per operation** |

Unknown acknowledgement is a failure. There is no resend, no poll and no
destructive guess.

### 6.6 `unsafe`: one audited module

C1 and telemetry are `#![forbid(unsafe_code)]`. C2 **cannot** be:

```rust
#![deny(unsafe_code)]        // crate-wide
// encoding::codec is the single audited exception
```

The reason is mechanical and stated in the source: the pinned zstd codec needs the
C FFI, and **a lint that is `forbid`ed cannot be allowed back on for one module**
(E0453). So `unsafe` is denied crate-wide and allowed on exactly one module,
`encoding::codec`, whose documentation carries the FFI inventory.
`core/tools/check_product_boundary.py` rejects `unsafe` anywhere else in this
crate — the deviation is enforced, not merely documented.

### 6.7 The five pack framings

`core/crates/layerfs-storage/src/pack/layout.rs`

> A lane is a **framing**, not a worker or a second store, and existing locators
> stay stable when a pack's directory grows.

```text
   LFPACK\0\0 │ control area (16 B) │ directory │ group bodies
   └── PACK_MAGIC ──┘
```

| Version | Lane | Carries | Directory entry |
| ---: | --- | --- | ---: |
| 1 | `Ordinary` | mapping nodes, file states, tree pages, symlinks | 16 B |
| 2 | `Native` | chunk payload records (one per group entry) | 16 B |
| 4 | `WholeFile` | compact whole-file records | **4 B** (starts only) |
| 6 | `PooledMetadata` | pooled physical-metadata value groups | — |
| 7 | `Singleton` | one oversized record alone in its pack | — |

**Versions 3 and 5 belong to other profiles and are rejected explicitly. No reader
trial-decodes them.**

`PackLane::for_role` maps logical role → lane deterministically:

```text
   WholeFile                            ──► WholeFile
   Chunk                                ──► Native
   ExtentLeaf  ExtentBranch  FileState  ┐
   InodeLeaf   DirectoryLeaf  ...       ├─► Ordinary
   FilesystemRoot  Attribute*  Symlink  ┘
```

Lane limits:

| Limit | Ordinary / Native / WholeFile | PooledMetadata | Singleton |
| --- | ---: | ---: | ---: |
| pack limit | 256 KiB (`PACK_LIMIT`) | 256 KiB | 16 MiB + 4 KiB |
| group body limit | 65,536 (`GROUP_LIMIT`) | 16 KiB | singleton limit |
| group count | 256 | 256 | 1 |

`GROUP_TARGET = 48 KiB` is compared against the group's own **framed** identity,
not against a sum of per-record framed lengths — the source notes the latter
"would count the shared count and end offsets once per record".

For the whole-file lane specifically, `WHOLE_FILE_COMPACT_DROP = 8`: the record
itself is stored and its framing is dropped at assembly time.

### 6.8 Physical representation selection — policy versus failure

`core/crates/layerfs-storage/src/encoding/delta/select.rs`

> A missing object starts from a prepared compressed FULL alternative. A PREFIX
> trial happens only when exactly one eligible candidate is acquired under the
> role's depth, work and memory policy, and the selection compares the **complete
> framed record cost** including the base identity.

```text
   object to store
        │
        ▼
   1. prepare a compressed FULL alternative          ← ALWAYS, first
        │
        ▼
   2. candidate acquisition
        │
        ├── candidate supplied (advisory predecessor) or found in the session cache
        │
        ▼
   3. eligible under the role's depth / work / memory policy?
        │
        ├── NO  ──► FULL       ┐
        ├── absent ──► FULL    ├── POLICY DECISION
        └── >1 eligible ──► FULL ┘   (a trial happens only for EXACTLY ONE)
        │
        ▼
   4. PREFIX trial  ──►  compare COMPLETE framed record cost
        │
        ├── prefix cheaper  ──► PREFIX record, base_object_id recorded
        └── else            ──► FULL
        │
        └── codec / allocation / READ FAILURE
                 ──► returned as a FAILURE, never an alternative representation
```

That last branch is the entire no-fallback rule made concrete, and the distinction
is the one to probe in qualification:

| Situation | Outcome | Class |
| --- | --- | --- |
| no candidate supplied or found | FULL | **policy decision** |
| candidate absent from storage | FULL | **policy decision** |
| candidate present but ineligible (depth/role) | FULL | **policy decision** |
| chain would exceed its budget | FULL | **policy decision** |
| codec, allocation or read failure | **error returned** | **failure** |

`DeltaCounters` distinguishes all of them: `prepared_full`, `trials`,
`prefix_selected`, `full_losses`, `no_candidate`, `absent_candidates`,
`ineligible_candidates`, `work_exceeded`. A receipt can therefore show *why* FULL
was chosen rather than only that it was.

**Chain bounds are separate from depths**, and the source says so explicitly: "a
deeper chain is still bounded by the bytes it may decode and encode."

| Bound | Value |
| --- | ---: |
| `CHAIN_CANONICAL_LIMIT` | 512 KiB |
| `CHAIN_ENCODED_LIMIT` | 256 KiB |
| `DEPENDENCY_PACK_CACHE_BYTES` | 4 MiB |
| `METADATA_DECODED_WORK_LIMIT` | 32 MiB |
| `METADATA_CHAIN_CANONICAL_LIMIT` | 8 × 8,192 |
| `METADATA_CHAIN_ENCODED_LIMIT` | 17 × 8,193 |

The per-save `DepthCache` holds at most `DEPTH_CACHE_ENTRIES = 4_096` live
`ChainCost { depth, canonical }` entries. Its purpose is prospective: it records
"exactly what a read of that object charges its chain budget: the object itself and
every dependency, **so a producer can refuse to create a dependency a later read
could not reconstruct**."

### 6.9 Pooled metadata — the inode-value lane

`core/crates/layerfs-content/src/object/inode_leaf.rs` defines the grammar; C2
decides whether such a leaf is stored pooled or whole. Role 5 in the logical model
carries that meaning: "The role carries logical structure only; C2 owns whether
such a leaf is stored pooled or whole."

```text
   logical inode leaf row    (81 B)  = serial (8 B) + inode value (73 B)
   the leaf page's recorded subtree-bytes field = n × 81  ← ENCODED ROW WIDTH

   pooled physical form:
      leaf row (12 B) = serial (8 B) + ordinal u32        ← POOLED_ROW_BYTES
      value object    = "LFSIVL1\0" (8 B) + value (73 B) = 81 B value
                        wrapped in the LFSO envelope      ← 94 B canonical
                                                    POOLED_VALUE_CANONICAL_BYTES
                                                     (= 9 + 4 + 81)
```

The value is therefore stored **once** and referenced by ordinal, which is what
makes repeated inode values (a directory with many empty regular files, say) cost
12 bytes instead of 94 at each occurrence.

`PoolIndex` (`encoding/pool/index.rs`) is the bounded candidate set:

```text
   BTreeSet<(fingerprint : i64, ordinal : u32)>
     • bounded by METADATA_INDEX_VALUES = 131_072 entries
     • RESETS WHOLESALE when a group would exceed that bound
     • the fingerprint is only a CANDIDATE FILTER;
       full value bytes decide equality
```

The consequence is stated precisely, and it is the right one:

> It retains at most 131,072 entries and resets wholesale when a group would exceed
> that, which reproduces the reference window exactly: **eviction causes a duplicate
> physical value, never a lost one.** A failure invalidates the whole set so a
> partly advanced index can never be reused.

The catalogue is **authoritative** and the index is **disposable derivation**, so a
failed save invalidates it **whole** — "invalidation is a reset, never a repair",
and a reopened Store re-synchronizes it from the catalogue on first use.

`METADATA_INDEX_VALUES` carries a note that a 32 MiB constant once sat beside it
with no reader, "overstat[ing] the real bound by an order of magnitude" — another
instance of the one-owner-one-figure discipline.

### 6.10 Record reconstruction

`core/crates/layerfs-storage/src/encoding/decode.rs`

> Every intermediate is authenticated. The pack header and group directory are
> validated before a body is touched, a compressed body is checked against its
> declared decoded length, the record grammar is validated before use, and the
> rebuilt canonical object **must match the length recorded for its locator**. A
> corrupt record is an error and **never selects another decoder**. A PREFIX record
> is decoded only against the exact base payload the resolver reconstructed and
> authenticated; **there is no trial decode and no fallback.**

The identity check is **one hash per resolved record** (#178 **P2-6**, 2026-09-18).
The resolver computes `ObjectId::for_bytes` over each decoded record to
authenticate it against the locator that named it and **returns the identity it
authenticated** with the bytes; a wave then compares identities instead of hashing
the same bytes a second time. Before the change the requested object of every wave
was hashed twice — once by the resolver, once by `cas/read.rs` — which the register
recorded as RT-08. The check itself is unchanged in kind and in strength: a record
whose bytes hash to another identity is still refused with an integrity error, and
a case that rewrites a locator to claim a different identity is what pins it.

```text
   decode_canonical(pack, location, capacities, base, workspace)
        │
        ├── canonical_length in (0, CANONICAL_LIMIT]           ← before anything else
        ├── parse_header(pack)                                 ← header validated
        ├── group_view(pack, header, location.group_number)     ← directory validated
        ├── record_range(...)                                  ← record grammar validated
        │
        ▼
   tag determines the route:
        FULL   ──► decompress / copy
        PREFIX ──► requires `base = Some(...)`, the AUTHENTICATED base payload
        │
        ▼
   rebuilt canonical object must match location.canonical_length
```

The base's presence must agree with the record's own tag. A prefix record given no
base, or a full record given one, is an error rather than a guess.

`FULL` records may be compressed. `encoding/full.rs` states the split: "chunk and
whole-file payloads carry a prefix-capable frame, while **mapping pages are stored
as exact canonical bytes** inside an ordinary group." Nothing there selects a
representation and nothing retries — a failed codec call is returned as a failure.

A whole-file record whose planned compact form cannot fit a normal pack is planned
into the **singleton lane** instead, with the same frame bytes and the same
FULL/PREFIX choice. That is a placement decision, not a representation change.

### 6.11 Read waves

```text
   Store::read_canonical_batch(ids)
        │
        ├── READ_OBJECT_LIMIT = 4_096          ← one wave's declared ceiling
        │                                         (C1 filesystem side declares
        │                                          the same figure: 4_096)
        ▼
   grouped SQL lookup  ──►  locators
        │
        ▼
   per-locator reconstruction, bounded by:
        • the wave's captured retained-pack ceiling
        • the dependency pack cache (4 MiB, released wholesale)
        • chain canonical / encoded limits
        ▼
   values returned IN DEMAND ORDER with EXACT CARDINALITY
```

`StoreReadCounters` reports `objects`, `packs_read`, `pages`, `ceiling`, `edges`,
`max_depth`, `canonical_bytes`, `group_decodes`, `opens` — including the **ceiling
applied to every acquired location**, so a receipt can show which visibility
watermark a read actually observed, the ordinary-lane group bodies the wave
decompressed (added at #178 **V6**), and the connections the wave opened.

**Two lifetimes, one wave.** `Store::read_batch` is the wave: it opens its own
connection, captures the ceiling, decodes and closes. `StoreProvider` — the bridge
C1 reads through — is an **operation**: its first wave opens a `ReadSession` (one
connection with the declared profile plus one decode arena) and every later wave of
that operation reuses both, so an operation's connection cost is `O(1)` rather than
`O(waves)`. The session lives behind a `RefCell` in the provider, never in the
`Store`: the Store is shared and `Sync`, while a session is one operation's private
state, and the provider is therefore `!Sync` by construction.

The wave's declared demand bound is enforced on both doors: `read_objects`'s own
`check_read_demand` runs in the Store's entry point and in `ReadSession::read`, and
the provider checks it **before** the session exists, so a demand over
`READ_OBJECT_LIMIT` is refused without opening anything.

A session also carries one `GroupCache` (#178 **P2-4**, 2026-09-18): decoded
ordinary-lane group bodies keyed by `(pack, group)`, bounded by
`DECODED_GROUP_CACHE_BYTES` (512 KiB, released wholesale when the bound is crossed)
and dropped with the session. It is what makes `k` records of one group cost one
decompression across an operation instead of one per record, and a hit charges no
`group_decodes`. It is never a visibility shortcut: the ceiling is re-read per wave
while the cache outlives a wave, so the resolver checks the location's pack against
its own ceiling **before** the cache is consulted.

What a session deliberately does **not** pool is the ceiling. It is the publication
watermark, so it is re-read for every wave and a save that completed between two
waves is visible to the second one; pooling it would turn an operation's later
waves into a snapshot of its first. `opens` reports `1` on the wave that opened the
session and `0` on the waves that reused it.

### #286 measured-history physical-density profile (2026-09-30)

The r008–r036 Phase B source selected **payload level 9** and **ordinary/pooled group level 19**, retaining the same pinned windows, checksums, no-worker rule, 16 MiB caller-owned encode arena, maximum frame/group/pack sizes and read compatibility. This changed encoded bytes and CPU work, not canonical content identities or the persistent pack grammar. The source-pinned #188d [codec sweep](../../../docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/W1-codec.md) found approximately 3.88 MiB of physical saving for levels3/1→9/19 on the original stride10 population at +7.589 CPU seconds; these are prior diagnostic estimates, not #286 candidate results. The current source restores **payload level 3 and group level 1** after r035's encoder counters showed the extra save cost; existing level-9/19 frames remain readable. A save still removes only its pooled pack's unused append reservation inside `storage.finish`, before publication and inside the timer; no post-sample VACUUM or offline shrinkage is credited.

### #286 tight whole-file pack directory (2026-09-30)

The new writer uses **whole-file pack framing version 18** with sixteen reserved four-byte group starts (64 bytes), while version17 and older packs remain readable with their original256-slot body offset. The grouped record grammar, per-record identity/dependency checks, total pack size and bounded reader remain unchanged. The source-pinned stride1 r012 Store had1,002 whole-file packs and no pack with more than eight groups; a sixteen-slot writer therefore removes960 bytes of unused *directory reservation* per pack for that observed population without splitting one of its existing packs. That 961,920-byte arithmetic is a prospective shape diagnostic, **not** an allocated-byte PASS or a claim about every workload. The next at-run receipt decides the strict C2+C5 `st_blocks*512` gate. The version18 parser and source-aware accounting understand both offsets and fail closed on a group count beyond the version's slots.

### #286 tight ordinary/native pack directories (2026-09-30)

This document's current source also writes ordinary version19 and native version20 with sixteen reserved sixteen-byte entries (256 bytes); old ordinary version9 and native version15 keep their 256-slot (4,096-byte) body offset on read. The same `parse_header` and group-directory continuity checks use each pack's version, and a new pack claiming17 groups is refused. The bounded ordinary/native group grammars, stored-payload tag handling, maximum pack/body sizes and public object identities remain unchanged. The source-pinned r013 Store had275 ordinary packs with at most four groups and237 native packs with at most six. The resulting1,966,080-byte directory reduction is prospective arithmetic, while only a new at-run exclusive C2+C5 block reading can pass the unchanged strict storage gate.

### #286 one-insert closed packs (2026-09-30)

This document's current source binds a newly created, already closed pack of at most256 KiB as one complete BLOB INSERT. Packs that may receive later groups and larger singleton packs keep the previous `zeroblob` plus incremental-BLOB writer. The r014 original Store held1,514 eligible version18/19/20 packs; their former insertion plus three BLOB writes is a count-driven explanation for repeated SQLite page dirties, not a measured space saving. The assembled pack grammar, bounded capacity, public identity and read path are unchanged. The next sealed at-run receipt, including this write work within the timer, alone decides actual exclusive C2+C5 allocation and command wall.

### #286 new-Store SQLite page size (2026-09-30)

This document's current schema creation requests2,048-byte SQLite pages before creating the first application table. The r015 source's 4,096-byte original Store used76,341,248 apparent bytes and84,135,936 allocated bytes; `dbstat` attributed1,215,261 bytes of unused page space to `object_packs` and613,651 to `objects`. A labelled offline `VACUUM` of ordinary independent copies measured74,969,088 apparent bytes at2 KiB versus75,456,512 at4 KiB; those copies are **diagnostics**, not candidate storage or time evidence. The next original at-run file must decide whether the smaller page layout improves actual exclusive allocation without crossing the unchanged command bound. Existing schema-10 Stores of any supported SQLite page size remain readable; pack framing, canonical identity and resource ceilings are unchanged.

### #286 bounded pooled append reservation (2026-09-30)

This document's current writer uses pooled pack version22 with a128 KiB assembled/append cap. The unsampled version21 remains readable at64 KiB and old version12 at its original256 KiB. All retain256 reserved sixteen-byte directory slots and the same group/record grammar; a save whose groups outgrow128 KiB starts another pack. The **selected stride10/3 Stores** have17/53 pooled packs, medians106,169/41,798 B, with7/5 above128 KiB; their temporary256 KiB reservations contain2,499,883/10,927,867 B of unused capacity before tail close. The source-pinned r017 run-only Store also has157 pooled packs, but it did not set this policy. This is a measured shape and a prospective mechanism, not an allocated-byte saving claim. The writer's pack/buffer bound tightens; no new cache, worker, external spool or post-run compaction is introduced. A new at-run C2+C5 `st_blocks*512` receipt under the unchanged strict ceiling decides qualification.

### #286 bounded hint-index cleanup and pooled v21 return (2026-09-30)

This document's current writer again uses the existing64 KiB v21 pooled format, with v22's128 KiB and v12's256 KiB limits preserved on read. Selected-tier r018 measured131,072 B less original-owner allocation than r016 at128 KiB while both tiers passed, and left ample command headroom. New C2 Stores also omit `signatures_save`: normal candidate lookup uses the slot primary key, while the index's only consumer is failed-save cleanup over a table capped at8,192 rows. Old schema-10 Stores carrying the extra index still validate, and cleanup stays paged and fail-closed. The r019 index occupied100,352 B in2 KiB pages; that is a source-backed opportunity, not an at-run saving. Both edits preserve canonical objects, authenticated reads and the original storage/time thresholds; a new sealed receipt must establish their effects.

The later selected-FULL level12 experiment at `9b084cd27` was reversed after [r025](../issues/286/experiments/20260930-history-stride1-v3-r025.md) observed **322,362 fewer pack-body bytes but671,744 more original C2 allocated bytes**. The subsequent r008–r036 profile used payload level9/group level19; the current source restores level3/1 within the same fixed16-MiB workspace. The original diagnostic, sample and failure stay in the #286 ledger.

### #286 bounded page reclamation experiment, reverted (2026-09-30)

The source at `6fd7fce46` requested SQLite `auto_vacuum=INCREMENTAL` on new C2 Stores and called `PRAGMA incremental_vacuum(1)` inside each save transaction. The [r027 diagnostic](../issues/286/experiments/20260930-history-block-diagnostic-r027.md) showed a16-MiB original-owner allocation jump, and a same-volume synthetic transaction showed that one-page reclamation can release an unused preallocated tail. In the actual history selection, however, [r028](../issues/286/experiments/20260930-history-v3-r028.md) passed stride10/3 while [r029](../issues/286/experiments/20260930-history-stride1-v3-r029.md) **failed** the unchanged stride1 gate by143,360 B, **4,096 B worse** than prior level9 r021. The current source therefore restores the original new-Store `auto_vacuum=NONE` and removes the save-time pragma; old mode2 Stores remain readable/writable under SQLite's ordinary format support. No raw-file truncation, file-sized spool, external VACUUM, sync or recovery path was introduced. The unsuccessful treatment's receipts remain in the #286 ledger; it is not a product storage claim.

### #286 bounded physical headroom before pack insertion (2026-09-30)

The [r030 diagnostic](../issues/286/experiments/20260930-history-preallocation-diagnostic-r030.md) kept C2/C5 bytes identical to r021 but avoided its16-MiB APFS allocation step by reserving small physical increments on the original Store. The current source moves that work into C2's `insert_pack` path **before** each new bounded pack row, inside the save's existing write transaction and measured child. It asks the host for physical headroom through the next pack's declared capacity plus2 MiB, rounded to1 MiB, with one request capped by the singleton-pack bound plus3 MiB. On Darwin the already-locked `nix` crate's safe `F_PREALLOCATE` wrapper reserves from physical EOF; on Linux its safe `fallocate(FALLOC_FL_KEEP_SIZE)` wrapper reserves without changing logical length. The earlier unverified Linux `posix_fallocate` branch at `aed28dda1` would extend logical EOF, so it was corrected before any Linux execution or release claim. No product `unsafe` boundary, new third-party package, extra worker, preconstructed Store, file-sized spool, `fsync` or fallback is introduced. A missing path, unsupported platform, short reservation, I/O error or logical-length change fails explicitly before pack insertion; ordinary save cleanup handles the failure. This is physical placement work, not a new canonical or SQLite schema format. R030 is causal evidence only: fresh original-owner selected and stride1 receipts must decide storage, command/verifier bounds, semantics and cleanup.


### #302 PostgreSQL C2 engine (2026-10-03)

Described against parent `e828bb35f` and step 6 source. `layerfs-metadata` owns
five C2 tables and the pack sequence, with no save/publication identity, group
or dependency table. `pack` carries immutable descriptors for both destinations
and bytea bodies only for metadata. SQL remains under `sql/`: schema bootstrap
plus dedicated operational files in `sql/queries/storage/`; Rust embeds them and
owns checked parameter binding, execution, decoding and outcome classification.

The seven port units use explicit parameter types through the complete published
PostgreSQL driver. One dedicated I/O worker hosts that driver and its bounded
runtime; the public ports remain synchronous. The observer counts actual frontend
Sync/simple Query and backend ReadyForQuery frames, as well as socket/protocol
bytes. It skips data rather than decoding another protocol implementation.
Deadlines cover connection setup and wire operations. Timeout/lost acknowledgement
or malformed transport makes the handle terminal, with no retry, reconnect,
cancel connection or alternate address. Unexpected TLS refusal retains a diagnostic
reason without including credentials. The native TLS connector verifies chain/name
and requires encryption; it never downgrades to the local/plain profile.

Allocation locks the single policy row while reserving a sequence block and/or
ordinals, so concurrent ranges cannot overlap. Atomic registration inserts packs,
first-wins object locators, catalogue rows, signature changes and ordinal/window
changes in one statement/transaction. The existing per-group window recurrence is
retained. Unused-tail release is conditional on the current allocator watermark.
Signature rows cannot refer to absent objects; stale stamps do not replace newer
ring entries. The engine does not inspect canonical/record/pack contents.

Current owner scope is local acceptance; remote/cloud deployment qualification is
deferred. TLS configuration/capability remains. Local TLS proxy tests prove the
mechanism at their stated fixture, with earlier fixture failures retained; they
are not a remote-provider claim. C5 history is the next rollout slice.


### #302 standalone namespace Init (step 8)

This description covers the same-commit implementation based on `cba8a2804`.
`layerfs-project::init` scans a Unix directory, constructs through C1, saves
through the two C2 ports and publishes the genesis LayerStack through C5. The
caller supplies authority/name/scope seed, scratch parent and deadline. The
existing server algorithms are adapted beside their old home until cluster 2
retirement; this is duplication during relocation. Four file constructors feed
one bounded Save owner. Init explicitly selects four acknowledged payload
uploads at a time, with all acknowledgements before registration; ordinary
begin_save retains one upload producer. S3's explicitly selected four-connection
window supplies independent streams. No engine is named by project source; real
composition is in examples/tests. Canonical/physical formats and namespace
construction rules are unchanged. Typed failures stop the operation; scratch
cleanup failure is explicit, and no payload is retried or deleted on a guess.
The functional fixture counts and independent verification carry no timing or
acceptance claim. The release harness/corpus/cache contract still awaits step 10.

## Owner-directed measured optimization (2026-10-04)

The #302 optimization after source base 988eb538b keeps the existing five pack
lanes and framing. Immutable lane queues share one aggregate PACK_LIMIT byte
charge (including active-lane framing) and BATCH_OBJECT_LIMIT row budget. Lane
switches no longer alone force partially filled packs to close. Closure seals
and flushes referenced lanes before registration; all queues drain at wave and
save boundaries. Different cross-lane grouping is permitted by this optimization
and is not a claim of byte-identical mixed-lane packs. Existing canonical/codec
parity and independent reopen checks remain required. No additional producer
or upload worker, wider cache window or relaxed physical pack limit is used.

Pack reservations use a conservative next-wave bound (one pack per object,
pooled-value packs per inode leaf, and preceding open groups); an existing block
is consumed when it can cover that bound. The allocation request keeps its
original size. Final seals use the actual remaining-group bound. IDs are never
reclaimed or made contiguous across independent reservations.

PostgreSQL object registration is one bounded INSERT FROM unnest with RETURNING;
a materialized input and returned winners derive lost IDs in input order,
without a second snapshot lookup. The public provider rejects duplicate input
identities before wire submission; first-wins races and the encompassing atomic
registration still apply. No deployed schema is silently altered by open.

For already available bounded S3 bodies, the client sends body bytes without an
Expect exchange and still requires the complete conditional-create response.
Unknown acknowledgement outcomes remain terminal and are never resent. Required
key/body authentication remains at the opaque ObjectStore boundary. C1 exposes
an immutable checked PoolingLeaf view, so pooled ordinal assignment and body
construction share a canonical decode; malformed canonical/ordinal/body checks
remain. Full result and any failed checks belong to #302 progress/evidence.

Init now admits sealed bodies through a rolling pool of at most4 scoped upload
workers per ready list. A completed slot can take the next body without a
chunk-wide barrier. The same body ownership and byte limits hold; every admitted
attempt joins before any registration, and failure stops new admission without
retry or orphan deletion. S3 leases a free member of its existing socket window
before I/O; ordinary callers exceeding that fixed window wait, not expand it.


### Operation-owned immutable pack reuse

This change is based on source fd65493c1. `Reader` retains its authenticated
sealed-pack map between demands, alongside its existing decode workspace and
group/pool caches. Acquisitions still pass through the same digest, framing,
domain and reconstruction checks. The existing aggregate4MiB eviction rule and
single oversize-pack exception apply; no cache budget is enlarged. A fresh
Reader starts empty. Published packs are immutable and pack IDs are never reused,
so retaining an authenticated body within the operation does not reinterpret a
later publication. Results remain in demand order, including duplicate IDs.


### Logical reference membership and physical reconstruction

This change is based on896e84c3b. Save-wave membership still batches offered IDs,
logical references and declared predecessors. Chain prefetch now selects only
offered IDs for exact reuse and declared physical predecessors. Logical children
remain subject to admission and atomic-publication closure checks; their bodies
are acquired when actually reconstructed, not merely because a new parent names
them. Automatically selected physical candidates still undergo the selector's
bounded authenticated chain acquisition. Exact reuse, lost-winner comparison,
cycle/depth/work checks and full pack authentication are unchanged. This removes
no required read check and enlarges no cache or publication bound.


### Bounded signature publication

This change is based onf3913c845. SQLite signature registration uses ordered
multirow UPSERT pages inside the same atomic publication. Each page is bounded
by the actual SQL-length and variable limits and the existing512-object admission
bound. Scalar/binding descriptors are bounded; signature/identity bytes are
borrowed. Stale stamps are ignored and equal-stamp later publications retain the
existing replacement rule. Foreign-key failure rolls back the entire publication,
including bodies and preceding signature rows. No acknowledgement, transaction,
profile, canonical/physical format or working-byte bound is enlarged.


### Explicit checkpoint allocation release

This implementation is based on269e520f7. A successful unobstructed explicit
checkpoint also transfers unused extents beyond the main file's logical EOF to
an exclusively created empty same-volume scratch, then closes/removes it. A
retained main-file descriptor targets the opened file. No logical bytes are
copied or truncated. Busy checkpoints do no allocation work; readonly authority
is refused before SQL. Missing capabilities and filesystem cleanup errors fail
explicitly. Uncertain transfer/close/metadata outcomes quarantine the session,
without retry or guessed source deletion. No third-party package identity or
unsafe product code is introduced: existing nix0.31.3 supplies the safe wrapper.
Checkpoint wall includes this work; before/after main allocation is reported.
WAL/FULL/fullfsync/page/cache/worker/canonical/physical policies remain unchanged.
Process-crash tests cover acknowledged content after completed cleanup; physical
power-loss behavior and atomic in-syscall crash interruption are not claimed.


### Partial immutable pack queues across input waves

This change is based on8d4775e33. An input-wave boundary no longer alone drains
partially filled lane queues. Queues retain the existing aggregate256KiB charge
including lane framing and512-row limit. Pressure seals them through the same
packer. Dependency/advisory closure and same-save reads force needed queues to
seal, while ready packs remain published at each wave. Finish drains every group/
queue and completes reference-closed acknowledgements. No published BLOB append,
new producer, buffer/row/byte/cache expansion or changed failure policy is used.
Mixed-lane physical grouping can differ; canonical identities and wire grammar
remain unchanged. Private pending signatures remain unregistered until their
object is ready, under the existing bounded ring.

### Exact initial pooled reservation lookahead

This change is based on a0201bfef. Before admission, a wave with no prior
ordinal demand synchronizes the existing pooled index. If that authenticated
index is empty, up to four new, unlocated inode leaves are decoded to count
their distinct values. One acknowledged exact reservation covers that demand;
ordinary per-leaf assignment and group construction retain their order and
small-save ordinal density. Actual reservation diagnostics count one call; the
later16-leaf block switch counts the exact fresh leaf demands coalesced by it.
Nonempty indexes retain the original per-leaf reuse/window logic. Existing
reservation ownership, final unused-tail release, conflict and uncertain-outcome
refusal remain unchanged. Temporary lookahead retains at most660fixed73-byte
values (48,180value payload bytes plus collection overhead) and one decoded leaf
(up to165rows), and releases these before admission. This fixed owner is separate
from the unchanged4MiB/512object input wave,256KiB framing queues and131,072value
index; it is not a whole-importer or total RSS bound. No durability/worker/cache/
transaction limit changes or automatic retries are introduced.

### Pack tail sufficiency checked at each allocating operation

Based on37eabc68f. Pending-object sealing checks the current unfinished group/
queue pack bound. Each new object checks that bound plus one possible object
pack and the maximum value-group packs for an inode leaf. The existing wave
allocation block remains, raised only if the concrete conservative bound needs
more. Checks reuse acknowledged remaining IDs until that operation's bound no
longer fits, before the operation and never after an allocation failure.
Finish/read/closure bounds and first-writer base acknowledgement are unchanged.
This replaces a whole-wave speculative check that could discard a small tail
before a larger small-record wave which used only a few packs. No consumed-ID
recycling, buffer/cache/worker/profile/port-bound change or retry is added.
Physical pack IDs can change; canonical identities do not.

### Retained selection observations

Based on0e32111e3. The fixed four-entry successful-save history now retains the
already collected codec selection profile, including final-drain work. Its
cumulative selection fields cover acknowledged saves only; inclusive stage
counters still include attempted work. The profile contains FULL/delta/candidate
cost/eligibility/acquisition and compression-probe observations. Nested spans
remain overlapping, not additive. This exports actual runtime telemetry without
a test-only product path, changed selection, timer policy or payload owner.

The observed FULL/delta fields now also cover pooled leaf encoding, alongside
ordinary record selection; a group field covers ordinary group framing/codec
and pooled value-group compression. These are actual fixed aggregate clocks
around those unchanged operations. The runtime has no benchmark-selected route.
Group counters also charge failed calls internally; acknowledged-save history
only exports successful saves. Duration fields do not choose representations.

### Statement lifetime observations

Based on3cf8fd6ff. Shared SQLite diagnostics keep seven fixed per-statement phase
aggregates and a second COMMIT-only array: prepare/cache checkout, binding/query
creation, next (SQLite step plus rusqlite's DONE reset), row mapping, explicit
cursor drop, VM/status accounting and cached statement drop. These clocks surround
the same ordered calls, including failed attempts. They do not replace statements,
retry or choose a route. Next cannot be labeled pure step because the pinned
library performs reset within it. The arrays are nested within inclusive statement/
transaction clocks and cover the ordinary query wrapper; schema batch execution
is separately timed and not silently included. No payload/cache/worker bound or
journal/synchronization change is introduced.
