# File construction, localized edits and reads

> **Status:** Research; informative and not a product contract.

API addition after `f5558fc22`: content provides `construct_runs` over the
bounded `FileRuns` data/zero/EOF source. It preserves the ordinary streamed root
while reusing complete zero subtrees in logarithmic hole work. See
[payload streams](31-payload-streams.md) and its canonical equality evidence.
Earlier source descriptions/evidence below retain their original scope.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

The #252 Service input adapter in `layerfs-server/src/service/save/file_stream.rs`
describes the product source in the same commit as this note. It accepts one
validated frozen final `Base`/`Local`/`Zero` extent sequence, derives ordered C1
edits in a bounded disk spool for existing files, and calls the unchanged C1
canonical edit builder. A fresh file uses the same wire operation and C1's
streaming constructor. This changes the Service input route, not the file's
canonical format or C1 partition rules; older sections keep their source pin.

---

## 3. File construction (C1)

### 3.1 The representation dispatch

`core/crates/layerfs-content/src/contract/policy.rs` owns one selector:

```rust
pub const fn representation(self, logical_len: u64) -> Representation {
    if logical_len == 0                                  { Representation::Empty }
    else if logical_len < self.small_file_threshold_bytes { Representation::WholeFile }
    else                                                  { Representation::Chunked }
}
```

```text
                          logical_len
                               │
              ┌────────────────┼────────────────┐
              │                │                │
           == 0             < cutoff         >= cutoff
              │                │                │
              ▼                ▼                ▼
        ┌──────────┐    ┌─────────────┐   ┌──────────────────────┐
        │  Empty   │    │ WholeFile   │   │      Chunked         │
        │          │    │             │   │                      │
        │ defined  │    │ ONE object  │   │ FileState root over  │
        │ empty    │    │ LFS5SML\0   │   │ a CDC extent tree    │
        │ file-    │    │ value       │   │                      │
        │ state    │    │             │   │                      │
        └──────────┘    └─────────────┘   └──────────────────────┘

   cutoff = small_file_threshold_bytes
          default 131_072 (128 KiB)
          accepted range 131_072 ..= 1_048_576, POWER OF TWO ONLY
```

The cutoff is genuinely configurable and **validated before any work**: a
non-power-of-two cutoff is refused at the entry point, not at the first object
that happens to depend on it. The source states the reason a non-power-of-two
cutoff is rejected: it "would make the transition probe ambiguous", since the
streaming constructor probes exactly `cutoff` bytes to decide the branch.

`representation()` is `const` and accepts any candidate; `validated()` is a
separate call that enforces the ranges. `capacities()` documents that it can be
reached with a policy nobody accepted — including the zero cutoff in `new`'s own
doc example — and therefore derives the whole-file limit with a **saturating**
subtraction, asserting the invariant in debug builds rather than computing a
wrapped limit.

### 3.2 Whole-file form

```text
   value layout (before the LFSO envelope wraps it):

     0        8        10                   10 + payload
   ┌────────┬────────┬──────────────────────────────────┐
   │LFS5SML\0│ ver u16│            payload               │
   │  8 B   │  = 1   │                                  │
   └────────┴────────┴──────────────────────────────────┘
     WHOLE_VALUE_HEADER = 10
```

A whole-file object costs `WHOLE_FILE_CANONICAL_OVERHEAD = 23` bytes over its raw
payload: the 13-byte bytes-role envelope plus the 10-byte whole-file value header.
The chunk lane's equivalent is 21 bytes, because a chunk value carries only its
eight-byte magic.

`encode_whole_file` documents its own peak honestly, and the comment is worth
quoting because it is the opposite of a performance claim:

> The cut this comment used to claim is not implemented: the value is built in its
> own allocation and `encode_bytes_object` allocates the canonical object around
> it, so **the peak is roughly twice the value plus its framing**. The memory
> ledger states that figure; the comment no longer claims otherwise.

### 3.3 Streaming probe

`construct_stream` handles a source of unknown length by probing a bounded prefix:

```text
   source (unknown length)
        │
        │  read_at_most(source, cutoff)          ← bounded prefix probe
        ▼
   prefix.len() < cutoff ?
        ├── yes ──► construct_bytes_in(prefix)   definitive: EOF reached below cutoff
        └── no  ──► Cursor::new(prefix).chain(source) ──► construct_chunked(...)
```

