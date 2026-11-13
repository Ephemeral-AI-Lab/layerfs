# Physical storage

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How finalized canonical objects become records in immutable packs, how they
are read back authenticated, and the ports a persistence backend implements.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-storage` |
| Source | [`pack/`](../../../../core/crates/layerfs-storage/src/pack), [`save/`](../../../../core/crates/layerfs-storage/src/save), [`read/`](../../../../core/crates/layerfs-storage/src/read), [`port/`](../../../../core/crates/layerfs-storage/src/port), [`store/`](../../../../core/crates/layerfs-storage/src/store), [`encoding/`](../../../../core/crates/layerfs-storage/src/encoding) |
| Uses | [Objects](../objects/README.md) |
| Used by | [Engine: Store](../../engine/store/README.md), [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Commit](../../engine/commit/README.md), [Engine: Project Init](../../engine/project-init/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md) |

## Picture

```text
  FinalizedObject (canonical bytes, role, references, predecessors)
        |
 +------v------------------ Save (one producer) ----------------------+
 | PendingBatch --> wave: locate, reuse or select --> lane group      |
 |  (512 objects,          FULL / STORED / PREFIX /  --> seal pack    |
 |   publication bytes)    COPY/INSERT (encoding.md) --> Publication  |
 +------------------------------------|-------------------------------+
                                      | reserve() / publish()
 +--------------------- PackPersistence (engine backend) -------------+
 |  policy locate read_packs read_pack_selection read_scoped_pack     |
 |  inode_values signatures reserve publication_pack_cost publish     |
 +------------------------------------^-------------------------------+
                                      | locate() / read_scoped_pack()
 +--------------------- Reader (one operation) -----------------------+
 | locate --> prefetch chains --> acquire groups --> PackCache        |
 |        --> resolver: decode base first, hash, compare with the id  |
 +--------------------------------------------------------------------+
        v
  canonical bytes, each checked against its ObjectId
```

`Storage` ([`store/handle.rs`](../../../../core/crates/layerfs-storage/src/store/handle.rs))
is the handle over one `PackPersistence`. It reads the policy once, derives
`StorageCapacities`, owns the locator cache and signature ring, and opens one
Save at a time. The SQLite and PostgreSQL implementations live in
`layerfs-persistence` ([Engine: Store](../../engine/store/README.md)). Only
the Disposable profile (SQLite WAL, `synchronous=OFF`) may be run; Durable
execution is `NOT_RUN — disabled by owner until explicit reauthorization`.

## What it does

### Packs

```text
 offset 0       8      12      16      20      24
 +--------------+------+-------+-------+-------+
 | "LFPACK\0\0" | ver  | groups| used  |   0   |   control area, LE u32
 +--------------+------+-------+-------+-------+
 | directory: exactly `groups` entries         |
 |  ordinary, chunk, singleton: 16 B           |
 |   start | encoded len | decoded len | codec |
 |  whole file: 4 B start                      |
 +---------------------------------------------+
 | group bodies ... ending exactly at `used`   |
 +---------------------------------------------+
 group body: count | end[0] | end[1] | ... | record 0 | record 1 | ...
```

| Lane | Holds | Writes | Also reads | Group codec |
| --- | --- | --- | --- | --- |
| `Ordinary` | every tree and metadata role, pooled inode leaves, program pages | 26 | 9, 19, 24 | raw or zstd level 1 |
| `Chunk` | chunk payload records | 25 | 10, 15, 20 | raw |
| `WholeFile` | compact whole-file records (no lengths; raw = canonical − 23) | 23 | 11, 14, 17, 18 | raw |
| `Singleton` | one whole-file record too large for a 256 KiB pack | 16 | 13 | raw |

Every other version is `UnsupportedPolicy { field: "pack framing version" }`,
including 21 and 22 of the retired pooled value-group lane; older versions
with reserved directory slots keep their own body offset
([`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs)).
A pack's key is `ObjectKey`, the SHA-256 of all its bytes, computed once by
`PublishedPack::seal`.

