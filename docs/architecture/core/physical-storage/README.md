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
 |  policy locate locate_chains read_packs read_groups                |
 |  read_pack_selection read_scoped_pack inode_values signatures      |
 |  reserve publication_pack_cost publish                             |
 +------------------------------------^-------------------------------+
                                      | locate_chains() / read_groups()
                                      | / read_scoped_pack()
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
`layerfs-persistence` ([Engine: Store](../../engine/store/README.md)). Product
paths open the Normal profile (SQLite WAL, `synchronous=NORMAL`); Durable
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

A sealed pack is never appended to. A Store compaction may build it again
from the records it keeps: `retain_records` in
[`pack/repack.rs`](../../../../core/crates/layerfs-storage/src/pack/repack.rs)
copies each kept record as stored, frames changed groups through the same
group builders, and returns the new pack only after it validates and every
kept record compares equal to its source. On read, the header and directory are
checked before any body: magic, version, reserved word, group count, `used`
length, directory continuity, codec per lane, strictly increasing record ends.

### Saves

```text
 begin_save --> the handle's kept id block, else reserve(packs 4096, ordinals 16384)
 accept(obj) x N --> PendingBatch (512 objects or publication_bytes)
   bound reached --> wave:
     locate ids, references and predecessors (slices <= 4,096 ids)
     duplicate in wave or stored with same role+length --> reused
     new --> every reference stored/pending/sealed? else MissingDependency
         --> select representation (encoding.md) --> lane group
     flush: signature-ring changes, then publish ready packs
 finish --> last wave; seal every lane; publish, releasing the ordinal tail past
            the 128 a keeping handle keeps (every unused ordinal on any other
            handle); keep the unused ids on the handle
       --> WriteOutcome
 release_kept_ordinals --> before an orderly drop of a keeping handle: one
            publication releasing its kept ordinals (same conditional rewind)
```

