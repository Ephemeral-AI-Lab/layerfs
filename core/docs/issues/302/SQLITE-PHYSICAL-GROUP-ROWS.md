# Embedded SQLite physical group rows — reviewable option

> Status: Proposal; target LayerFS v0.1.7; not a released contract.
> Product reviewed at b1151f732; count/qualification source 509d9f164.
> Owner approved this treatment in this task on 2026-10-04.
> Implementation is present; new-schema count and qualification remain pending.

## Decision and evidence

Approve a new, explicitly selected embedded SQLite schema for new Stores that
stores each complete encoded group in its own row, preserves the exact assembled
pack representation for whole-pack reads/audits by reconstruction, and keeps all
current publication, memory, transaction, visibility and worker bounds.
No automatic migration or duplication of whole pack bodies is proposed.
PostgreSQL/MinIO milestones and the M4 pause are outside this decision.

The implemented demand-owned Save change reduces requested VFS bytes42.20%,
from1,798,505,411 to1,039,486,710B. The current1,039,486,710B remains27.99% above original reference812,164,935B. Current
acquisitions16,462 are already near original16,484; another global cache increase
or whole-first promotion is neither justified nor proposed.

Sparse version18 Save reads return193,426,533B but request395,380,277B inside
BLOB reads. The201,953,744B excess is close to the227,321,775B total reference
gap, without proving all gap bytes have this cause. The cold public-SQLite
offset vehicle returns32768B in every distinct case:

| Row bytes | Offset | VFS requested B |
| ---: | ---: | ---: |
|262144|0|40624|
|262144|32768|73424|
|262144|131072|171824|
|262144|196608|237424|
|32768|0|40624|