| Assembly step | Rule | Source |
| --- | --- | --- |
| Group | seals when its frame would pass `GROUP_TARGET` 48 KiB or 512 KiB canonical; body ≤ 64 KiB (ordinary, chunk) | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs), [`pack/assemble.rs`](../../../../core/crates/layerfs-storage/src/pack/assemble.rs) |
| Compression | ordinary groups only, kept when the frame plus its entry fits the raw body | `compress_group_body` in [`encoding/codec.rs`](../../../../core/crates/layerfs-storage/src/encoding/codec.rs) |
| Queue | all lanes share 256 KiB and 512 rows; flush on pressure, reference closure or finish | [`save/seal.rs`](../../../../core/crates/layerfs-storage/src/save/seal.rs) |
| Fit | `append_fits`: ≤ 16 groups and ≤ the lane's pack limit, charged exactly | [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |
| Place | next pack id from the Save's reserved block, one-pass assembly, one SHA-256 | `Packer::place` in [`save/seal.rs`](../../../../core/crates/layerfs-storage/src/save/seal.rs) |

A sealed pack is never appended to. On read, the header and directory are
checked before any body: magic, version, reserved word, group count, `used`
length, directory continuity, codec per lane, strictly increasing record ends.

### Saves

```text
 begin_save --> reserve(packs 4096, ordinals 16384)
 accept(obj) x N --> PendingBatch (512 objects or publication_bytes)
   bound reached --> wave:
     locate ids, references and predecessors (slices <= 4,096 ids)
     duplicate in wave or stored with same role+length --> reused
     new --> every reference stored/pending/sealed? else MissingDependency
         --> select representation (encoding.md) --> lane group
     flush: signature-ring changes, then publish ready packs
 finish --> last wave; seal every lane; publish; release unused ordinals
       --> WriteOutcome
```

| Rule | Source |
| --- | --- |
| A stored object with the same role and length is reused and **not read back**; a role or length mismatch is `Collision`; a corrupt stored copy fails at its first read | [`save/wave.rs`](../../../../core/crates/layerfs-storage/src/save/wave.rs) |
| `publication_bytes`: 4 MiB − 1 by default and on SQLite, 16 MiB − 1 on PostgreSQL; an object larger than the bound goes alone | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs), [`save/batch.rs`](../../../../core/crates/layerfs-storage/src/save/batch.rs) |
| Reservation blocks size allocation windows only: packs 4,096 (1–8,191), ordinals 16,384 (1–131,072); a refill replaces the window; only the last ordinal tail is released | [`store/reservations.rs`](../../../../core/crates/layerfs-storage/src/store/reservations.rs), [`save/reservation.rs`](../../../../core/crates/layerfs-storage/src/save/reservation.rs) |
| Pooled inode leaves: a value keeps the ordinal a named base leaf gave it or one assigned earlier in this Save (memo ≤ 131,072, cleared whole); otherwise the next ordinal. Values publish before any leaf naming them; nothing is rebuilt from history | [`save/pooled.rs`](../../../../core/crates/layerfs-storage/src/save/pooled.rs) |
| Publication: reference-closed (an open group a ready member names is sealed first), topologically ordered (a cycle is `Integrity`), split before 8,191 rows or `publication_bytes`; one `publish` per batch | [`save/publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs) |
| First writer wins: `Published.lost` lists locators already present; each lost id is read back and compared, a difference is `Collision` | same |
| One Save per handle (`Integrity("one save per storage handle")`); separate handles may save concurrently | [`save/state.rs`](../../../../core/crates/layerfs-storage/src/save/state.rs) |
| Acknowledged batches stay published if the Save later fails; after a failed `accept` the Save is terminal (`Aborted`); dropping it invalidates the signature ring | [`save/operation.rs`](../../../../core/crates/layerfs-storage/src/save/operation.rs) |

`WriteOutcome` reports inserted and reused objects, FULL and PREFIX records,
packs, stored, metadata and canonical bytes; `Storage::save_work` returns
invocation counts and wall time for nine Save stages
([`save/work.rs`](../../../../core/crates/layerfs-storage/src/save/work.rs)).
Two things are called seal: a group/pack seal inside a Save, and the Store
seal (`SqliteHandles::seal`,
[`store/seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs)),
which consumes the sole SQLite session for an offline handoff and refuses
with `Busy` while a `Storage`, `Reader` or `Save` is retained. A live daemon
keeps its Store open across Saves.

### Reads

```text
 read_objects(ids)      <= 4,096 ids, <= 32 MiB canonical
   1 locate     LocatorCache, then PackPersistence::locate (<= 4,096 ids)
   2 cohorts    packs that fit the 2 MiB pack cache together, in order
   3 prefetch   level by level: acquire groups, read base ids, locate bases
   4 fetch      PackCache::acquire_groups(missing groups)
   5 decode     resolver: walk chain, decode base first, hash each object
   --> canonical bytes in demand order
```

