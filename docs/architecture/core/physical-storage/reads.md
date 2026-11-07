# Reads

[Architecture](../../README.md) › [Core](../README.md) › [Physical storage](README.md) › Reads

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Locating and fetching stored objects.

| | |
| --- | --- |
| Component | [Physical storage](README.md) |
| Source | [`core/crates/layerfs-storage/src/read`](../../../../core/crates/layerfs-storage/src/read) |

A `Reader` (from `Storage::reader`) serves one operation and implements
`layerfs-content`'s `AuthenticatedObjects` trait. One demand:

```text
 read_objects(ids)          <= 4,096 ids, <= 32 MiB canonical in total
   |
   1 locate      ids --> ObjectLocation (cache, then PackPersistence::locate)
   2 prefetch    level by level: acquire groups, read each record's base id,
   |             locate the bases; until no base remains
   3 schedule    sort roots by (pack, group, record)
   4 fetch       per pack: PackCache::acquire_groups(missing groups)
   5 decode      Resolver::resolve_at(root): walk chain, decode base first,
   |             hash each object and compare with its id
   v
 canonical bytes, in demand order (repeated ids keep their slots)
```

| Bound | Value | Failure | Source |
| --- | --- | --- | --- |
| Ids per demand | 4,096 (`READ_OBJECT_LIMIT`) | `CapacityExceeded { what: "storage.read_objects" }` | [`objects.rs`](../../../../core/crates/layerfs-storage/src/read/objects.rs) |
| Canonical bytes per demand | 32 MiB (`READ_CANONICAL_BYTES_LIMIT`) | `CapacityExceeded { what: "storage.read_canonical_bytes" }` | same |
| Unknown id | — | `ObjectMissing(id)` | same |
| Prefetch levels | `MAXIMUM_DELTA_MAX_DEPTH` + 1 | `Integrity("prefetch dependency depth")` | [`prefetch.rs`](../../../../core/crates/layerfs-storage/src/read/prefetch.rs) |

## Locating an object

```text
 Fetch.locate(ids)
   +-- in LocatorCache or absent set? --> hit (no port call)
   +-- otherwise, pages of <= 4,096 ids --> PackPersistence::locate
          each row checked, its PackInfo remembered, then cached
          ids not returned --> absent set (cleared at each demand)
```

