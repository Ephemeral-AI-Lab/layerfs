# Phase 4 G6 bounded canonical extent-tree candidate specification

Status: **SPECIFICATION READY / EXECUTION BLOCKED BY G5 AND SHADOW**

Research disposition: **`G6_SPEC_READY_PENDING_G5_BASELINE`**

Date: 2026-08-23

This specification defines the smallest candidate that could supersede the
current suffix-sensitive canonical-v2 K64/F64 mapping. It does not authorize
Rust changes, a mapping profile, migration, a G6 binary, a measured campaign,
SDK/VFS integration, or Phase-4 closure.

Two entrances remain mandatory:

1. the metadata-only CD32–64 shadow in the companion benchmark plan must prove
   deterministic history-independent roots, ordinary locality, bounded
   resources, and honest adversarial fallback; and
2. G5 must produce one sealed terminal source/executable/evidence baseline and
   a reusable engine/schema boundary.

## 1. Decision and revised claim

The candidate is a **hard-node-bounded content-defined measured sequence tree**
over existing canonical-v2 payload occurrences.

It is not a conventional mutable B+ tree, an edit-history rope, a piece table,
an overlay chain, a carrier, or a pack.

The intended architecture is:

```text
Application
    +-- direct SDK
    +-- later OS virtual-filesystem adapter
    |
    v
one portable extent resolver
    |
    v
one canonical authenticated measured sequence tree
    |
    +-- immutable canonical CAS payload objects
    `-- SQLite expected-head visible publication
    |
    v
optional OS projection accelerator
```

The accepted complexity language is:

```text
fresh build                Theta(payload bytes + extents)
range read                 O(height * bounded fanout
                             + intersecting extents + returned bytes)
ordinary raw mutation      expected O(height + local resynchronization
                                        + streamed replacement bytes)
multi-island mutation      expected O(k*height + unique CDC scan
                                        + unique tree replay)
mapping-only hard worst    Theta(remaining extent suffix)
raw-mutation fallback      Theta(replacement remainder
                                        + raw suffix + extent suffix)
resident mapping state     O(height * max node + bounded batches/buffers)
full native export         Theta(file bytes)
```

No G6 document or benchmark may call the candidate hard `O(log E)` for
arbitrary insertion/deletion. A passing ordinary-fixture result proves that
population only.

The G5/G6 boundary is now explicit. G5 supplies the eventual sealed baseline,
`TrustedLocalDev`, position-preserving warm-native behavior, and honest
`FullFallback`. G6 owns variable-size raw insert/delete, shorter/longer
replacement, bounded atomic multi-splice, variable FastCDC occurrence counts,
dual-coordinate diff/coalescing, virtual count-changing visibility, and native
variable-size routes. Nothing in evolving G5 evidence proves those G6 paths.

## 2. Preserved invariants

G6 preserves unless a later separately authorized migration says otherwise:

- Phase-1 canonical object framing and `ObjectId` hash domain;
- canonical payload `Bytes` objects;
- FastCDC 8/16/32-KiB boundaries, normalization, and seed;
- CAS immutable no-replace semantics and incumbent authentication;
- COW immutable roots and ordered delta semantics;
- exact errors and first/cleanup/reconciliation/dominant precedence;
- `Verified` default and explicit Store-lifetime `TrustedLocalDev`;
- one synchronous writer transaction and one publication COMMIT per
  state-changing atomic mutation; an empty normalized plan performs neither;
  a nonempty plan constructing the prior root uses one transaction rolled back
  and zero publication COMMITs;
- rollback journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`,
  `mmap_size=0`, and the accepted cache-spill policy;
- expected-head checking and fresh ambiguous-outcome reconciliation;
- checked resource accounting, terminal `Q=0`, and bounded descriptors;
- old-or-new native publication;
- no hidden retry, pool, async runtime, worker fan-out, second database,
  second authoritative mapping, or additional durability boundary.

## 3. Explicit non-goals

The first G6 variable does not include:

- new CDC boundaries;
- compression, packing, payload deltas, carrier files, or GC;
- a projection-only tree claimed as canonical improvement;
- native physical extent IDs in canonical objects;
- production FUSE, FSKit, ProjFS, or public SDK integration; G6 still owns one
  promotable virtual endpoint and benchmark-private native route adapters;
- APFS/Linux-specific bytes in canonical encoding;
- a generic backend/provider/adapter framework;
- cross-process projection seeds or a persistent materialization cache;
- concurrency workers beyond the separately accepted bounded projection model;
- migration of an existing user store;
- 500-MiB execution;
- a hard scale-independent wall-time claim.

## 4. Canonical profile boundary

Any durable candidate is a new profile, provisionally mapping version 3.
Version 1 and version 2 bytes retain their identities forever.

The future profile-ID preimage must bind at least:

```text
domain = "layerfs/mapping-profile/v3\0"
leaf occurrence codec ID
internal descriptor codec ID
mapping role tags (root/leaf/internal)
minimum entries
natural-cut algorithm ID
natural-cut threshold
maximum entries
level/domain-separation rule
root/tail/singleton-collapse rule ID
maximum accepted level/depth
directory page ceiling
delta page ceiling
```

These values are format bytes, not runtime tuning switches. The exact profile
ID is **Unavailable** until the shadow selects and freezes the cut predicate.

## 5. Canonical node model

### 5.1 Payload extent occurrence

Reuse the canonical-v2 occurrence exactly:

```text
ChunkExtentV3 {
    raw_length: u32be,
    object_id:  [u8; 32],
}
```

Required invariants:

- `0 <= raw_length <= 32,768`; zero-length occurrences remain in the
  canonical sequence and participate in cut/root identity;
- `object_id` authenticates a complete canonical Phase-1 `Bytes` object;
- decoded payload length equals `raw_length`;
- the stored extent always covers the complete payload object: implicit source
  offset zero and source length `raw_length`;
- no arbitrary stored payload slice, hole, native locator, carrier offset, or
  physical extent appears in the canonical leaf;
- repeated identical object IDs are legal occurrences.

Range routing preserves but never fetches a zero-length occurrence. Subtree
raw lengths may therefore be zero and accumulated logical ends may repeat;
subtree extent counts provide structural progress.

The resolver may return a transient partial slice at a requested range edge:

```text
ResolvedSlice {
    object_id,
    source_offset,
    length,
    logical_offset,
}
```

It must prove `source_offset + length <= raw_length` with checked arithmetic.

### 5.2 Internal descriptor

```text
ChildDescriptorV3 {
    subtree_raw_length:    u64be,
    subtree_extent_count:  u64be,
    child_id:              [u8; 32],
}
```

The descriptor is 48 bytes. It stores local subtree measures, not a
parent-relative cumulative end. This keeps an unchanged later child descriptor
byte-identical when an earlier child changes length.

### 5.3 Leaf bytes

Provisional body:

```text
mapping magic[8]
version u16be = 3
role u8 = FILE_EXTENT_LEAF = 0x02
count u32be
count * ChunkExtentV3
```

Wrapped in the unchanged canonical Phase-1 `Bytes` object. Derived canonical
size is:

```text
28 + 36*count
```

### 5.4 Internal node bytes

Provisional body:

```text
mapping magic[8]
version u16be = 3
role u8 = FILE_EXTENT_INTERNAL = 0x07
level u8
count u32be
count * ChildDescriptorV3
```

Derived canonical size:

```text
29 + 48*count
```

Levels are measured from the leaves so growing a root never relabels an
existing lower node:

```text
leaf output level                    0
internal output level                direct child level + 1
root level                           level of its direct children
maximum internal output level        11
maximum root-to-leaf edges           12
maximum simultaneously active nodes  13
```

The provisional maxima follow from `subtree_extent_count:u64` and the minimum
ordinary fanout of 32: at most `ceil((2^64-1)/32) <= 2^59` leaves reduce to at
most 64 level-11 internal children. The shadow must mechanically reverify this
derivation. Decoders reject an excessive level before allocating or following
a child.

### 5.5 File root bytes

Provisional body:

```text
mapping magic[8]
version u16be = 3
role u8 = FILE_EXTENT_ROOT = 0x01
mode u32be
total_raw_length u64be
total_extent_count u64be
level u8
count u32be
count * ChildDescriptorV3
```

Derived canonical size:

```text
49 + 48*count
```

The unique empty **occurrence sequence** has exactly one root with total length
zero, extent count zero, level zero, and zero children. A nonempty occurrence
sequence whose every extent is zero length has total length zero but nonzero
extent count and at least one leaf. No empty leaf exists.

## 6. Provisional canonical partition rule

This rule is the metadata-shadow hypothesis, not an implemented profile.

For each complete leaf occurrence or internal child descriptor, the shadow
uses this byte-exact predicate:

```text
cut_preimage =
    b"layerfs/g6/cd32-64/cut/v1\0"
    || role_u8
    || output_level_u8
    || u32be(entry_bytes.len)
    || entry_bytes

digest = BLAKE3(cut_preimage)
marker = (digest[0] & 0x1f) == 0

role_u8 = 0x01 for ChunkExtentV3
role_u8 = 0x02 for ChildDescriptorV3
```

`output_level_u8` is zero for a leaf entry stream and is the direct child's
level plus one for an internal descriptor stream. Entry positions are
one-indexed within the current group. Positions 1–31 are ineligible; the first
marker at positions 32–63 closes the node **including the marker entry**;
position 64 closes it inclusively regardless of the marker. Only the final
tail at a level may contain fewer than 32 entries, and it may not contain an
earlier eligible marker. A singleton final tail with siblings is retained.

Under an explicitly illustrative iid-digest assumption, expected occupancy is
`31 + sum(k=0..32, (31/32)^k) = 51.7763` entries and the probability of a
forced cut at 64 is `(31/32)^32 = 36.2055%`. These are estimates, not format
guarantees. The shadow reports natural/forced occupancy and resynchronization
separately at every level. Changing the predicate after observing results
requires a new prospective shadow method; it is not tuning within an attempt.

The role/level domain prevents a leaf entry from silently sharing boundary
semantics with an internal descriptor or another level.

### 6.1 Minimal-root exception

Natural partitioning does not create a redundant top level. At every
prospective parent level, the streaming builder holds the first at most 64
descriptors before emitting an ordinary node:

1. if EOF arrives before descriptor 65 and no ordinary node has already been
   emitted at that level, the file root embeds the complete 0–64 descriptor
   stream directly, regardless of natural markers;
2. when descriptor 65 arrives, the stream is partitioned by the ordinary
   CD32–64 rule, including any buffered marker decisions;
3. this rule repeats until the root is the lowest legal top level;
4. validation rejects a root whose direct internal children can be flattened
   by one level into at most 64 descriptors.

The root exemption, role tags, levels, marker inclusion, and inclusive forced
cut are canonical profile bytes. Tests cover direct-root versus two-internal
adversaries for every top descriptor count 32–64. Mapping codec role tags
`root=0x01`, `leaf=0x02`, and `internal=0x07` reuse the version-separated v2
values and are distinct from the cut-domain `role_u8` tags. Golden vectors
freeze empty-root, one-leaf, and one-internal canonical bytes/ObjectIds before
the shadow.

### Canonical validator

Full validation reconstructs every level's ordered entry stream and rejects:

- a nonfinal node below 32 or above 64;
- a node that passed an earlier eligible marker;
- a nonfinal node not ending at the first eligible marker or forced maximum;
- a final tail containing an earlier eligible marker;
- an alternate singleton/top-level representation;
- a root whose level, total length, extent count, or child summaries disagree;
- any fresh-build/edit-build root mismatch for the same ordered occurrences.

### Hard limitation

The public deterministic marker is a performance heuristic, not an authority
assumption. No-cut, every-cut, repeated-ID, and chosen-marker streams can make
incremental reconstruction continue to EOF. The forced maximum keeps nodes
and memory bounded. It does not remove the hard suffix-linear case.

## 7. Full builder

The builder holds:

- one partial leaf of at most 64 extents;
- one partial internal node of at most 64 descriptors per active level;
- one cut-state value per level;
- checked total length/count;
- bounded CAS/SQL output windows.

For each CDC chunk:

1. construct/authenticate the canonical payload object through the existing
   CAS boundary;
2. append `(raw_length, ObjectId)` to the leaf cut detector;
3. finalize at the first legal natural or forced cut;
4. propagate its `(subtree length, extent count, ObjectId)` descriptor upward;
5. apply the same deterministic rule at every parent level;
6. finalize tails at EOF, emit the minimal root, and bind its
   `CanonicalSegmentationWitness` to the frozen FastCDC construction.

