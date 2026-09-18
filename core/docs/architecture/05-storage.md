# Storage (C2, `layerfs-storage`)

> **Status:** Research; informative and not a product contract.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

---

## 6. Storage (C2 — `layerfs-storage`)

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
                 full_records, prefix_records, delta, chain, pool }
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
`prefix_records`, plus `DeltaCounters` for selection outcomes and `ChainCounters`
for base acquisition.

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
`max_depth`, `canonical_bytes`, `opens` — including the **ceiling applied to every
acquired location**, so a receipt can show which visibility watermark a read
actually observed, and the connections the wave opened.

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

What a session deliberately does **not** pool is the ceiling. It is the publication
watermark, so it is re-read for every wave and a save that completed between two
waves is visible to the second one; pooling it would turn an operation's later
waves into a snapshot of its first. `opens` reports `1` on the wave that opened the
session and `0` on the waves that reused it.
