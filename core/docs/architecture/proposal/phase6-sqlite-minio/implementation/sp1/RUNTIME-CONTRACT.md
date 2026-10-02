# Strict SP1 runtime allocation

Status: implementation checkpoint against parent
`285dd3f4a54a896344e389b51a8495300b98b4ca`; experimental adapter, dependent
enablement remains off until all four witnesses and integration gates pass.

The first-party C2 `PackAccess` seam captures lookup eligibility separately from
body I/O. Existing SQLite implements it. The strict adapter binds
`LogicalUse::MetadataGraph` or `RegularFileGraph` at the C1 caller. Metadata use
always selects SQL. Regular-file use selects MinIO only for WholeFile/Chunk;
its mapping objects remain SQL. Global canonical descriptors refine a known use;
they do not authorize placement. Required domain locator, use and captured scope
are checked before cache answers. No absent location triggers backend probing.

The new experimental catalog allocation is application ID `P6S1`, schema1.
It does not migrate P6L2/schema2 or change standalone C2 schema10/profile1.
One global monotonic body-order namespace covers both domains. A process-unique catalog instance namespace qualifies shared reader caches across SQL owners. A sealed body
order is immutable, and private generation changes require cache invalidation
or a distinct immutable snapshot order. A single immutable ordinal namespace
serves SQL value groups. SQL stores complete existing C2 pack envelopes; groups
are not bare bytes disguised as a pack. All canonical IDs and codec profiles
remain unchanged. Metadata and payload bodies cannot occupy each other's row
shape. Per-use reference facts qualify shared SQL mapping bodies.

The catalog uses indexed point and bounded keyset queries, at most128 rows per
registration/group page and less than4MiB canonical work per transaction. SQL
locks and statements end before provider/codec work. MEMORY journal, synchronous
OFF, temp_store MEMORY, cache2MiB, mmap0, busy timeout0 and one attempt retain the
experiment profile. These provide runtime atomicity without cloud durability.

The existing P6META7/action0–7 transport is historical prototype wiring and is
not a strict catalog transport. The separately allocated strict wire prefix is
P6SP1V1, requests/replies16384B, metadata pages8192B. Its action table is
maintained in the source; standalone publication is excluded. Both reply
assembly buffers coexist32KiB plus request/native buffers. Public routing remains
gated while integration and engine-epoch/failure refinements are in progress. Strict public routing stays disabled until a
separately allocated source-reviewed wire covers scoped saves, bodies, use
facts, groups, readiness and publication; it cannot reuse untyped locator calls.

Simultaneous allocation ledger (source bounds, not physical measurements):

| Owner | Live allocation | Admission/lifetime |
| --- | --- | --- |
| One serialized daemon C2 encoder |16MiB encode arena | Existing policy, one active construction producer; no per-domain encoder |
| C2 decode owner |1MiB arena | One declared read or writer acquisition owner; no per-domain duplicate |
| Shared admitted-FULL index | Existing704KiB allowance | One activated domain at a time, authoritative SQL rehydration required on switch |
| Depth cache | Prior4096 unqualified-key/value byte allowance | Qualified physical keys reduce entry count to2730 on64-bit; no quota increase |
| Ordinary/pool body caches | Existing C2 limits | Globally unique immutable orders, operation lifetime; scoped lookup precedes hits |
| Bounded SQL group page | At most128 descriptors (8192B on64-bit) | Statement released before codec/provider; additional live Vec/header |
| Bounded candidate page | At most128 rows (10240B on64-bit) | Existing ring hydration/staging, single activated domain |
| Pooled decoded values/index | Existing C2 limits | Global ordinals, selected scope before hits; disposable derivation |
| Pending canonical/result windows | Each less than4MiB,512 pending objects | Shared construction windows; drain/reuse with backpressure, not total population caps |
| Global SQL pager |2MiB per connection | Separate global owner; MEMORY journal/transfer/result bytes additionally charged |
| Experimental provider response |1MiB maximum | Existing adapter bound; larger single bodies unsupported explicitly |
| Workspace |16MiB / backing1GiB | Unchanged; eager16MiB encoder is charged to serialized daemon operation, not an unproved Workspace fit |
| Linux container |512MiB | Unchanged; complete attribution and physical high-water proof remain open |

No supported whole-owner memory claim follows from these individual bounds.
SQL journals, live engine, temporary transfer assembly, groups/tails, ordinal
memo, C1 assembly, native buffers and all simultaneous caches still need actual
admission and physical attribution. S2 owns the daemon-lifetime scoped live
engine; SP1 does not import its separate uncommitted source. C1's unfinished
draft8MiB-1 gate, non-pausing installation, concurrency, import/GC/cutover and
cloud durability remain separate.
