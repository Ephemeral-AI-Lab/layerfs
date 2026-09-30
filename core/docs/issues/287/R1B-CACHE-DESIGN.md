# R1b mapping cache admission and current-batch ownership

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Read-only design, 2026-09-30. R0 parent:
> `1d2fc8c2987a46acb906a1b9e720bb4cac116c7e`.
> The inspected product source is unchanged from its first parent
> `7edddbdb8e8512627aed0ed42533ef099d802384` in the cache paths below.
> No product edit, build, test or benchmark was performed for this review.

This note covers the smallest complete mapping-cache admission/correctness path
inside R1b. Root owns the later C1 indexed-state port and C2 metadata scratch
integration. R1a is being implemented independently; this note does not touch
its admission/catalog files. SC-01, SC-02, SC-04, SC-06 and SC-07 require correct
bytes and selected roots, bounded resident ownership and explicit refusal. Wider
mapping populations and Server/SQLite/physical resource composition remain
separate required gates in [the R0 resource audit](R0-RESOURCE-AUDIT.md).

## Source trace and failure mechanism

The public surface is
[mapping exports](../../../crates/layerfs-content/src/file/mapping/mod.rs#L20),
lines 20–23: `PageCache`, `RangeCursor`, `ReadCounters`, navigation constants and
the mapped range reader. No test-only visibility change is needed.

| Boundary | Exact source observation at R0 | Consequence |
| --- | --- | --- |
| Construction | [PageCache](../../../crates/layerfs-content/src/file/mapping/read.rs#L93), lines 93–111 | Derived `Default` sets `limit=0`; `new()` selects 64; `bounded(0)` clamps to one. These are different policies today. |
| Retention | [insert/make_room_for](../../../crates/layerfs-content/src/file/mapping/read.rs#L119), lines 119–137 | `insert` does not check count, capacity, role width or table growth. Caller-controlled `make_room_for` is the only eviction check. |
| Read batch | [Wave::pages](../../../crates/layerfs-content/src/file/mapping/read.rs#L184), lines 184–225 | It computes `missing` from current cache hits, may clear the cache, fetches only `missing`, clones fetched pages into cache, then retrieves every requested page from cache. |
| Edit insertion | [EditObjects::load_node](../../../crates/layerfs-content/src/file/edit/tree.rs#L159), lines 159–190 | A fetched canonical vector is cloned into cache before decoding; this path never calls `make_room_for`. Its other draft/reference maps have independent accounting gaps. |
| Cache sharing | [apply_edits](../../../crates/layerfs-content/src/file/edit/apply.rs#L62), lines 62–75 and 132–139; [comparison cursor](../../../crates/layerfs-content/src/file/edit/compare.rs#L47), lines 47–58 | One cache spans comparison and construction. Correct admitted entries should remain reusable across those phases. Eviction can cost reads, but cannot change bytes/roots. |
| Current traversal | [range cursor](../../../crates/layerfs-content/src/file/mapping/read.rs#L397), lines 397–426 | Every ascending segment traverses from the immutable root, passing the same caller cache. Output coverage is checked exactly. |
| Canonical node bound | [decoder](../../../crates/layerfs-content/src/file/mapping/codec.rs#L170), lines 170–177, 204–221, 245–268 | Canonical length is checked against 8,192 bytes before decoded vectors. Decoded leaf/branch entry capacities are bounded by the checked wire shape and page partition. This does not bound the incoming Vec's spare capacity. |

Two source-derived witnesses distinguish the problems. They are deterministic
public-API test designs, not claimed executed reproductions:

1. Prepopulate a 64-page cache with the mapping root, one page needed by the next
   32-page leaf wave, and 62 other valid immutable pages. The root demand is a
   hit. The leaf wave records one hit and 31 misses. `make_room_for(31)` clears
   all 64 entries; the provider returns only the 31 misses. Final lookup of the
   formerly cached hit returns `MissingObject`, despite a valid provider/tree.
2. With `PageCache::bounded(1)`, a 32-page wave currently inserts all 32 pages
   after a single pre-batch clear. Adding eviction to each `insert` alone would
   drop the first pages before final lookup and create the same `MissingObject`.
   It would fix resident count while breaking legal small-cache traversal.

The common cause is that the cache is simultaneously treated as disposable
retained state and as the sole owner of pages needed by an in-progress batch.
A cache hit that will be evicted must first gain its current-batch owner. Neither
another point query nor a larger minimum cache resolves the ownership contract.

## Smallest complete replacement

Keep the canonical codecs, 32-page grouped navigation, root/non-root cache keys,
and ordinary 64-page default. Change the owner boundary in three focused places:

1. Put cache state/admission in a focused `file/mapping/cache.rs`, reexported
   through the existing small `mod.rs`. `Default` delegates to `new()`;
   preserve `bounded(0)`'s existing one-page meaning. A retained entry is admitted
   inside insertion, including replacement and real incoming Vec capacity.
   Eviction remains deterministic wholesale eviction. Never retain more than
   the caller's requested page count. Do not silently increase a limit below 32.
2. `Wave::pages` owns a bounded current-batch set before altering retained cache
   state. Snapshot hit bytes into that owner, collect the unique missing keys,
   make one grouped provider call, check exact cardinality, and move its returned
   vectors into the batch. Validate every mapping page's width/context before a
   retained copy. Cache retention is a subsequent independent admitted action.
   The returned demand order and traversal decode borrow from the batch owner;
   they never depend on the cache retaining all current pages.
3. `EditObjects::load_node` decodes/checks the fetched page and summary before
   retention, then uses the same insertion admission. The current local fetch
   and decoded node remain owned through the call even if cache eviction occurs.
   It may transfer the fetched canonical allocation into cache after decoding
   rather than clone it if no later caller needs those exact bytes.

For the first implementation, returning a bounded `Vec<Vec<u8>>` in demand order
is sufficient and keeps the existing internal traversal call shape. A page used
twice in one batch may share one immutable owner with typed references if this
is implemented coherently; do not introduce that extra representation merely
to name the fix. At most 32 requested navigation entries are involved. Deduplicate
the same `(ObjectId, root_context)` key while retaining demand-order/cardinality.
Root context remains part of the key because the decoder enforces different
partition rules in that context.

Take all hit snapshots before any insertion or pre-batch eviction. Then every
miss is either moved into its checked batch slot or the operation returns the
provider/cardinality/codec failure. Emit no requested leaf bytes from a batch
whose navigation-page validation has failed. Never clear and then rediscover a
hit by an unplanned reread; never rescue a miss with a point-query loop.

Select a fallible checked `insert` that returns
`BoundedCapacityExceeded`/`ResourceUnavailable` when the supplied owner cannot
fit; update legitimate callers together. `make_room_for` may remain as a
prospective eviction hint, but correctness/admission must not depend on a caller
remembering it. Check canonical length and actual incoming Vec capacity before
retention, and reserve table/growth and copied canonical capacity before they
are allocated. Replacing an existing key still checks its incoming owner and
clone overlap. An incoming invalid canonical page is a codec failure, never
silently cached or repaired. Both read and edit routes use this one policy.

Document cache reuse as **reuse while the page remains retained**. Current
comments promising one demand for the union of every range's paths overstate
finite-cache behavior. Eviction legitimately causes future grouped reacquisition;
neither data correctness nor a million-page no-reread promise follows from 64
retained entries. Preserve existing small unaffected canonical/reference cases
and their real call counts.

## Capacity accounting and exact limits

Count and canonical length alone are not an exact cache memory bound. The
current `HashMap<(ObjectId, bool), Vec<u8>>` simultaneously owns:

- Each canonical Vec's **capacity**, including spare retained bytes.
- Hash-table bucket/control capacity and alignment, which outlive `clear`.
- The key/value descriptors and HashMap state. `HashMap::capacity()` reports
  entry capacity, not a safe exact allocation-size API for the underlying table.
- Current-batch canonical owners, temporary demand/miss/output vector capacities,
  and the page currently decoded into its bounded leaf/branch entry vector.
- A clone under construction beside the old cache allocation or replacement;
  release credit only after the old owner is actually dropped.

The cache's old count bug can grow table high-water through the edit path. Once
insertion enforces a fixed count from creation, table high-water is bounded by
that count, but its byte charge still needs an explicit method. Retain HashMap
initially to avoid changing the existing lookup algorithm during the owner fix.
For the standard 1..64-page class, admit a conservative fixed **16 KiB cache
table/descriptor allowance before table growth**, and validate it against the
exact pinned Rust 1.85.1 standard-library/target real-allocator allocation shape
at handoff. This is a proposed allowance; this read-only note does not assert
that the current compiler/container allocation has been measured inside it.
If that proof fails, keep the strict class unsupported and revise the declared
representation/profile before another proof. Do not hide excess behind canonical
length or invent a HashMap allocation multiplier.

Use the role's 8,192-byte canonical maximum for cache payload owners only after
checking actual retained capacity. The 64-page encoded payload allowance is
`64 * 8,192 = 524,288 bytes`; a 32-page current-batch allowance is at most
`32 * 8,192 = 262,144 bytes`, provided its incoming capacities are independently
admitted. Their combined encoded capacity is 786,432 bytes, before the proposed
16 KiB table allowance, batch descriptors and decoded node/transient copies.
Decoded entry capacity uses actual `size_of::<ExtentSlice>()` or
`size_of::<ChildDescriptor>()`, Vec capacity, enum/stack context and simultaneous
encoded owners. Do not label 8 KiB of wire bytes as 8 KiB of decoded memory.

The present implementation transiently has three copies of fetched page bytes:
the provider `values`, clones retained in cache, and clones made by final cache
lookup. Moving provider vectors into the current batch removes one of those
copies. Admission still includes the retained copy and any demand duplicates.
For cached hits, the retained original and batch copy overlap. Do not count a
moved allocation twice; do not return its credit at the end of a provider call
while the batch/cache owns it.

The current authenticated-provider contract
([access.rs](../../../crates/layerfs-content/src/object/access.rs#L23), lines
23–51) promises identity, demand order and cardinality, but accepts no caller
capacity permit and says nothing about each returned Vec's capacity. Therefore
a length check after provider return cannot prove upfront total acquisition
admission. The strict R1b/R1c composition requires the frozen
`ReadWindowPermit` extension at that genuine supplied-I/O boundary. C2 must check
locator/role lengths and provider output/decode/pack demand before its allocations;
the returned owner carries its existing charge to C1. Existing independent
providers retain their explicit compatibility scope and cannot be described as
strict Server byte-admitted providers by this cache change.

Limits outside the first standard cache class remain explicit:

- 1..64 retained pages and 32 current navigation requests are the standard class.
  `bounded(L>64)` is an existing independently requested compatibility shape;
  it requires separately declared metadata/payload arithmetic before strict
  Server use. Do not silently clamp it to 64 or increase its byte pool.
- Incoming canonical length above 8,192 is rejected by the mapping codec before
  decoded allocation/caching. Excess **capacity** with legal length has separate
  owner/permit handling; it is not a canonical format error.
- This slice does not bound the resident traversal `level`/`next` frontier,
  draft/parent/committed maps, full directory binding vectors, external graph
  state, SQLite native heap or OS file cache. Paged integration remains required.
- A safe cache-count/Vec requested-capacity proof is not allocator metadata/RSS
  or physical-provider containment proof. No native-engine/physical PASS follows.

## External public-API proof

Name the cohesive checkpoint **R1b-cache: current navigation ownership and admitted
retention**. Its tests must prove real behavior, not source spelling or a private
`len()` test hook. Use only public mapping/C1/Store interfaces and external
delegating observation. Do not change product visibility for testing.

1. **Mixed cached hits at eviction.** Build a valid immutable mapping with at
   least 65 distinct leaf pages through public canonical builders. Use independent
   deterministic logical bytes. Prepopulate the public cache with its valid root,
   one first-wave leaf and 62 other valid pages. Read the 32-leaf range with
   `RangeCursor`; require exact complete output, exact coverage and one grouped
   missing-page demand. The earlier source-derived failure must disappear without
   a second request for the evicted hit.
2. **Cache smaller than a batch.** Use limits 1 and 31, then 32 and 64, over the
   same valid wide mapping. Require a 32-page navigation batch where that shape
   demands it, byte-exact output and exact ascending follow-up segments. Enumerate
   `get(id, context)` over the fixture's known mapping identities after segments
   and require no more than the declared retained count. A smaller limit may cost
   later reads; it must not cause `MissingObject`, silently retain 32, or degrade
   grouped acquisition into 32 point calls.
3. **Default and local-edit route.** Use `PageCache::default()` through the public
   cursor and an ordinary `apply_edits` case touching more than 64 distinct old
   mapping pages. Require exact independent final bytes/root/partitions and old
   bytes. Preserve the pre-existing independent v1 sealed reference cases in
   [edit_reference.rs](../../../crates/layerfs-content/tests/edit_reference.rs#L664),
   lines 664–707. The wide case verifies insertion coverage through both comparison
   and localized construction, without relying only on manual cache insertion.
4. **Owner/refusal behavior.** Exercise legal canonical bytes with admitted and
   excess owner capacity at the real supplied-provider boundary, replacement of
   a retained key, malformed/oversized navigation bytes and wrong cardinality.
   Require the precise selected failure, no invalid retained entry and no planned
   data output before failed-batch validation. No retry or allocation-error
   fallback. Test global refusal before the provider's dependent allocation once
   `ReadWindowPermit` is implemented; until then this specific claim is unrun.
5. **Real resource/provider composition.** Put the storage-backed variant under
   `layerfs-storage/tests/`, where the existing dependency on C1 permits real
   `StoreProvider` use without a new dependency. Delegate authentication unchanged
   while externally observing grouped demand IDs/order. Wrap the real system
   allocator only to observe actual requested capacity, not to fake/fail
   allocations. Admit and report fixture/oracle/output ownership separately.
   Verify table/Vec/decoded/clone overlap against the declared method, then
   checked final release. Record omitted allocator/native/cache domains honestly.

Public C1-only authenticated in-memory proofs retain that narrower provider
label; they do not substitute for the real Store resource proof. The existing
deep-tree range test at
[file_read.rs lines 737–793](../../../crates/layerfs-content/tests/file_read.rs#L737)
provides a useful unchanged navigation correctness reference. Its tested shape is
not evidence that cache crossing or actual provider byte admission passes.

Run the meaningful frozen covering tests once after implementation; diagnose any
red result from its output/source before the demonstrated correction and covering
commands. Include product boundary/self-tests, locked fmt/Clippy/owning checks at
the coherent handoff. Do not build/test concurrently with R1a ownership here.
No verification command was executed for this design and no performance or
resource qualification result is reported.