No source-sized occurrence vector or complete decoded mapping is admitted.

## 8. Local splice/update algorithm

Inputs:

```text
authorized edit-base scope and authenticated parent root
CanonicalSegmentationWitness binding the parent profile/root to the frozen
  FastCDC construction/edit chain
CanonicalReplacement binding one normalized atomic mutation plan
expected head
```

`splice` is an internal publication primitive. It never accepts an arbitrary
caller-supplied occurrence sequence or caller-supplied new coordinate.

```text
CanonicalReplacement {
    base_profile
    base_root
    base_segmentation_witness
    islands: BoundedVec<ReplacementIsland, 64>
}

ReplacementIsland {
    old_start: u64
    old_length: u64
    replacement_length: u64
    replacement_source: bounded_stream
    replacement_digest_and_source_binding
}
```

All islands use the pinned base root's half-open old coordinate system. For
normalized island `i`:

```text
old_end_i = old_start_i + old_length_i

delta_before_i
  = sum(j < i, replacement_length_j - old_length_j)

new_start_i = old_start_i + delta_before_i
new_end_i   = new_start_i + replacement_length_i

final_length
  = old_file_length
  - sum(old_length_i)
  + sum(replacement_length_i)
```

Checked signed accumulation may use `i128`; all accepted coordinates and the
final length fit `u64`. Before target writes, structural normalization rejects
unsorted, overlapping, out-of-range, or overflowing inputs; merges adjacent
islands deterministically; combines same-offset zero-length insertions in
declared stream order; removes structurally empty `[x,x) -> empty` islands;
and rejects a 65th real normalized island. Apply the island bound after this
normalization. If no real islands remain, return the prior root/head with no
target objects, transition, witness, writer transaction, or publication
COMMIT. Replacement
sources are already bound by declared length/digest; actual short/long/digest
mismatch during streaming fails before visible publication, with any immutable
unreachable puts counted and no target head. An empty plan is a no-op with zero
target writes and zero publication COMMITs.

The 64-island bound limits compact plan state, not replacement magnitude.
Arbitrarily large replacement bytes stream through owned segments no larger
than 1 MiB. The plan never owns all replacement bytes or all generated
occurrences.

`CanonicalReplacement` is constructed only by the frozen FastCDC edit path.
Each normalized island carries an internally generated transcript binding the
base profile/root/witness, old coordinates, derived new coordinates,
replacement length/digest/source, restart boundary, old/new cursors, emitted
occurrences, and exact rejoin/EOF/fallback outcome. Caller-authored rejoin or
occurrence evidence is rejected. Thus the tree canonicalizes one ordered
occurrence sequence, while the frozen CDC witness establishes the stronger
one-raw-byte-content/one-root property.

`CanonicalSegmentationWitness` is distinct from complete-closure authority. It
is produced by a full frozen-FastCDC build/scrub or inductively by an exact
canonical edit witness, and it binds the exact profile/root. It cannot satisfy
`Verified` object authority, cannot turn Trusted history into Verified
authority, and cannot be inferred from arbitrary valid leaf bytes. A root
without it is readable under its profile but is not an ordinary splice base;
the caller must first perform a complete authenticated FastCDC normalization
or receive the exact noneditable-segmentation error.

Algorithm:

1. authenticate and pin the parent root and every selected path;
2. normalize and validate the complete old-coordinate island plan before
   target writes;
3. locate each first affected leaf using subtree raw lengths;
4. start each CDC influence scan at an authenticated old chunk boundary at or
   before its first changed byte;
5. if one influence scan reaches the next island before exact rejoin,
   deterministically coalesce the islands into one CDC cluster;
6. stream retained prefixes, replacement sources, and authenticated old suffix
   input through frozen FastCDC;
7. accept rejoin only at exact old/new semantic cursor alignment plus an exact
   canonical boundary and authenticated base occurrence identity;
8. emit new leaves and descriptors by the exact partition rule, stopping tree
   replay only at exact node-ID/boundary/cursor convergence;
9. reuse all later old subtrees by `ObjectId` after exact convergence;
10. if a CDC cluster does not rejoin, either fail before publication with the
    typed bounded-resynchronization error or perform one explicit fallback
    from the earliest unresolved cluster; never rescan the suffix per island;
11. construct the new namespace/root/transition plus target
    `CanonicalSegmentationWitness` and qualify publication;
12. consume one ordered construction proof covering retained, deleted, and
    replacement bytes/occurrences exactly once and establishing inductive
    equivalence to a fresh frozen-FastCDC plus fresh-tree build from the
    exact-root base witness, normalized coverage, restart/rejoin boundaries,
    cursor alignment, occurrence order, and output root;
13. expected-head check, one writer transaction, one COMMIT for the complete
    state-changing plan;
14. on uncertain acknowledgement, freshly reconcile requested/prior/different/
    ambiguous.

Correctness never depends on a probable marker match. Rejoin requires exact
semantic cursor alignment, a canonical scanner-reset boundary, and canonical
object identity. Per-island transcripts are ephemeral proof state. They never
enter canonical node bytes, transition identity, or the target segmentation
witness; equal final bytes remain history-independent.

The product operation does not run a fresh full-file FastCDC/tree build.
Focused tests, Stage-A shadow work, and frozen benchmark oracles may compare
against an actual independent full build outside the product ACK path and its
timers.

If a nonempty input plan nevertheless constructs the exact prior root, return
the typed `NoChange` result. Roll back the one begun writer transaction and all
staged SQLite CAS work: zero transition/head/witness writes, zero publication
COMMITs, zero retained new/unreachable objects, and exact terminal transaction,
Q, and scope cleanup. This decision is made from the constructed canonical
root, not by trusting caller claims.

Supported shapes:

- same-size replacement;
- early/middle/late insertion;
- early/middle/late deletion;
- arbitrary shorter/longer replacement;
- up to 64 normalized atomic mutation islands;
- append and truncate;
- net-zero insert/delete shifts;
- replacement spanning multiple leaves;
- complete-file deletion to the unique empty root.

