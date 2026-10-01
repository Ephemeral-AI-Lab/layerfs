# V4b bounded physical packs and batched locators

> Status: Research; informative and not a product contract.

Parent report `88e9a37b43506157f559daec53ec141ef0b50415`; runtime
`776f0f879e52e401d03f4a69301d8c076f1b2a59`. Tracking #294 / #293.
V4a's one-session proof does not remove unary packs, duplicate lookups or
per-object decoder initialization. V4b owns these boundaries before large import
and namespace work; full scope stays in CHECKLIST.md.

## Frozen interfaces and limits

The new experimental native namespace is `P6META5`, retaining authenticated
session/request-ID/quarantine semantics from V4a. Existing actions 0..4 retain
bootstrap/lookup/register/snapshot/publication meaning. Action 5 looks up a u16
count of 1..128 exact 32-byte identities; reply has the same u16 count and ordered
presence/locator results, including duplicates. Action 6 registers 1..128 locators
and returns normalized locators in the exact same order. Every input/result has
exact EOF and <=16 KiB metadata. Five action counters become a fixed seven-cell
array, never a request history. Unsupported versions fail rather than migrate.

A locator carries canonical ID32, role1, canonical length u64, pack digest32,
group u32, record u32 and positive authority-local pack ID u64. Registration
inputs carry pack ID zero (unassigned); the single global SQLite owner assigns
and returns the actual ID. Lookup returns that recorded ID. Pack identity is its
full SHA256, not a truncated hash; the numeric ID is a stable cache key allocated
under the same authority and never recycled. Pack catalog and locator rows are
published after pack ACK and canonical authentication in one bounded transaction.
No payload BLOBs enter global SQLite.

Use existing C2 profile limits unchanged: ordinary/native pack256 KiB,
group target48 KiB, lane-specific actual group-count limits (the legacy ceiling
is256; tight ordinary/native/whole-file writes use the smaller public lane limit),
records per group8191, pending objects512,
pending canonical bytes4 MiB minus1. A read/registration page has128 locators;
its retained canonical results have a declared 4 MiB-minus1 budget before payload
reads. All group/pack format bounds remain the actual C2 bounds, including compact
whole-file, native and singleton grammar. Do not assume every lane allows the
same record/group layout; use the actual public pack sizing/build functions.
The inode-leaf ordinary FULL experimental representation remains explicitly
unqualified for shipping pooling/delta; neither is silently replaced.

One construction producer, one compression workspace, one admitted private
pending window. It retains finalized canonical objects by ownership until a
count/byte limit or explicit finish drains them. Same-operation C1 dependency
reads may resolve exact private pending canonical bytes; this pays for construction
inside Commit and is not a warmed prior-phase cache. Independent operations share
no pending window. No namespace/file population vector or lifetime object cache.
Every read authenticates the complete pack digest, exact locator and canonical ID;
codec/group/pack scratch is bounded and shared within the read wave. Reusing a
known locator avoids querying it again. Exact CAS byte/role/length comparison
remains required for existing objects, including same-window duplicates.

## Packing, acknowledgement and failure custody

Drain the pending window: batch membership, exact comparison for existing objects,
encode new objects into bounded lane groups and packs, PUT immutable pack keys,
obtain required ACKs, register normalized locators. Group/record indices come from
actual assembled placement, not guessed reference IDs. Seal a bounded pack when
its real assembled size/group bound requires it; singleton has its own real bound.
A finish returns only after every pending object has known registration. Call it
before bootstrap genesis publication and before candidate READY/final publication.
No emitted-but-unacknowledged reference may escape as a public ready root.

An upload/registration failure retains accepted Workspace writes and exact known/
uncertain facts; no resend, guessed adoption, rollback or orphan deletion on a
guess. SQL transactions do not span network uploads. A private pending canonical
read is not proof of persistence. Batches that cannot fit fail before effects;
no hidden quota, memory, worker or deadline increase. Actual simultaneous-window
accounting and physical observations remain separate gates, not a heap-only claim.

## Owning proof and collection

Own focused benchmark-runtime physical packing, pending, read-window and metadata
batch modules; update only their actual callers/interfaces. Product sources stay
unchanged in this research submilestone. External contract tests cover bounds,
wire count/order/EOF, exact group/record placement and refusal/custody. Real MinIO
proves storage/authentication and the same full SDK/FUSE/two-head path proves
integration. Instrument pack/PUT/GET, batch/request, duplicate, canonical/encoded
bytes and actual high-water window counts with fixed state.

One prospective full-provider treatment on the existing literal 4 KiB create/
overwrite workload, source/binary/image/provider pinned, fresh output, child15 s,
proof9.5 s. This is the affected source treatment after V4a, not an unchanged-arm
repeat. Compare work counts; uncontrolled cache timings remain INELIGIBLE. No
larger case or speed PASS inferred. Initial lane/placement component proofs are
labelled separately from public Workspace Commit. Before any additional workload
collection, publish its prospective exact case/oracle/topology/timer membership.

Exit: real C1/C2 logical identities and bytes preserved, exact packed locators
resolve through MinIO after producer removal, real ACK/conditional history and
cleanup pass, admitted windows/refusal and batch order/cardinality pass. Record
actual packing/request-count effect and every unqualified observation. Next V4c:
indexed changed rows, immediate-base edits, paged namespace construction/certified
incremental publication, inherited mount/import and source retirement. The three
named shapes, seven families and complete DeepSeek/locality goal remain open.
