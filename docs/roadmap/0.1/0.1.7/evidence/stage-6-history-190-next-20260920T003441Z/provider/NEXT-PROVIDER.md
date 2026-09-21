# Next provider-read target for #190

Status: source research only; no product/harness changes, builds, new timings or
benchmark runs. Reviewed working tree based on
`10b9d4a6cf9d88267d508cb010cc82950e080d77` while the completed parent-batching
change was being prepared for commit. All source references below are repository
paths with one-based line numbers. They describe the inspected source, not a
measured attribution or a speedup prediction.

## Result

The most specific next lead is **pooled inode-leaf reconstruction**, not opening
SQLite on each read and not an unbounded catalogue scan. The generic provider
already reuses its connection, decompression workspace and ordinary decoded-group
cache. Its pooled-inode branch instead creates a fresh `PoolReader` for **each
requested inode leaf**. That reader decompresses the physical record's group in
both the chain-discovery pass and the reverse reconstruction pass. Its bounded
pack and decoded-value caches are then discarded at the end of the leaf.

This is confirmed duplicate work in source. How much of the retained-history
provider time it consumes remains **NOT MEASURED**. In particular, existing
ordinary-lane `group_decodes` and `packs_read` counters omit this pooled branch,
so low values from those counters cannot falsify the lead.

The smallest safe next experiment is to count and time pooled record/group work,
then reuse the already-decompressed physical record group across the two passes,
under an existing declared byte bound. Extending pooled caches across the whole
operation is a later lever with visibility, mutable-pack and refusal-semantics
risks; it is not the first implementation recommendation.

## Existing measurements and their limits

The parent supplies these values from the completed optimization report. This
research did not rerun or independently rederive them:

| Selection | Filesystem ns | Provider ns nested within filesystem | Waves | Requested objects |
| --- | ---: | ---: | ---: | ---: |
| stride10 | 14,832,606,168 | 13,792,721,269 | 66,616 | 74,279 |
| stride3 | 53,702,386,711 | 52,197,491,759 | not copied here | not copied here |

Provider elapsed is overlapping, not additive. Requested-object counts are not
distinct-object counts, SQL query counts, pack-read counts or physical disk I/O.
The two elapsed figures do not select among SQL, pack copies, pooled
decompression, reconstruction, hashing or allocator costs.

## End-to-end path

```text
C1 asks for authenticated IDs
  |
  v
StoreProvider::read_wave
  check read_objects ceiling before opening anything
  reuse ReadSession (first call opens connection + decode workspace)
  |
  v
ReadSession::read
  query CURRENT retained_pack_ceiling for this wave
  |
  v
read_objects
  paged object-location SELECT (up to 128 IDs/query)
  reject found location above the captured ceiling
  build by-ID map; allocate wave-local compressed pack cache
  for EACH demanded ID:
    |
    +-- ordinary/non-inode leaf ------------------------------+
    |  Resolver: walk dependency edges                       |
    |    each edge -> singleton locator SELECT                |
    |    pack blobs cached within this wave, <=4 MiB          |
    |    ordinary decoded groups retained in session, <=512KiB|
    |  decode oldest base -> requested object                 |
    |  authenticate reconstructed bytes against each ID      |
    |                                                        |
    +-- InodeLeaf -------------------------------------------+
       NEW PoolReader FOR THIS REQUESTED LEAF
       walk chain: record() -> decompress physical group
       reverse chain: record() -> decompress same group again
       resolve pooled value ordinals through catalogue
       fetch/decompress/authenticate required value groups
       rebuild canonical leaf; hash against requested ID
       DROP pooled pack + decoded-value caches
  |
  v
return canonical bytes in original demand order
```

Source anchors:

- `core/crates/layerfs-storage/src/cas/provider.rs:109`: demand check, lazy
  session, read and public-counter propagation; `:156` and `:166`: C1 bridge.
- `core/crates/layerfs-storage/src/cas/read.rs:143`: session fields;
  `:179`: per-wave demand check and publication-ceiling query;
  `:61`: batched location query; `:69`: ceiling rejection;
  `:77`: wave-local pack cache; `:81`: per-ID resolver.
- `core/crates/layerfs-storage/src/sqlite/schema.rs:275`: one indexed policy-row
  query for the watermark. Pooling this watermark would change the declared
  between-wave visibility behavior (`cas/read.rs:139`).
- `core/crates/layerfs-storage/src/sqlite/lookup.rs:55`: bounded location pages,
  SQL text/parameter construction and `prepare_cached`; `:89`: singleton wrapper;
  `:148`: full pack BLOB SELECT copied into `Vec<u8>`.