For normalized island count `k<=64`, coalesced CDC cluster count `c<=k`, total
streamed replacement bytes `R`, unique CDC bytes `C`, leaf replay `D_l`, and
descriptor replay `D_j` at level `j`, the ordinary target is:

```text
O(k*H + C + D_l + sum(D_j))

expected ordinary locality
  O(R + sum(local_resynchronization_i) + k*H)
```

This is file-size insensitive for a fixed mutation shape, not mutation-size
insensitive. Deletion volume is reported separately and need not be fetched
byte-for-byte when authenticated measured subtrees prove removal. A successful
earliest-unresolved raw fallback is
`Theta(R_remaining + raw_suffix_bytes + extent_suffix_entries)`; a bounded
fail-closed attempt publishes nothing.

## 9. Portable extent resolver

The first stable engine boundary is intentionally small:

```text
resolve_byte_range(snapshot_read_scope, root, start, end, emit_slice)
stream_extents(snapshot_read_scope, root, emit_extent)
splice(edit_base_scope, root, CanonicalReplacement, expected_head)
diff_splices(parent_scope, parent_root, target_scope, target_root, emit_splice)
```

No adapter registry is required.

Every selective operation carries explicit authority. A `Verified`
`snapshot_read_scope`, `edit_base_scope`, or `parent_scope` contains a valid
complete-closure receipt for the exact root. If no such receipt exists, the
Store performs a fresh complete authenticated scrub before selective traversal
or returns the exact authority error. `TrustedLocalDev` may instead use only
the explicit Store-lifetime assumption authorized by final G5. That assumption
never becomes Verified authority. Every mapping or payload object actually
fetched remains identity-authenticated in both modes.

### Range resolution

1. validate `start <= end <= root.total_raw_length`;
2. authenticate the complete root and establish the scope authority before
   trusting measures of unopened descendants;
3. scan the bounded child descriptors, accumulating local subtree lengths;
4. authenticate only intersecting internal nodes/leaves;
5. validate every selected subtree length/count against authenticated child
   content;
6. collect at most one bounded batch of payload IDs at a time;
7. authenticate every fetched payload object before exposing any slice;
8. emit slices in exact logical order;
9. prove emitted coverage equals the request with no gap/overlap;
10. release all Q ownership on success and every failure.

An empty range still authenticates the root and emits zero bytes.

### Cursor and batching

- one root-to-leaf path;
- one current leaf;
- one payload-ID batch, provisionally at most 64;
- one output buffer at most 1 MiB;
- bounded readahead only after sequential access is directly established;
- no query per output extent when a bounded `get_many_authenticated` path can
  preserve exact per-object validation;
- no source-sized decoded-node or prefix-offset cache.

### Internal range proof behavior

The first implementation need not define a public compressed proof format. Its
verification transcript contains:

- exact root canonical bytes;
- every selected internal/leaf canonical object;
- complete sibling descriptors contained by those authenticated nodes;
- every selected canonical payload object;
- requested range and exact emitted coverage.

A verifier recomputes every `ObjectId`, role, level, cut rule, subtree summary,
root total, and output slice. Any future portable proof serialization must be a
separate versioned format over this same transcript.

## 10. Structural diff

Count-changing projection requires both coordinate systems:

```text
DiffSplice {
    old_start: u64,
    old_length: u64,
    new_start: u64,
    new_length: u64,
    replacement_extent_plan: bounded_or_streamed,
}
```

`diff_splices(parent_scope, parent, target_scope, target, emit_splice)` compares
authorized authenticated roots:

- equal node IDs prune complete equal subtrees while retaining their distinct
  old and new logical bases;
- unequal local subtree lengths shift later logical starts but do not force
  payload traversal;
- unequal leaves compare ordered `(length, ObjectId)` occurrences and emit
  paired replacement splices;
- old spans are sorted/nonoverlapping in the old coordinate system and new
  spans are sorted/nonoverlapping in the new coordinate system;
- same-size patch ranges derive only from splices with equal old/new lengths;
- replacement plans are streamed or bounded, never collected without a cap;
- if the cap would be exceeded, return `FullProjectionRequired`.

The emitted diff is recomputed from the authenticated parent and target roots;
it is never copied from the caller's `CanonicalReplacement`. CDC influence can
expand or merge changed extent ranges, and latest projection may skip multiple
committed revisions. Output coalescing is legal only when old and new
coordinate systems are both compatible. The output cap is independent of the
64-island input cap.

For latest projection coalescing, retain only the final pending target root.
When it becomes active, diff from the worker's current root to that target.
Never concatenate ranges expressed in different revision coordinate systems.

## 11. Concurrency model

### Canonical readers and writer

- one ordered writer connection;
- immutable roots permit concurrent readers;
- every open/read pins one exact root for its entire operation;
- a reader opened before COMMIT may finish on the old root;
- a reader opened after successful COMMIT observes the new root;
- a “latest” lookup resolves the visible head once, then becomes exact;
- no mid-stream root switch;
- missing old-root objects while a reader is pinned is a corruption/GC error;
  GC is outside G6.

### Projection queue

- at most one in-flight request;
- at most one pending target root;
- exact requests are never replaced;
- only pending latest-following work may be replaced by a newer latest target;
- submitted/coalesced/started/published/cancelled/failed/stale equations close;
- worker projection performs zero canonical SQLite writer transactions and
  zero canonical COMMITs;
- canonical publication already happened before projection enqueue.

## 12. Trust and authentication

- `Verified` is the default Store-lifetime policy.
- `TrustedLocalDev` is explicit and may skip only the eager current/parent
  closure scrub authorized by the final G5 contract.
- every fetched/new/incumbent payload and mapping object identity check remains
  unconditional in both modes;
- trusted assumptions never become verified receipt-covered authority;
- no verified carry-forward after trusted history;
- verified reopen after trusted history performs a complete scrub;
- rollback freshness remains `NotProtected` without external monotonic
  authority;
- root pinning and range resolution do not infer authority from SQLite keys,
  node measures, native files, cache hits, or platform events.

### Segmentation-witness lifecycle

`CanonicalSegmentationWitness` is root-and-profile-bound authenticated
publication metadata. It is stored with the visible transition/receipt in the
same SQLite database, writer transaction, and publication COMMIT; it is not a
canonical mapping object, not a second authority/database, and not a
complete-closure or Verified receipt. Reopen decodes and authenticates the
witness before admitting the root as an edit base. Missing, wrong-profile, or
wrong-root state yields the exact noneditable-segmentation error or triggers an
explicit complete normalization path—never an inferred success.

