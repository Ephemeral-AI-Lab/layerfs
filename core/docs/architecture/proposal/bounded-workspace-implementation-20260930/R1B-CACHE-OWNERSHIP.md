# R1b-cache current navigation ownership and bounded retention

> **Status: Current general guide.**
> Audited against parent `765202c45e11b3c96b2b16c40b35e810a7270d34`,
> 2026-09-30. This note changes with its owning source. Actual checkpoints,
> covering outcomes and limitations are recorded in the
> [append-only log](../../../issues/287/IMPLEMENTATION-LOG.md).
> No benchmark, whole-Server physical memory or release admission follows from
> this cache subdelivery.

The source audit in [R1B-CACHE-DESIGN](../../../issues/287/R1B-CACHE-DESIGN.md)
identified an ownership error: a navigation batch recorded its cache hits,
evicted them while making room, fetched only its misses, then looked every page
up in the changed cache. A valid mixed-hit batch could return `MissingObject`.
The edit path also inserted without the caller-required eviction hint, and
derived `Default` selected a different limit from `new()`.

## Retained pages and current batches

`file/mapping/cache.rs` owns retained mapping bytes keyed by immutable ObjectId
and root context. Public insertion is fallible: it checks canonical width,
actual incoming Vec capacity and context-valid mapping grammar, and retention
itself enforces the requested count. All insertion paths share the same owner
boundary. A small private-field `CheckedPage` carries immutable codec facts to
internal retention, which accepts that checked owner rather than raw Vec bytes.
The caller/provider continues to supply identity authentication.

`new()` and `Default` select64 pages. `bounded(0)` retains its existing one-page
meaning; positive counts select that count, including counts below the32-page
navigation wave. Caller-selected counts above64 retain their compatibility scope
and do not inherit the standard class's byte qualification.

Each navigation call owns at most32 requested canonical pages. Every retained
hit gains a current-batch copy before any eviction. Missing identities are
deduplicated and acquired in one grouped provider call, with exact cardinality;
the returned allocation moves into its first demand, and duplicate demands get
checked copies. The complete batch's capacities, context grammar, levels and
selected parent byte/extent summaries are checked before retained-cache changes
or that batch's payload emission. No point-query rescue repairs an evicted hit.

Option/result descriptors overlap during transfer, while canonical byte owners
move rather than forming a second full batch. Decoder scratch is bounded per
page; traversal subsequently decodes the canonical page for its actual entries.

The retained cache can then evict independently. New copies use fallible exact
reservation; existing retained entries can be reused. Returned demand order and
traversal rely on the current batch, so limits1 or31 preserve the32-page grouped
acquisition and byte result. Eviction can require a later grouped reacquisition;
the finite cache does not promise one demand for an arbitrary union of paths.

The edit route decodes the fetched mapping page and checks its expected summary
before retention. It transfers the canonical allocation after that check rather
than making the old redundant fetched-page clone. Existing v1 mapping/FileState
formats, IDs and partition rules remain unchanged. Parent summaries now validate
selected child totals as well as the root. This checks required pages; it does
not eagerly authenticate an unrequested subtree or buffer a whole read before
output. A later wave may fail after an earlier validated prefix was emitted.

## Exact resource scope

The retained payload allowance in the standard class is
`64 * 8192 = 524288` bytes of actual canonical Vec capacity. A32-page current
batch owns at most262144 such bytes. Table/bucket/control storage, Vec
descriptors, decoded entries, transient copies and traversal/output/provider
ownership are separate simultaneous terms. The prospective16KiB retained-table
allowance must be checked against the actual pinned Rust/target allocation shape;
HashMap entry capacity alone does not give its byte size.

The owning resource test delegates to the real System allocator and observes
requested allocation layouts/deallocation. It never substitutes or fails an
allocation. Exact-size attribution and selected-thread scope are explicit in
its output. Fixture/reference/trace/output ownership is separate. This method
does not measure allocator overhead, stack, SQLite/zstd C heap, RSS, file cache
or kernel memory, and cannot establish physical containment.

At the owning Darwin/aarch64 Rust1.85.1 check, the64-page prefill held328496
encoded bytes and8328 table/other bytes. Peak table/other requests were12496
bytes, peak decoded-size requests5120, and total simultaneous peak341944.
All captured owners released to zero after cache drop; the stack cache
descriptor was56 bytes. The real StoreProvider read captured a1966029-byte
simultaneous Rust-request peak, including its opened session, batch, cache and
decoder work. After provider drop, captured live ownership was10328 encoded
plus8328 table bytes; cache drop released all of it. Exact-size classes can
include other roles, such as a128-entry payload-demand Vec with the same layout
as decoded extents. These are finite requested-allocation facts, not native or
physical bounds. Raw output/method and all exclusions are in the checkpoint.

The existing authenticated-provider interface returns allocations before a
caller `ReadWindowPermit` exists. Checking capacity at C1 retention prevents an
oversized owner from entering this cache; it cannot prove upstream allocation
refusal. Supplied-I/O/global byte admission remains an open R1 gate. Frontier,
draft/parent/finalized maps and exact graph populations still require paged
integration. This change advertises no strict Server memory capability.

## Owning correctness and later qualification

The independent finite fixture directly encodes a valid v1 branch with65
distinct128-entry leaves and30 additional immutable pages. Its logical bytes,
profile and partitions are determined before candidate reads. The mixed prefill
selects a root, one next-wave leaf and62 other pages. Exact output and grouped
missing-ID demand must pass without rereading the evicted hit. Limits1/31/32/64,
Default and ordinary edits use the public C1 interfaces; the Storage variant
uses real Store/StoreProvider and its normal authentication.

Malformed mapping bytes, oversized actual capacity, wrong cardinality and
inconsistent selected child summaries retain their exact failure scope and
create no invalid cache entry or payload output from the failing batch. Existing
sealed v1 edit-reference vectors remain independent compatibility evidence.
Byte checks for a wider new edit are distinguished from an independently
predicted new root/partition matrix; candidate output never supplies its own pin.

The owning checkpoint records exact commands and all failing/corrected output.
No existing seven-family or Family2 campaign and no new Family8/9 row runs here.
#288 receives the changed cache/current-batch mechanism and these scoped proofs
for later qualification. Physical/global admission and R1's other gates stay open.
