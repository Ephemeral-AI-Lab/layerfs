# Note to Read After F3 — Local Storage Lessons from Cursor's Git Architecture

Status: future local-optimization planning note only. Read this document only
after F3 has a terminal disposition and final read-only audit. This note does
not relabel any F3 result, authorize F4, select a SQLite profile, resurrect a
rejected carrier, change the physical format, or claim the 200 MiB/s target.

Prepared: 2026-08-20 from the cited Cursor article, retained LayerFS evidence,
and read-only local architecture/optimization reviews.

Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` on
`codex/empty-worktree`. Never touch the sibling `layerfs` repository. Preserve
all F3 artifacts and the terminal accepted implementation. Do not commit unless
the user explicitly asks.

Primary external source:

- [Cursor, "Git at any scale"](https://cursor.com/blog/git-at-any-scale)

Supporting SQLite sources:

- [SQLite internal versus external BLOBs](https://www.sqlite.org/intern-v-extern-blob.html)
- [SQLite BLOB binding API](https://www.sqlite.org/c3ref/bind_blob.html)
- [SQLite WITHOUT ROWID](https://www.sqlite.org/withoutrowid.html)

## 1. Entry gate

Before using this note:

1. Read the complete terminal F3 report, raw evidence, independent audit, and
   manifest verification.
2. Preserve every earlier F3 `FAIL / REVISE / revert` result without rewriting
   its hypothesis or disposition.
3. Identify the exact retained executable and source. If F3 did not pass, the
   accepted F2-v3 implementation remains the control.
4. Freeze the new post-F3 mapping, COMMIT, durable, CPU, RSS/Q, pager, storage,
   identity, and correctness baseline.
5. Do not carry an F3 hypothesis forward merely because it reduced a mechanism
   counter. Require measured removable wall time.

If F3 still has an active diagnostic, candidate, audit, or unresolved custody
question, stop. Finish F3 first.

## 2. Executive conclusion

Cursor did **not** replace Git packfiles with a better local object format.
Cursor concluded that packfiles are a poor distributed source-of-truth and
coordination primitive while retaining ordinary Git repositories and
packfiles on local NVMe as the hot execution representation.

Cursor's architecture is approximately:

```text
durable truth
  = ordered immutable WAL entries in S3

atomic publication
  = small compare-and-swap update of a WAL index

hot execution
  = ordinary local Git repository and packfiles on NVMe

recovery/materialization
  = rebuild the local repository from the WAL

maintenance
  = one primary compacts; other replicas consume the compacted result
```

Cursor's `CAS` in this context means **compare-and-swap**. LayerFS `CAS` means
**content-addressed storage**. Never conflate the two.

The local LayerFS lesson is:

> Keep graph traversal local, persist immutable bulk data efficiently, publish
> it through a small atomic visible-head record, optimize physical
> bytes/copies/index locality rather than statement count, and prevent
> segment/index fanout from growing without bound.

It is not:

> Replace SQLite with a Git-style packfile.

The carrier is therefore a **last-resort fallback hypothesis**, not the
expected post-F3 direction. Historical evidence creates a presumption against
it. Continue with the accepted SQLite engine unless direct measurement shows
that SQLite-specific BLOB/pager work dominates, smaller SQLite/shared-core
changes cannot remove enough time, and a durability-complete carrier lower
bound exposes unusually large headroom.

## 3. Cursor's core findings

### 3.1 Logical DAG order and physical pack order disagree

Git operations walk a dependent DAG. A commit reveals its tree and parent; a
tree reveals files and subtrees; a packed delta object may reveal another
physical dependency.

Git pack generation primarily optimizes compressed size. Objects may not be
stored in traversal order, and cross-object delta chains add physical hops.
That is tolerable on local NVMe but performs poorly through a networked or
block-replicated filesystem.

Do not generalize this into "all contiguous storage is bad." The failure is a
specific combination:

```text
dependent graph walk
+ compression-first physical ordering
+ delta-base hops
+ remote or high-latency storage
```

### 3.2 A remote object-ID key/value store is not a free solution

Git appears to map naturally to `object hash -> object bytes`, but the next
object hash often is not known until the current object has been fetched and
decoded. A remote object store therefore serializes dependent round trips.
Git clone also still requires a packfile on the wire, forcing the server to
reconstruct a pack after retrieving individual objects.

This is a network-latency result. It is not evidence that local SQLite or a
local content-addressed index is inherently inefficient.

### 3.3 Many immutable packs create index and compaction debt

Each Git pack has an index. If packs accumulate, lookup can require probing
many indexes. Multi-pack indexes and geometric maintenance reduce the cost but
do not eliminate eventual compaction.

The general lesson is:

```text
an efficient lookup repeated across an unbounded number of indexes
is not an efficient lookup
```

If LayerFS ever introduces immutable segments, lookup cost must not grow
linearly with segment count.

### 3.4 Separate immutable bulk persistence from small publication state

Cursor persists a push's immutable bulk data before publishing a pointer to it
through the WAL index. Publication is small and linearizable; the bulk data is
never acknowledged first and made durable later.

The locally useful ordering is:

```text
prepare authenticated immutable bulk
  -> make the bulk durable
  -> atomically publish a small manifest/head
  -> acknowledge