Fresh requested/prior/different/ambiguous reconciliation reads the visible
head, profile, transition/receipt, and segmentation witness together and
compares the requested tuple. A requested root with a missing/mismatched
witness is not requested-visible. The exact witness codec/schema and retained
old-root lookup lifecycle remain **Unavailable pending the sealed G5 engine
boundary** and must be frozen before Stage B; this unresolved representation
does not block the metadata-only Stage-A shadow.

## 13. Canonical publication and timers

### Publication

```text
target immutable objects/nodes staged and authenticated
  -> target root/transition and segmentation witness constructed
  -> expected head verified
  -> one visible-head/profile/transition/witness write set
  -> one COMMIT dispatch
  -> return or fresh reconciliation
  -> canonical ACK
```

No projection work is moved before COMMIT merely to improve a timer.

### Required timer endpoints

```text
t0  edit request start
t1  canonical construction complete
t2  COMMIT dispatch
t3  COMMIT return
t4  reconciliation complete / canonical ACK
t5  projection enqueue
t6  projection worker start
t7  virtual root installed
t8  first requested range returned
t9  native projection dispatch
t10 native projection durable ACK
t11 native projection complete after seed install/cleanup/residue
t12 cold full materialization complete
t13 complete operation wall
T0  complete campaign start
T1  complete campaign end
```

Equations:

```text
canonical_ack
  = canonical_construction
  + precommit_qualification
  + commit_dispatch_to_return
  + postreturn_wrapper
  + reconciliation_if_needed

edit_to_virtual_visible
  = canonical_ack
  + projection_dispatch
  + queue_wait
  + virtual_root_install

first_range_return
  = edit_to_virtual_visible
  + tree_resolution
  + CAS_fetch_and_authentication
  + caller_delivery

native_durable_ack
  = seed_validation
  + clone_or_create
  + patch_shift_reflink_or_stream
  + data_sync
  + metadata_sync
  + rename
  + directory_sync
  + fresh_reconciliation_if_needed
  + postpublication_descriptor_and_root_verification

native_projection_complete
  = native_durable_ack
  + successor_seed_installation
  + temp_and_descriptor_cleanup
  + residue_verification

complete_campaign_wall = T1 - T0
```

All reported timers are endpoint differences such as `t4-t0`, `t7-t0`,
`t8-t0`, `t10-t9` (native durable ACK), `t11-t9` (native completion),
`t12-t9` (cold-full route completion), `t13-t0` (complete operation), and
`T1-T0`. Nested intervals are never added twice. Canonical ACK, virtual
visibility, first range, native durability, native completion, and cold full
materialization are separate claims.

## 14. Native projection adapters

Canonical identity is platform-independent. Route selection occurs only after
the target root and authenticated diff exist.

| Capability/condition | Physical route | Required label |
|---|---|---|
| virtual SDK/VFS | Resolver only | `VirtualNoNativeFile` |
| same length + exact protected seed + bounded ranges | whole clone + sparse patch | `CloneSparsePatch` |
| exact seed + insertion at EOF | private whole clone + append changed bytes | `TailAppend` |
| exact seed + deletion through EOF | private whole clone + truncate | `TailTruncate` |
| APFS non-tail count change + admitted suffix | whole clone + bounded suffix shift + patch | `CloneShiftPatch` |
| Linux aligned range reflink | reflink prefix/suffix + patch boundary blocks | `RangeReflinkSplice` |
| Linux aligned insert/collapse | clone + extent operation + patch | `InsertCollapsePatch` |
| unsupported/cap/authority failure | sequential authenticated reconstruction | `FullFallback` |
| explicit standalone export | sequential authenticated materialization | `ColdFullExport` or honest cache class |

Every native route preserves:

```text
exact immutable source/seed
private temp creation/clone
bounded write/shift/reflink/stream
data fsync
metadata application and sync
atomic rename
parent-directory fsync
fresh old/new/different/ambiguous reconciliation
exact cleanup/residue accounting
```

For one normalized splice over parent length `B`, old span `[a,b)`,
replacement length `N`, signed delta `N-(b-a)`, target length `T`, and surviving
old suffix `S=B-b`:

```text
TailAppend       requires a=b=B, N>0, S=0; changed work Theta(N)
TailTruncate     requires N=0, b=B, S=0
CloneShiftPatch is Theta(S+N); observed shift reads/writes are S/S
FullFallback     is Theta(T+E) with Omega(T) destination logical writes
ColdFullExport   is Theta(T+E) with Omega(T) destination logical writes
```

`shifted_suffix_bytes` is `S`; combined native logical transfer for APFS shift
plus patch is derived as `2*S+N`. Neither quantity is physical I/O.

`CloneShiftPatch` uses overlap-safe bounded checked copies on its private
clone. For positive delta it first extends to `T`, then moves the surviving
suffix backward from high offsets to low offsets before patching the `N`
replacement bytes. For negative delta it moves the suffix forward from low
offsets to high offsets, patches replacement bytes, then truncates to `T`.
Zero delta is not this route; use the exact position-preserving or normalized
multi-splice route. Every partial `pread`/`pwrite` and source/destination offset
is checked, and publication is forbidden until the complete private target
verifies.

Stage-B preflight freezes the `NativeCapabilitySetV1` schema, product-wrapper
probe, conditional schedule, and environment envelope. Each actual screen/gate
captures and hashes the receipt inside its locked complete wall using tiny
disposable files in the real destination directory. It binds OS/kernel,
filesystem/mount/device identity, block sizes, whole clone, range reflink,
insert/collapse support and exact errno/granularity, clone isolation, and probe
cleanup. Conditional cells emit their scheduled `NotApplicable` result when
absent; they are never replaced after timing.

Linux selection reports exactly one route. Frozen precedence is tail routes,
then `InsertCollapsePatch` when whole-clone plus aligned fallocate is proven,
then `RangeReflinkSplice` when aligned reflink is proven, then
`FullFallback`. The unselected Linux mechanism is
`NotApplicable(NotSelectedByFrozenPrecedence)`, never a slash-combined route.

