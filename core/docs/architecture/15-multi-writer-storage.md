# Save ownership and publication in the schema 8 candidate

> **Status:** Current general guide.

This describes the landed #192 implementation merged at
`152b9c3a2e8ec2536a1d63601b681e1f7ef34455`, and the #216 change that replaces its
fixed two-slot writer model with the configured per-Store writer budget
(`152b9c3a2` plus this change). The
[selected C2 checkpoint](proposal/service-daemon-transport/implementation/evidence/optimization-selected-checkpoint-20260920.json)
and the
[decision record](proposal/service-daemon-transport/implementation/09-optimization-decisions.md)
track the design choices and the incomplete service/large-load qualification. This
page is not release or performance evidence.

The persisted candidate format is schema 8. It preserves the canonical profile,
content hashes, codecs and pack framing. Older schemas, including schema 7, are
rejected without migration. The core lockfile is unchanged.

## Ownership and visibility

One logical Store admits as many private saves as its persisted writer budget
allows: `store_policy.max_concurrent_writes`, 1..=64, default 2. The budget is
read inside the allocation transaction, so every sandbox and process using the
file shares one number and the next writer is refused, without waiting, as soon
as the live private save count reaches it. A retained owner counts like any
other, so lowering the budget never makes unresolved ownership reusable.
`begin_save` records the lowest free slot inside `1..=budget`; `saves.active_slot`
itself spans the whole supported space, so rows written under a higher budget
stay valid after it is lowered. See the
[concurrency controls](../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md)
for the operator-facing definition and its change behavior.

A native registry shares a database
arbitration mutex across handles to the same device/inode, including hard-link
aliases; it retains at most 64 live file identities and weak references to their
owners. Unsupported platforms fail explicitly. This is local process arbitration,
not a cross-process lock service. External SQLite contention still gets one
attempt with zero busy timeout.

`begin_save` commits a persisted save-slot reservation before allocating codec
workspaces. It then releases database ownership. Each save owns its connection,
compression/decompression workspaces, pending canonical batch, encoded groups,
pack tails, candidate indexes and cleanup identity. Input waits and construction
or encoding occur outside SQLite write transactions.

The six STRICT tables are:

| Table | Ownership and lookup |
| --- | --- |
| `store_policy` | Persisted construction policy, the authoritative writer budget, publication sequence, published pack range ceiling, monotonic pack/ordinal allocators and bounded pooled-window cursor |
| `saves` | Never-reused save ID, a unique private slot inside the supported slot space or a publication sequence, and the save's highest pack ID |
| `object_packs` | Pack ID, owning save ID and body; indexed by save/pack for cleanup |
| `objects` | `(object_id, save_id)` primary key with role, canonical length and pack/group/record locator; indexed by save/object for cleanup |
| `metadata_value_groups` | Unique ordinal spans and authenticated group locator; ownership derives from the pack |
| `content_signatures` | At most 8,192 hint slots, each with its owning save |

The native connection carries one private TEMP read-scope row. Ordinary read waves
capture the current publication sequence. A save's scope captures publication at
acquisition and additionally admits its own private data. Every locator/dependency
lookup and pack/value-group acquisition applies that scope. The stored pack ceiling
is still a range check and a counter; a hole below it does not grant visibility.

One locator row per simultaneously private owner can exist for one ObjectId, so
the read-side ownership bound is the supported slot space (64) rather than the
current budget: data written under a higher budget stays valid after it is
lowered. After the first publication, new saves compare/reuse that eligible
object; only saves already active can still finish a private copy. Lookups page
IDs, cap candidate rows, reject over-bound catalogs and select the oldest
eligible locator. A stored identity
outside the reader's scope is `StorageError::Unpublished`, distinct from absence.
The service preserves the existing failed-provider/integrity outcome classes.

Before a group transaction commits, concurrent candidates are reconstructed under
their own scopes and compared by exact authenticated canonical bytes, role and
length. The same database lock covers comparison and insertion. Reading a foreign
private candidate for collision validation does not make it reusable or allow it
to satisfy a logical, delta or pooled-value dependency. Corruption/read failure is
an error, never an alternate encoding or retry.

## Short transactions and memory owners