```

Crash-before-publication may leave unreachable residue. It must never expose a
head whose referenced bytes are not already durable.

### 3.5 Cursor publishes distributed results, not a local-format benchmark

Cursor reports distributed read scaling and push throughput, but the article
does not publish:

- a single-host packfile-versus-alternative write benchmark;
- a new local object format;
- a packfile-free local Git implementation;
- a compression or write-amplification comparison;
- a traversal-local physical layout; or
- evidence that an external WAL improves one local durable write.

Do not use Cursor's pushes-per-second numbers to predict LayerFS full-create
latency. Cursor states that it is still investigating better on-disk layouts.

## 4. What already maps well to LayerFS

LayerFS already avoids most Git pack pathologies:

- canonical objects are stored whole;
- there are no cross-object compression-delta chains;
- the K64/F64 graph is shallow and bounded;
- one mapping leaf reveals up to 64 ordered chunk references;
- SQLite is embedded and local, not a remote per-object RPC;
- immutable objects, root, transition, and receipt precede visible-head
  publication; and
- one SQLite COMMIT atomically publishes the complete visible-head tuple.

The logical shape is already:

```text
source + CDC
  -> whole authenticated canonical objects
  -> bounded leaves and branches
  -> root and transition
  -> complete visible head written last
  -> one durable COMMIT
```

Relevant local contracts:

- [Logical persistence mapping](../implementation-detail/phase-4/mapping/logical-persistence.md)
- [Visible-head migration specification](../implementation-detail/phase-4/storage/sqlite/visible-head.md)
- [Algorithm complexity analysis](../implementation-detail/phase-4/algorithm/complexity-analysis.md)
- [Retained full-create lifecycle](../implementation-detail/phase-4/wp4m/f-series/planning/retained-100-mib-lifecycle.md)

Cursor therefore validates LayerFS's immutable-bulk/atomic-head separation. It
does not show that the current logical object graph should change.

## 5. Existing LayerFS evidence about packed storage

### 5.1 Phase 2 packed in-memory CAS reached parity, not a material win

The Phase 2 experiment replaced per-object payload ownership with one
contiguous payload plus an `ObjectId -> offset/length` map. It did not add
durability, compression, delta chains, recovery, or compaction.

After pre-sizing removed buffer-growth copies, the measured improvement was
only approximately `0.09%` to `0.94%`, below the `5%` promotion threshold.
Contiguous ownership alone was not a useful speed optimization.

See [Phase 2 packed-CAS evidence](../implementation-detail/phase-2/opt-2-packed-cas.md).

### 5.2 The Phase 4B carrier failed in its index and replay paths

The Phase 4B candidate was closer to a local append log than to a Git pack:

```text
one append stream
+ whole authenticated objects
+ direct offsets
+ disk-backed index
+ one commit marker
+ one sync
```

Its raw sequential append was not the clearest failure. The retained evidence
reported approximately:

```text
5,363 object lookups
55,240 index-page reads
10.3 index pages per lookup