Every route records requested route, selected route, eligibility, capability
hash, route-execution outcome, fallback source/reason, and the separate
publication outcome. The route-execution vocabulary is closed:

```text
CompletedSelectedRoute
NotApplicablePlatform
NotApplicableCapability
NotSelectedByFrozenPrecedence
IneligibleGeometry
IneligibleAlignment
SuffixCapExceeded
AcceleratorFailedPrivateTempDiscarded
FallbackCompletedFromFreshTemp
VerificationFailedPrivateTempDiscarded
```

The publication-outcome vocabulary is separately closed:

```text
NoPublicationAttempted
RequestedVisibleWithoutReconciliation
RequestedVisibleAfterReconciliation
PriorVisibleAfterReconciliation
DifferentVisibleAfterReconciliation
AmbiguousAfterReconciliation
```

`NotApplicable*` route outcomes imply `NoPublicationAttempted`. An
acknowledged successful native publication implies
`RequestedVisibleWithoutReconciliation`. An ambiguous/error return requiring
fresh observation yields exactly one of the four `*AfterReconciliation`
outcomes.

An unexpected
accelerator failure discards its private temp; fallback, when allowed, starts
from a fresh temp. If visibility may already have changed, reconciliation runs
before any further publication and no second native publication is attempted.

OS-specific facts never change canonical node bytes or roots. APFS has a
documented whole-file clone API but no documented public arbitrary-range clone
in that interface. Linux reflink and insert/collapse operations are
filesystem/alignment-dependent. Capability absence follows the frozen
preflight route; an unexpected runtime failure goes directly to explicit fresh
`FullFallback` or fails closed, never opportunistically races accelerators.

## 15. Resource contract

Provisional hard rules for the shadow and later candidate:

- no `O(file bytes)` or `O(extent count)` resident mapping;
- node count and bytes bounded by min/max rules;
- resolver additional live `Q <=4 MiB` on scheduled ranges;
- individual owned buffer `<=1,048,576` bytes;
- at most 64 normalized compact mutation-island descriptors;
- one live replacement input segment `<=1,048,576` bytes and one 32-KiB
  FastCDC chunk buffer; arbitrary total replacement magnitude remains streamed;
- one active coalesced CDC influence cluster, not one suffix buffer per island;
- no unbounded pending extent/range/history/proof vector;
- decoded-node cache bounded by both count and bytes, or absent initially;
- simultaneously live decoded descriptor count
  `O(max_node_entries * active_height + frozen batch capacity)`; total
  persistent descriptor count remains `Theta(E)`;
- every capacity charged before allocation and released by its real owner;
- terminal Q, transaction, scope, worker, in-flight, pending, descriptor,
  temp, journal, and residue state exactly zero where applicable;
- live canonical mapping `<1%` of logical bytes for ordinary K64 files;
- stretch live canonical mapping `<=0.30%`;
- no complete mapping duplication per ordinary retained revision;
- exact logical/apparent/allocated storage observations kept separate;
- physical I/O, stable media, cache residency, and continuous peaks remain
  Unavailable unless a direct source observes them.

The final RSS ceiling is pending the sealed G5 process shape.

## 16. Direct counters

Every applicable row emits:

- file size, mutation case/magnitude/position, raw-byte delta `DeltaB`, old/new
  occurrence counts and independent occurrence delta `DeltaE`, and exact route;
- input/normalized island count, CDC cluster count, logical/CDC coalescing,
  rejection index/reason, and final-length equation;
- per-island old start/length, derived new start/length, cumulative delta
  before/after, replacement declared/read/hashed bytes, and source digest;
- CDC restart boundary/predecessor, old prefix/replacement/suffix-probe bytes,
  unique and summed CDC bytes scanned, old/new cursor rejoin, and exact
  rejoin/EOF-local/coalesced/bounded-fail/full-fallback class;
- old/new/local occurrence counts and emitted replacement extent count;
- new/reused/authenticated/deleted-logical/deleted-fetched payload objects and
  bytes;
- tree height before/after;
- unique leaf/internal/root nodes read, authenticated, created, reused, written,
  plus shared ancestors across islands;
- encoded node bytes by level;
- natural/forced cuts and occurrence/descriptor replay to rejoin by level;
- split/merge/root grow/root shrink;
- unchanged subtree reuse count and covered logical bytes;
- shifted-but-reused logical bytes for net-zero mutations;
- suffix payload objects fetched/written;
- suffix mapping nodes rewritten;
- range fragments, mapping fetches, CAS fetches/batches, authenticated and
  returned bytes;
- workload SQL separately from instrumentation SQL;
- transactions, COMMIT dispatches, returns, errors, reconciliation calls;
- segmentation-witness decode/authenticate/read/write and reconciliation tuple;
- projection requested/selected/outcome route, capability-set hash,
  fallback-from/reason, native parent/target/splice geometry, logical
  read/write/clone/reflink/shift/full-fallback bytes;
- tail append payload fetch/write, truncate/extend calls, APFS shift read/write,
  Linux ioctl arguments/results/errno and shared/boundary bytes;
- data/metadata/directory sync and rename calls;
- Q component/current/high-water/terminal, RSS, buffer capacity, descriptors;
- logical/apparent/allocated DB/journal/authority/native endpoints;
- current-live, retained-union, unreachable, and per-revision history bytes;
- node fill, mapping fragmentation, and extents per 1-MiB read.

Every field is exactly one of `Observed(source/API)`, `Derived(equation)`,
`NotApplicable(reason)`, or `Unavailable(reason/source)`.

## 17. Correctness and fault matrix

### Codec/topology

- empty file and one-byte file;
- 31/32/33 and 63/64/65 extents;
- exact internal boundaries at `32*32-1/32*32/32*32+1` and
  `64*64-1/64*64/64*64+1` extents;
- root grow and shrink;
- early/middle/late insertion and deletion;
- raw mutation cases whose frozen oracle yields `DeltaE<0`, `=0`, and `>0`;
- 0/1/63/64/65 real normalized islands; one zero-effect island; 64 real
  islands plus discarded zero-effect islands; unsorted, overlapping,
  adjacent, and same-offset zero-length insertions; normalization to an empty
  no-op or one island;
- nonempty semantic same-root `NoChange` with one rolled-back writer
  transaction, zero publication COMMITs, zero retained objects, and exact
  terminal cleanup; failure during discovery has the same no-publication
  closure;