The frozen cutoff bounds the probe: at most `cutoff` bytes are buffered before the
scanner takes over. The source is explicit that this is "an ordinary bounded read
of the stable source, **not a retry**", and that a short read reaching end of input
below the cutoff is a definitive result.

### 3.4 Frozen CDC — two-byte rolling GEAR

`core/crates/layerfs-content/src/file/cdc/gear.rs`

```text
   MINIMUM_CHUNK_BYTES =  8_192   (8 KiB)
   TARGET_CHUNK_BYTES  = 16_384   (16 KiB)
   MAXIMUM_CHUNK_BYTES = 32_768   (32 KiB)
   NORMALIZATION_SHIFT = 2
   PROFILE_SEED        = 0

   SMALL_MASK         = 0x0000_d903_0353_7000
   LARGE_MASK         = 0x0000_d901_0353_0000
   SHIFTED_SMALL_MASK = 0x0001_b206_06a6_e000
   SHIFTED_LARGE_MASK = 0x0001_b202_06a6_0000
```

The scanner keeps **one owned buffer of exactly `MAXIMUM_CHUNK_BYTES`** and never
retains the whole input. `profile_id()` hashes the label, every constant, all four
masks **and all 256 gear multipliers** into a 32-byte digest, which is pinned
inside every `FileState`. Any drift is a `ProfileMismatch` on decode — never silent
re-chunking.

The canonical builder is `layerfs-content/src/file/mapping/build.rs`; chunk objects
are framed with the mapping layer's own chunk encoding. All chunk payloads are
bounded by the CDC grammar's maximum, so a slice reaching past 32 KiB is refused
when the slice is constructed rather than deferred to the read that first uses it.

### 3.5 The extent tree

`core/crates/layerfs-content/src/file/mapping/types.rs`

```text
   FileState                       ← canonical root, role 5
     logical_len    : u64
     extent_count   : u64
     tree_level     : u8
     profile_id     : ObjectId      ← pinned frozen CDC profile
     mapping_root   : ObjectId

   Branch { level, subtree_logical_bytes, subtree_extent_count,
            children: [ChildDescriptor] }          ← role 4

     ChildDescriptor { cumulative_logical_end : u64,   ← cumulative, not per-child
                       cumulative_extent_end  : u64,
                       child_object_id        : ObjectId }

   Leaf { subtree_logical_bytes, extents: [ExtentSlice] }   ← role 3

     ExtentSlice { payload_object_id : ObjectId,
                   source_offset     : u32,
                   logical_length    : u32 }
```

Grammar bounds:

| Bound | Value |
| --- | --- |
| `MIN_ENTRIES` (non-root page) | 64 |
| `MAX_ENTRIES` | `MAX_MAPPING_ENTRIES` = 128 |
| `MAX_LEVEL` | 31 |
| `MINIMUM_ROOT_ENTRIES` (branch) | 2 |
| `MINIMUM_ROOT_LEAF_ENTRIES` | 0 |
| `MAX_NODE_OBJECT_BYTES` | 8,192 |

**Validation is inline.** Every field is checked while it is built or decoded, and
"no separate validation pass runs afterwards". Two canonicality rules are worth
naming, because both are enforceable and both are easy to get wrong:

1. **A non-root page below `MIN_ENTRIES` is `NonCanonicalPagePartition`.** The
   canonical partition must be unique — two encoders given the same extents must
   produce the same bytes, and a short non-root page would allow a second
   partition of the same content.
2. **Adjacent same-payload extents are rejected.** A leaf whose neighbouring
   extents name the same payload object at contiguous offsets is
   `NonCanonicalPagePartition`: the encoder must coalesce, because otherwise the
   same logical mapping has many encodings.

Branch pages are checked for strictly increasing cumulative totals, and the root's
totals must equal the recorded `subtree_logical_bytes` /
`subtree_extent_count`. A root branch requires at least two children — "a root
summary is never a single child".

### 3.6 The streaming builder holds only boundary pages

`ExtentBuilder` (`file/mapping/build.rs`):