106.3 MiB carrier
427.9 MiB reopen reads
4.02x reopen read amplification
```

The later proxy remained `11.69%` slower than SQLite and was not a fair full
logical-workload promotion row. The implementation was deleted; the report is
historical evidence only.

See:

- [Append-only first-implementation findings](../implementation-detail/phase-4/storage/append-only/first-implementation-findings.md)
- [Rejected Phase 4B specification](../implementation-detail/phase-4/storage/append-only/spec.md)
- [Rollback deletion record](../implementation-detail/phase-4/rollback/deletion-record.md)

The lesson is not "append-only can never work." It is:

> Do not resurrect the old carrier without first eliminating its index-page
> fanout, full-carrier discovery scan, repeated authentication/replay, and
> incomplete benchmark equivalence.

## 6. Current physical lower bound

The accepted F2 planning anchor for the retained fixture is approximately:

```text
objects                           5,372
canonical bytes written           105,291,554
SQLite page size                  4,096 bytes
dirty-page writes                 26,676
derived pager-write bytes         109,264,896
final logical/apparent DB bytes   109,268,992
page-cache spills                 6,675
```

The derived equation is:

```text
26,676 * 4,096 = 109,264,896 pager bytes
```

This is nearly one final database image for approximately 105.3 MiB of
canonical bytes. It is **not** physical-media I/O evidence: VFS write calls,
journal/temp writes, sync calls, cache effects, and media bytes remain separate
observations.

The result nevertheless prevents an unsupported claim that SQLite currently
rewrites many complete database images during a fresh create. A new carrier
must still perform the fundamental `Theta(B)` durable-byte work. Its possible
gain comes from fewer page operations, B-tree mutations, overflow-page steps,
copies, temporary writes, or a smaller publication working set.

The accepted F2 timing anchor is:

```text
mapping/proof       approximately 492.777 ms
COMMIT diagnostic   approximately 168.426 ms
durable capture     approximately 659.593 ms
target              at most 500.000 ms
remaining gap       approximately 159.593 ms = 24.20%
```

Refresh these numbers from the terminal accepted F3 evidence before planning
the next milestone.

## 7. Post-F3 decision rule

Do not select the next implementation from architectural taste. Select it from
the terminal F3 causal evidence and a same-work Memory/SQLite ceiling.

For the F2 anchor:

```text
5% of mapping = approximately 24.639 ms
5% of durable = approximately 32.980 ms
```

If a proposed mechanism has a directly measured removable upper bound below
approximately 33 ms of durable time, it cannot clear the prior 5% floor even
under ideal implementation. Stop before writing it.

A strategically useful follow-up should expose roughly 60--80 ms of removable
budget, not merely a large counter.

Examples:

- proven statement-subjournal cost -> consider one statement-semantics
  candidate;
- proven transient BLOB-copy cost -> consider one lifetime-safe borrowed bind;
- proven page/overflow cost -> consider one SQLite physical-profile campaign;
- proven shared CDC/hash/encode cost -> optimize the shared core;
- proven SQLite BLOB/pager dominance after smaller experiments -> measure a
  sequential-carrier lower bound;
- no mechanism with enough removable time -> retain the current engine and
  stop the branch.

## 8. Ranked local-first optimization suggestions

The ranges below are planning priors only. They are not acceptance gates or
performance claims.

| Rank | Candidate | Improvement class | Prior durable gain | Authorization |
|---:|---|---|---:|---|
| 0 | Finish terminal F3 attribution and same-work Memory ceiling | decision gate | none by itself | required first |
| 1 | SQLite `4K` versus `8K/16K` page-size campaign | physical constant factor | roughly `3--8%` | new profile milestone |
| 2 | Lifetime-safe borrowed canonical BLOB binding | remove one full byte-copy pass | roughly `1--4%` | isolated FFI milestone |
| 3 | Sequential-carrier lower bound only | test whether payload separation has enough headroom | unknown until measured | last-resort diagnostic after smaller paths fail |
| 4 | Immutable segment plus SQLite locator rows | sequentialize payload writes | roughly `5--15%`, below the carrier authorization gate | forbidden unless the lower bound qualifies |
| 5 | Self-indexed sealed segments plus small SQLite manifest/head | reduce SQLite object mutations | perhaps `10--20%`, high uncertainty | do not design unless the lower bound and simpler prototype pass |
| 6 | Geometric segment compaction | bound later read amplification | no initial foreground-write gain | do not design until a retained carrier shows measured multi-segment regression |

No listed prior closes the `24.20%` F2 durable gap by itself with high
confidence. A combination may reach the target, but every retained change
requires its own causal A/B evidence.

## 9. Smallest SQLite physical-profile experiment

The retained average canonical object is approximately:

```text
105,291,554 / 5,372 = approximately 19,600 bytes
```

At a `4 KiB` SQLite page size, a typical object spans several pages. SQLite's
official BLOB study gives a hardware-dependent rule of thumb that `8 KiB` or
`16 KiB` pages can work well for large-BLOB I/O. The study used different
Linux/ext4/SATA conditions and explicitly requires local measurement; it also
contains results where SQLite BLOB storage outperformed direct files.

A separate campaign may compare newly created databases with:

```text
A   4,096-byte pages
B1  8,192-byte pages
B2 16,384-byte pages
```

Keep exact:

- source, CDC sequence, canonical bytes, IDs, root, and transition;
- `synchronous=FULL` and rollback journal `DELETE`;
- `temp_store=FILE` and `mmap_size=0`;
- caller-thread execution;
- byte-fixed page-cache budget;
- one writer transaction and one publication COMMIT;
- ambiguous-outcome reconciliation; and
- complete post-COMMIT verification.

Measure mapping, COMMIT, durable and lifecycle wall, CPU instructions/cycles,
page count/size, dirty-page events, spills, logical/apparent/allocated bytes,
RSS/Q, same-count edit, scrub, reconstruction, and range reads.

Never infer:

```text
fewer dirty-page events
  => fewer VFS calls
  => fewer physical bytes
  => faster wall time
