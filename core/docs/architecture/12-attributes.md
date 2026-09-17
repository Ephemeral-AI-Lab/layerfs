# Attribute trees

> **Status:** Research; source-backed description of the current tree. Informative,
> not a product contract.

Part of the [replacement-core architecture](README.md) set. Source pin
`ce2d738ff`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

Chapter numbers are global to the set: this paper holds **chapter 17**.

---

## 17. Attribute trees

### 17.1 Where this sits

An attribute tree holds the metadata that is neither a name nor file content:
permission mode, modification time, and any generic `domain + key` pair a caller
wants to attach. Each inode points at one, through
`InodeValue.metadata_root` ([§2.4](02-objects.md)).

```text
   FilesystemRoot ──► inode table ──► InodeValue { kind, namespace_ref_count,
                                                   content_root,
                                                   metadata_root } ──┐
                                                                     │
                                        ┌────────────────────────────┘
                                        ▼
                          ┌──────────────────────────────┐
                          │  ATTRIBUTE TREE  (LFS4MET)   │
                          │  leaf/branch pages of        │
                          │  (domain, key) → value root  │
                          └──────────────┬───────────────┘
                                         │
                              ┌──────────┴──────────┐
                              ▼                     ▼
                    portable typed keys      generic opaque domains
                    "portable" + mode        any nonempty UTF-8 domain
                    "portable" + mtime       (≤ 64 B) + key (≤ 255 B)
```

Two facts make this subsystem unusually self-contained:

- **There is no platform interpretation anywhere.** `keys.rs` states it directly:
  *"There is no operating-system whitelist, no platform interpretation and no
  permission enforcement anywhere in this component."* A domain is data. The
  reserved `portable` domain is the only one with typed meaning, and only for its
  two keys.
