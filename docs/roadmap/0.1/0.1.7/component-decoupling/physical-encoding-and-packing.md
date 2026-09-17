# Physical encoding and packing: representations, candidates and bounded storage

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Components 5 and 6 within [C2 physical storage](object-storage.md).
Tracking: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Three read-only reviews covered payload encoding, packing/lifetimes and physical
metadata against the [pinned reference source](content-io-memory-audit.md#1-source-provenance-and-evidence-levels).
No implementation, build, benchmark or measured improvement is claimed.

This document owns terminology, selection/packing decisions and the cut list.
[Policy/tables](content-storage-policy-and-tables.md) owns persisted policy and
descriptor proposals. [Handoff](finalized-object-handoff.md) owns acceptance versus
completion. [File content](file-content.md) and [filesystem trees](filesystem-tree.md)
own logical construction; they do not inherit the storage algorithms below.
The [save/persistence design](admission-and-persistence.md) owns write authority,
bounded transactions, locator/base indexes, read visibility and cleanup.

## 1. Decisions

1. Reuse the existing shared payload prefix codec and pinned parameters. Share
   selection/work-accounting functions with explicit role/profile data; do not
   invent a codec framework or force distinct formats through one grammar.
2. Preserve exact CAS reuse, useful bounded candidates, compression decisions and
   group membership. Batch actual acquisitions and remove redundant preparation.
3. Keep bounded compatible pack append as the initial target lifetime. Decide
   placement before materialization and assemble only the selected write once.
   Seal-once insertion is not an additional runtime mode or failure fallback.
4. Keep inode-value pooling and pooled-leaf delta. Build/hash each value-group body
   once. Replace its disposable SQLite fingerprint index with a bounded Store-owned
   standard-library ordered set, subject to exact-semantic/resource qualification.
5. Remove the RAW-singleton temporary-file round trip through consuming in-memory
   assembly with proven allocation capacity. Preserve supported sizes and encodings;
   headroom/reallocation coexistence remains an implementation proof obligation.
6. Keep C1/C2 independently callable and environment-neutral. No Workspace type,
   checkpoint, path layout, daemon role or transport protocol belongs in encoding.
7. One attempt per operation. A condition requiring retry is failure: no codec,
   query, transaction, lock-acquisition or transport retries, including hidden
   SQLite/SDK retries. A lost acknowledgement remains an unknown persistence outcome.

These close the proposed design choices, not implementation admission. If a target
fails correctness, memory, storage-size or performance gates, revise it before
shipping; do not keep a hidden old/new execution fallback.

## 2. Terms used in this design

| Term | Meaning |
| --- | --- |
| Canonical object | Immutable logical bytes and their ObjectId; all physical representations must reconstruct those exact bytes |
| Logical role | Meaning of those bytes: WHOLE_FILE, CHUNK, mapping/tree node, etc.; not a compression or pack choice |
| WHOLE_FILE | Nonempty regular-file payload below configured construction cutoff; source calls this SmallContent |
| CHUNK | Canonical chunk payload; can serve regular files or metadata ropes |
| Provenance | Explicit context distinguishing those uses when physical policy requires it; separate from diagnostic flags |
| Physical encoding | The stored representation of an object, including applicable FULL/DELTA, compression or pooling |
| FULL | No direct delta-base dependency. It may be compressed; a FULL pooled leaf still depends on value groups |
| DELTA | Representation requiring a selected base to reconstruct its target |
| PREFIX | Payload delta technique: Zstandard compresses target bytes using base bytes as a prefix; delta matching and compression share that codec call |
| Delta-base candidate | Advisory existing ObjectId worth considering; called a hint in existing source. It becomes a required dependency only if selected |
| Selected base | The mandatory physical dependency recorded by a chosen DELTA encoding |
| Chain depth | Number of delta-base edges back to a FULL ancestor, not number of versions |
| Closure/work limit | Encoded acquisition, decoded/canonical byte work or other processing across dependencies; distinct from peak live memory |
| Compression frame | Codec output inside a record, or compressed group body for a group-compressed format |
| Physical record | One stored object representation with its framing, optional base reference and encoded bytes |
| Pack group | Bounded framing unit containing one or more records; record directories and compression are format-specific |
| Pack | Group directory plus groups, normally stored in one object_packs BLOB row |
| Locator | pack_id, group_number and record_number identifying an object's record |
| Framing lane | Records/groups compatible with one pack grammar; not another component or worker |
| Open pack | A current-admission pack eligible for compatible group append; existing group/record ordinals remain stable |
| Sealed pack | No further groups will be appended by this operation; sealing is not transaction acknowledgement |
| Compact inode value | 73-byte kind/reference-count/content-root/metadata-root value, distinct from logical attributes such as mode/mtime |
| Value pooling | Reuse equal inode values through Store-local ordinal numbers |
| Pooled leaf | Physical inode leaf replacing each serial + full value with serial + ordinal; reconstructs the original canonical leaf |
| Inode-value group | Up to 165 values encoded in an authenticated pack group; metadata_value_groups catalogues ordinal ranges and their pack/group location and digest |
| Fingerprint index | Disposable candidate filter from truncated value fingerprint to ordinals; authenticated complete equality decides reuse |
| Selection reason | Why reuse, FULL or DELTA was chosen; normal ineligibility is not an execution failure |
| Retry | Reattempting failed work, including lock/query/transaction or transport attempts hidden by a library; forbidden |
| Unknown persistence outcome | The operation failed to establish acknowledgement, but its write may have committed; never permission to resend or delete |

Automatic encoding/compression/packing means callers use the storage operation
and C2 performs these steps. It does not mean every object becomes DELTA or every
layer gets compressed again. Native chunk and whole-file records already contain
compressed frames while their outer group framing is RAW. Ordinary/metadata
formats may compress group bodies. RAW group framing does not imply raw payloads.

## 3. Complete flow and independence

```text
supplied canonical objects + established role/provenance + optional candidates
                                  |
                        admission: exact CAS reuse
                          /                    \
                     present                  missing
                   exact reuse                   |
                                  bounded candidate/base acquisition
                                                |
                                   physical representation selection
                             /                  |                  \
                       payloads          inode-value leaves     ordinary trees/
                     FULL / PREFIX       pooled FULL / DELTA    mapping/attributes
                             |                  |                ordinary FULL
                             +------------------+------------------+
                                                |
                             applicable record/group compression
                                                |
                                      selected records/groups
                                                |
                                   exact append/new placement
                                                |
                                   assemble selected write once
                                                |
                              atomic packs/locators/catalogue writes
                                                |
                                   storage completion barrier
```

Admission owns membership results, race checks, write authority and completion. Encoding
owns representation decisions and reconstruction; packing owns framing, exact fit
and locators. These remain ordinary functions/modules. A prepared admission handle
is not automatically a serialized plan or a pure SQL artifact.

Pure codecs operate on supplied operands. Complete C2 save/read accepts canonical
objects/IDs and an explicitly selected backend, without running C1, Workspace,
FUSE or history operations. C1 needs no knowledge of C2 caches, SQL handles or
pack grammar. Host/container/cloud adapters provide bounded acquisition and write
capabilities. Dependency work stays visible in the operation using it.

## 4. Payload selection and delta-base candidate quality

Reuse [NativeEncoder/ContentProfile](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L378).
The codec is already shared; the target consolidates duplicated admission policy,
accounting and movement around it. Role-specific framing and validation remain.

```text
missing target
      |
prepare its compressed FULL alternative
      |
eligible bounded candidate + permitted work/memory?
      | no                         | yes
      |                            v
      |                     compress using prefix
      |                            |
      +--------- compare full record cost --------+
                         |
                 selected FULL / DELTA
```

Scheduling candidate acquisition versus FULL encoding may differ by role to avoid
overlapping decoder/encoder workspaces. Preserve the existing useful lifetime
discipline. Both alternatives and required acquisition are real measured work.

### Candidate sources and consistency

WHOLE_FILE first considers its explicit predecessor. If no usable anchor is acquired,
the current chain-enabled path can consult the bounded content-keyed FULL-winner
cache. No caller candidate therefore does not imply FULL:

```text
WHOLE_FILE: explicit predecessor -> if no eligible anchor, bounded FULL cache
CHUNK:     first supplied candidate from bounded predecessor correspondence
```

The [small candidate cache](../../../../../crates/layerfs-layerstack-store/src/objects/small_candidates.rs#L1)
uses a 128-KiB allowance, 1,024 entries and eight-hash signatures, without an
input-sized signature allocation. Only actually admitted FULL winners enter it.
It can survive admission sessions in the same live Store; reopen starts without
it. Preserve its ordering, tie-break and lifetime. It is normal bounded selection,
not an error-recovery fallback or a new global similarity search.

The [chunk cursor](../../../../../crates/layerfs-content/src/file/rope/read.rs#L395)
can supply four distinct IDs, but [native admission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L649)
tries only the first. Preserve that baseline; do not turn four supplied IDs into
four codec trials during cleanup. Ranking/search changes require separate evidence.
The whole-file cache is not consulted again after an acquired predecessor loses
the savings or encoded-closure comparison. Preserve at most one PREFIX trial per
target; do not add a second candidate after a completed losing trial.

Bind candidate generation to an immutable base and explicit correspondence.
Current-result edit offsets are not automatically original-base positions after
insert/delete. Keep valid candidates through the handoff; do not discard useful
information for a faster-looking construction number. Cache state, candidate order
and declared budgets are part of reproducibility; canonical bytes remain identical
even when physical choices differ with permitted state.

Batch distinct initial native candidate locations within the bounded admission
wave, as whole-file predecessor locations already do. Reuse still-valid locators
and already-live authenticated base data. Preserve logical work charging: fewer SQL
requests must not silently grant additional candidates or weaken race checks.

### Cost, limits and failure meaning

Compare actual encoded record cost including the base reference and framing.
The native rule currently requires 37 + prefix frame bytes < 5 + FULL frame bytes.
Whole-file framing similarly accounts for its extra base ID. Preserve each format's
rule and native group membership planned from the FULL alternative, not the selected
smaller delta. Smaller records alone do not prove a smaller retained Store.

| Outcome | Treatment |
| --- | --- |
| Exact CAS hit | Reuse after required integrity/collision checks |
| No usable optional candidate, incompatible eligible-base role, or declared work/depth budget exhausted | Ordinary FULL selection with an explicit reason |
| DELTA does not beat the required cost threshold | Ordinary FULL selection |
| Declared capacity/workspace reservation unavailable before starting optional PREFIX work | Candidate ineligible under the declared policy; count preparation already done |
| Codec/allocation/reset failure after starting an invocation, corrupt data, missing required base, I/O/SQL/network failure or invalid framing | Fail the operation; no retry, saved-FULL recovery, alternate codec/backend or full rebuild |

Remove broad [generic-I/O error-to-FULL handling](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L767).
Check supported frame, workspace and overlapping allocation capacities before
starting. A completed nonwinning DELTA trial can select prepared FULL; an actual
failed trial cannot. Required reconstruction remains strict.

### One attempt, no retries

```text
declared inputs + checked policy/capacities
                    |
         one bounded operation attempt
              /                 \
       acknowledged            failure
              |                   |
           success        return failure immediately
                          no restart / replay / backoff
```

Disable the reference [five-second SQLite busy handler](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L531)
in the replacement (zero busy timeout); BUSY/LOCKED are failures. Disable automatic
query, transaction and network retries in selected adapters/SDKs. A backend whose
client cannot satisfy this contract is not a supported adapter. No retry counter,
backoff scheduler, resequencing or retry wrapper is added.

Admission's planned late equality/collision check may establish ordinary exact
reuse. An invalidated session/ownership guarantee, ordinal race or transaction failure
fails the operation; it cannot rebuild a batch or reread/reprepare until it wins.
Planned bounded backpressure before a storage attempt, stable-input passes required
by the construction algorithm, and comparing successfully encoded alternatives
are ordinary work. They do not recover a failed step.

Lost remote acknowledgement returns failure with an explicit unknown persistence
outcome. Never resend the write, report it rolled back, or delete possibly committed
data. A separately requested authoritative outcome inspection may determine what
persisted; it does not replay the operation. Cleanup applies only to established
failed unpublished ownership, and cleanup failure is surfaced without retry.

Reuse aggregate selection counters: supplied candidates, usable bases, missing
optional candidates, budget/depth exclusions, FULL wins, DELTA wins and exact CAS
reuse. Do not add per-object trace retention. A low delta percentage can be healthy
when CAS already avoids new objects; full retained storage and read/write cost
decide optimization quality.

### Accepted design note (2026-09-18): the zstd FFI boundary and the `forbid` deviation

`layerfs-content` and `layerfs-telemetry` are `#![forbid(unsafe_code)]`, so every
index, cast, slice and lifetime in them is compiler-checked.
`layerfs-storage` is **not**, and that is a recorded deviation, not an oversight:
the pinned Zstandard codec is the C FFI, and Rust cannot combine a crate-level
`forbid(unsafe_code)` with even one module of FFI (`allow` cannot override
`forbid`; rustc rejects the combination with E0453 - verified on `+1.85.1`).

What the crate does instead, and why it is at least as auditable:

- `unsafe` is **denied crate-wide** (`lib.rs`) and **allowed on exactly one
  audited module**, `encoding/codec.rs`, whose module documentation carries the
  complete FFI inventory (every `zstd_sys` entry point the product calls).
- The product boundary guard (`core/tools/check_product_boundary.py`) rejects
  `unsafe` anywhere else in the crate and rejects a storage `lib.rs` that drops
  the `deny`, so the boundary is machine-enforced on every future tree, not just
  this one.
- Every `unsafe` block in the audited module carries its own `SAFETY` argument;
  frame sizes are read from the frame header and checked against declared limits
  before any decompression, and decompression is exact-size into a validated
  destination. Correctness of the C side is `zstd-sys 2.0.16`'s, not this
  product's.

The audited surface at this round's commit is twelve `unsafe` items in
`encoding/codec.rs`: two numeric-return checks (`ZSTD_isError`,
`ZSTD_getErrorCode`), two static-context constructors (`ZSTD_initStaticCCtx`,
`ZSTD_initStaticDCtx`), the encode call sites (`ZSTD_CCtx_reset`,
`ZSTD_CCtx_setParameter`, `ZSTD_CCtx_setCParams`, `ZSTD_CCtx_setFParams`,
`ZSTD_CCtx_refPrefix`, `ZSTD_compress2`, `ZSTD_getCParams`), the decode call
sites (`ZSTD_DCtx_reset`, `ZSTD_DCtx_setParameter`, `ZSTD_DCtx_refPrefix`,
`ZSTD_decompressDCtx`), the frame validators (`ZSTD_getFrameHeader`,
`ZSTD_findFrameCompressedSize`) and one `unsafe fn` (`parse_frame_header`) that
wraps them. No `unsafe` exists in any other module, and the guard keeps it that
way. No memory-safety proof is claimed: this is an audited boundary, not a
proof.

## 5. Capacities and stored formats

Keep defaults T=128 KiB, whole-file depth 8 and chunk depth 4. Depth is independent
of each role's encoded/canonical/raw-work limits and live-memory limits. Existing
whole-file closure limits include 512 KiB canonical and 256 KiB encoded bytes;
native admission includes a 1-MiB raw closure and separate acquisition/work bounds.
Physical metadata has its own limits, not payload 8/4.

For an accepted larger T, derive exact canonical/frame/parser/codec/pack/singleton
capacities before work. A large valid FULL object must work even if DELTA is
ineligible under unchanged chain limits. Cover incompressible input. A construction
cutoff cannot double as the reader's stored-object validity limit. Opening a Store
does not reinterpret its objects against another cutoff.

The [capacity proposal](content-storage-policy-and-tables.md#making-the-whole-file-cutoff-genuinely-configurable)
owns the exact profile/schema decisions. Fixed constraints include Small raw/frame
limits, codec window/workspace, record grammar, producer batch size and v4 pack
ceilings. Test-only new_whole helpers and the old whole-owner reader do not prove
larger production SmallContent support. No new format version per numerical setting;
any genuinely required grammar change needs an explicit compatibility contract.

| Existing format family | Treatment |
| --- | --- |
| LFPACK v1 | Active ordinary FULL/mixed records and oversized RAW singletons; enum name Legacy does not make it obsolete |
| v2 | Active native chunk records with compressed FULL/PREFIX frames |
| v3 | Whole-file small-content grammar used by older supported profiles |
| v4 | Active compact whole-file small-content framing |
| v5 | Older unpooled metadata reader of the **reference** profile - **this profile implements no v5 reader and refuses v5 packs; scope decision recorded below** |
| v6 | Active pooled physical inode metadata - **the candidate writes value groups in v6 and pooled leaf records in v1; deviation recorded below** |
| LFCNT1 / 107 and associated whole-owner/slice representations | Explicit read compatibility; not ordinary WHOLE_FILE SmallContent or plain FULL merely because no delta-base column is set |

**Recorded deviations (v0.1.7 candidate, Stages 3-4).** Two rows of the table
above describe the v0.1.6 reference rather than what the candidate writes, and both
are recorded here instead of being left implicit:

* **v6 / pooled metadata.** The candidate writes pooled **value groups** in the v6
  pooled lane (`encoding/pool/value_group.rs`, `PackLane::PooledMetadata`) and
  pooled **leaf** records in the v1 Ordinary lane (`cas/owner.rs::select_pooled`
  returns `PackLane::Ordinary`), distinguishing the two by the
  `objects.object_role` column. It is self-consistent, covered by
  `physical_formats`, and reads back through the role column. It is *not* what this
  table's v6 row describes. Moving pooled leaf records into v6 is a format change.
  **Owner decision, 2026-09-17 (closeout escalation E2): the format change is
  waived.** The shipped assignment stands exactly as `physical_formats` asserts it,
  and no code changes. The owner's reply and the three dispositions it answers to
  are recorded verbatim in `stages-3-4-closeout-report.md` §6 and quoted in
  [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170).
* **v5 / unpooled metadata.** The candidate rejects v5 explicitly and by design
  (`parse_header` refuses every version it does not implement). It could not act on
  a v5 pack in this batch even if it wanted to: its schema identity is deliberately
  not the reference's (`sql/schema.sql`: `application_id = 1279677261`,
  `user_version = 4`), so a reference Store is not a candidate Store. This is a
  scope statement for v0.1.7 rather than a claim that v5 readers exist here.
  **Owner decision, 2026-09-17 (closeout escalation E2): the table's v5 row is
  corrected above** - it no longer reads "Supported older unpooled metadata
  reader", because this profile does not implement one. The refusal stays; only the
  row's wording moved. Recorded with the same owner reply in
  `stages-3-4-closeout-report.md` §6 and quoted in #170.

Use [explicit format dispatch](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L111)
and retain required [whole-owner compatibility](../../../../../crates/layerfs-layerstack-store/src/objects/whole.rs#L1).
Unknown/contradictory input errors; no retry through another decoder. Active format
families cannot be deleted simply to reduce a version count. Exact accepted read/
write/profile retirement remains a compatibility gate before implementation.

## 6. Physical inode-value encoding

Logical attributes remain in C1. Here the pooled value is the compact inode's
73-byte kind/count/content-root/metadata-root tuple. A canonical row uses serial
8 bytes + value 73 bytes; the physical pooled row uses serial 8 + ordinal 4.

```text
canonical compact inode leaf
             |
reuse equal values / assign pending ordinals
             |
      +------+------------------------------+
      |                                     |
pooled leaf                             new values
FULL / eligible COPY-INSERT DELTA       exact FULL group body
then applicable group compression           |
      |                                digest + compression
      +------------------+------------------+
                         |
                 groups in bounded packs
                         |
        object locators + ordinal-range catalogue
```

Pooling precedes metadata delta. Pooled and unpooled delta bases are not mixed.
Metadata delta uses COPY/INSERT over pooled leaf bytes, then expands and
authenticates every intermediate canonical leaf. The shared PREFIX codec covers
payloads; it does not replace this distinct metadata algorithm.
A FULL pooled leaf still needs its value groups. The catalogue identifies their
pack/group locations and exact decoded-body digest. Preserve ordinal chronology,
first-encounter allocation, pending-value reuse, group limits and atomic catalogue
updates. The exact 73-byte comparison decides reuse; a fingerprint never does.

### Build and digest once

```text
CURRENT: per-value wrappers -> encode/compress group -> clone -> decompress -> hash
TARGET:  exact checked group body once -> hash body -> compress -> retain result
```

Remove the [compression round trip](../../../../../crates/layerfs-layerstack-store/src/objects/admission/metadata_values.rs#L112)
and per-value wrapper/all-None-candidate allocations where the checked writer can
emit directly into that same body. Hash the exact count/offset directory/records,
not just concatenated values. Preserve group membership and compression selection.

Ordinary/metadata mixed FULL/DELTA groups retain their existing savings threshold:
at least max(64 bytes, ceil(FULL encoded bytes / 8)). Group compression is retained
only when compressed bytes plus 16 fit within the raw body's size. Keep metadata
and ordinary grouping targets distinct from their absolute format ceilings.

### Replace the disposable SQL fingerprint index

Design target: Store-owned BTreeSet<(u64 fingerprint, u32 ordinal)> plus current
ordinal cursor and retained count. Use standard-library ordered range lookup;
no new dependency, unbounded map, per-value heap-vector index or permanent DB table.

```text
sync authenticated groups
         |
whole next group would exceed 131072 entries? -> clear retained window
         |
insert (fingerprint, ordinal) pairs; advance validated cursor

lookup distinct fingerprints
         |
bounded range queries -> sort/deduplicate candidate ordinals
         |
authenticate groups -> compare all 73 bytes -> smallest exact-match ordinal
```

Preserve the current [whole-group reset recurrence and lookup semantics](../../../../../crates/layerfs-layerstack-store/src/objects/metadata.rs#L283),
including collisions, cold/reopen behavior and Store-owned lifetime. Unsigned tuple
ordering is safe because lookup uses exact fingerprints and globally sorted ordinals.
Do not shrink the retained window or reset on every operation, which could worsen
value reuse. Retain catalogue chronology/header audit; reopening still has O(groups)
header work and bounded retained-window payload acquisition.

On any failed synchronization, discard the accelerator and return operation failure;
do not rebuild and retry. Initialization/reopen may derive a new accelerator from
authenticated storage for a separately invoked operation. Partial cursor/state
must never be reused. Catalogue-ordinal races or transaction failures also fail
without resequencing/repreparing. Do not clone the whole tree before sync or invent
an in-memory rollback system. Failed unpublished cleanup invalidates it as required.

This target removes the temporary SQLite file, second connection, private schema,
SQL paging/transactions and path cleanup. It is not yet a measured memory win:
charge B-tree nodes, allocator/split transients, query vectors and decoded groups.
The old requested 4-MiB cache is not old total RSS; tuple payload bytes are not new
total memory. Prove same selected ordinals, acceptable lookup/sync/clear/rebuild
cost and no resource/storage-efficiency regression before replacement. If gates
fail, revise the target; do not silently disable pooling or fall back to old SQL.

## 7. Packing: place first, assemble once per write

Keep bounded compatible append. A finalized canonical object can live in a pack
whose group directory is still extended; its bytes/identity and existing locator
meaning remain immutable. An operation may write that pack more than once.

```text
CURRENT retained-tail path
incoming groups -> assemble incoming pack
               -> clone old + incoming groups
               -> assemble merged pack
               -> UPDATE existing BLOB

TARGET
old open-tail lengths / counts / format + incoming groups
               |
exact format/length/count fit decision
        /                      \
compatible append             new pack
        \                      /
assemble only selected layout from borrowed groups
               |
bounded atomic write
               |
move retained group ownership after success
```

Remove [incoming materialization before placement](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1263)
and [cloned candidate assembly](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2388).
Check exact format-specific lengths, counts, singleton constraints and required
base-pack chronology/visibility before copying. Planned capacity exhaustion selects
a new pack; framing/assembly/ownership errors fail without selecting another pack.

Keep authentic canonical operands until the last required exact collision
comparison under valid exclusive ownership; no redundant late pass is added.
A private absence proof must
remain valid under enforced writer authority; invalidation by an unexpected writer
fails without refreshing membership or replaying preparation. Known disjoint writes
by the same exclusive operation may preserve that proof. Expected CAS hits still
receive exact comparison. Compressed FULL is not directly comparable canonical
data. Retain original
inputs or validated RAW group ranges without introducing a decompress/re-encode
pass merely to avoid intermediate pack allocation.

Preserve FULL-size-based native group membership, role-specific group targets,
record ordering, base-pack chronology and shared packs across file boundaries.
No transaction/pack flush per object or file. Sealing a pack is separate from SQL
commit and final operation acknowledgement.

Move retained open-pack ownership only after the selected write succeeds. A failed
UPDATE, changed/missing target row or unexpected affected-row count fails; do not
retry as INSERT, change placement or assemble another candidate.

### Why not choose seal-once insertion now?

Holding every pack in memory until sealing changes current object/base visibility,
candidate availability and short-pack density. It can require pending-object lookup
state or dependency-triggered early seals. Retain the simpler compatible lifetime
while eliminating duplicate assembly. Revisit insert-once only as a separate
measured replacement; do not implement parallel modes selected by errors or topology.

### Oversized singletons

The current [RAW singleton path](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1197)
writes a temporary file, drops canonical input, allocates a pack and reads it back.
Target consuming in-memory assembly. Reuse existing allocation when sufficient
capacity is established; otherwise charge any allocation relocation or input/output
overlap before it occurs. Coordinate a supported allocation plan through existing
ownership boundaries, without exposing pack prefixes or a buffer manager to C1.

The largest supported object must still work; reducing accepted sizes or ignoring
transient reallocation does not satisfy no-spill qualification. This remaining
capacity proof blocks the cut if unresolved. The existing oversized exception is
v1 RAW: it does not authorize converting a selected compressed FULL/PREFIX object
to RAW merely to evade another grammar's cap.

## 8. Reads, memory and remote placement

```text
bounded object/locator demands
            |
group required records and dependencies by physical acquisition
            |
read/decompress required groups
            |
iterative reconstruction with bounded base/output ownership
            |
canonical authentication -> requested results
```

Reuse current grouped reads and iterative reconstruction. Batch native initial
candidate locations and physical value-group descriptors where semantics permit;
do not promise that arbitrary dependent chains become one request. Preserve
intermediate canonical authentication, required role/length checks and cycle/
chronology rules. A final hash alone does not replace every dependency check.

Metadata PoolRead shares bounded decoded values across a demand wave while each
chain resets its work allowance. Preserve that distinction and its [existing limits](../../../../../crates/layerfs-layerstack-store/src/objects/metadata.rs#L466).
Metadata currently has 16 edges and a **65 536-byte canonical closure** with
additional pool/acquisition work bounds. **Corrected 2026-09-17 (C10):** this
sentence said "128-KiB canonical closure", which no constant supports. The source
is `METADATA_CHAIN_CANONICAL_LIMIT = 8 * 8_192 = 65 536` and
`METADATA_CHAIN_ENCODED_LIMIT = 17 * 8_193 = 139 281`
(`core/crates/layerfs-storage/src/policy.rs`). Work budgets are not extra
allocation buckets.

Count simultaneous allocations, not unrelated maxima:

```text
canonical collision operands + current bases + codec workspaces
+ selected records/groups + all live framing-lane open tails
+ chosen assembled BLOB + locator/reference bookkeeping
+ Store-owned candidate/fingerprint indexes + decoded read-wave reuse
+ database/adapter request copies + shared allocator/runtime state
```

Use capacity, account connection/Store multiplicity and preserve reader/encoder
lifetimes that intentionally prevent workspace overlap. A bounded pack or index
alone is not a total-memory proof. Queue/backpressure and operation concurrency
follow the handoff; SQLite/SDK/OS memory remains attributable under the measured
claim. No new cache may conceal required reads or grant a warm comparison.

Remote adapters perform bounded grouped acquisition/writes under declared provider
limits and atomicity, not one RPC per local BLOB call. Reuse canonical bytes and
explicit object fields, not local connection handles, paths or a Workspace-shaped context.
Writer authority, read visibility, failed-attempt ownership and unknown outcomes
remain admission/persistence obligations under the single-attempt rule. No SDK
retries, write replay or automatic unknown-outcome polling is allowed. Final
object/history writes can share
the required transaction without putting history policy in this component.

## 9. Reuse and cut list

| Cut | Replacement / proof |
| --- | --- |
| Duplicated whole/chunk selection and work bookkeeping | Small shared functions using existing codec plus explicit role/framing policy; preserve units and scheduling |
| Native first-candidate point locator queries | Bounded grouped acquisition; reuse existing whole-file batching rather than inventing another query layer |
| Repeated signature calculation and provenance hidden in diagnostic bits | Carry once-computed signatures; explicit regular-file provenance; preserve useful scan placement/overlap |
| Frame -> record -> group copies and decode raw -> rewrapped canonical allocation | Consume/move buffers or write final bounded representation directly where feasible, with identical framing and full capacity accounting |
| Broad error-to-FULL or assembly-error-to-new-pack handling | Explicit eligibility/fit outcomes; propagate real failures |
| SQLite busy waiting/retries, SDK retries and failed-batch replay | Single attempt; expose failures and unknown persistence outcomes without re-execution |
| Refreshing stale absence proofs and reconciling unexpected competing publication | Enforced writer authority and one planned admission decision; invalidation fails |
| Unnecessary private uniqueness reconstruction | Reuse proven private batch ownership; keep external duplicate/collision checks |
| Metadata compress -> clone -> decompress -> digest | Build exact group body/digest once, compress once |
| Per-value wrappers and all-None delta vectors for known FULL-only pool groups | Checked direct body emission, retaining exact record framing and validation |
| Temporary SQL fingerprint index | Bounded Store-owned BTreeSet with identical window/collision/ordinal/failure semantics, gated on memory and speed |
| Incoming pack assembly plus cloned merged-pack candidate | Placement before materialization; borrowed group inputs and one selected assembly per write |
| RAW singleton temporary-file write/read/hash cycle | Consuming in-memory assembly after supported-capacity proof |
| Format branches with no required writer/reader after explicit compatibility decision | Retire only proven obsolete paths; active v1/v2/v4/v6 and required compatibility remain |

Source anchors for payload cuts: [selection](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L383),
[initial native read](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L1312),
[record copies](../../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L89),
[canonical reconstruction](../../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L114),
[private uniqueness](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L201).
The [stale-epoch refresh](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1295)
is a further target under the no-retry rule; ordinary planned exact-CAS comparison stays.
Other sections link metadata/packing evidence. A source-visible duplicate is a
candidate for removal, not proof of faster complete operations.

Keep exact CAS/collision comparison, supported codec parameters and resets, useful
bounded caches, optional-versus-required dependency distinction, genuine batch
reads, physical metadata pooling, current grouping/density policy and explicit
format readers. Preserve existing valid absence proofs only under their actual
write-authority conditions; they are not portable assumptions about remote writers.
Invalidation fails the current operation without refreshing and rerunning it.

## 10. Qualification and design closure

Use the [I/O comparison contract](content-io.md#7-measurement-and-completion) and
[repository measurement rules](../../../../../AGENTS.md). Freeze matched input,
role/profile, cache/index state, candidate order, worker count, transaction/visibility
semantics and backend before claiming a gain. The reference already has shared
codecs and caches; do not count them as newly introduced improvements.
Faster failure is not faster successful storage: a BUSY/resource failure cannot
replace a required successful case or earn a latency PASS. Preserve supported
workloads through checked capacities and explicit writer ownership.

| Dimension | Required evidence |
| --- | --- |
| Correctness | Exact canonical reconstruction, collision/dependency integrity, profile compatibility and acknowledged completion |
| Candidate quality | Same retained candidate universe and selections under unchanged policy; explicit reasons for ineligibility; preserve useful supplied information |
| Storage | Total retained database/pack/index/value-group footprint and tail occupancy, not just selected delta frames |
| Speed | Complete save/read plus candidate/index/group/packing work; preserve useful overlap and count reopen/sync work where required |
| Memory | Actual simultaneous allocations and relevant SQL/SDK/cache residency, including all Store-owned indexes and singleton relocation |
| Actual work | Queries, base reads, copies, hashes, codec trials, pack assemblies and bytes rewritten |

Cover CAS-heavy inputs; whole/chunk edits and transitions; long histories; candidate
absence, cache cold/retained states and shifted offsets; incompressible accepted
large objects; depth and closure boundaries; high/low inode-value reuse; fingerprint
collisions, whole-group eviction, failed sync and reopen; scattered ordinal demands;
pack boundaries and small final tails; corrupt/missing required dependencies;
assembly/SQL failures; BUSY/LOCKED, stale absence/ownership/ordinal races, competing
publication, unexpected UPDATE cardinality, codec failures and
lost acknowledgements. Assert no query/write/codec replay after failure, including
inside adapters; slow-consumer backpressure remains bounded ordinary work.

Use existing coarse timer scopes around acquisition, encoding, packing and storage.
Storage finish measures remaining drain/commit work, not all earlier writes; nested/
overlapping times are not additive. Component counters and external receipts record
work/resource evidence. No per-object trace growth or new monitoring framework.
For example, inject child scopes into the same production functions used by
standalone and integrated calls:

```text
storage.save
  membership
  base.acquire
  physical.encode
    payload encoding or inode-value pooling/encoding
  pack.build
  persistence
  storage.finish
```

These are illustrative coarse labels, not a new API or mandatory passes per object.
Pure encoding measures supplied target/base operands; complete storage also pays
for acquiring them. The [integrated timing contract](content-storage-design.md#8-read-path-environment-independence-and-timing)
owns scope nesting and attribution. A failed required child fails its operation;
there is no retry subtree.

The proposal choices are fixed in section 1. Remaining implementation gates are
exact format/capacity compatibility, allocation-safe singleton assembly, BTreeSet
failure/resource/selection equivalence, borrowed pack assembly with valid collision
operands, and matched speed/storage/memory proof. Admission/SQLite execution now
has its detailed proposal, including earlier-base placement for new-write cleanup.
Exact SQL/profile codes, public APIs and crate layout remain implementation gates.
No runtime fallback to the reference, speculative format migration, successful-
version rollback or assumed future Workspace shape is introduced.