```

Those implications require separate observation.

Page size changes the SQLite physical format, base hashes, page count, and
likely endpoint bytes. It must not be smuggled into F3 or another
format-preserving milestone.

## 10. Borrowed BLOB binding experiment

SQLite documents that `SQLITE_TRANSIENT` copies bound bytes before the bind
call returns. The current rusqlite path requests approximately one complete
canonical-byte image through BLOB bindings.

If terminal evidence attributes enough time to that copy, one isolated private
wrapper may compare the existing path with
`sqlite3_bind_blob64(..., SQLITE_STATIC)`. The Rust slice must remain alive
until `sqlite3_step` completes and the parameter is reset, rebound, cleared, or
the statement is finalized.

Required focused tests include success, constraint, reset, rebind, clear,
finalize, rollback, and early-error paths. No SQLite callback may retain a Rust
slice beyond its proven lifetime.

This removes at most one `Theta(B)` memory-copy pass. It does not remove
canonical encoding, identity hashing, B-tree insertion, pager writes, durable
bytes, or COMMIT. Stop if copy counters and instructions/cycles do not move as
predicted or durable wall fails the preregistered gate.

## 11. Sequential-carrier lower bound before any backend

The architecture already permits a later file-backed carrier only if large
SQLite BLOB rows are directly proven to dominate:

- [LayerFS durable-storage architecture](../architecture.md)

This diagnostic is a last resort. Do not run it merely because sequential
writes appear attractive. First require terminal evidence that SQLite-specific
BLOB/pager work dominates, a same-work Memory/SQLite ceiling confirms the gap,
and smaller page-size, copy, statement, and shared-core opportunities are
insufficient.

Do not implement recovery, compaction, migration, GC, a new format, or a
second production engine first. Measure an idealized but durability-complete
lower bound:

```text
exact authenticated canonical stream
  -> sequential immutable segment framing
  -> exact checksums and locator/index bytes
  -> required file and directory synchronization
  -> small SQLite catalog/head transaction
  -> one SQLite COMMIT
  -> fresh independent reconciliation and verification
```

All durability work belongs inside the timer. A segment synchronized before
SQLite publication may leave an orphan after a crash, but SQLite must never
publish a segment that was not already durable. Orphan cleanup must never
delete a segment referenced by any committed generation.

The lower-bound disposition is prospectively fixed:

```text
durable improvement below 20%
  -> FAIL; permanently close the carrier branch for this Phase 4 program

durable improvement from 20% through less than 25%
  -> STOP / explicit user review; insufficient automatic prototype authority

durable improvement at least 25%
  -> may authorize one separately reviewed prototype; no production authority