- `core/crates/layerfs-storage/src/encoding/delta/read.rs:137`: pooled special
  case and fresh reader; `:170`: ordinary dependency singleton lookup;
  `:199`: ordinary dependency authentication;
  `:449`: bounded ordinary compressed-pack cache.
- `core/crates/layerfs-storage/src/encoding/decode.rs:104`: ordinary decoded-group
  cache hit avoids group decompression, but the enclosing resolver still needs
  the pack header/group view; the cache does not avoid every pack acquisition.

## Concrete repeated work

### A. Pooled physical groups decoded twice per chain element

`encoding/pool/read.rs:234` calls `record()` while discovering each dependency.
`:265` calls it again during reverse reconstruction. `record()` at `:334`
materializes the group every time, calling `decompress_group` at `:336` for
Zstandard groups. It has no decoded physical-group cache. It also copies the
selected record into a fresh vector at `:345`.

For a successful pooled chain of `d` edges, there are `d + 1` elements and exactly
`2 * (d + 1)` physical-record extractions on this path. Each extraction of an
element in a compressed group invokes group decompression. For a compressed FULL
pooled leaf (`d = 0`), that is two decompressions before value resolution. This
formula describes the code path; the corpus distribution of `d`, codecs and
shared groups has not been measured here.

The ordinary resolver already avoids this repeated group decoding through
`GroupCache`. Reuse that established discipline rather than inventing a new
unbounded canonical-object cache. A possible implementation shares the existing
decoded-group cache with pooled record extraction, preserving all framing and
canonical identity checks. It must also respect differing owner lifetimes and
mutable pack bodies in save callers.

### B. PoolReader caches discarded per requested leaf

`encoding/delta/read.rs:141` allocates `PoolReader::new()` and returns at `:157`.
The pack cache at `encoding/pool/read.rs:29` and decoded values at `:30` therefore
do not survive across sibling requested leaves, even within one `read_objects`
wave. This is shorter-lived than generic documentation calling the reader a
wave cache. Code ownership, not that comment, establishes the actual lifetime.

Repeated leaves can fetch the same complete pack BLOB and decode/authenticate the
same value group again. Cross-leaf cache reuse could eliminate this work, but
must first be counted and must preserve every current refusal and capacity.

### C. Ordinary pack cache is wave-local even when group decoding is pooled

`cas/read.rs:77` creates a new compressed-pack map each wave. The ordinary branch
still obtains the pack before consulting its decoded group (`delta/read.rs:217`,
`:263`, and `decode.rs:90`). Repeated singleton demands can therefore copy the
same pack BLOB from SQLite again while charging no new ordinary decompression.
SQLite's page cache means this is not proof of a disk read.

Do not simply move compressed packs into `ReadSession`: saved packs can be
rewritten as groups are appended. `encoding/pool/read.rs:51` documents the hazard;
save placement explicitly releases pooled pack bodies at
`core/crates/layerfs-storage/src/cas/placement.rs:181`.

### D. SQL lookup overhead remains, but some apparent scans are already bounded

Each wave rereads the publication ceiling and submits paged locator demand;
each dependency uses a singleton locator query. The locator query is already
prepared-statement cached (`sqlite/lookup.rs:70`). `pack_bytes` and `group_for`
use `query_row` directly (`sqlite/lookup.rs:150`, `sqlite/pool.rs:97`), so using
the existing prepared-statement cache is a small possible lever, but no elapsed
share is established for SQL preparation.

The pooled catalogue is not fully scanned for each ordinal: `sqlite/pool.rs:98`
selects the predecessor group with `ORDER BY first_ordinal DESC LIMIT 1`, and
`encoding/pool/read.rs:367` keeps the last covering catalogue row within the
leaf. A query is repeated when traversal returns to another group. Do not claim
one catalogue query per inode or a full catalogue scan without measured query
counts and query-plan evidence.

## Counter hole that must be closed first

The pooled early return at `encoding/delta/read.rs:137–157` updates only
`objects` and `canonical_bytes` for the requested leaf. It does not propagate
pooled pack fetches, dependency edges, chain depth, physical-group decompressions
or value-group decompressions. The outer resolver's `packs_read` remains zero
for that branch. Existing ordinary `group_decodes` is accurately named for the
ordinary decode path, but does not cover all physical groups in Ordinary packs:
pooled leaves stored in that lane take the separate path above.