| Check on each locator row | Source |
| --- | --- |
| No more rows than ids asked; each id asked once and returned at most once | [`fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| `pack_id` positive and equal to the row's pack descriptor | same |
| Canonical length ≤ 16 MiB, group < 256, record < 8,191 | same |
| Pack descriptor: length 32 bytes to 16 MiB + 4 KiB; a metadata pack ≤ 256 KiB | `check_info` in same |

| Cache | Bound | Release | Source |
| --- | --- | --- | --- |
| `LocatorCache` (positive) | 4,096 entries | second-chance victim selection that skips the current demand's ids (searched sorted, so a probe costs a logarithmic search even when the demand is larger than the cache); after a publication the locations the Save won are kept (they are the Store's winners) and lost ones are invalidated and located again | [`locator_cache.rs`](../../../../core/crates/layerfs-storage/src/read/locator_cache.rs), [`fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Published bodies and values | 2 MiB of metadata pack bodies (the published `Arc`s), 512 KiB of pooled inode values | the next Save clears them; oldest first at the bound | [`fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |
| Absent set (negative) | 4,096 ids | cleared at the start of each demand, and cleared whole at its bound | [`fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs) |

Only a reader from `Storage::reader_after_save` reads the published bodies and
values: it continues the latest Save's operation (a Commit's logical count and
binding hints, layerfs-project #68). Every other reader acquires every body it
needs. Everything either returns is authenticated the same way.

`base_location` drops a negative mark before it asks again. A base named by a
stored record is always stored, and another writer may have published it after
the mark was taken (F-036).

## Fetch and prefetch

| Port call | Used for | Integrity | Source |
| --- | --- | --- | --- |
| `read_scoped_pack` | `acquire_groups`: the groups a demand needs | Whole: length and SHA-256 checked. Units: framing checked, bytes not hashed; each used object is authenticated by its id. | [`fetch.rs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs), [`port/read_scoped.rs`](../../../../core/crates/layerfs-storage/src/port/read_scoped.rs) |
| `read_packs` | whole metadata packs and payload packs by id | length and SHA-256 checked by `PersistedPack::authenticate`, then header, declared length and domain | same, [`port/read_pack.rs`](../../../../core/crates/layerfs-storage/src/port/read_pack.rs) |
| `read_pack_selection` | strict selected reads: the whole pack is scanned and hashed, only chosen ranges are kept | SHA-256 over every byte | [`port/read_selection.rs`](../../../../core/crates/layerfs-storage/src/port/read_selection.rs) |

A demand is served one cohort of packs at a time, in physical order: as many
of its packs as fit the 2 MiB pack cache together by their located lengths
(at least one; [`Fetch::cohort`](../../../../core/crates/layerfs-storage/src/read/fetch.rs)).
Each cohort is prefetched and resolved before the next is acquired, so a pack
a large demand acquires (a base read-ahead window, layerfs-project #97) stays
cached until its objects are resolved and is not acquired a second time. A
demand within the bound is one cohort.

When the provider takes packs whole (`whole_pack_bytes` above the control
prefix, as PostgreSQL does), each cohort reads its uncached packs that are no
larger than that and fit the 2 MiB pack cache together in one `read_packs`
call, if there are at least two, before acquiring groups
([`Fetch::whole_packs`](../../../../core/crates/layerfs-storage/src/read/fetch.rs));
the cohort's kept packs are reserved first so the read does not evict them.

The reply must match the plan: the same descriptor, the same choice (whole or
ranges) and the same extents, else `Integrity("selected strategy/extent reply")`.
A provider that does not implement a selected read returns
`BackendUnavailable`; there is no fallback to a whole read.

Prefetch ([`prefetch.rs`](../../../../core/crates/layerfs-storage/src/read/prefetch.rs))
walks every root's chain of a cohort one level at a time, pack by pack, before
any of its objects is decoded. Discovery and decoding share the operation's pack and group caches.

## Read units

`GroupPlan` ([`units.rs`](../../../../core/crates/layerfs-storage/src/read/units.rs))
sees the pack's control area and directory, then chooses before body I/O:

| Choice | When | Diagnostics counter |
| --- | --- | --- |
| Whole | singleton lane | `whole_due_singleton` |
| Whole | pack length ≤ the provider's `whole_pack_bytes`: 4,120 bytes (`PACK_READ_PREFIX_BYTES`) by default, the 256 KiB pack limit on PostgreSQL, where each ranged read is a round trip | `whole_due_small` |
| Whole | wanted groups cover at least half the pack, or need more than 4 ranges | `whole_due_density` |
| Whole | this owner already kept another group of the pack | `whole_due_reuse` |
| Ranges | otherwise; adjacent groups coalesce into one range | `range_selected` |

Wanted groups must be strictly ascending and ≤ 4,096. The ranges, prefix and
scan scratch together stay within 2 MiB (`DEPENDENCY_PACK_CACHE_BYTES`).
Selected bytes become `PackUnit`s; see [Encoding](encoding.md#pack-units).

Caches a caller can keep across demands
([`provider.rs`](../../../../core/crates/layerfs-storage/src/read/provider.rs)):

| Type | Holds | Bound |
| --- | --- | --- |
| `Reader` | decode arena, pack cache, decoded-group cache, pooled reader | one operation |
| `ReadCache` | the same, kept by the caller | 2 MiB + 512 KiB + 512 KiB, plus one isolated singleton |
| `KeptReads` | the caches without the arena, so it is `Send` | `KeptReads::ORDINARY_BYTES` (3 MiB) |

`Reader::file_lengths` answers file sizes without reading payload: a whole
file's length is its locator's canonical length − 23; a chunked or empty file
reads only its small file-state object
([`length.rs`](../../../../core/crates/layerfs-storage/src/read/length.rs)).

## Counters

`Storage::diagnostics` returns `Diagnostics`, cumulative operation counts per
handle. They are counts, never timings
([`counters.rs`](../../../../core/crates/layerfs-storage/src/read/counters.rs)).

| Group | Fields |
| --- | --- |
| Port calls | `policy`, `locate`, `read_packs`, `read_pack_selections`, `value_groups`, `signatures`, `reserve`, `initial_reservations`, `reservation_refills`, `ordinal_reservations`, `publish` |
| Acquisition | `whole_selected`, `whole_due_*`, `range_selected`, `range_scan_bytes`, `range_acquired_bytes`, `range_materialized_bytes` |
| Bytes | `payload_reads`, `payload_read_bytes`, `pack_read_bytes`, `pack_write_bytes` |
| Locator cache | `locator_hits`, `locator_misses`, `locator_evictions`, `locator_eviction_probes`, `locator_second_chances`, `locator_bookkeeping_live_peak_bytes` |
| Pack cache | `pack_hits`, `pack_misses`, `pack_evictions`, `pack_evicted_bytes`, `directory_validations`, `directory_unshared_walks`, `directory_entry_bounds` |
| Other | `prefetch_group_decodes`, `forced_seals`, `pooled_groups`; `pooled_packs` and `reserved_directory_bytes` are always zero and kept for the schema |

Per-chain work is `ChainCounters`
([`encoding/delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs)),
and pooled work is `Reader::pooled_read_counters`.

## Related

- [Encoding](encoding.md) — the resolver and record decoding.
- [Packs](packs.md) — pack layout and the pack cache.
- [Ports](ports.md) — the read ports and their carriers.
- [Objects: Access](../objects/access.md) — the authenticated read contract.

---

← [Physical storage: Saves](saves.md) · [Up: Physical storage](README.md) · [Physical storage: Ports](ports.md) →