| Part | Rule | Source |
| --- | --- | --- |
| Locator checks | each id asked once and returned at most once; pack id and descriptor agree; length ≤ 16 MiB, group < 256, record < 8,191 | [`read/fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Locator cache | 4,096 entries, second-chance eviction that skips the current demand; after a publication won locations are kept, lost ones invalidated | [`read/locator_cache.rs`](../../../../core/crates/layerfs-storage/src/read/locator_cache.rs) |
| Absent set | 4,096 ids, cleared at each demand; a base named by a stored record drops its negative mark before it is asked again | [`read/fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Whole-pack batch | when the provider takes packs whole (`whole_pack_bytes`: 4,120 B default, 256 KiB on PostgreSQL), a cohort's small uncached packs are read in one `read_packs` call | same |
| Read units | `GroupPlan` sees the directory, then takes the pack whole (singleton, small, wanted groups ≥ half or > 4 ranges, already kept) or coalesced ranges; scratch ≤ 2 MiB | [`read/units.rs`](../../../../core/crates/layerfs-storage/src/read/units.rs) |
| After a Save | only `Storage::reader_after_save` reuses the latest Save's published bodies (2 MiB) and values (512 KiB) | [`read/fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Kept caches | `Reader` for one operation; `ReadCache` and `Send`able `KeptReads` (3 MiB) across demands | [`read/provider.rs`](../../../../core/crates/layerfs-storage/src/read/provider.rs) |
| File lengths | `Reader::file_lengths`: whole files from locator length − 23 without payload; chunked or empty files read only their file state; this attests no payload | [`read/length.rs`](../../../../core/crates/layerfs-storage/src/read/length.rs) |
| Counters | `Storage::diagnostics`: cumulative counts per handle (port calls, acquisition choices, bytes, cache hits), never timings | [`read/counters.rs`](../../../../core/crates/layerfs-storage/src/read/counters.rs) |

### Ports

| `PackPersistence` method | Contract |
| --- | --- |
| `policy` | the persisted `StoragePolicy`, once per handle |
| `locate` | ≤ 4,096 distinct ids; absent ids omitted |
| `read_packs` | whole packs ≤ 2 MiB in total, or one singleton, each a length- and SHA-256-checked `PersistedPack` |
| `read_pack_selection` | strict selected read: the whole pack is scanned and hashed, chosen ranges kept; default `BackendUnavailable` |
| `read_scoped_pack` | selected units, framing checked, not hashed; each used object is authenticated by its id; default `BackendUnavailable` |
| `inode_values` | ≤ 4,096 ascending ordinals in one statement |
| `signatures` | ≤ 8,192 signature-ring rows in insertion order |
| `reserve` | one block of pack ids and/or ordinals |
| `publish` | one atomic `Publication` (packs, locators insert-if-absent, inode values, signatures, released ordinals); returns `Published { lost }` |

[`port/persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs)
also has `whole_pack_bytes`, `publication_bytes` and `publication_pack_cost`.
A reply that differs from the plan is `Integrity`; a missing selected read is
`BackendUnavailable`, never a whole-read fallback.

| `PersistenceError` | Meaning | `StorageError` |
| --- | --- | --- |
| `Busy` | lock or session held; no write effect | `Busy` |
| `BackendUnavailable`, `Missing`, `Malformed` | unavailable, absent, or a reply outside the port grammar | `Io` |
| `Refused { status }` | definite refusal; `"Capacity"` is out of space | `Io`; `is_capacity_refusal()` |
| `Uncertain` | outcome unknown | `UnknownOutcome`, never resent |

`Acquisition` ([`port/acquisition/`](../../../../core/crates/layerfs-storage/src/port/acquisition))
is Project Init's working-row port: owner `{ operation, epoch }` (stale is
`Stale`), read windows of 512 rows / 256 KiB, write windows of 4,096 rows /
1 MiB, breadth-first order with children by name bytes; nothing is retried.

### Reuse and read cost

Three kinds of reuse can meet in one Save:

```text
  previous file  [ A ][ B ][ C ][ D ]     new file  [ A ][ B*][ C ][ E ]
  A, C   same canonical bytes and ID  --> exact reuse, nothing written
  B*     different bytes, B offered   --> FULL vs PREFIX against B
  E      different, no candidate      --> FULL or STORED
  new pages and file state            --> written; then namespace, history
```

| Delta chain cost | Value | Source |
| --- | --- | --- |
| Depth (default) | whole file 16, chunk 8, pooled metadata 8; 0–50 accepted | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Canonical work per chain | 4 × cutoff (512 KiB at 128 KiB; 4 MiB at 1 MiB) | `CHAIN_CANONICAL_CUTOFFS` |
| Encoded work per chain | 2 × cutoff (256 KiB at 128 KiB) | `CHAIN_ENCODED_CUTOFFS` |
| Selection test | chain decoded bytes + target length ≤ 4 × cutoff, chain encoded bytes + target length ≤ 2 × cutoff | [`delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs) |

Stored-byte savings cost read work: a 4 KiB logical read may need a whole
chunk, a complete group and several delta bases, and a whole-file object is
decoded whole before slicing. Neither reuse nor delta promises reads
proportional to the bytes requested.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Reads verify what they decode: every chain element is hashed against its locator's id | `Resolver::resolve_charged` in [`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| A whole pack across the port matches its length and SHA-256 key | `PersistedPack::authenticate` in [`port/read_pack.rs`](../../../../core/crates/layerfs-storage/src/port/read_pack.rs) |
| A pack's key is computed once, at seal | `PublishedPack::seal` in [`port/persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs) |
| Header and directory are validated before any body; no trial decoding | `parse_control_header`, `visit_directory` in [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |
| A publication is closed under direct references | `register_ready` in [`save/publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs) |
| Locators are insert-if-absent; the first writer wins and a loser is compared | [`port/persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs), [`save/publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs) |
| One attempt per port call: no retry, no second codec, no whole-pack fallback | [`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs) |
| A Store's policy row is validated at open and never migrated | `StoragePolicy::validated` in [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| `unsafe` only in the audited zstd module | `#![deny(unsafe_code)]` in [`lib.rs`](../../../../core/crates/layerfs-storage/src/lib.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Pack (ordinary, chunk, whole-file) / singleton | 256 KiB / 16 MiB + 4,096 | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Groups per written pack / readable older pack; records per group | 16 / 256; 8,191 | [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs), [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Canonical object | 16 MiB, else `CapacityExceeded { what: "encoding.canonical_length" }` | [`encoding/full.rs`](../../../../core/crates/layerfs-storage/src/encoding/full.rs) |
| Read demand | 4,096 ids and 32 MiB, else `CapacityExceeded { what: "storage.read_objects" / "storage.read_canonical_bytes" }`; unknown id `ObjectMissing` | [`read/objects.rs`](../../../../core/crates/layerfs-storage/src/read/objects.rs) |
| Pack above the read's visibility ceiling | `VisibilityCeiling` | [`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| Disk full | `Refused { status: "Capacity" }`; only that is `is_capacity_refusal()` | [`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs) |
| Reference not stored | `MissingDependency` | [`save/wave.rs`](../../../../core/crates/layerfs-storage/src/save/wave.rs) |

## Demonstration

In-memory `PackPersistence`
([`tests/support/memory_metadata.rs`](../../../../core/crates/layerfs-storage/tests/support/memory_metadata.rs))
or no provider; no platform gate, feature, Docker or environment. Each
command gets a wall timeout of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`physical_formats.rs`](../../../../core/crates/layerfs-storage/tests/physical_formats.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test physical_formats` | any | Each framing recognized by its version, unknown versions refused, bounds enforced, directories sized to their groups |
| [`verified_pack_read.rs`](../../../../core/crates/layerfs-storage/tests/verified_pack_read.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test verified_pack_read` | any | A wrong length, digest or domain is refused |
| [`delta_base_budget.rs`](../../../../core/crates/layerfs-storage/tests/delta_base_budget.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test delta_base_budget` | any | A base over the chain budgets is refused and the object stored FULL; a corrupt base inside them fails the Save |
| [`capacity_refusal.rs`](../../../../core/crates/layerfs-storage/tests/capacity_refusal.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test capacity_refusal` | any | Only `Refused { status: "Capacity" }` counts as full disk |

## Pages

| Page | What it covers |
| --- | --- |
| [Encoding](encoding.md) | `encoding/`: codec frames, FULL/STORED/PREFIX selection, chain decoding, pooled leaves and program pages, pack units and the pack cache. |
