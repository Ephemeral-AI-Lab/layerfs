# Storage and durability

Status: research only. This report does not authorize a page profile, schema,
carrier, VFS, journal mode, weaker sync, or production integration.

Snapshot: 2026-08-20, accepted F2-v3 plus sealed F4-A evidence. **Observed**,
**Derived**, **Hypothesis**, and **Unavailable** are used literally.

## Result first

Cursor's central local lesson is not “packfiles are inefficient.” Cursor calls
packfiles convenient on a local machine and keeps ordinary Git repositories on
local NVMe. Its criticism is directed at remote dependent DAG reads, many
independent pack indexes, delta-compressed physical hops, and repeated
compaction across replicas. The local mechanism worth borrowing is narrower:
prepare immutable bulk bytes, keep a dense offset index, make them durable,
then atomically publish a small reachability record
([Cursor, “Git at any scale”](https://cursor.com/blog/git-at-any-scale)).

**Observed:** LayerFS's fresh SQLite path already writes almost exactly one
final-image worth of bytes, so there is no giant byte-amplification prize. It
does, however, issue about 26,675 individual 4-KiB main-database writes. The
gross mapping-direct-VFS plus COMMIT-main-DB-write wall is about 72 ms, and the
final main-DB sync is about 43 ms. Mapping direct VFS includes more than
main-DB write callbacks, so 72 ms is a mixed ceiling, not an exact write
interval. The cheapest credible direction is therefore a
prospective 8/16-KiB SQLite page-size experiment. The strongest disruptive
direction is not a Git delta pack: it is a hybrid immutable value log for
canonical bytes with SQLite retaining the offset catalog and complete visible
head.

**Recommendation:** test physical granularity before architecture. If larger
pages cannot remove at least 33 ms, run one exact hybrid-value-log lower-bound
diagnostic. Build no custom index or compactor unless the simpler SQLite offset
catalog proves the value separation itself.

## 1. Current physical path

### 1.1 Schema, profile, and publication order

- **Observed:** the accepted benchmark opens one SQLite connection, enforces
  rollback-journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`, and
  `mmap_size=0`, then stores `object_id`, kind, canonical length, and canonical
  BLOB in one ordinary table. The complete visible head is a separate single
  row (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:1861-1913`).
- **Observed:** full create enters `BEGIN IMMEDIATE`, streams source/CDC/object
  construction into that transaction, consumes the F2 construction proof,
  stages one head, and dispatches one `COMMIT`
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2102-2128`
  and `:9841-9916`).
- **Observed:** COMMIT dispatch, return, and a fresh independent read-only
  reconciliation are separate operations; a dispatch error or lost
  acknowledgement is not treated as rollback proof
  (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:2675-2722`
  and `:2879-3012`).
- **Observed:** the simpler production engine has the same essential profile,
  ordinary rowid object table, one `BEGIN IMMEDIATE`, and one COMMIT
  (`crates/layerfs-engine/src/lib.rs:683-715`, `:717-799`, `:404-463`, and
  `:607-674`).

### 1.2 Sealed storage observations

The following are **Observed** in the five valid F4-A rows and final storage
audit (`target/wp4m-f4a-residual-attribution-k64-20260820-v1/FINAL-REPORT.md:30-58`
and `:88-108`):

| Measure | Retained value |
|---|---:|
| Source | 104,857,600 bytes |
| Canonical object payload | 105,291,554 bytes / 5,372 objects |
| SQLite page size/count | 4,096 / 26,677 |
| Final apparent DB | 109,268,992 bytes |
| Dirty page writes | 26,676 |
| Cache spills | 6,675 |
| Mapping VDBE+pager / direct VFS | 48.854 / 24.282 ms |
| COMMIT VDBE+pager / direct VFS | 18.199 / 93.031 ms |
| COMMIT main-DB writes / sync | 48.194 / 42.818 ms |
| COMMIT main-journal sync | 0.133 ms |
| Durable diagnostic | 636.837 ms |

One representative sealed raw row further observes:

```text
mapping main-DB writes    6,674 calls / 27,336,704 requested bytes
COMMIT main-DB writes    20,001 calls / 81,924,096 requested bytes
total                    26,675 calls / 109,260,800 requested bytes
journal mapping writes       12 calls /     13,348 requested bytes
journal COMMIT write           1 call /         12 requested bytes
```

Source: `target/wp4m-f4a-residual-attribution-k64-20260820-v1/rows/row-3.json`,
fields `f4a.mapping.vfs` and `f4a.commit.vfs`.

**Derived:**