`ReadCounters.pages` (`cas/read.rs:64`) counts only root demand locator pages;
it excludes dependency locator, catalogue, ceiling and pack queries. It is not
a total SQL-query count or an actual SQLite B-tree page count.

Recommended diagnostic outputs, collected at the actual work site:

| Counter/span | What it resolves |
| --- | --- |
| Requested role and repeated object IDs per operation | Whether inode leaves dominate requests; no assumption from total waves |
| Pooled leaf count, chain edges and physical-record extraction count | Test `2 * (leaves + edges)` for successful complete traversals |
| Pooled physical-group decompressions and decoded bytes | Work omitted from existing ordinary `group_decodes` |
| Pooled value-group decodes/authentication, cache hits and evictions | Quantify the per-leaf cache-lifetime loss |
| Pack BLOB fetch count and copied bytes, ordinary versus pooled | Distinguish SQL acquisition/copy from logical object demand |
| Locator, catalogue, ceiling and pack-query elapsed/calls | Attribute query/setup work without calling it disk I/O |
| Reconstruction and identity-hash elapsed | Distinguish codec, copy/apply and authentication CPU |

Use disjoint timers inside provider elapsed and retain an explicit residual.
Record instrumentation overhead, feature/environment state, source/harness
identities, workload pins and cache eligibility. External sampling can identify
hot functions without a private product test hook; exact work counters should be
ordinary product telemetry if added, not test-only branches in production.

## Bounds and correctness constraints

| Resource | Existing limit / owner |
| --- | --- |
| Locator page width | 128 IDs, `policy.rs:69` |
| Compressed dependency pack cache | 4 MiB body bytes, `policy.rs:119`; clear all on overflow |
| Decoded ordinary groups | 512 KiB body bytes, `policy.rs:143`; operation-owned on the provider path despite older wave-only comments |
| Decoded pooled values | 512 KiB value bytes per reader, `policy.rs:127`; clear all on overflow |
| Pooled decoded work | 32 MiB per chain, `policy.rs:145` |
| Default metadata depth | 8, `policy.rs:147`; caller policy may differ |
| Metadata canonical / encoded chain budgets | 65,536 / 139,281 bytes, `policy.rs:149–151` |

Never expand these budgets or hide extra simultaneous cache copies. Extending
`PoolReader` lifetime is not automatically harmless: `load_group` returns on a
cache hit before adding decoded-work charge (`pool/read.rs:145–153`). A broader
cache can therefore alter which work-limit refusals occur. Test exact refusal
parity, including warm-versus-fresh sequences; do not equate successful output
equality with unchanged resource behavior.

Visibility must precede cache use (`pool/read.rs:139`); the publication ceiling
must remain per wave; and compressed pack invalidation on writes must remain.
Do not remove locator, digest, chronology, framing or canonical identity checks.

## Ranked next actions and checks

1. **Measure pooled physical-group decoding first**, including the missing work
   counters above. This is the most concrete avoidable operation in source.
2. **Reuse bounded decoded physical groups across the two pooled chain passes**
   using the existing cache approach. A focused fixture must read one compressed
   FULL pooled leaf and a multi-element chain, prove equal canonical bytes and
   errors, and demonstrate fewer actual decompression calls. No policy/default
   change and no new dependency are needed in principle.
3. **Only if measured significant**, reuse pooled authenticated value groups
   between sibling leaves. Check per-chain work/refusal parity and ceilings
   before widening lifetime beyond one wave.
4. **Only if SQL/pack copy dominates**, investigate statement reuse or safe pack
   acquisition reuse. Raw pack caches cannot be retained across writes merely
   because object IDs are immutable.

Existing regression homes are
`core/crates/layerfs-storage/tests/metadata_pool.rs` (corrupt groups `:240`, missing
catalogue `:263`, unfinished group visibility `:294`, chain work refusal `:572`,
50-link chain `:677`, cache bound `:764`, pack invalidation `:847`),
`tests/visibility.rs:358` and `:500`, and `tests/provider_errors.rs:65` and `:115`.
New checks belong in external tests; no private source inclusion or product
test-only switches. The same public read path must be exercised.

After correctness, use one fresh diagnostic baseline and one candidate on
stride10 with identical instrumentation; confirm stride3 only after a mechanism
reduces actual work. Keep original wall/cold/worker rules and report any unrun or
ineligible row. This document does not authorize a timer exception, claim release
admission, or turn prior measurements into a new sample.

Production changes in this squad: none. Production LOC delta: 0 (no commit made;
this statement is not an exact-snapshot commit LOC receipt).