Group compression precedes the database lock. Pack placement runs under the lock
with a fresh global allocator cursor, and writes the pack, owned locators and
owner's range information in a short transaction. No open write transaction spans
the next input read or encoding call. A pack tail can be appended only by its
private owner; published packs are immutable.

Pending batches remain 512 objects / 512 KiB with the existing single-object
exception. Open multi-record groups also seal at the 512 KiB canonical boundary,
so compression cannot hide an arbitrarily large canonical group. Group row cost
is checked against the 8,191-row transaction bound. The existing singleton format
can carry one canonical record up to 16 MiB, with at most 4 KiB pack framing; its
isolated transaction is an explicit larger-record case, not a claim that all
transactions fit a 4 MiB physical journal.

Read waves keep the 4,096-object count limit and now also enforce 32 MiB of returned
canonical bytes, counting repeated IDs. Ordinary reads check locator lengths before
payload acquisition. Same-save reads check pending ownership before copying and
the combined pending/stored demand before acquiring the stored remainder. The
limit bounds result ownership; pack/group caches and decode workspaces are separate
owners. A read with more requested bytes fails with a typed capacity error.

Each save has a 16 MiB fixed encode workspace and a 1 MiB fixed decode workspace.
The content index retains its fixed 8,192-slot structure. The pooled index retains
at most 131,072 entries; its `live_bytes` is a declared entry charge, not exact
allocator/RSS measurement. Shared published indexes and per-save copies all count
simultaneously. A returned operation result, pending/closing saves and collision
decode scratch remain owned until their respective lifetimes end.

Final publication finishes every group and bounded hint batch, advances the
publication sequence and flips one save row. It does not move or scan that save's
locator rows. The two changed policy/save rows make all same-save dependencies
eligible together. SQLite index access is not claimed constant-time independent
of database size.

SQLite remains MEMORY journal / synchronous OFF / no WAL / zero busy timeout.
Connection counts, transaction shape, page-cache settings, native heap, OS page
cache and process RSS are distinct observations. No crash-durable acknowledgement,
exact process-memory cap, throughput figure or managed-provider qualification is
established by these source bounds.

## Ordinals and disposable indexes

Fresh pooled values reserve a monotonic ordinal range for one leaf in a short
transaction before encoding. Abort can leave gaps, which are never reassigned.
The policy row records the start and reserved-value count of the current candidate
window. A whole reservation that would exceed 131,072 values starts a new window.
Private and failed reservations also consume this allowance.

A cold index scans only that ordinal interval and filters by visibility. There
can be at most 131,072 one-value groups there. It no longer scans all history or
requires global contiguity. Group overlaps, invalid widths, digests and out-of-span
ordinal references still fail. An overlapping/failed save can reduce reuse and
produce duplicate physical values; canonical identity and dependency correctness
remain unchanged. Sequential whole-group eviction and the 131,200-value external
fixture remain part of qualification.

Candidate indexes are private during a save. Only a completed save can seed the
shared in-memory indexes. Persisted content hints carry save ownership and are
flushed in bounded preparation batches. An overwritten hint can remove a future
compression opportunity; it cannot publish content or make a foreign private
candidate eligible. No object/chunk reservation coordinator exists.

## Failure and ordinary reopen

A definite failure rolls back only the current transaction. Cleanup deletes that
save's locators and hints in bounded pages, then one owned pack per transaction;
value-group rows cascade from their pack. The final cleanup transaction removes
the save row and releases its slot. No pack-ID baseline range grants deletion
authority. Other saves can use the database between cleanup transactions.

Unknown COMMIT/ROLLBACK outcomes quarantine ownership. The product neither retries
nor deletes on a guess. Failed cleanup retains the slot. Ordinary reopen preserves
those private rows and slots while published roots remain readable. Each
unresolved owner consumes one unit of the budget - whether its slot is inside the
current budget or above it after a lowering - so a Store whose live owners reach
the budget refuses new saves, and only an explicit external decision about a
named save releases one. There is no automatic recovery, mutation replay or
resumable upload.

The bridge/service still require their separate concurrency, memory and actual
Docker acceptance. A C2 functional PASS alone does not enable a writer budget
above two in the service or qualify O01–O11 as a group, and no budget has a
performance qualification.