```text
   incoming chunks / retained slices
        │
        ▼
   ┌──────────────────────────────────────────────────────────────┐
   │  levels: Vec<Pending>            Pending = Extents | Children │
   │  flush_at = capacities.stream_flush_entries                   │
   │                                                               │
   │  a level is FLUSHED as soon as it can no longer change,       │
   │  so retained entries are bounded by                            │
   │        tree height × page capacity                             │
   │  — NOT by file length and NOT by the number of edits          │
   └──────────────────────────────────────────────────────────────┘
        │
        ▼  emit FileState LAST, from facts already established
```

`MappingBuild` carries the facts established while building, so **no root reread is
required afterwards**: `root`, `logical_len`, `extent_count`, `tree_level`,
`chunks`, `peak_pending`, `nodes`. `peak_pending` records the largest number of
decoded entries the builder held at once, which is the number a memory claim would
have to be made against.

One subtlety in this area is documented as a correctness matter, not a style note:
the extent tree's own node header records an encoded-row-width total, and the
source records that an earlier revision wrote the *value* width instead, producing
different canonical bytes and therefore **different object identities for the same
logical page**. The sealed fixture pins the correct form. See [Known open items](README.md#known-open-items-at-the-pin) for the
parallel case in the inode leaf, where the same class of mistake is recorded explicitly.

---

## 4. Localized edits and reads (C1)

### 4.1 The edit dispatch — dispatch on the result, never the route

`core/crates/layerfs-content/src/file/edit/apply.rs`

The rule is stated at the top of the module and it is the whole design:

> The base is opened once, the edit stream is validated once and the representation
> of the **result** decides the route, **never the route deciding the result**.

```text
   apply_edits(policy, capacities, reader, EditRequest{root, edits, source}, consumer, scope)
        │
        │  policy.validated()                       ← refuse an unsupported profile FIRST
        ▼
   FileView::open(reader, root)                    ← acquire + classify the base ONCE
        │
        ├── view.logical_len() != edits.base_len()  ──► InvalidEdit("declared base length")
        │
        ├── edits.is_empty()  ──────────────────────► return the BASE ROOT unchanged
        │
        ├── compare_replacements(...) == NoOpVerdict::Equal
        │        every replacement byte-identical  ──► return the BASE ROOT unchanged
        │
        ▼
   policy.representation(edits.final_len())        ← route chosen from the RESULT length
        │
        ├── Empty      ──► emit the defined empty representation
        ├── WholeFile  ──► assemble the final bytes into one allocation
        └── Chunked    ──► ExtentBuilder frontier (below)
```

The two early exits matter: an edit stream that changes nothing returns the base
root **itself**, so a no-op edit costs no new objects and no new identity.
`compare_replacements` compares within a bounded window
(`COMPARE_WINDOW_BYTES`), which is what makes "byte-identical" checkable without
holding both versions in memory.

### 4.2 The chunked splice

```text
   immutable base FileState
        │
        │   for each change range (in current-result coordinates):
        │
        │     1. split the old mapping at the range start
        │            └──► left  subtree REUSED by ObjectId
        │
        │     2. split the tail at the range end
        │            └──► right subtree REUSED by ObjectId
        │
        │     3. boundary extents are SLICED — never re-CDC'd
        │
        │     4. ONLY the replacement bytes enter FastCdc
        │
        ▼
   new middle subtree from new chunk objects
        │
        ▼
   concat( left , middle , right )
        │
        ├── coalesce mergeable neighbours        (adjacent same-payload extents
        │                                         would be NonCanonical)
        ▼
   emit the new FileState
```

The reuse is by identity: an untouched range is **not re-chunked, not re-read**
and its objects keep their identities, so an edit costs the changed path plus the
boundary pages that prove the partition — not the file length.

Two builders exist — `file/edit/tree.rs` (the localized frontier, 895 lines) and
`file/mapping/build.rs` (streaming construction) — and the source states why that
is safe rather than a divergence risk:

> The same builder serves complete-file construction and known edits, so both
> produce the identical canonical partition for identical extents.

`EditCounters` reports what the frontier actually did. Complete construction and a
whole-file result report **all zeros**, which is the honest answer — no unfinished
mapping node exists on those paths. A chunked edit reports the nodes it actually
published and the largest frontier it held.

### 4.3 Logical reads

`core/crates/layerfs-content/src/file/read.rs`

```text
   read_all_bounded(reader, root, maximum, sink, scope)
        │
        ├── content.acquire    ── acquire the root ONCE
        ├── content::classify  ── whole-file or chunked, from those bytes
        │
        ├── content.logical_len() > maximum  ──► BoundedCapacityExceeded
        │
        ▼  emit_classified(...)
             │
             ├── WholeFile ──► emit the requested slice directly
             └── Chunked   ──► bounded extent traversal
                                (one decoded leaf at a time)
```

- The root is acquired **once per read** and classified from those bytes; a whole
  file pass never holds the mapping.
- Bytes reach the sink **in logical order**, whether a range crosses extents or
  mapping pages.
- `read_range` reads a logical sub-range under the same discipline.
- `RangeCursor` (`mapping/read.rs`) serves a sequence of ascending sub-ranges of
  one chunked file through the same traversal, keeping the mapping pages it has
  already acquired in a `PageCache` the caller owns. A page two ranges share — the root of every one of them, and
  every other ancestor of their union path — is one provider demand and one
  `nodes_read` charge for the whole sequence instead of one per range. The cache
  holds at most `READ_NAVIGATION_CACHE_PAGES` (2 × `READ_NAVIGATION_WAVE` = 64)
  pages and is emptied wholesale when it would exceed that, so a long operation's
  retained pages stay inside a declared ceiling; a whole-file base never builds a
  cursor at all, because its retained ranges are slices of the one payload.

`read_all_bounded` is the form that takes a caller-declared maximum; `read_all` is
the unbounded convenience wrapper. A caller that must bound its work uses the
former, which fails closed with `BoundedCapacityExceeded` rather than reading a
file it was not prepared to hold.

### Known-edit memo enforcement correction, 2026-10-07

This component correction follows parent `a41d131f2`; earlier descriptions and
measurement receipts keep their source pins. The stored-node route in
[EditObjects::load_node](../../crates/layerfs-content/src/file/edit/objects.rs)
now decodes a demanded canonical page and checks its actual level, logical bytes
and extent count before retaining it. A malformed page, impossible non-root fill,
wrong summary or original provider refusal returns once and does not populate
or evict the memo. A cached hit still checks the requested summary and actual
root/non-root decode context without another provider demand.

On a successful stored miss, `load_node` calls the existing
`PageCache::make_room_for(1)` before insertion and moves the returned canonical
allocation into the cache. The former route inserted without invoking that
caller-owned eviction and cloned the canonical allocation. The default allowance
remains 64 pages, and wholesale eviction remains the existing cache policy;
an evicted page is acquired again when subsequently needed. Canonical framing,
split/concat algorithms and returned roots are unchanged by this repair.

The external [edit_cache tests](../../crates/layerfs-content/tests/edit_cache.rs)
exercise more than two default windows of distinct valid non-root pages, exact
decoded summaries, retained hits/evicted demands, visible cache membership,
allocation transfer and malformed/refused inputs. Test source alone is not a
passing receipt. This repair covers the mapping memo only: the deferred draft,
parent-reference, detached, resolved/emission collections and sparse localized
editing remain K1 work. It establishes no whole-operation resident bound or
integrated Commit/performance acceptance.

### Lazy small-result assembly and edit ownership, 2026-10-07

Following parent `4090cb9a2`, [small-result assembly](../../crates/layerfs-content/src/file/edit/apply.rs)
consumes `Plan::advance` one segment at a time. It no longer builds a `Vec` of
all segments before reading retained or replacement bytes. One `RangeCursor`
and the same page memo remain alive across all ascending retained ranges. The
Plan holds scalar cursor state and one pending replacement; final WholeFile
bytes remain bounded by the selected construction cutoff. The validated,
stable, replayable `EditSequence` contract and first-error propagation remain;
there is no additional metadata prepass. WholeFile output is emitted only after
the complete assembly succeeds. Earlier source reads can precede a later lazy
input failure, and that failed operation is not replayed.

The [external generated regressions](../../crates/layerfs-content/tests/edit_lazy_plan.rs)
do not retain their input sequence in an edit vector. They observe the first
assembly replacement read before later edit rows, exact bytes/root against an
independent fresh construction, first-source-failure stopping before future
rows/emission, and one mapping-root demand across thousands of deletions into a
two-byte result. These are structural/public-behavior cases, without a timing or
whole-process memory claim; source presence is not a passing test receipt.

The owning source responsibilities are now explicit:
[objects.rs](../../crates/layerfs-content/src/file/edit/objects.rs) owns drafts,
reference/detached/resolved collections, loading, release and final emission;
[tree.rs](../../crates/layerfs-content/src/file/edit/tree.rs) owns the existing
split/concat/coalesce and root-partition algorithms. The public `EditObjects`,
`EditCounters` and `EDIT_DEFERRED_LIMIT` exports are unchanged. This is source
relocation, not a second tree algorithm or measured simplification. The memory
profile's 8MiB-derived draft limit, growing identity collections, sparse
replacement/comparison and owning indexed-backend requirements remain K1 work.

### Shared backed editor, 2026-10-07

The subsequent source change after `889836c44` supplies
`apply_edits_backed` and one tagged canonical edit engine for both profiles.
The owning responsibilities above move into focused
`edit/{engine,state,references,resolution,draft_codec}.rs`; `objects.rs` retains
the public memory facade. Stored, editable Node and already-canonical draft Page
references have distinct exact tags. No missing draft becomes a Stored read.
Draft/reference/detached/resolution/emission state can now reside behind a
caller-owned indexed port while tree decisions and canonical bytes stay in
Content. The memory route retains its original charge and refusal.

The source correction also checks every acknowledged draft resolution against
its requested summary and root/non-root context. A 51-byte retained witness
records those facts; an accepted object ID alone cannot validate changed child
boundaries. The engine attempts each mapping and FileState acceptance once,
retains pending/known acknowledgement state and propagates bookkeeping failure
without resend. Superseded leaves release before replacement allocation;
branches retain their parent through both joins to preserve repeated children.
See [backed file-edit state](50-backed-file-edit-state.md) and the explicit
[Workspace/Daemon custody adapter](52-workspace-edit-backing-port.md).

This source supplies backed file-edit state, not captured sparse normalization,
Save/history/known install, total process residency or Commit qualification.
Earlier source-pinned descriptions and failed receipts retain their scope.

### Run-aware edits and selected filesystem state, 2026-10-07

The [sparse serial/progress checkpoint](../issues/307/SPARSE-SERIAL-PROGRESS-20261007.md)
records the subsequent source and functional checks after `dcdf52758`. Its
[run-aware edit boundary](54-run-aware-localized-file-edits.md) adds
`EditSource::read_run_at` while preserving the existing byte-source default.
Complete run construction and localized replacements use the same frozen
Scanner/ExtentBuilder; zero spans preserve continuous CDC state, canonical
partitioning and actual predecessor hints. Comparison's positive zero memo
retains at most 64 witnesses. A whole payload/subtree is remembered only after
its entire referenced byte domain is proven zero; every hit rechecks the exact
summary and root/non-root context. Partial zero does not certify a larger domain.
Original comparison provider identity, absence, denial and decoder errors stop
before construction or backing effects. Later run-source/consumer failures stop
at their original boundary and retain already accepted output, without fallback
or failed-operation replay.

The additive [backed filesystem serial state](53-backed-filesystem-serial-state.md)
uses the existing opaque indexed record protocol for its context, new-parent
membership/held state, rebuilt directory roots and initial counts. Sealed
header/fresh cursors provide exact membership, root iteration and final inode
rows without collecting those domains in resident maps or complete row vectors.
The resident API keeps its prior containers and error order. Required missing
records refuse; a declared retained directory must still have its rebuilt ROOT
during the later supplied-value pass. The attempt marker binds local record
interpretation, not complete root reachability, alias uniqueness or authority.

The [captured input port](55-captured-sparse-run-cursor.md) supplies exact retained
inode points and a fixed forward run cursor under the original reader/root/
generation/installed floor. A cell-sized Window still pays existing byte/mask
composition and the Daemon's bounded return clone. Missing cells alone remain
Inherited; actual cutoff or lower EOF establishes local Zero. Reader acquisition,
original failure custody and explicit release remain with the caller.

These are component source and functional boundaries. Captured final-state edit
normalization, grouped validator/addition/graph state, whole-base alias walks,
reducer/touched/zero/release backing, complete root qualification and the actual
Save/history/Commit/install composition remain unfinished. Existing remaining
state refusals and earlier evidence retain their scope. No whole-operation
residency, physical I/O, performance or E/Q gate follows from these checks.

### Captured file normalization and indexed reference release, 2026-10-07

The subsequent [captured-file/reducer checkpoint](../issues/307/CAPTURED-FILE-REDUCER-20261007.md)
adds the Workspace [captured-file adapter](56-captured-file-normalization.md)
and extends the [indexed filesystem serial state](53-backed-filesystem-serial-state.md).
`CapturedFileEdits::prepare` derives the operation's BaseView from the Workspace,
checks its route against the exact retained reader and construction-record owner,
and rebinds through that same authorized client only when the captured root differs
from the current binding. Authenticated file identity and cheap length facts,
one retained FileView classification, and the captured inode point determine the
base and final sizes. Only logical PathNotFound establishes immutable absence;
provider absence, denial, identity failure or mismatched length facts retain their
original failure rather than selecting a new-file branch.

One forward captured cursor normalizes surviving local changes into coalesced
overwrites and at most one trailing deletion or extension. It splits the
authenticated base EOF: inherited positions below it preserve the base, while
inherited positions at or above it are logical Zero, including masked Window
positions whose pending bytes are ignored. Numbered 25-byte edit records and a
sealed private context reside behind the original indexed operation owner; the
adapter retains no complete edit vector. The [fallible borrowed-view entry point](54-run-aware-localized-file-edits.md)
uses the existing canonical edit engine without reopening or classifying the
file root. Localized edit boundaries retain their canonical meaning; equality
with an independently repartitioned whole-file construction is not promised.

Returned data and inherited-mask allocations are checked before retaining a
Window, and returned raw record allocations are checked before decoding. A
Continue must advance actual metadata probes monotonically under the same
reader, serial, logical size and layer identities; claimed work with an unchanged,
restarted or retreating cursor refuses instead of creating a polling loop. Every
port shares the first terminal captured, record, Content or provider failure.
The consuming attempt returns its result with original reader/record/client
custody, retains already accepted children on failure, and performs no implicit
owner release or replay. Multiple files can use disjoint file-record scopes under
one capture and operation owner. Releasing that reader does not resolve an
in-flight capture. Retained reads can survive logical close; new normalization
record writes still require a live Workspace and return its actual Closed failure.

The P13 extension stores authenticated count/effect rows, touched membership,
FIFO zero-release work and directory cursor frames in the same guarded record
protocol. Bounded full-key windows drive the canonical reducer and release
decisions without collecting those domains in resident maps or sorting runs.
Coupled child-count, membership, queue and cursor changes use exact guarded
updates; required missing records and original failures stop later work. Counts
derive from authenticated base counts and signed effects. Positive overflow now
returns LengthOverflow instead of becoming zero. The final-row iterator forwards
its original error immediately to the fallible inode engine, so a failed row
cannot masquerade as normal EOF and finalize a filesystem root. Traversing a
newly accepted directory requires the explicitly supplied accepted-object reader;
it has no fallback to the initial reader or inferred same-Save authority.

These are file and selected reference/release component implementations with
public functional proofs. Namespace final-state normalization, grouped validator
and addition/graph state, whole-base and rebound alias walks, P14 immutable root
qualification, and actual same-Save/Save/history/install/unknown composition
remain unfinished. The retained refusal for a newly added child inside a rebuilt
then released existing directory also remains engineering work. The old resident
route keeps its allowances; the indexed path establishes no aggregate residency,
physical I/O, performance or E/Q acceptance. Earlier source-pinned evidence keeps
its original scope.

The current checkpoint retains 408 distinct passing host bodies: Content 287,
Overlay 8, Workspace 50, Daemon 25 and SDK 38. Its Linux receipts retain 373:
Content 287, Overlay 8, Workspace 50, Daemon 25 and the three Linux-enabled SDK
bodies. The other 35 selected SDK bodies are macOS-only and are omitted on Linux.
The host captured selection preserves its original thirteen passes and fixture
failure in receipt 16, then adds only the corrected failed body from receipt 18;
Linux covers all fourteen under the corrected fixture. Build failures 07/12 and
fixture failure 16 remain unchanged. These functional results do not supply a
final lint/native proof, sealed source identity, LOC comparison or E/Q admission.