```

The `20%` floor matches the historical Phase 4B review threshold. The preferred
`25%` headroom reflects the current approximately `24.20%` F2 program gap and
the likelihood that complete recovery, indexing, cleanup, migration, and
protected-read work will consume part of an idealized result.

Every byte of framing, index construction, authentication, file and directory
synchronization, SQLite catalog/head work, COMMIT, reconciliation, reopen, and
verification required by the proposed shape belongs inside the lower-bound
timer. A payload-only or scanner-only result is invalid.

On a result below `20%`, preserve the evidence and stop. Do not try new segment
caps, indexes, footer shapes, caches, compression, compaction, or storage
combinations in this phase. Additional file/directory synchronization may
erase the SQLite pager savings.

## 12. Higher-risk self-indexed segment design

Do not design, specify, code, or benchmark this structure until both of these
gates pass:

1. the durability-complete lower bound improves durable capture by at least
   `25%`; and
2. one separately authorized simpler carrier-plus-SQLite-locator prototype
   passes its complete correctness, durability, write, read, edit, resource,
   storage, and recovery contract.

Only then may a later milestone evaluate a bounded self-indexed segment:

```text
Segment
  header
  whole canonical object frames in source/traversal order
  page-packed sorted ObjectId -> offset/length/kind entries
  authenticated index/root offsets
  checksummed footer and visible end

SQLite
  segment descriptors
  root / transition / receipt
  complete visible head
```

The intended algorithmic boundary change is:

```text
current SQLite object mutations:  Theta(unique objects)
future SQLite metadata mutations: Theta(new segments)
```

Total full-create work remains:

```text
Theta(source bytes + objects/references)
```

This is nevertheless a real database-mutation reduction, unlike changing
`5,372` SQL executions into `103` while still inserting `5,372` rows.

The design must solve the historical carrier failures before qualification:

- pack many locator entries into each fixed-size index page;
- retain key ranges or equivalent routing so lookup selects a page directly;
- binary-search within the page;
- use a small bounded verified-page/locator cache;
- locate and authenticate the latest footer with bounded work;
- never reopen by scanning and rehashing the complete historical carrier;
- never probe an unbounded list of segment indexes;
- keep index construction bounded for the 100-GiB requirement;
- never trust an offset without canonical-byte authentication; and
- preserve exact range, reuse, typed-error, durability, and ambiguous-outcome
  semantics.

Do not add Git-style cross-object delta compression. Preserve whole canonical
objects and favor source/traversal locality over compression-first ordering.

## 13. Compaction is a later read-amplification control

Geometric compaction does not improve the first foreground full-create write.
Do not design or implement it from this note. It becomes relevant only after a
carrier has independently passed and been retained, immutable segments have
actually accumulated, and lookup, reconstruction, scrub, or range performance
has measurably regressed.

Measure protected reads at explicit segment counts such as:

```text
1, 4, 16, 64, 256 segments
```

Add compaction only after a preregistered segment-count/read-amplification
threshold fails. A safe compaction transition must build and synchronize the
replacement, atomically publish a new manifest, retain old segments through
ambiguous publication, and delete unreachable segments only after fresh
committed-manifest verification.

Do not run a full repack on every foreground capture.

## 14. What not to do

### Do not import Cursor's distributed machinery

Do not add any of these for the present local goal:

- S3 or another remote WAL;
- replicas, gossip, ETags, or rendezvous hashing;
- arbitrary primaries or distributed compare-and-swap;
- 3PC, consensus replacement, or remote freshness checks;
- remote per-object content-addressed lookup; or
- primary/replica compaction distribution.

They solve fleet availability and horizontal scaling, not the retained local
100-MiB full-create bottleneck.

### Do not confuse Cursor's WAL with SQLite WAL mode

Cursor's WAL is an ordered logical repository-operation history in object
storage. It is not evidence for changing LayerFS from rollback-journal
`DELETE/FULL` to SQLite WAL. The historical LayerFS WAL diagnostic was slower
for fresh ingest and remains non-authoritative.

### Do not resurrect the rejected carrier unchanged

Do not restore the deleted Phase 4B implementation, collision-chain index,
full reopen scan, incomplete qualification benchmark, or broad error model.
Reuse the evidence, not the code.

Do not treat the presence of this note as carrier authorization. Until the
last-resort lower-bound entry conditions and prospective gate are separately
approved, no carrier source, format, schema, migration, recovery, index,
footer, cleanup, compaction, or production abstraction may be added.

### Do not introduce Git pack pathologies

Do not add:

- cross-object delta chains;
- compression-first random object ordering;
- an unbounded number of independently searched segments;
- one file per object;
- a full repack or rewrite on each capture;
- source-sized staging or an unbounded in-memory locator map; or
- background compaction before a measured need.

### Do not optimize another proxy

Do not equate:

```text
fewer SQL executions
fewer index pages
fewer dirty-page events
fewer logical bytes
```

with faster durable capture unless the exact causal A/B wall evidence agrees.

### Do not apply `WITHOUT ROWID` blindly to large BLOB rows

SQLite notes that ordinary rowid tables can be preferable for large records:
`WITHOUT ROWID` stores the complete row in the primary-key B-tree and can
reduce internal-node fanout. It is more plausible for a future compact
`ObjectId -> segment/offset/length/kind` locator table than for the current
approximately 20-KiB average BLOB row.

### Do not weaken semantics to buy throughput

Every future candidate must preserve:

- exact canonical bytes, IDs, roots, transitions, closure, and delta;
- immutable authenticated CAS and incumbent reuse;
- caller-thread bounded execution unless a separate milestone authorizes a
  different model;
- a complete atomic visible-head publication;
- durability before acknowledgement;
- fresh ambiguous-outcome reconciliation;
- checked lengths/counters and exact typed errors;
- bounded exact Q with terminal zero;
- exact ranges and reopen verification; and
- same-count small-edit regression protection.

## 15. Recommended post-F3 order

```text
terminal F3 disposition and final audit
  -> freeze the accepted control and refresh the phase budget
  -> run the same-work Memory/SQLite ceiling and residual attribution
  -> select only a mechanism with enough removable wall time
  -> if page/overflow work dominates, run 4K/8K/16K SQLite profile A/B
  -> if transient copy dominates, run one borrowed-binding A/B
  -> exhaust measured smaller SQLite/shared-core opportunities
  -> only if SQLite BLOB/pager work still dominates, request separate
     authorization for the last-resort carrier lower bound
  -> below 20%: permanently close the carrier branch for this phase
  -> 20% to below 25%: stop for explicit user review
  -> at least 25%: permit review of one separately preregistered simple prototype
  -> consider self-indexed segments only after that simpler prototype passes
  -> consider compaction only after a retained carrier has measured
     multi-segment read regression