- **Nothing here is a second file format.** Values reuse the existing canonical
  grammars ([§17.5](#175-value-roots-are-extent-only-files)).

The subsystem is 1,300 production lines across `build.rs`, `codec.rs`, `keys.rs`,
`patch.rs`, `portable.rs`, `read.rs` and `value.rs`.

---

## 17.2 The key grammar

```text
   AttributeKey { domain: String, key: Vec<u8> }

   domain   nonempty, ≤ MAXIMUM_ATTRIBUTE_DOMAIN_BYTES = 64, valid UTF-8,
            no NUL
   key      ≤ MAXIMUM_ATTRIBUTE_KEY_BYTES = 255, no NUL, otherwise opaque

   reserved: domain == "portable"  ⇒  key ∈ { b"mode", b"mtime" }
              every other domain accepts any key bytes

   ordering: domain bytes first, then key bytes  — a TOTAL order, and the
             order every page and every patch list must be sorted by
```

The reserved domain is enforced at construction: `AttributeKey::new` refuses
`portable` with any key other than the two typed ones, with
`InvalidRecord("portable attribute key")`. So a caller cannot invent
`portable` + `uid` — there is nowhere to put ownership, and the grammar refuses the
attempt rather than storing it as an opaque value.

`PORTABLE_KEYS: [&[u8]; 2] = [b"mode", b"mtime"]` is the entire typed surface.

Patch lists are checked for strict order and uniqueness by
`check_patch_order`, which refuses a repeat with `NonCanonicalOrdering` — two
patches for one key would make the result order-dependent.

---

## 17.3 The page grammar

```text
   LFS4MET\0 │ version u16 = 1 │ in-page role │ level │ … │ rows
   └── ATTRIBUTE_MAGIC ──┘

   EMPTY_PAGE_BYTES    = 44   (envelope 13 + node header 31)
   NODE_HEADER_BYTES   = 31

   LEAF row    = domain + key + required-flag + value root   (+ LEAF_ROW_OVERHEAD   = 37)
   BRANCH row  = domain + key + child identity               (+ BRANCH_ROW_OVERHEAD = 36)

   exact page size = 44 + Σ(row widths)
```

Sizes are **arithmetic, not trial encodings** — the same number the encoder
writes, so a partition decision never clones or encodes a candidate page to
discover whether it still fits. `row_bytes(key, overhead)` is the single function
both the builder and the encoder agree on.

Fill rules, shared with the directory and inode formats
([§5.3](04-filesystem.md)):

| Rule | Value | Enforced by |
| --- | --- | --- |
| page ceiling | `MAXIMUM_PAGE_BYTES` = 8,192 | `AttributePage::fits` |
| non-root fill | ≥ `MINIMUM_FILLED_PAGE_BYTES` = 3,277 (the 2/5 rule) | `AttributePage::filled` |
| branch level | `level > 0` **and** `count >= 2` | `decode_attribute_page` |
| tree depth | ≤ `MAXIMUM_TREE_LEVEL` = 31 | `lookup_many` |

The branch rule is the one to note: an attribute branch must have **at least two
children**. A single-child branch is non-canonical, so the partition is unique.

### 17.3a Two role numberings — a real trap

Attribute pages carry a **role byte inside their header** that is *not* the
`ObjectRole` code. This is true of every tree page in the product, and the two
numberings collide in six places.

| Page kind | in-page role byte | `ObjectRole` code |
| --- | ---: | ---: |
| Directory leaf | 1 | **7** |
| Directory branch | 2 | **8** |
| Inode leaf | **7** | **6** |
| Inode branch | 8 | **9** |
| Attribute leaf | 9 | **11** |
| Attribute branch | 10 | **12** |
| Symlink target | 5 | **13** |
| Filesystem root | 6 | **10** |

Read the collisions carefully:

```text
   7   in-page  = INODE LEAF        ObjectRole = DIRECTORY LEAF
   5   in-page  = SYMLINK           ObjectRole = FileState
   6   in-page  = FILESYSTEM ROOT   ObjectRole = InodeLeaf
   1   in-page  = DIRECTORY LEAF    ObjectRole = WholeFile
   9   in-page  = ATTRIBUTE LEAF    ObjectRole = InodeBranch
   10  in-page  = ATTRIBUTE BRANCH  ObjectRole = FilesystemRoot
```

Only the sorted engine has a mapping function between them — `page_role::<F>`,
which matches four in-page bytes to their `ObjectRole`s. The attribute builder
**bypasses it** and assigns `ObjectRole::AttributeLeaf` / `AttributeBranch`
directly, because its pages never pass through the sorted engine.

So there is **no single place** where the two numberings are reconciled. A reader
who assumes they are the same number will be wrong six times, and the mistake is
silent: both are `u8`, both are in range `1..=13`, and the `objects.object_role`
CHECK constraint accepts either.

---

## 17.4 The builder is streaming with exact sizing

`AttributeTreeBuilder` consumes entries in **strictly ascending key order** and
never holds the whole tree.

```text
   push(entry)
        │
        ├── the pending leaf's exact canonical size is arithmetic on row widths
        │   (never a trial encode)
        │
        ├── fits the page ceiling?  ──► append to the pending group
        │
        └── would exceed it        ──► seal the oldest group, start a new one
                                        (at most TWO groups stay pending;
                                         the third seals the oldest)

   finish()
        │
        ├── rebalance_tail: move entries FORWARD until the last leaf group
        │   reaches the 2/5 fill rule
        ├── seal each level the same way
        └── return the root ObjectId
```

The two-pending-group discipline is what makes rebalancing possible: sealing the
third group is what guarantees the second can still accept entries moved forward
from the first.

`rebalance_tail` and `rebalance_branch_tail` exist because the 2/5 fill rule
applies to **every non-root page**, including the last one. A tail group that would
be too small is corrected by moving entries forward, and the builder's own comment
records the requirement: the partition must match the reference builder's
**exactly**, because a different partition is different canonical bytes.

An empty tree is legal and is a real object: `finish` with no entries emits an
empty leaf page, so every inode has a `metadata_root` whether or not it carries
attributes.

---

## 17.5 Value roots are extent-only files

An attribute value is **a complete file representation that always carries an
extent leaf**, even for four bytes of mode:

```text
   emit_value(bytes)
        │
        ├── Chunk object        encode_chunk_object(bytes)        role 2
        ├── ExtentLeaf page     1 ExtentSlice { payload, 0, len } role 3
        └── FileState           { logical_len, extent_count 1,
                                  tree_level 0, profile_id,
                                  mapping_root = leaf }           role 5

   ⇒ a 4-byte mode is stored as three canonical objects
```

The source states why the small/large cutoff must not apply here:

> the regular-file small/large cutoff must not reclassify it, so a four-byte mode
> value is stored exactly like any other extent-backed value. The chunk payload,
> leaf and file-state objects are the existing canonical grammars; **no second CDC
> path or metadata-specific file format is introduced.**

The alternative — storing a mode as a `WholeFile` object — would make the value's
representation depend on the construction cutoff, and a Store opened under a
different cutoff would classify the same bytes differently. Pinning every value to
the extent-backed form removes that dependency.

**The declared bound is enforced on the write side, not only the read side.**
`emit_value` refuses a value above `MAXIMUM_ATTRIBUTE_VALUE_BYTES` = 32 KiB, which
is *derived* from `cdc::MAXIMUM_CHUNK_BYTES` rather than restated, so the two
cannot drift. The source gives the reason a larger constant would be wrong: *"a
bound above it would be a limit no write path can reach and no read path can
return."*

`read_value` re-checks the same bound from the decoded `FileState` before reading,
so a caller cannot be handed a value it did not ask to hold.

---

## 17.6 Portable metadata is deliberately narrow

```text
   PortableMetadata { mode: u32, mtime_seconds: i64, mtime_nanoseconds: u32 }

   kind        accepted mode mask     extra rule
   ────────    ──────────────────     ──────────────────────────────
   regular     0o777                  —
   symlink     0o777                  mode must be EXACTLY 0o777
   directory   0o1777                 sticky bit permitted

   mtime_nanoseconds ≤ MAXIMUM_NANOSECONDS = 999_999_999
```

The variant is `symlink`: it does not merely restrict the mask, it requires mode
to *be* `0o777`. And a directory is the only kind permitted the sticky bit.

**There is no uid, gid or atime anywhere.** `portable.rs` states it: *"No uid, gid
or atime semantics are added, and no platform-specific value is interpreted."*
This is the same boundary `core/AGENTS.md` draws — *"Stage 5 attributes are
portable mode/mtime plus bounded generic key/value data; do not port Apple-specific
codecs/semantics"* — so an importer that needs ownership has nowhere to put it, by
design rather than by omission.

`read_portable` is strict where the grammar is permissive: both fields are
*optional* in the page grammar but *required* by the portable contract, so a
directory missing mode or mtime, or carrying a value that fails the typed check,
**fails explicitly** rather than defaulting.

---

## 17.7 Patching preserves untouched values without decoding them

```text
   apply_patches(reader, objects, base_root, patches)

        stream the base tree in key order ──┐
        stream the patch list in key order ─┤  one merged pass
                                            │
        key in both?      ──► Set writes a new value root; Remove drops the entry
        key only in base? ──► PRESERVED: its stored value root is copied by
                              identity, and its value bytes are NEVER READ
        key only in patch?──► inserted

        ⇒ rebuild through AttributeTreeBuilder, once
```

`AttributePatchWork.preserved` counts exactly the keys that were carried through
without reading their values — which is what makes the claim checkable rather than
asserted.

Two shortcuts: an **empty patch list returns the base root unchanged** (no page is
rebuilt), and both typed and generic keys travel the identical route —
*"there is no per-domain branch and no platform dispatch."*

`visit_keys` / `visit_keys_counted` enumerate keys without reading values, bounded
by `MAXIMUM_ATTRIBUTE_KEYS` = 4,096, which the source calls *"a declared operation
bound, not a format bound"* — the grammar bounds a page, not a tree.

---

## 17.8 Multi-key reads share one wave per level

`lookup_many` is the interesting read, and its shape is the reason a batch costs
less than a loop of point lookups:

```text
   keys: [k0, k1, k2, …]      one frontier, carried down the tree

   level = [(root, is_root, None, [0..keys.len()])]
        │
        ▼
   ┌─ one read_canonical_batch(ids) for the WHOLE level ─────────────────┐
   │                                                                     │
   │  Leaf page:   binary_search per index  → answer                    │
   │                                                                     │
   │  Branch page: partition_point per index, then GROUP indices by the  │
   │               child they descend into (BTreeMap<usize, Vec<usize>>)  │
   │               ⇒ one child is demanded ONCE however many keys hit it  │
   └──────────────────────────────┬──────────────────────────────────────┘
                                  ▼
                       next level = only the children actually needed
                                  │
                                  ▼  repeat until the frontier is empty
```

So the cost is `O(levels)` read waves, not `O(keys)`: *"a batch costs one
authenticated read per shared page instead of one traversal per key."* Duplicate
demands are answered in demand order, and `depth` is bounded by
`MAXIMUM_TREE_LEVEL`, so the loop cannot run away.

`read_value_bounded` and `read_opaque` then take the resolved value root and read
the value, bounded by the caller's maximum.

---

## 17.9 Work counters

| Type | Field | Reports |
| --- | --- | --- |
| `AttributeBuildWork` | `leaves`, `branches` | pages emitted |
| | `peak_pending_entries` | largest entry set held before sealing |
| | `peak_pending_children` | largest child set held before sealing |
| `AttributePatchWork` | `base_entries`, `base_pages` | how much of the base was streamed |
| | `set`, `removed` | patches applied |
| | **`preserved`** | keys carried through **without decoding** |
| | `values_emitted` | new value roots |
| `AttributeReadWork` | `pages_read`, `read_waves` | read cost |
| | `values_read` | value roots fetched |

`peak_pending_entries` and `peak_pending_children` are the memory claim; the
builder's doc says at most two groups stay pending, so these are bounded by page
capacity, not by tree size.

## 17.10 Complexity

`e` = entries, `P` = entries per page (≤ ~740 one-byte-key rows), `k` = keys
requested, `v` = value bytes, `L` = tree height ≤ 31.

| Operation | Time | Peak memory | Storage |
| --- | --- | --- | --- |
| Build (`push` × e) | O(e) | **O(P)** — two pending groups | O(e/P) pages |
| `finish` + rebalance | O(P · L) | O(P · L) | — |
| `emit_value` (v bytes) | O(v) | O(v) | 3 objects |
| `apply_patches` (b base, p patches) | O(b + p) | O(P) | O((b+p)/P) pages |
| `lookup_many` (k keys) | O(L · P) per level | O(children per level) | — |
| `read_value` (v bytes) | O(v) | O(v) | — |

Deliberately not O(e) in memory anywhere: both the builder and the patcher stream,
and the reader descends one level at a time.

## 17.11 What is not established

- **The two role numberings are not reconciled anywhere.** §17.3a records the six
  collisions. Whether that is intentional (each grammar owning its own byte) or
  accumulated is not determinable from source; what is determinable is that no
  single mapping function exists for the attribute or filesystem-root paths.
- **No measurement.** Every bound above is a declared constant. The 2/5 fill rule
  and the page capacities are arithmetic, not measured fill distributions.
- **No reference comparison** is made in this paper. The reference tree has its own
  metadata codec; equivalence is a compatibility question that Stage 5's records
  own, not a reading.