```text
dirty-page bytes = 26,676 * 4,096 = 109,264,896
final DB / canonical bytes = 109,268,992 / 105,291,554 = 1.03778
requested main-DB writes / final DB = 109,260,800 / 109,268,992 = 0.999925
average canonical object = 105,291,554 / 5,372 = 19,600.8 bytes
```

The path writes approximately one database image, not two or ten. The likely
opportunity is fewer pager/B-tree operations and larger writes, not eliminating
large logical write amplification.

**Unavailable:** requested VFS bytes are not physical-media bytes. APFS and the
device may coalesce, cache, allocate, compress, or amplify them. The retained
evidence does not expose media bytes or drive flush completion, and this report
does not substitute apparent/allocated size or wall time.

### 1.3 Durability qualification is narrower than the project wording implies

**Observed:** LayerFS freezes `synchronous=FULL`, not `EXTRA`, and does not set
SQLite `PRAGMA fullfsync` (`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs:1877-1885`).

SQLite's official documentation says FULL invokes VFS `xSync` and protects
atomicity/consistency/isolation in rollback mode, but the last transaction is
not necessarily durable across power loss after DELETE-journal unlink; EXTRA
also syncs the containing directory. SQLite also says `fullfsync` is off by
default and controls macOS `F_FULLFSYNC`
([SQLite synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous),
[SQLite fullfsync](https://www.sqlite.org/pragma.html#pragma_fullfsync)). Apple's
own `fsync(2)` documentation distinguishes host-to-drive flushing from
`F_FULLFSYNC`, which asks the drive to flush buffered data
([Apple `fsync(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html)).

**Derived:** existing benchmark evidence proves the configured SQLite profile,
one COMMIT, SQLite return/reconciliation behavior, and observed VFS sync calls.
It does not prove `F_FULLFSYNC`, directory durability after journal deletion,
or survival of a physical power cut.

**Recommendation:** performance experiments must first match the accepted FULL
profile exactly. Separately, product wording should distinguish “accepted FULL
profile” from “strongest macOS power-loss guarantee.” Turning on EXTRA or
fullfsync would be a durability-policy experiment, not an optimization.

## 2. Non-negotiable storage invariants

1. Exact canonical bytes, object IDs, raw IDs, mapping/root/transition, and
   authenticated incumbent equality.
2. One writer and one complete visible-head publication. No payload is reachable
   before its required durable boundary.
3. One SQLite transaction and one COMMIT for the accepted SQLite lane; a hybrid
   carrier requires separate authorization and an explicit two-file crash model.
4. A pre-dispatch failure leaves the prior head authoritative; a post-dispatch
   ambiguity is reconciled through a fresh independent read.
5. Bounded streaming memory and terminal `Q=0`; no source-sized sort, pack, or
   index staging.
6. Fresh reopen, full scrub, reconstruction, and exact ranges remain protected.
7. No logical length, page count, allocation, or RSS is reported as physical I/O.
8. No new persistent sidecar is called “metadata free”; its apparent, allocated,
   orphan, cleanup, migration, and corruption behavior must be measured.

The active algorithm explicitly excludes carriers, packs, WAL, compaction, and
repacking (`implementation-detail/phase-4/algorithm/spec.md:90-92` and
`:132-150`). Any such research result needs a new versioned physical profile;
it cannot be folded into the accepted F2/F4 executable.

## 3. What Cursor and primary storage work actually contribute

### 3.1 Cursor/Git

**Observed external result:** Cursor says Git packfiles are convenient locally,
keeps normal Git repositories on local NVMe, and separates bulk pack reception
from the much smaller reference publication. Its compaction problem is many
pack indexes and delta-packed random traversal, not the mere existence of a
contiguous file. Git's own multi-pack index stores one sorted object-ID list and
maps IDs to pack/offset with `O(log N)` lookup; incremental geometric repacking
keeps layer count logarithmic
([Git MIDX](https://git-scm.com/docs/multi-pack-index),
[Git repack](https://git-scm.com/docs/git-repack)).

**Local inference:** if LayerFS ever introduces a carrier, it should be an
uncompressed immutable value log with direct offsets first. Git-style delta
chains optimize size but add dependent physical hops—the opposite of exact
range/reconstruction locality.

### 3.2 Key/value separation

WiscKey separates large values from its sorted key structure to reduce movement
and write amplification, but it also introduces a value log and garbage
collection ([Lu et al., FAST 2016](https://www.usenix.org/conference/fast16/technical-sessions/presentation/lu)).

**Local inference:** LayerFS's immutable CAS is a better fit than an update-heavy
KV store because live canonical objects are never overwritten. A value log
still accumulates aborted/unpublished tails and any future garbage-collection
policy, so “immutable” does not make recovery or reclamation free.

### 3.3 Log structure

The original LFS work obtained sequential foreground writes by moving permanent
data and indexes into segments, then paid a segment-cleaning cost later
([Rosenblum and Ousterhout](https://dsf.berkeley.edu/cs262/2005/LFS.pdf)).

**Local inference:** sequential append is only a win if lookup and reopen avoid
rescanning the log and if cleaning is not charged invisibly. LayerFS already has
direct evidence of the failure mode: its deleted carrier produced 55,240 index
page reads for 5,363 lookups and about 4.02x reopen read amplification
(`implementation-detail/phase-4/storage/append-only/first-implementation-findings.md:161-220`).

## 4. Ranked local alternatives

All upside ranges below are **Hypotheses**, not observed LayerFS results.

### Rank 1 — 8-KiB and 16-KiB SQLite page profiles

**Mechanism:** create fresh databases at 4, 8, and 16 KiB before schema
creation. Keep canonical work, schema semantics, transaction, FULL/DELETE,
cache byte budget, and one COMMIT exact. Larger pages should reduce the number
of overflow pages and `xWrite` calls for approximately 19.6-KiB rows.

SQLite permits power-of-two pages from 512 to 65,536 bytes and recommends the
default for most applications. Its historical BLOB study found 8/16 KiB often
best for large-BLOB I/O but explicitly requires testing on the target hardware;
newer measurements found internal SQLite BLOBs about 35% faster than direct
disk I/O for 10-KiB values
([SQLite page size](https://www.sqlite.org/pragma.html#pragma_page_size),
[internal versus external BLOBs](https://www.sqlite.org/intern-v-extern-blob.html)).
Those old Linux/ext4 results are priors, not macOS/APFS evidence.

**Algorithmic:** unchanged `Theta(B + N)` work and `Theta(B)` durable bytes;
constant-factor reduction in pages, overflow links, VFS calls, and possibly
B-tree depth. Bigger pages may increase small-read and small-edit amplification.

**Amdahl ceiling:** gross mapping-direct-VFS plus COMMIT-main-DB-write wall is
`24.282 + 48.194 = 72.476 ms`; mapping VFS includes other file-kind callbacks,
so this is not an exact main-DB-write timer. VDBE+pager adds 67.053 ms. A page change cannot
remove all of either. A plausible planning range is 20-60 ms durable saving,
with no evidence yet for the upper end.

**Smallest decisive experiment:** one warmup and five balanced adjacent pairs
for 4K versus 8K, then 4K versus 16K only if 8K is valid. Freeze a byte-based
cache budget. Measure VFS calls/bytes/wall, pager writes/spills, phase walls,
file sizes, CPU/RSS/Q, fresh scrub/reconstruction/ranges, and same-count edit.

**Success:** at least 5% mapping and durable gain, 4/5 wins, no protected
phase/resource/storage regression over 5%, exact identities/work, no residue.
**Kill:** fewer write calls without at least 33 ms durable saving, larger
apparent/allocated storage over 5%, or edit/read regression over 5%.

### Rank 2 — SQLite VFS allocation/chunk-size hint

SQLite defines `SQLITE_FCNTL_SIZE_HINT` so a VFS can preallocate expected growth
and `SQLITE_FCNTL_CHUNK_SIZE` to extend/truncate in larger chunks, explicitly to
reduce fragmentation and sometimes improve writes
([SQLite file controls](https://www.sqlite.org/c3ref/c_fcntl_begin_atomic_write.html)).

**Hypothesis:** a 1-8 MiB chunk hint on the fresh database may reduce APFS file
extension/allocation overhead without changing logical pages or SQL.

**Algorithmic:** no complexity change. Gross ceiling is below the 72.476-ms
mixed mapping-VFS-plus-COMMIT-main-write ceiling; actual allocation-only wall is **Unavailable** and
probably much smaller.

**Smallest decisive experiment:** first verify system SQLite/macOS VFS accepts
the opcode and changes extension behavior. Then one exact 4K A/B with VFS
truncation/extent/write observations. **Success:** at least 33 ms durable saving
and no final allocation increase. **Kill:** `SQLITE_NOTFOUND`, no observable
allocation behavior change, or under-5% durable gain.

### Rank 3 — hybrid immutable value log plus SQLite catalog/head

**Hypothesis, disruptive:** retain SQLite for authority but move canonical BLOB
bytes to one engine-private append-only value log:

```text
stream canonical frame -> append to existing value log
                       -> authenticate exact ObjectId
finish all new frames  -> synchronize value log
SQLite transaction     -> object_id/kind/length/offset rows
                       -> root/transition/receipt/complete head
                       -> one SQLite COMMIT
```

The crucial ordering is payload durability before catalog visibility. A crash
before COMMIT leaves unreachable tail bytes; a committed SQLite head may only
reference bytes already synchronized. Before-dispatch failure may truncate only
when writer exclusivity and absence of ambiguity are proven. After dispatch,
fresh SQLite reconciliation must precede any tail decision.

This is not the rejected carrier. The rejected design put its custom collision
index and visible marker inside the carrier, causing 10.3 page reads per lookup
and full-log reopen scans. The hybrid deliberately reuses SQLite's index,
locking, recovery, and head publication.

**Algorithmic:** full create remains `Theta(B + N)` and stores `Theta(B + N)`.
SQLite mutates `Theta(N)` small catalog records instead of `Theta(B)` BLOB
payload pages; the value log performs one sequential `Theta(B)` append. Reads
add one SQLite lookup and one direct offset read, followed by the same complete
object authentication.

**Amdahl ceiling:** current gross SQLite-specific mapping+COMMIT composite is:

```text
48.854 mapping VDBE/pager
+ 24.282 mapping VFS
+ 18.199 COMMIT VDBE/pager
+ 93.031 COMMIT VFS
= 184.366 ms
```

A hybrid must retain catalog work, one payload sync, one SQLite COMMIT, and
authentication. An optimistic 60-130 ms durable saving is plausible enough to
measure but unsupported. From the accepted 659.593-ms row, 60 ms yields about
166.8 MiB/s and 130 ms about 188.8 MiB/s; it probably does not reach 200 MiB/s
alone.

**Smallest decisive experiment:** a non-production exact-work lower-bound
harness on the retained canonical stream. A is current SQLite BLOB persistence;
B appends identical framed canonical bytes to one pre-existing file, syncs it,
inserts exact offset rows into SQLite, publishes the same complete head, commits,
reopens, authenticates every object, reconstructs, and verifies ranges. No
custom index or compactor.

**GO:** at least 60 ms and 10% durable improvement, 4/5 wins; exact canonical
bytes/IDs/root/transition; protected read lifecycle within 5%; storage within
5%; bounded Q; one payload sync plus one SQLite COMMIT exactly; crash-state
enumeration closes. **Kill:** under 33 ms saving, two syncs erase the gain,
reconstruction/ranges regress over 5%, orphan/tail custody is ambiguous, or
physical I/O cannot be honestly observed.

### Rank 4 — self-indexed immutable segments

Only if Rank 3 proves value separation but SQLite's `Theta(N)` catalog remains
dominant, write a dense sorted `(ObjectId -> offset,length,kind)` footer inside
each immutable segment and put only segment descriptors plus the visible head
in SQLite. Git's MIDX is the relevant local pattern.

**Algorithmic:** foreground SQLite mutations fall from `Theta(N)` to
`Theta(segments)`, while segment build remains `Theta(B + N)`. Lookup is
`O(L log U)` across `L` segment layers unless a MIDX or compaction bounds `L`.

**Risk:** new format, sorted-index construction, segment discovery, orphan
cleanup, multi-segment lookup, migration, and eventual compaction. The deleted
carrier demonstrates that a poor index/reopen design can erase sequential-write
benefits.

**Decisive gate:** do not prototype until Rank 3 attributes at least 33 ms to
SQLite offset-row work after payload separation. Then require at least another
33 ms saving and no read amplification above 1.25x. Otherwise keep SQLite's
catalog.

### Rank 5 — metadata-only `WITHOUT ROWID`, after value separation

SQLite explains that a non-integer primary key in an ordinary table creates a
second unique-index B-tree, while `WITHOUT ROWID` can use one clustered tree.
It also explicitly warns that traditional rowid tables tend to be faster for
large rows/BLOBs and suggests average rows far below a page
([SQLite `WITHOUT ROWID`](https://www.sqlite.org/withoutrowid.html)).

**Recommendation:** do not apply it to the current approximately 19.6-KiB BLOB
rows. It becomes a legitimate small experiment only for a future metadata-only
offset table whose rows are roughly tens of bytes. Incremental BLOB I/O also
does not work on `WITHOUT ROWID` tables.

## 5. Conventional ideas with low ceilings

| Idea | Evidence-based disposition |
|---|---|
| Borrowed `SQLITE_STATIC` BLOB binding | Officially avoids the `SQLITE_TRANSIENT` copy, but F4-A's entire bind-call upper bound is only 2.745 ms; reject as headline optimization ([SQLite binding lifetime](https://www.sqlite.org/c3ref/bind_blob.html)). |
| Incremental `zeroblob` + BLOB writes | Same pager payload and page writes; may remove a small copy but adds API calls. The measured copy ceiling is too small. |
| Larger cache / disable spill | Current path already retains roughly 20,001 dirty pages to COMMIT and spills 6,675. Eliminating spills requires about another 27 MiB of dirty cache and shifts writes into COMMIT; it does not reduce required bytes. Test only under an explicit RSS budget. SQLite says spilling is normally advantageous ([cache spill](https://www.sqlite.org/pragma.html#pragma_cache_spill)). |
| Exclusive locking | May remove small lock transitions but has no measured 33-ms component; low priority. |
| Deferred unique-index construction or sorted insert | Might improve random 32-byte key-index mutation, but requires schema/DDL or bounded sorting and is capped by the 48.854-ms mapping VDBE+pager composite. Consider only after page size. |
| Atomic-write file controls | SQLite supports them only when the VFS returns success; unsupported systems return `SQLITE_NOTFOUND`. Verify capability before designing around it ([SQLite atomic write controls](https://www.sqlite.org/c3ref/c_fcntl_begin_atomic_write.html)). |

## 6. Anti-recommendations

1. **Do not equate “packfile” with “faster.”** Git packs add compression,
   deltas, dependent reads, per-pack indexes, and compaction. LayerFS should add
   none without separate measured need.
2. **Do not restore the deleted append-only carrier.** It was 11.69% slower than
   a conservative SQLite proxy and had 4.02x reopen reads; both proxy lanes were
   non-promotion workloads
   (`implementation-detail/phase-4/storage/append-only/first-implementation-findings.md:8-38`).
3. **Do not use one file per object.** It replaces database page work with 5,372
   directory entries and open/stat/close metadata operations, recreating an
   already documented failure mode.
4. **Do not switch to WAL as a local ingest shortcut.** The historical local
   diagnostic was 16.8% slower for fresh ingest, and WAL/checkpoint changes the
   frozen profile and write lifecycle
   (`implementation-detail/phase-4/storage/append-only/spec.md:196-221`).
5. **Do not weaken sync or move it after the durable timer.** The 42.818-ms
   main-DB sync is required work under the accepted profile, not removable
   overhead.
6. **Do not claim physical-byte savings from fewer pages.** Page/VFS requests,
   APFS allocation, block operations, and physical media bytes are different
   observations.
7. **Do not add compaction now.** A single value log with immutable retained CAS
   objects needs no foreground compactor for the retained create case. Add
   reclamation only when measured orphan/dead bytes require it.
8. **Do not rename or replace an open SQLite database.** SQLite documents that
   unlinking or renaming an open database yields undefined and undesirable
   behavior ([SQLite corruption guidance](https://www.sqlite.org/howtocorrupt.html)).

## 7. Amdahl table and decision order

Using the F4-A 636.837-ms diagnostic only as a common denominator:

| Gross component | Wall | Perfect-removal ceiling | Direction |
|---|---:|---:|---|
| Mapping direct VFS + COMMIT main-DB writes (mixed gross ceiling) | ~72.476 ms | 11.38% | page granularity / value separation |
| Main-DB COMMIT sync | 42.818 ms | 6.72% | required; only shrink dirty payload before it |
| Mapping+COMMIT VDBE/pager | 67.053 ms | 10.53% | page/schema/value separation |
| All SQLite mapping+COMMIT gross | 184.366 ms | 28.95% | impossible to remove fully |
| Journal sync | 0.133 ms | 0.02% | stop |
| Transient bind-call upper bound | 2.745 ms | 0.43% | stop |

Perfect removal is not achievable. A replacement must still write about 105
MiB, synchronize it, publish a catalog/head, and authenticate after reopen.

Recommended order:

1. 4K versus 8K page size.
2. 4K versus 16K only if the first comparison validates the mechanism.
3. Allocation/chunk-size hint only if VFS evidence shows extension overhead.
4. Hybrid value-log lower bound if page profiles cannot save 33 ms.
5. Self-indexed segments only if the hybrid proves SQLite offset rows remain a
   separately dominant cost.

The decisive question for later specialists is:

> Can fewer/larger SQLite pages or a payload-before-catalog hybrid remove at
> least 60 ms of durable wall while writing the same canonical bytes, preserving
> one atomic visible-head publication, bounding memory, and keeping fresh
> scrub/reconstruction/ranges within 5%?

Until an experiment answers yes, SQLite BLOB storage remains the rational
local default.