- old/new coordinate overflow, final-length underflow, declared-length/digest/
  source mismatch, replacement short read/error/cancellation;
- independent and CDC-overlapping islands, cluster coalescing, first-island
  no-rejoin before the next island, and one earliest-unresolved fallback;
- multi-leaf replacement;
- atomic two-island net-zero shift and atomic four-island mixed mutation;
- standalone shorter and longer replacement;
- complete-file deletion;
- append and truncate;
- deterministic natural/forced split and deterministic merge after deletion;
- distinct edit histories reaching identical final occurrences/root;
- insert then delete returns the prior root;
- two different valid edit plans reaching equal final raw bytes produce the
  same occurrence sequence, mapping root, and segmentation witness;
- repeated identical payload IDs;
- no-cut, every-cut, marker-at-min, marker-before-max, forced-max streams;
- alternate encoding, invalid final tail, redundant top level;
- legal zero-length occurrences preserved in identity/counts and skipped by
  nonempty range intersection; oversized extent rejected;
- identical raw bytes with an alternate valid chunk segmentation rejected at
  publication or reconstructed only through the frozen canonical FastCDC
  witness;
- missing/wrong-root segmentation witness and attempted edit of an optional
  read-only legacy-preserved conversion rejected before target writes;
- wrong subtree length/count/level;
- gap/overlap/out-of-range transient slice;
- wrong role, missing object, identity mismatch, cycle, malformed node,
  over-depth, overflow.

### Publication/concurrency/trust

- stale expected head;
- old-root reader across concurrent COMMIT;
- new-root reader after COMMIT;
- fault before COMMIT;
- COMMIT return success/error;
- lost acknowledgement reconciling requested/prior/different/ambiguous;
- trusted assumption never becoming verified receipt authority;
- verified reopen after trusted history scrubs;
- cancellation during traversal/node write/COMMIT;
- exact Q and transaction cleanup on every exit.

### Projection/native

- exact request versus latest coalescing;
- projection failure after canonical ACK;
- count change with virtual visibility and zero native bytes;
- invalid seed/cap/capability fallback;
- clone/reflink/insert alignment failure;
- TailAppend/TailTruncate eligibility, APFS forward/backward shift, frozen
  Linux route precedence, route NotApplicable, and forced FullFallback;
- accelerator-private-temp discard before fresh fallback and no fallback after
  ambiguous visible publication without reconciliation;
- native fault before rename;
- rename lost ACK;
- directory-sync lost ACK and retry failure;
- restart with pending projection;
- destination wrong-kind/symlink/substitution;
- terminal worker/queue/descriptor/temp/residue zero.

## 18. Migration

### First candidate

- fresh isolated benchmark-private v3 store/profile only;
- no existing-store mutation;
- no existing v2-to-v3 conversion is implemented in the first candidate;
- v2 remains readable through its exact v2 profile and identities;
- payload canonical objects remain byte-identical and may be reused only under
  exact store authority;
- fresh v3 reconstruction, ranges, roots, segmentation witnesses, and fault
  behavior are independently verified;
- original v2 store retained byte-for-byte as rollback/read authority.

### Later migration analysis

- explicit old/new profile dispatch;
- receipts and visible heads bind the exact profile;
- old writers reject v3 heads;
- no v2 parent to v3 child edge without an explicit cross-profile migration
  transition;
- the only supported path to an **editable** v3 head is a complete
  authenticated raw-byte reconstruction followed by a fresh frozen-FastCDC v3
  build and `CanonicalSegmentationWitness`; this intentionally normalizes away
  legacy alternate segmentation/zero-length occurrences and creates new
  mapping/root identities under an explicit transition;
- an optional occurrence-preserving conversion, if ever required, uses a
  distinct read-only legacy-preserved profile, retains every repeated and
  zero-length occurrence, and is rejected as an ordinary v3 splice base;
- conversion never widens the ordinary splice API to arbitrary segmentation;
- no silent profile-field flip or identity reinterpretation;
- interruption exposes exactly prior v2 or requested v3 head;
- old retained roots remain readable;
- no eager historical rewrite in the first migration;
- storage high-water reports old mapping + new mapping + journal/temp;
- no payload duplication merely because mapping version changes;
- downgrade is forbidden; rollback is an explicit supported transition, not
  binary rollback;
- mixed-profile GC remains out of scope.

Projection-only shadow data is non-authoritative and deleted after its exact
research custody requirement; it cannot become a second durable mapping.

## 19. Optimization targets

Targets remain prospective until the G5 baseline is sealed. Current values are
planning anchors, not a frozen G6 acceptance contract.

### Algorithmic/work targets

| Target | Required | Stretch |
|---|---:|---:|
| Structural occurrence-count `+1/-1` fixed-radix mapping reduction | at least 10x against the same frozen A row | at least 20x |
| Raw mutation with frozen oracle `DeltaE != 0`, non-tail, local route | candidate file-mapping bytes `<=floor(A/10)` | direct work explained by replacement extents plus unique paths |
| Raw mutation with `DeltaE = 0` | protect current changed-spine behavior plus exact format delta | improve |
| Arbitrary-size single/multi-island canonical work | `O(k*H + unique CDC scan + unique tree replay)` on ordinary local route | near `O(R + local resync + kH)` |
| Unchanged suffix payload fetch/write on qualifying fast route | exactly 0 / 0 | same |
| Unchanged suffix subtree rewrite after exact rejoin | exactly 0 | same |
| Tree height | bounded by min fanout | no extra level versus model |
| Virtual count-change native full reconstruction | exactly 0 | same |
| TailAppend/TailTruncate shifted suffix | exactly 0 | same |
| APFS middle CloneShiftPatch shifted suffix | exact `B-b`, read/write counters exact | lower only by changing physical route, not canonical claim |
| FullFallback classified as native fast route | forbidden | same |
| Writer transactions / publication COMMITs | state-changing `1/1`; normalized-empty `0/0`; semantic same-root `1/0` rolled back | same |
| Terminal Q | exactly 0 | same |

Fallback rows report complete suffix work and never count toward the local
claim.

### User-visible targets

