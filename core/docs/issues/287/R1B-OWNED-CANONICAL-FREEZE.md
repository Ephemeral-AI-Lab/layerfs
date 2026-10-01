# R1b returned canonical last-owner foundation

> Prospective interface and source freeze, 2026-10-01. Published parent
> `53b6bf741a5693f4d00ec98b914ce34645ee9ab3` plus coordinated R1 source.
> No Cargo/test/provider proof has run for this delivery.

SC-02/03/04/07/08 require returned buffers to retain their funding through slow
consumers, cache copies and moves. Owned implementation is C1 object/access.rs,
object/mod.rs, new object canonical-budget/buffer/batch files, mapping/cache.rs
and read.rs; C2 cas/provider.rs/read.rs owned methods and external proofs. Other
Source/namespace/draft/Server/Store scheduling ownership stays with its agents.

The default canonical **data-capacity** class remains32MiB, matching the existing
larger returned-canonical compatibility ceiling. Caller-selected smaller budgets
retain their exact limits. These credits count actual Vec capacity, not Vec len;
descriptors, HashMap allocation, decoded structures, C2 engine/cache/native/OS
memory remain separately unqualified. No32MiB buffer budget is an8MiB strict read
or a global176MiB/process/physical admission claim.

Concrete types:

- `CanonicalBudget`: fixed limit and atomic live returned/copied data capacity.
  Idle contexts reserve no full payload window. No lease independently refunds.
- `CanonicalLease`: exclusive prospective capacity credit; split/move retains
  exact byte ownership. Drop returns its actual held credit once.
- `CanonicalReadPermit`: count/exact prospective output-capacity and budget grant
  selected before a provider call. Refusal occurs before that call's effects.
- `CanonicalBuffer`: private immutable Vec followed by its non-cloneable lease.
  Explicit fallible copying reserves prospective clone capacity before allocation.
  Actual data destruction precedes credit return; no owned-path raw Vec escape.
- `OwnedCanonicalBatch` and its owning iterator: exact demand cardinality, immutable
  qualification and owned buffers. Moving elements cannot detach their leases.

AuthenticatedObjects gains an owned method supplied a permit and optional timer
scope. Its default calls the existing raw API and explicitly marks the adapter
compatibility/unqualified for upstream allocation. Existing raw APIs and provider
source scopes stay unchanged; no allocation error selects another method.
C2 StoreProvider overrides the owned method, receives its permit before effects,
and checks selected locator canonical totals before decoding/pack acquisition.
It moves canonical Vecs into permit-funded buffers and checks actual capacities.
The result qualifies returned-buffer ownership only; lower working/cache shape
remains an original mandatory open gate.

Provider profile is prospectively selected: existing32MiB compatibility versus
unsupported Strict8. Strict8 refuses before connection/open effects. No policy
constant, read count4096,32MiB raw return ceiling, cache4MiB/512KiB, worker or
deadline is reduced or increased. There is no error-driven raw fallback.

Mapping stores CanonicalBuffer owners in the existing64-page/count-policy cache.
Wave cache hits and duplicate/context copies acquire their own data credits
before allocation. Missing navigation and payload waves call the owned method.
Decoded CheckedPage moves preserve credit; retained copies use the same budget.
Canonical grammar, navigation order/counts,32-page grouping and existing local
edit retention remain unchanged. Table/decoded/window descriptor qualification
is not established merely by attaching a canonical-data lease.

An exact96MiB lower working class is **not** asserted: pack_bytes copies a full
SQLite BLOB up to16MiB+4KiB, truncates used length, and caches currently charge len.
For example, a Pooled128KiB used header in a16MiB BLOB can retain16MiB capacity
while charging128KiB;32 such retained entries look4MiB but own512MiB. The source
requires a separately frozen loader/cache-capacity repair or explicit supported
larger shape before an owning working-class/aggregate proof. There is no invented
reservation arithmetic or native/global fit claim here.

Required exits: real C2 owned results remain charged while held after provider
drop; exact capacity/spare-capacity handling, pre-effect output exhaustion, actual
clone/refund through System observation, cache crossing with1/31/32/64 retained
pages, authentic independent bytes/v1 roots, and strict-provider pre-open refusal.
Raw-adapter tests preserve their visibly unqualified upstream scope. Root owns
coherent builds, owning proofs, LOC and publication; old receipts are not relabeled.

Exact role window: `CanonicalReadKind::MappingNodes` accepts only ExtentLeaf and
ExtentBranch locators; `ChunkPayloads` accepts Chunk; Any preserves the independent
provider's original general demand. Mapping missing nodes reserve count times
8192 data bytes; payloads reserve count times `chunk_canonical_len(MAXIMUM_CHUNK_BYTES)`
(existing overhead21). Cache hits and context/duplicate copies reserve each actual
canonical length before copying. Overlap is retained cache plus copied wave pages
plus the new provider result window plus separately funded retained copies. All
share the same exact CanonicalBudget and a held result can refuse a later call.
The C2 length check is prospective; transfer checks actual returned Vec capacities
against the already reserved window. Unqualified raw adapters cannot claim this
producer admission solely by wrapping their result after allocation.

Pack repair plan frozen before implementation: keep the existing row length and
publication guards and read a borrowed SQLite ValueRef under the query-row callback;
validate the declared used length against the full borrowed row, then allocate and
copy only those declared used bytes. This avoids a Rust full-BLOB Vec before the
policy sees its capacity. `truncate` is not an allocation shrink. Cache retention
must sum actual Vec capacities and make its wholesale crossing decision using the
incoming actual capacity. The 4MiB retention ceiling stays4MiB. Existing admitted
packs whose declared used body itself exceeds4MiB require an explicitly separate
current-body working owner or pre-effect unsupported/refusal profile; retaining a
single oversized body cannot be described as a4MiB cache. No32MiB canonical grant
funds these packs, decode arenas, metadata or native heap. Exact transient/cached
pack and decoder overlap still needs compiled/requested-allocation proof before
any working budget is qualified. This loader/cache algorithm change waits for
root's prospective control observations and target publication; current source
has deliberately not changed it and its concrete capacity defect remains open.

`CanonicalBudget::new(limit) -> ContentResult<CanonicalBudget>` admits explicit
limits no larger than32MiB and refuses larger choices before constructing the
authority. `compatibility()` infallibly selects the frozen32MiB class. This
hard maximum applies across held returned data and all its funded copies under
one continuing authority, not just each wave. Distinct authorities and the
Save/index176MiB ledger are not an aggregate/global admission.
