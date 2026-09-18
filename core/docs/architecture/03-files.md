# File construction, localized edits and reads

> **Status:** Research; informative and not a product contract.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

---

## 3. File construction (C1)

### 3.1 The representation dispatch

`core/crates/layerfs-content/src/policy.rs` owns one selector:

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