SQLite3.51.0,4KiB pages,cache_size-2048,mmap0, fresh read-only child and
zero-resident-page attestation for each distinct case. These are structural
requested-byte counts, not LayerFS speed, physical-device traffic or schema
storage qualification. SQLite's cursor overflow map is populated lazily; a
fresh high-offset read traverses earlier links. No dependency modification.
[SQLite source](https://raw.githubusercontent.com/sqlite/sqlite/master/src/btree.c).

Subtracting that193,426,533->395,380,277 sparse excess from current total gives
837,532,966B, about3.12% above reference. This is an arithmetic opportunity,
not a promised product result: row overhead, descriptor queries, codec/work
dependencies and cursor overhead still need measurement.

## Physical representation

Keep PackInfo IDs/domain/digest/length and ObjectLocation IDs/role/canonical
length/pack/group/record unchanged. Pack format versions and serialized canonical
objects do not change. Only SQLite's storage of those immutable pack bytes changes.

Proposed authoritative relations:

- pack: pack_id, domain, digest, length, complete control/reserved directory,
  group_count. The control bytes are exactly the original pack prefix.
- pack_unit: unit_id INTEGER PRIMARY KEY, pack_id, group_number, original
  offset, length, body; UNIQUE(pack_id,group_number), foreign key to pack.
  Body is one complete original encoded group. No duplicate whole-body column.

The exact SQL must use STRICT tables, positive IDs, existing length/count limits,
immutable update/delete triggers and authoritative foreign-key bindings.
A pack's units cover precisely the original validated directory extents in
order, with no gaps, overlaps, duplicate ordinals or stray bytes. Legacy supported
format widths retain their full reserved directory. Singleton remains one complete
bounded body unit. Pooled metadata can have up to256 units; tight ordinary/native/
whole-file packs up to16. No new unit digest promise is introduced: scoped reads
still authenticate accessed canonical objects/dependencies, while whole reads
reconstruct all serialized bytes and validate the existing full pack SHA256.

New schema selection is explicit at Store creation, separate from Durable/
Disposable persistence profile. Opening an old supported schema selects its
existing monolithic reader by declared version; unsupported versions fail before
I/O. No automatic promotion, backfill, retry or error-driven reader selection.
New-schema Stores are rejected by old binaries. No released compatibility
guarantee is silently revised.

## Read path and integrity

C2 retains the complete-directory parser, whole/range planner and complete-group
grammar. SQLite maps requested logical pack extents onto authoritative unit rows
inside the same existing read transaction. Metadata descriptor binding and actual
BLOB lengths are checked before data is returned. Unit cursors open/read/checked
close serially; none survives the transaction or a write. Coalesced logical
ranges can involve multiple physical unit reads; their real calls and bytes are
reported, never inferred from logical range count.

Whole reads/audits reconstruct control+all units into the existing bounded whole
carrier and run PersistedPack::authenticate against the original pack key.
Scoped reads return the distinct AcquiredPackRead unit carrier, make no unread
full-pack digest claim, and retain all current canonical/dependency/pooled value
digest/role/chronology/cycle/eligibility/work/visibility checks. Save physical-root
bases and every pooled chain node authenticate before use. Missing/extra/reordered/
truncated units or changed controls fail explicitly. No alternative route follows
an error.

The existing2MiB/4096 encoded cache,512KiB decoded/value caches,32MiB/4096 canonical
output, singleton, per-chain decoded/encoded/canonical work and one construction
worker stay unchanged. Reassembly and range transfer share existing acquisition
allowances; no whole-body-plus-unit copy cache, canonical base memo, prefill or
cross-write cursor retention is added.

## Publication and cost accounting

One acknowledged publication transaction owns descriptor/control, every unit,
locators, catalogue rows, signatures and first-wins results. No locator is visible
before its complete immutable physical input exists. Any definite failure aborts
all unit/descriptor/locator work; uncertain outcomes keep existing quarantine and
no retry. Private ordinary sealing and mutable pooled value invalidation remain.

Every inserted control/unit row must count toward the existing8191 physical-row
transaction ceiling, rather than counting one logical pack as one physical row.
Every control and unit byte, including required metadata costs, must count toward
existing physical transaction/acquisition/publication allowances. The implementation
must add prospective exact fanout accounting before a bounded page is sent and split
pages when necessary, preserving original object/lost-ID order and first-wins
acknowledgement. A fixed maximum fanout must not silently reduce or enlarge the
transaction contract; review the narrow port cost seam against real providers.

Encoded bytes are relocated, not duplicated. SQLite row headers, indexes, unused
leaf/overflow page space and increased INSERT/open/close counts can still cost
storage and time. Those costs must be measured on the original corpus under
unchanged storage ceilings. No hypothetical savings offset a measured FAIL.

## Reviewable implementation sequence and acceptance

1. Add explicit schema/version selection and validated control/unit representation.
   Keep old schema reader deterministic and unchanged; add no migration.
2. Add exact publication fanout/byte accounting under the existing caps and rollback/
   first-wins/concurrent-winner/private invalidation tests.
3. Implement scoped extent mapping and full serialized reassembly/audit with checked
   cursor/transaction outcomes; preserve backend-neutral C2 canonical consumers.
4. Add external tests for all supported lanes/legacy widths, sparse/dense/coalesced/
   singleton reads, canonical and dependency corruption, missing/extra/misbound units,
   unread corruption via separate full audit, visibility and row/byte limits.
5. Run required Core test/Clippy/fmt/boundary checks once at final source; freeze
   source, observer and new schema-specific benchmark/census identity.
6. Take one fresh labeled history17 cause count. Require Save VFS requests close
   to original reference, retained filesystem sparse gains, identical roots/
   canonical inventory and storage within its unchanged ceiling.
7. New same-identity reference pins and one matched10/3/1 performance/proof campaign
   retain all current cold/worker/time/12s-proof/content limits. No old proof promotion,
   unchanged sample retries or budget inflation. Durable/Init/all-seven stay separate
   incomplete gates until actually qualified.

Owner explicitly authorized this schema treatment after reviewing the proposal.
Actual implementation uses the complete control bytes to derive group count;
PackInfo and all serialized bytes are unchanged. The prospective pack-cost seam
charges exactly 1+group_count rows and serialized bytes+56+32*group_count binding
bytes. Default creation stays monolithic; opens use the declared stored version.
New-schema qualifications must prove the byte target and unchanged storage caps.