```

At every step:

1. preregister the one variable, counters, equations, commands, overhead gate,
   and retain/revise/revert rule;
2. preserve exact identities, work, durability, transaction/publication
   semantics, storage observations, and protected regressions;
3. use balanced adjacent A/B evidence with independent recomputation; and
4. stop when correctness, resource, or wall evidence rejects the hypothesis.

## 16. Expected payoff

The Cursor article does not raise the expected payoff of statement batching
and provides no basis for promising that a carrier alone reaches 200 MiB/s.

Current planning expectations are:

```text
causally supported post-F3 SQLite change    likely single-digit durable gain
SQLite page-size/profile tuning             approximately 3--8%
borrowed BLOB binding                       approximately 1--4%
simple immutable segment + SQLite index     approximately 5--15%
self-indexed segment architecture           perhaps 10--20%, high uncertainty
remaining F2 program requirement            approximately 24.20%
```

These are prioritization estimates only. Replace them with measured host and
fixture evidence. The correct next optimization is the cheapest one that
removes enough directly observed milliseconds without weakening correctness or
durability. The carrier priors are below the `20%` lower-bound floor and
therefore argue against implementation unless direct evidence materially
changes the estimate.

## 17. Final decision rule

Retain SQLite and the accepted post-F3 implementation unless direct evidence
shows that SQLite-specific BLOB/pager/index work dominates and a smaller
format-preserving change cannot remove enough time.

Reconsider a local immutable carrier only when all of these are true:

- a same-work Memory/SQLite ceiling attributes a material gap to SQLite;
- page-size and proven copy/statement opportunities are exhausted or too
  small;
- a separately authorized durability-complete sequential-carrier lower bound
  improves durable capture by at least `25%`;
- the design solves the historical `10.3` index-page/lookup and `4.02x` reopen
  amplification failures;
- the new index and recovery path remain bounded at the 100-GiB requirement;
  and
- exact semantics and protected workloads remain enforceable.

Otherwise, do not build the carrier. A lower-bound result below `20%`
permanently closes the carrier branch for this Phase 4 program; preserve the
evidence and do not iterate on carrier variants. Continue with the accepted
local SQLite engine and optimize only the largest measured residual.