| Rule | Source |
| --- | --- |
| A stored object with the same role and length is reused and **not read back**; a role or length mismatch is `Collision`; a corrupt stored copy fails at its first read | [`save/wave.rs`](../../../../core/crates/layerfs-storage/src/save/wave.rs) |
| `publication_bytes`: 4 MiB − 1 by default and on SQLite, 16 MiB − 1 on PostgreSQL; an object larger than the bound goes alone | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs), [`save/batch.rs`](../../../../core/crates/layerfs-storage/src/save/batch.rs) |
| Reservation blocks size allocation windows only: packs 4,096 (1–8,191), ordinals 16,384 (1–131,072); a refill replaces the window. A finished Save leaves its unused pack ids on the handle (`IdBlock`), and the next Save on it reserves only when a range runs out (#768). A keeping handle (`Storage::keeping_ordinals`: the Store service's Commit producers, which live between Commits) also keeps the first `KEPT_ORDINALS` (128) unused ordinals, and its last publication releases the tail past them; any other handle releases every unused ordinal there, as before #768. The release rewinds the Store's counter when nothing was reserved after the block (a tail that is not rewound is lost, as before). The owner of a keeping handle returns its kept ordinals before every orderly drop (`Storage::release_kept_ordinals`: one publication of the same conditional rewind, sent once, never on a Commit's path): a Workspace close that finds the Store's idle slot taken, a Store close (`StoreService::close`, a daemon's `close_bound`) and a Commit that failed definitely. Only a crash, an unknown outcome or a refused release (a quarantined session) loses them, ≤ 128 per handle; a returned block not adjacent to the counter (another handle reserved after it) is not rewound and stays a gap of ≤ 128, as a finish's tail always was. A dropped handle also loses its ≤ 4,096 unused pack ids (`i64`, never released). A Save that fails, refuses, ends uncertain or is dropped unfinished drops the block. Ids are never reused: an allocator moves back only by that conditional rewind over ids never assigned, and a compaction copies both counters or abandons when they moved | [`store/reservations.rs`](../../../../core/crates/layerfs-storage/src/store/reservations.rs), [`save/reservation.rs`](../../../../core/crates/layerfs-storage/src/save/reservation.rs), [`store/handle.rs`](../../../../core/crates/layerfs-storage/src/store/handle.rs) |
| Pooled inode leaves: a value keeps the ordinal a named base leaf gave it or one assigned earlier in this Save (memo ≤ 131,072, cleared whole); otherwise the next ordinal. Values publish before any leaf naming them; nothing is rebuilt from history | [`save/pooled.rs`](../../../../core/crates/layerfs-storage/src/save/pooled.rs) |
| Publication: reference-closed (an open group a ready member names is sealed first), topologically ordered (a cycle is `Integrity`), split before 8,191 rows or `publication_bytes`, a pack always in the batch of its first row; one `publish` per batch. A Workspace Commit holds the last batch for its one write transaction (#768): while held, the handle's own reads see its rows, values and pack bodies, and rows whose pack an earlier batch published, from the held set (`read/held.rs`) | [`save/publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs), [`read/held.rs`](../../../../core/crates/layerfs-storage/src/read/held.rs) |
| First writer wins: `Published.lost` lists locators already present; each lost id is read back and compared, a difference is `Collision` | same |
| One Save per handle (`Integrity("one save per storage handle")`); separate handles may save concurrently | [`save/state.rs`](../../../../core/crates/layerfs-storage/src/save/state.rs) |
| Acknowledged batches stay published if the Save later fails; after a failed `accept` the Save is terminal (`Aborted`); dropping it unfinished invalidates the signature ring and drops the handle's kept ids and published bodies and values | [`save/operation.rs`](../../../../core/crates/layerfs-storage/src/save/operation.rs) |

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
   1 locate     LocatorCache, then PackPersistence::locate_chains (<= 4,096 ids)
   2 cohorts    with chains: the roots one page covers, a page being one
                read_groups call of <= 2 MiB returned for the roots' and their
                chains' uncached groups, consumed before the next page;
                without: packs that fit the 2 MiB pack cache together, in
                order, then the whole-pack batch
   4 prefetch   level by level: acquire groups, read base ids, bind each base
                to its chain member or locate it; a level that locates also
                locates the deeper bases its members' cached records name
   5 fetch      PackCache::acquire_groups(missing groups)
   6 decode     resolver: walk chain, decode base first, hash each object;
                a failure on a hinted chain rebuilds once without hints
   --> canonical bytes in demand order
```

| Part | Rule | Source |
| --- | --- | --- |
| Locator checks | each id asked once and returned at most once; pack id and descriptor agree; length ≤ 16 MiB, group < 256, record < 8,191 | [`read/fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Locator cache | 4,096 entries, second-chance eviction that skips the current demand; after a publication won locations are kept, lost ones invalidated | [`read/locator_cache.rs`](../../../../core/crates/layerfs-storage/src/read/locator_cache.rs) |
| Absent set | 4,096 ids, cleared at each demand; a base named by a stored record drops its negative mark before it is asked again | [`read/fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Whole-pack batch | when the provider takes packs whole (`whole_pack_bytes`: 4,120 B default, 256 KiB on PostgreSQL), a cohort's small uncached packs are read in one `read_packs` call | same |
| Chain hints (#758) | every locate wave calls `locate_chains`; a malformed answer (unasked root, two chains for one root, more members than the role's depth, a member outside the locator bounds or with length 0, a directory that is not exactly `body[0..body_offset)`) is `Integrity("port chain locator")`, and the whole answer (rows and chains) is checked before any of it is kept. When a walk decodes base `X` from a record whose chain has a member there, `X` is bound to that member and kept as a hinted locator, so the walk's own locate sends nothing; a hinted locator serves only the demand that bound it (every demand starts by evicting them). A member whose id differs from `X`, whose pack descriptor contradicts a known one, or a list that does not end exactly at a FULL record, is stale: dropped, counted (`chain_hint_stale`) and the walk locates on. A pack descriptor only a bound member gave is a hint too: forgotten with the hints and at the next demand. It is confirmed, and counts as authoritative from then on, by promotion or by any later `remember_pack` of an equal descriptor: a locate row, a whole-pack reply (length and digest authenticated), but also a group read's ranged admission or a pack-cache hit, which carry the descriptor the group was read under and may be the member's own; so a member's descriptor whose group was read is not forgotten with the hints. A descriptor that contradicts a still-marked one makes the hint stale (counted once; strict refuses) and drops every hint at once, so a later failure of that walk at a hinted element is not rebuilt on a read (the failure stands); one that contradicts a confirmed descriptor fails as `inconsistent pack descriptor`. Pack ids are never reused, so either contradiction means a provider answered a chain unlike its own pack row. A failure a hint can cause (`ObjectMissing`, a provider `Missing`, an `Integrity` that is not a provider defect such as a malformed answer) on a chain holding a hinted locator evicts every hinted locator, turns hints off until the next demand and rebuilds once; a second failure stands, and a provider defect is never rebuilt. On a read, pooled leaves and program pages bind no member (their pool reader has no rebuild): their bases are located from authoritative rows. A chain walk level that has to locate also carries the deeper base ids that the chains of the bases it locates point at (`Fetch::chain_ahead`): member `k` gives the position of the level-`k` base, and the base id the record there names is read from bytes the pack and decoded-group caches already hold, which are only peeked (`PackCache::peek_group`, `GroupCache::peek`, `delta::read::cached_base`), as a guess for level `k + 1`. A chain whose base is already cached (bound from a member, or located earlier) is not followed: a chain a read binds binds its deeper bases with no locate. A group a guess needs decoded is decompressed into a decoded-group cache of that call's own (512 KiB, least recently used eviction, dropped after it) and reused while held there, so a level whose groups fit decodes each once; a body over 512 KiB is never kept and a level that interleaves more distinct groups than fit decodes an evicted one again. Every such decompression is counted in `prefetch_group_decodes`; presence checks set no locator-cache referenced bit. The guesses fill at most the rest of that locate's page of 4,096 ids, so they join only a locate the level makes anyway; a wrong guess's row is still cached, or remembered as absent, adding locator-cache pressure, and a guessed row whose authoritative descriptor contradicts a still-marked member descriptor makes the hint stale, as any authoritative row does. Nothing is trusted: their rows are authoritative, and every level still takes its base id from the record at its base's authoritative location, so a wrong guess (a stale member, a record that cannot be parsed) only leaves the next level to locate as before, with nothing counted stale. A program page's chain then costs the page's locate and one more for all its levels instead of one per level. A Save's demands, and the reads a Save makes of its lost registrations' winners, use chains only to prefetch groups, with one exception that keeps selection on authoritative locators: before a page's objects are admitted, the stored base whose chain selection may walk, when it came with a chain, is rebuilt with its members bound (for an ordinary object its first stored predecessor of an ordinary role whose cost the Save does not know; for a program page its first advisory predecessor when stored with the page's role, as `delta::branch::trial` takes it). The rule is wider than selection: it also checks the bases of objects this Save already holds privately or repeats within the wave (they never reach selection), and, for ordinary objects, of roles whose selection walks no base (file states, attributes, symlinks, filesystem roots, a whole file whose predecessor is a file state), so a base selection would not have read can still fail the Save when its failure is not a hint's; and the bound locators are kept as authoritative only after the rebuild hashed every element of the chain (`chain_verified_bases`). A hashed element's bytes are that object's, at a position only a catalog row can name, so selection walks the chain with no locate per level and charges the depth, canonical bytes and record bytes a reader walks; an element that fails its hash is never kept (the rebuild falls back once to authoritative locates, a counted stale hint). A program-page base (directory, extent or inode-table branch page; not a pooled leaf) is checked the same way by a pool reader of the Save's own, whose counts are discarded: its walk binds each member (`Fetch::binding_pages`, held only for this check) and hashes every element, base first, to the id its child's record names, with the canonical length the member gave. A member carries no role, so the hinted locator takes the root's and each hinted element's role is checked from its authenticated page header instead (`branch chain role`); its catalog row is not read. That reader has no rebuild, so a failure attributable to an element a hint located or reached through one (any element at or below the first hinted element, which an unauthenticated record named, being read, bound or decoded, for example a record there that names an element already walked; any element of a walk past the depth cap; or the chain work limit crossed anywhere in a walk with a hinted element, since its totals include the hinted elements' record and canonical lengths) evicts every hinted locator, counts a stale hint once, even when the walk already counted it (strict hints refuse) and leaves the chain to selection's own walk from authoritative locates, as it did before; any other failure (one at the authoritative root, for example a row whose length disagrees with its record) is the Save's own error, and strict hints do not turn it into `delta chain hint`. Without trusted chains (SQLite, or after a stale hint until the next demand) nothing is checked and the Save's calls are unchanged | [`read/hints.rs`](../../../../core/crates/layerfs-storage/src/read/hints.rs), [`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| Group reads (#758) | a demand is paged: each page is the longest leading run of consumers (a read's roots in physical order, a Save's objects in admission order) whose uncached groups and chains' groups fit one `read_groups` call of ≤ 2 MiB returned, with the cached groups the run needs counted too, so the pack cache keeps the whole page while the run is consumed; the next page is planned only then. A first consumer that does not fit alone has the packs that fit read. Ranges come from `GroupPlan::ranged` (whole only for a singleton or a pack within the 4,120 B control prefix, however sparse or many the ranges) where a directory is known (from the chain locate, or from the packs the handle published), else the whole pack; a pack that does not fit a call is acquired as before. `Ranges` answers pass through `AcquiredPackRead::acquire` over the directory and returned bytes, so framing checks are those of a scoped read, and every answer is admitted to the pack cache as it came (`PackCache::admit_groups`), with no second read; answers out of request order are `Integrity("group read reply")`. A pack only chain members name is a hint: a member contradicting a known descriptor is left out, and a `Missing` or `Malformed` call, or an answer that fails admission, for such a pack makes the hints stale (counted, off until the next demand) while the read goes on as before; a failure of a pack the rows themselves name stays hard | [`read/hints.rs`](../../../../core/crates/layerfs-storage/src/read/hints.rs) |
| Pooled chains (#768) | the pool reader's walk (pooled inode leaves and program pages, `encoding/pool`) prefetches like the ordinary walk: a level whose group is not held asks `prefetch_chain` first, so with batched chains the group, its members' and its local chain's groups come in one `read_groups`. Where the walk binds no member (a pooled leaf; a program page outside a Save's check of its bases), the locate of a level's base also carries the deeper base ids `chain_ahead` reads from the cached members (`Fetch::locate_ahead`), so a chain of depth 8 costs one group read and two locates instead of nine of each. Nothing new is trusted: located rows are authoritative and each level still takes its base from its record | [`encoding/pool/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/read.rs), [`read/hints.rs`](../../../../core/crates/layerfs-storage/src/read/hints.rs) |
| Local chains (#758) | when the provider's `batched_chain_reads` is true, the same group read also takes the chains the handle's own cache knows: the `base` of rows it published, followed through authoritative locators only (a lost row is invalidated, so its winner is located again) until a base is not cached or the record is FULL. A Save admits each slice a page at a time: the stored predecessors of the objects that are not stored themselves (an exact reuse reads none), with their chains. Default false (SQLite): nothing changes; PostgreSQL turns it on and answers `locate_chains` from stored ancestor lists ([PostgreSQL delta chains](../../engine/store/postgresql.md#delta-chains)) | [`read/hints.rs`](../../../../core/crates/layerfs-storage/src/read/hints.rs), [`save/wave.rs`](../../../../core/crates/layerfs-storage/src/save/wave.rs) |
| Strict hints | `Storage::set_strict_hints(true)`, for tests and benches only: a stale hint fails `Integrity("delta chain hint")` | [`store/handle.rs`](../../../../core/crates/layerfs-storage/src/store/handle.rs) |
| Read units | `GroupPlan` sees the directory, then takes the pack whole (singleton, small, wanted groups ≥ half or > 4 ranges, already kept) or coalesced ranges; scratch ≤ 2 MiB | [`read/units.rs`](../../../../core/crates/layerfs-storage/src/read/units.rs) |
| After a Save | the metadata pack bodies (2 MiB, oldest evicted first) and inode values (512 KiB) this handle's Saves published, added after each acknowledged publication, serve the next Saves' own reads and `Storage::reader_after_save` (#768); a plain `reader` still fetches. A Save that does not finish clears them | [`read/fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Kept caches | `Reader` for one operation; `ReadCache` and `Send`able `KeptReads` (3 MiB) across demands | [`read/provider.rs`](../../../../core/crates/layerfs-storage/src/read/provider.rs) |
| File lengths | `Reader::file_lengths`: whole files from locator length − 23 without payload; chunked or empty files read only their file state; this attests no payload | [`read/length.rs`](../../../../core/crates/layerfs-storage/src/read/length.rs) |
| Counters | `Storage::diagnostics`: cumulative counts per handle (port calls, acquisition choices, bytes, cache hits, `chain_*` and `group_read*`), never timings | [`read/counters.rs`](../../../../core/crates/layerfs-storage/src/read/counters.rs) |

### Ports

| `PackPersistence` method | Contract |
| --- | --- |
| `policy` | the persisted `StoragePolicy`, once per handle |
| `locate` | ≤ 4,096 distinct ids; absent ids omitted |
| `locate_chains` | `locate` plus, per located delta root, at most one `LocatedChain` (members nearest base first, each an optional id, `PackInfo`, group, record and canonical length, the last one FULL) and the `PackDirectory` of packs it names; default `locate` and no chains |
| `read_groups` | one answer per `GroupRead` in request order: `Whole(PersistedPack)` or `Ranges` (the bytes of each requested range, unauthenticated); ≤ 2 MiB requested per call; an empty range list asks for the whole pack; default `read_packs` per 2 MiB of whole packs |
| `read_packs` | whole packs ≤ 2 MiB in total, or one singleton, each a length- and SHA-256-checked `PersistedPack` |
| `read_pack_selection` | strict selected read: the whole pack is scanned and hashed, chosen ranges kept; default `BackendUnavailable` |
| `read_scoped_pack` | selected units, framing checked, not hashed; each used object is authenticated by its id; default `BackendUnavailable` |
| `inode_values` | ≤ 4,096 ascending ordinals in one statement |
| `signatures` | ≤ 8,192 signature-ring rows in insertion order |
| `reserve` | one block of pack ids and/or ordinals |
| `publish` | one atomic `Publication` (packs, locators insert-if-absent, inode values, signatures, released ordinals (since #768 the tail past the `KEPT_ORDINALS` (128) a keeping handle keeps, or alone, a handle's kept ordinals returned before its orderly drop)); each locator carries its record's logical delta `base`, which a provider may store in any form or ignore; returns `Published { lost }` |

[`port/persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs)
also has `whole_pack_bytes`, `batched_chain_reads` (default false; a provider
whose every read is a network round trip returns true), `publication_bytes`
and `publication_pack_cost`.
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
| Re-based delta (#758 fix 4) | a delta against a re-based chain root is kept only when its record is at most half the FULL record, so a chain's anchor reads at most the root's FULL plus half the target's FULL record (1.5 FULL records only when both are the same size; [encoding](encoding.md)) | `rebased_delta_kept` in [`delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs), [`delta/branch.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/branch.rs) |

Stored-byte savings cost read work: a 4 KiB logical read may need a whole
chunk, a complete group and several delta bases, and a whole-file object is
decoded whole before slicing. Neither reuse nor delta promises reads
proportional to the bytes requested.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Reads verify what they decode: every chain element is hashed against its locator's id | `Resolver::resolve_charged` in [`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| A chain hint costs speed, never correctness: it is checked against the record and the bytes, and a provider without chains (SQLite) sends exactly the statements it did before | [`read/hints.rs`](../../../../core/crates/layerfs-storage/src/read/hints.rs) |
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
| Disk full | `Refused { status: "Capacity" }`; `is_capacity_refusal()` | [`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs) |
| A declared bound refused | `CapacityExceeded { what, limit, actual }`; also `is_capacity_refusal()`, and `ProviderFailure` (not absence) to a provider read, never disk exhaustion | [`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs) |
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
| [`chain_hints.rs`](../../../../core/crates/layerfs-storage/tests/chain_hints.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test chain_hints` | any | A hinted chain is read with one locate and one group read; stale and misplaced members, and member packs that do not answer as named, fall back and are counted, or fail strict; malformed answers are refused and keep nothing; a provider defect is not replayed; a hinted locator serves one demand; program pages and pooled leaves bind no member on a read, yet a program page's chain below its first level is located in one locate from the bases its members' cached records name, and a member off the chain only brings back the per-level locates, with nothing stale; a chain a read binds adds no guess to another chain's locate, and the guesses count every decode and, when a level's groups fit the call's 512 KiB decoded-group cache, decode each group once; a Save binds members only to rebuild and hash its bases; a fresh-handle Save of a chained history, and of a directory whose pages are programs, sends one locate per wave and one group read per page, not a locate per base per level, and stores the default port's bytes; a misplaced member under a program base, or a hinted element whose page header declares another role, is a stale hint (strict refuses) and the Save stores the default port's bytes; a program base's own wrong row fails with its own error; a Save walks no chain selection does not walk; a hinted element whose record leads back to the authoritative root is a stale hint, counted once, and the Save stores the default port's bytes; an authoritative descriptor that contradicts a member's makes the hint stale and the read stays correct; a pack descriptor only a stale member gave goes with the hints; a group read is admitted with no second read; full rewrites send the default calls |
| [`rebased_delta_fraction.rs`](../../../../core/crates/layerfs-storage/tests/rebased_delta_fraction.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test rebased_delta_fraction` | any | A drifting whole file and a drifting extent page store FULL (whole) once a re-based delta would pass half the FULL record, and every version's read stays within the root's FULL, half a FULL and one one-version delta per further level; a re-based PREFIX that loses the comparison anyway is not counted in `rebased_full` |
| [`inode_branch_deltas.rs`](../../../../core/crates/layerfs-storage/tests/inode_branch_deltas.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test inode_branch_deltas` | any | Inode-table branch chains on a 50,000-inode table: edits within one leaf grow each chain to the metadata depth cap and re-base onto its root, keeping every program (no branch whole, `rebased_full` 0); the same table under spread edits stores both re-based root-branch programs whole (`full_branches` = `rebased_full` = 2) because each passes half the whole branch; every table reads back value for value and every branch page byte for byte |
| [`group_reads.rs`](../../../../core/crates/layerfs-storage/tests/group_reads.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test group_reads` | any | Ranged and whole answers, the per-call byte bound, request order, a cached range refusing its pack under another digest, corrupted ranges caught |

## Pages

| Page | What it covers |
| --- | --- |
| [Encoding](encoding.md) | `encoding/`: codec frames, FULL/STORED/PREFIX selection, chain decoding, pooled leaves and program pages, pack units and the pack cache. |