| Operation | Required planning target | Stretch |
|---|---:|---:|
| 1/10/100-MiB raw-mutation canonical ACK p50 | `<=20 ms` | `<=10 ms` |
| Raw-mutation canonical ACK p95 | `<=30 ms` | `<=15 ms` |
| Edit to virtual-visible p50/p95 | `<=20/30 ms` | `<=10/15 ms` |
| First 4-KiB range after virtual visibility | `<=5 ms` | `<=3 ms` |
| 1-to-100-MiB growth for the same mutation magnitude/shape | explained by height/local counters, not base-file suffix | near-flat direct work |
| 1-B to 1-MiB mutation growth | explained by replacement/CDC work | never claimed magnitude-independent |

The approximately 5-ms current 100-MiB result is structural occurrence-edit
evidence, not a raw mutation baseline. New raw operations use the prospectively
frozen same-semantic A arm. The provisional raw local-competitiveness allowance
is at most A +5 ms absolute at 100 MiB; protected existing operations continue
to use sealed G5.

Latency populations are separate for every
`(size, operation, position, route_class)` cell. Qualifying local and explicit
fallback rows are never pooled. With two or three observations, all values are
reported. With one observation, p50=p95=that value and the result is labeled a
single-observation semantic smoke, not population inference. For two sorted
values, p50 is the checked floor midpoint and p95 is the maximum; for three,
p50 is the middle value and p95 is the maximum. Every primary 100-MiB cell must
pass independently.

### Protected-regression rule

After G5 seals, freeze per operation:

```text
if control >= 5 ms:
    candidate <= control * 1.05
else:
    candidate <= control + 1.000 ms
```

For each protected operation, `control` and `candidate` are the checked
arithmetic means of their prospectively scheduled arms; every raw arm and
paired delta remains reported. One-pair operations reduce to the direct arm
comparison, while the compound first-after-reopen cell uses its balanced
two-pair means.

Current G4 values are valid observed planning anchors, but authority is
qualified: the v12 measured terminal remains `REVISE` under its frozen
percentage-only rule; a separate stage-level PASS used the user-approved
sub-1-ms materiality rule. The original measured gate was never relabeled.
These anchors illustrate, but do not freeze, prospective G6 ceilings:

| Protected operation | G4 anchor | Illustrative ceiling |
|---|---:|---:|
| full create | 279.463 ms | 293.436 ms |
| same-count edit | 8.043 ms | 8.445 ms |
| warm reconstruction | 237.214 ms | 249.075 ms |
| fresh reconstruction | 237.381 ms | 249.250 ms |
| full native materialization | 307.652 ms | 323.035 ms |
| reopen/head | 3.583 ms | 4.583 ms |
| returned 1-MiB range | 2.046 ms | 3.046 ms |
| one-byte incremental materialization | 4.104 ms | 5.104 ms |

Correctness, identity, durability, exact error, one-COMMIT, storage, Q, and
custody failures remain hard regardless of timer size.

## 20. Anti-cheat and promotion readiness

The later candidate must:

- live in the reusable engine/core path used by the SDK-shaped consumer;
- never copy semantic implementation into `src/bin`;
- accept arbitrary roots, sizes, positions, and repeated IDs;
- contain no fixture hash/root/digest/size special case;
- use the same product bytes in tests, screen, and gate;
- keep control/candidate inputs, process shape, cache class, trust mode,
  durability, requested operation, and output equal;
- distinguish raw-byte mutation from structural occurrence insertion; never
  compare a raw `+1 byte` candidate to CP-0008's synthetic `+1 occurrence`;
- derive every raw target occurrence manifest from an independent full frozen-
  FastCDC oracle shared by A and C; each profile-specific root matches its own
  frozen oracle while logical bytes/digest remain equal;
- recompute projection `DiffSplice`s from authenticated roots rather than reuse
  input mutation islands;
- observe counters rather than emit literal/synthetic successes;
- include preparation/preconditioning in complete wall and label cache class;
- keep canonical ACK, virtual visibility, and native durability distinct;
- never call native full fallback virtual projection;
- never call a projection-only index a canonical Big-O improvement;
- preserve failed attempts and never relax thresholds after rows exist;
- prove the implementation can be consumed later without reimplementation.

## 21. Entrance and stop rules

Under the updated responsibility boundary, even Stage-A shadow execution waits
for terminal G5; until then G6 remains documentation/research only.

Implementation remains forbidden until all are true:

```text
G5 terminal PASS sealed
metadata shadow exact roots/history/adversaries PASS
selected cut predicate/profile bytes frozen
reusable engine/schema boundary selected
segmentation-witness codec/schema/reopen lifecycle frozen
G6 preregistration amended to exact baseline hashes and gates
```

Before any Stage-C/D product screen, the same frozen candidate bytes must also
expose a reusable `layerfs-vfs`-shaped virtual-root endpoint and a native
materializer that consumes the same engine resolver, with a focused
promotion-readiness audit PASS. If final G5 does not supply those reusable
boundaries, a separate prospective integration amendment is required before
Stage C/D. A benchmark-local virtual endpoint or copied materializer is never
acceptable.

Authorized G6 work is sequential so causal attribution is retained:

1. **G6-T — canonical tree/resolver:** v3 codec/validator, streaming full
   builder, bounded atomic `CanonicalReplacement`, explicit earliest-unresolved
   fallback, range resolver, generalized construction proof, and focused
   identity/fault/Q tests.
2. **G6-V — virtual count-change:** the reusable virtual-root endpoint consumes
   the frozen G6-T resolver and proves `t7/t8` without native reconstruction.
3. **G6-N — native variable size:** TailAppend, TailTruncate, APFS
   CloneShiftPatch, frozen Linux route selection, FullFallback, and the shared
   durable publisher are added against the frozen G6-T/V base. G5 supplies the
   frozen authenticated `FullFallback` implementation and durable publisher;
   G6-N owns variable-size eligibility, selection, counters, and reuse of that
   fallback for G6 target roots, not a reimplementation or weaker substitute.
4. **Combined screen/gate:** one `<20 s` screen and one `<=150 s` measured gate
   use the final frozen bytes and protect G5 operations.

The implementation still contains no production mount/public SDK integration,
existing-store migration, compression, GC, or generalized provider framework.

Current status is **`G6_SPEC_READY_PENDING_G5_BASELINE`**. G6 implementation
and measurement are not authorized before terminal G5 PASS and the frozen
Stage-A shadow PASS.
