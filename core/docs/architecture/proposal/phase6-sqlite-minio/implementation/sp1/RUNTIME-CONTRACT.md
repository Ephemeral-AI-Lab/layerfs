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

The new experimental catalog allocation is application ID `P6S1`, schema2.
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
P6SP1V2, requests/replies16384B, metadata pages8192B. Its actions0–28 retain their allocation and action29 admits the immutable
owned capture. Every reply carries the current OS-random engine epoch. A native
session pins its first epoch and quarantines on a change or unresolved mutation;
there is no replay on an idle reconnect. Definite errors are restricted to
reviewed admission/query or acknowledged-abort paths. Publication remains a
host-side thin C5 authority rather than a wire command. Both reply assembly
buffers coexist32KiB plus request/native buffers. Public routing remains off
pending the S2 shared-engine capture and all four qualification witnesses.

Simultaneous allocation ledger (source bounds, not physical measurements):

| Owner | Live allocation | Admission/lifetime |
| --- | --- | --- |
| One serialized daemon C2 encoder |16MiB encode arena | Existing policy, one active construction producer; no per-domain encoder |
| C2 decode owner |1MiB arena | One declared read or writer acquisition owner; no per-domain duplicate |
| Shared admitted-FULL index | Existing704KiB allowance | One activated domain at a time, authoritative SQL rehydration required on switch |
| Depth cache | Prior4096 unqualified-key/value byte allowance | Qualified physical keys reduce entry count to2730 on64-bit; no quota increase |
| Ordinary/pool body caches | Existing C2 limits | Globally unique immutable orders, operation lifetime; scoped lookup precedes hits |
| Bounded SQL group page | At most128 descriptors (8192B on64-bit) | Statement released before codec/provider; additional live Vec/header |
| Bounded candidate page | At most128 rows (10240B on64-bit) plus conversion page | Existing ring hydration/staging, single activated domain |
| SQL candidate scratch | At most8192 rows /640KiB raw fields, plus TEMP B-tree/page/state overhead | One global scoped snapshot per walk; initialized once on writable engine, keyset pages, released at completion; read-only hydration explicitly unsupported |
| Pooled decoded values/index | Existing C2 limits | Global ordinals, selected scope before hits; disposable derivation |
| Pending canonical/result windows | Each less than4MiB,512 pending objects | Shared construction windows; raw exact reuse preserves unplaced eligibility; additional use facts share the encoded record |
| Private encoded tails/snapshots | One bounded tail per active domain/lane, assembled bodies at most1MiB | Appends reserve immutable snapshot orders; old empty descriptors are discarded only on definite SQL ACK; snapshot assembly adds a simultaneous copy |
| Strict transport | Requests/replies at most16384B; chunks8192B | Byte-bounded registration subpages; reply assembly/native decode and complete metadata receive buffers coexist and are charged separately |
| Global SQL pager |2MiB per connection | Separate global owner; MEMORY journal/transfer/result bytes additionally charged |
| Experimental provider response |1MiB maximum | Existing adapter bound; larger single bodies unsupported explicitly |
| Workspace |16MiB / backing1GiB | Unchanged; eager16MiB encoder is charged to serialized daemon operation, not an unproved Workspace fit |
| Linux container |512MiB | Unchanged; complete attribution and physical high-water proof remain open |

No supported whole-owner memory claim follows from these individual bounds.
SQL journals, live engine, bounded SQL candidate scratch, temporary transfer assembly, groups/tails, ordinal
memo, C1 assembly, native buffers and all simultaneous caches still need actual
admission and physical attribution. S2 owns the daemon-lifetime scoped live
engine; SP1 does not import its separate uncommitted source. C1's unfinished
draft8MiB-1 gate, non-pausing installation, concurrency, import/GC/cutover and
cloud durability remain separate.

Owned storage readiness binds Workspace incarnation, project/Branch, generation,
scope, profile and base root. One partial unique SQL index enforces one pending
owned capture per Workspace incarnation. Quarantine retains that ownership.
C5 publication verifies actual Branch context and reservation, stages the root,
requires the storage publication ACK, then conditionally commits through actual
C5. The authority has no provider/codec capability and retains known intermediate
facts on failure. Its C5 pager and Book/reservation facts are additional owners;
component success does not establish whole-daemon or container fit.

SQL COMMIT is attempted once with implicit rollback disabled. An unresolved
COMMIT quarantines the catalog engine before further normal access. Covered
registration/admission failures explicitly abort once; only a known abort ACK
permits definite abandonment. A genuine OS file-size refusal at COMMIT is qualified by the separate
strict-sql-commit-fault-r001 diagnostic; subsequent engine access refuses.
Failed-abort physical qualification remains NOT_RUN. Acknowledged provider objects are retained
on SQL refusal, and lost provider ACK is quarantined without resend or delete.

The native component proof authenticates the encrypted peer. The low-level
catalog handler still receives a trusted internal producer context; numeric save
IDs and submitted SaveContext fields are not a public Workspace authorization
scheme. Public daemon wiring must bind the authenticated Workspace/incarnation
and captured S2 view before invoking it. Cross-Workspace/multi-daemon permission
qualification remains a separate gate; matching C5 owner facts do not supply
that missing public dispatch binding.
