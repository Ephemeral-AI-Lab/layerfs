# Phase 4 logical persistence mapping

Status: WP4-P COMPLETE / PASS; WP4 complete; WP5 eligible/pending; Phase 4 not complete
Date: 2026-08-20
Scope: exact durable mapping and WP4 promotion authority; no WP5+ implementation
work

## 1. Decision

The selected LayerFS durable mapping composes the already-frozen Phase 1
`Object::Bytes` and `Object::Directory` formats. It adds no `ObjectKind`, no
second root identity, and no backend-specific locator.

The mapping is:

- chunk payload: the existing canonical `Object::Bytes(raw_chunk)`;
- file node: one typed Bytes root, fixed 64-reference typed Bytes leaves, and
  only the fixed-fanout typed Bytes branch levels required by its checked-u64
  reference count;
- directory node: a two-entry Phase 1 Directory wrapper, a typed Bytes metadata
  object, a typed Bytes index, and greedily byte-packed Phase 1 Directory pages;
- root: exactly the durable directory-node `ObjectId`;
- delta: one typed Bytes index plus greedily byte-packed typed Bytes entry
  pages; and
- every durable identity: the existing Phase 1 object hash of exact canonical
  object bytes.

The earlier research stop is retracted. WP4 has authority to define finite
durable admission without changing the meaning or identity of values that are
admitted. A directly constructed logical tree deeper than 256 components is
therefore rejected at the durable boundary with a typed resource-limit error;
this is not a change to Phase 3 mutation semantics. Conversely, the old
100,000-total-file-reference draft is withdrawn: 100,000 remains a per-object
direct-reference bound, not a logical-file or closure bound. File length,
reference count, descriptor totals, and cumulative work use checked `u64`.

WP4-P promotes fixed capacities K64/F64 and the DIR256K fallback as the one
compatibility profile while honestly accepting suffix-linear count-changing
edits. The deletion, independent-golden, fingerprint, and audit gates passed;
this grants compatibility authority without claiming either constant optimal.
Directory and delta records are variable-width and have no 64-entry rule.
Delta pages use the existing 8 MiB Bytes-field ceiling. Both retain the
existing 100,000 direct-reference limit.

## 2. Controlling contracts and semantic inventory

### 2.1 Authority

`../wp4m/fixed-radix-fast-lane-amendment.md` controls only its prospective
WP4-M evidence, count-changing-edit policy, and DIR256K fallback deltas. The
remaining authority below is unchanged.

This record preserves:

- Phase 1 canonical framing, `ObjectKind::{Bytes, Directory}`, strict
  directory ordering, the `layerfs/object\0` BLAKE3 domain, and immutable
  authenticated CAS behavior;
- the Phase 2 CDC profile: minimum 8,192 bytes, average 16,384 bytes, maximum
  32,768 bytes, normalization level 2, and seed 0;
- the ordered Phase 2 `LogicalFile` sequence and exact range bytes;
- Phase 3 tree metadata, COW value semantics, root content semantics, and
  sequential delta application; and
- the Phase 4 requirements for bounded objects, exact EOF, typed errors,
  authenticated closure, atomic root/delta publication, and reopen without
  process-local state.

The controlling requirements are in
`../rollback/spec.md:188-267` and the WP4
deliverable is in
`../rollback/implementation-plan.md:241-306`.
The only SQLite schema authority for this mapping is
`../storage/sqlite/visible-head.md`.

### 2.2 Persisted fields and current callers

| Logical value | Current semantic owner and callers | Exact durable fields |
|---|---|---|
| `Object`, `ObjectKind`, `ObjectId` | `crates/layerfs-core/src/object/{model,codec}.rs`, `identity/{digest,ids}.rs`; CAS, engine, evaluator, and all tree/content paths call these contracts | Existing Phase 1 canonical bytes, outer kind, and 32-byte object ID |
| raw `ChunkId` | `identity/mod.rs`, CDC/content/CAS callers | 32-byte raw-payload hash; not a strong object edge |
| canonical chunk object | capture, CAS put/load, range reconstruction | canonical Bytes `ObjectId` and exact canonical Bytes object |
| `ChunkReference` | `content/mod.rs`; full replace, range read, bounded edit/rejoin, evaluator | raw `ChunkId`, raw length, canonical chunk Bytes `ObjectId` |
| `LogicalFile` | `content/mod.rs`; `TreeNode::File`, mutation/delta/evaluator | mode, total length, ordered reference count, page descriptors and pages |
| `Metadata` | `cow/tree.rs`; file/directory constructors, mutation, delta diff/apply | unsigned 32-bit mode, including 0 and `u32::MAX` |
| file `TreeNode` | `cow/tree.rs`; roots, mutation, delta entries | the file root object itself; its `ObjectId` is the durable `NodeId` |
| directory `TreeNode` | `cow/tree.rs`; lookup/mutation/delta/root | metadata object, directory index, ordered page entries, and a two-entry wrapper |
| `RootHandle` / `RootId` | `cow/tree.rs`; mutation/delta/evaluator and engine publication | the root directory wrapper ID only; no parent in root identity |
| `Delta` / `DeltaEntry` | `delta/mod.rs`, `cow/mutate.rs`; `between`, `new`, sequential `apply` | parent, child, Vec order, exact operation tag/path/before/after IDs and modes |
| `RootRecord` / `DeltaRecord` | `crates/layerfs-engine/src/lib.rs`; capture, load, reopen, closure | the selected mapping supersedes provisional root-parent storage and opaque/ad-hoc delta identity after promotion |

`ChunkId` is currently a Rust alias of `ObjectId` and uses the same hash domain,
but its preimage is raw chunk bytes. The canonical chunk-object ID hashes the
Phase 1 Bytes framing plus the raw bytes. The types therefore have equal width
and domain but distinct semantic roles and normally distinct values.

The current `TreeNode` hash is expressly provisional
(`crates/layerfs-core/src/cow/tree.rs`). WP5 must replace it at the durable
boundary with the IDs frozen here. The engine's current ad-hoc
`delta_identity` (`crates/layerfs-engine/src/lib.rs:190-203`) is not the frozen
durable mapping.

The current eager `LogicalFile { chunks: Vec<_> }` implementation rejects more
than 100,000 total references in `content/mod.rs:49-68`. The user's 100-GiB
requirement explicitly supersedes that implementation limit for the durable
mapping: the ordered sequence and its provisional identity semantics remain,
but WP5 must add a lazy/streamed u64-count view and retain 100,000 only for a
single encoded object's direct children. An eager compatibility conversion may
preflight section 9.4 and return `AllocationBudgetExceeded`; it may not make
the durable file unrepresentable or silently allocate a source-sized Vec.

## 3. Common byte conventions

All integer fields are unsigned, big-endian, and fixed-width. No Rust enum
layout, `usize`, allocator behavior, or platform endianness is serialized.

Notation:

| Name | Bytes |
|---|---:|
| `u8` | 1 |
| `u16be` | 2 |
| `u32be` | 4 |
| `u64be` | 8 |
| `ObjectId`, `ChunkId`, `NodeId`, `RootId`, `DeltaId` | 32 raw digest bytes |

Every mapping-specific Bytes value begins with:

```text
mapping_magic   [8]byte = 4c 46 53 34 4d 41 50 00   # "LFS4MAP\0"
mapping_version u16be  = 1
record_tag      u8
```

Tags are:

| Tag | Meaning | Required outer Phase 1 kind |
|---:|---|---|
| `0x01` | file root / file node | Bytes |
| `0x02` | file reference page | Bytes |
| `0x03` | directory index | Bytes |
| `0x04` | directory metadata | Bytes |
| `0x05` | delta index / `DeltaId` object | Bytes |
| `0x06` | delta entry page | Bytes |
| `0x07` | file branch | Bytes |

The Phase 1 Bytes object remains exactly:

```text
"LFSO" | 0x01 | payload_len:u32be | inner_len:u32be | inner_bytes
```

The Phase 1 Directory object remains exactly:

```text
"LFSO" | 0x02 | payload_len:u32be | entry_count:u32be |
  repeated(name_len:u32be | name | child_kind:u8 | child_id:[32]byte)
```

`payload_len` includes the four-byte inner length or entry count. Phase 1
requires exact EOF and strict ascending canonical directory names.

For every canonical object `B`:

```text
ObjectId(B) = BLAKE3("layerfs/object\0" || B)
```

The hash covers the complete Phase 1 canonical object, not only its inner
mapping bytes. All lengths and counts use checked arithmetic before allocation.

## 4. Chunk mapping

For raw payload `R`:

```text
raw_chunk_id       = BLAKE3("layerfs/object\0" || R)
chunk_object       = canonical Object::Bytes(R)
chunk_object_id    = ObjectId(chunk_object)
raw_length         = len(R)
```

The admitted raw length is `0..=32,768`. Zero-length direct references are
valid because Phase 3 admits them even though the CDC scanner does not emit
them. On reconstruction the decoder authenticates `chunk_object_id`, requires
outer kind Bytes, obtains `R`, checks `len(R) == raw_length`, and recomputes
`raw_chunk_id`. Neither ID may substitute for the other.

## 5. File-node mapping

This section freezes the selected **K64/F64 fixed-radix production grammar**.
WP4-P deletion, independent goldens, fingerprints, and both audits passed, so
this is the external compatibility-promoted v1 grammar. Draft golden IDs from
earlier revisions have no deployed compatibility authority and do not
constrain this grammar.

### 5.1 Reference leaves

One semantic reference is exactly 68 bytes:

```text
raw_chunk_id       [32]byte
raw_length         u32be
chunk_object_id    [32]byte
```

A file reference leaf is:

```text
common_header(tag = 0x02)
reference_count    u32be                 # 1..=K, selected K=64
references         reference[reference_count]
```

Leaves are the consecutive ordinal partition of the ordered semantic
reference stream. Every nonfinal leaf has exactly K references and the final
leaf has 1..=K. There are no empty/sparse leaves. Repeated IDs, repeated
references, and zero-length references remain in sequence. Its complete
canonical size is `Leaf(c) = 28 + 68*c`; selected K64 has
`Leaf_max = 4,380` bytes.

### 5.2 File branches, root, and identity

One file branch is:

```text
common_header(tag = 0x07)
level              u8                    # 1..=9 for selected F=64
child_count        u32be                 # 1..=F, selected F=64
repeated child_count times:
  cumulative_end   u64be
  child_object_id  [32]byte
```

`level=1` children are reference leaves; `level>1` children are branches with
exactly `level-1`. Its complete size is `Branch(f)=29+40*f`; F64 therefore
bounds every branch at 2,589 bytes. Branches at every level use consecutive
fixed ordinal groups: every nonfinal group has exactly F children and the
final group has 1..=F.

The file root is:

```text
common_header(tag = 0x01)
mode                    u32be
total_raw_length        u64be
total_reference_count   u64be
branch_levels           u8               # 0..=9 for K64/F64
child_count             u32be             # 0..=F
repeated child_count times:
  cumulative_end        u64be
  child_object_id       [32]byte
```

Its complete size is `Root(r)=49+40*r`, at most 2,609 bytes for F64. The
root's canonical `ObjectId` is the durable file `NodeId`.

The unique empty file has zero length/count/levels/children. A nonempty
reference sequence has at least one leaf even if every raw length is zero. If
all leaves fit in the root (`P<=F`), `branch_levels=0` and root children are
leaves. Otherwise the root points to the highest branch layer. The encoded
height is the smallest height that fits; redundant unary/top levels are
invalid.

Let `C=total_reference_count`, `P=ceil(C/K)`, and, while the preceding count is
greater than F, define `B1=ceil(P/F)`, `B2=ceil(B1/F)`, and so on through
`Bh<=F`. For `C>0`:

```text
objects O(C) = P + sum(Bi) + 1
mapping M(C) = 68*C + 68*P + 69*sum(Bi) + 49
```

For `P<=F`, the sum is empty. These equations include every canonical
leaf/branch/root envelope and exclude chunk objects. The maximum reference
capacity with `h` branch layers is `K*F^(h+1)`. With K64/F64, every u64
reference count fits the radix topology with at most nine branch layers. That
topology claim is not an operational-admission promise: every derived
`P/Bi/O/M` value and operation counter is also checked. For K64/F64 the largest
`C` whose canonical mapping-byte total `M(C)` fits `u64` is
`267,036,007,400,295,520`; `C+1` makes `M=u64::MAX+26` and is rejected with
`LengthOverflow`. An operation whose cumulative W/D accounting overflows is
likewise rejected. These are mechanically derived arithmetic limits, not an
arbitrary smaller file-size policy; all 100-GiB cases in section 12 are far
below them.

Every cumulative end is a checked u64 sum of raw lengths through that child.
It may repeat because zero-length references/subtrees are legal. The final end
at every object equals its authenticated subtree raw length; full validation
also recomputes subtree reference counts, fixed partitions, root total count,
and root total length. A count/length/offset overflow is `LengthOverflow`, not
wraparound or a smaller implicit limit.

### 5.3 Snapshot validation and exact range routing

Normal reopen, range, and incremental edit may traverse only an authenticated
path **only** when the authoritative visible head carries the exact
generation-bound `ValidatedSnapshotReceiptV1` specified in section 9.5. The
receipt attests that the complete immutable child/transition closure passed
section 9.2 when that generation was published. It does not make fetched bytes
trusted: every root, branch, leaf, and chunk actually read is still fetched in
full and authenticated by its `ObjectId`. It also does not prove continued
presence; deletion/corruption is a typed failure when the affected object is
accessed. Without a valid receipt, the operation must perform full closure
validation before using unvisited cumulative summaries, or fail with the
receipt-specific typed outcome. It never silently trusts an index.

For requested file-global `start..end` after authenticating the file root,
initialize `node_base=0`. In each root or branch object, descriptor cumulative
ends are relative to that object's subtree. For descriptor `i`, let
`previous_end=0` for `i=0`, otherwise the prior descriptor's cumulative end,
and compute with checked u64 arithmetic:

```text
child_base = node_base + previous_end
child_end  = node_base + cumulative_end
```

Then:

1. require `start <= end <= total_raw_length`;
2. if `start == end`, return empty after the root check and fetch no child;
3. at each root/branch level visit, in ordinal order, exactly the descriptors
   satisfying
   `child_base < child_end && child_base < end && child_end > start`,
   authenticate each selected child, and descend with
   `node_base=child_base`; the first selected descriptor is the first one in
   that order satisfying the same predicate, including when traversal entered
   this node from an earlier sibling;
4. within each selected leaf, begin the checked running offset at the inherited
   `node_base` and fetch a chunk iff
   `chunk_start < chunk_end && chunk_start < end && chunk_end > start`; and
5. stop before the next child when the running offset is at least `end`.

The explicit nonempty-interval predicate skips equal ends and zero-length
subtrees. A request beginning at a nonempty boundary skips the preceding
child; a request ending there does not enter the next. A zero-length reference
is preserved by full reconstruction but never fetched for range data. Tests
must cover empty `0..0`, leading/interior/trailing zero references, equal ends
across leaf and branch boundaries, a cross-leaf/cross-branch request, and EOF.
For selected K64/F64, one mandatory replacement vector is 4,097 one-byte
references: its 65 leaves form two root branch children. Range `64..65` must
select the second leaf of the first branch, `4096..4097` must select the first
leaf of the non-first root branch, and `4095..4097` must cross the two root
branches. Any implementation that compares subtree-relative ends directly to
the unchanged file-global start fails this vector. A second mandatory vector
has 4,161 references: 4,096 one-byte references, then 64 zero-length
references forming the leading leaf of the second root branch, then one
one-byte reference. Range `4095..4097` must cross the root branches but must
not authenticate or fetch the empty leaf or any of its zero-length chunks.

With a valid receipt, a cold one-leaf range authenticates one root, exactly
`h` branches, one leaf, and the overlapping chunks. With K64/F64, the maximum
mapping bytes are `2,609 + h*2,589 + 4,380`; at the 100-GiB cases in section
12.1, `h=2` and the exact topology-specific maxima are 10,127, 10,447, and
11,607 bytes for the 32-KiB, retained-density, and 8-KiB counts. Branches are
never giant authentication units.

### 5.4 Selected constants and known edit ceiling

Flat/giant manifests are not structurally invalid, but they make normal range
and same-count edit work O(C), contrary to the required hot path. Retained
Phase 2 S1-100 evidence reports middle/EOF reference inspection of
2,642/5,284 for flat versus 18/36 for fixed K64
(`../../phase-2/handoff.md:173-223`). K64 is now selected by policy, not claimed
as a durable/SQLite optimum.

The exact near-4-KiB leaf alternative is K59: `Leaf(59)=4,040` and
`Leaf(60)=4,108`. For branches, F101 is the largest complete branch not above
4,096 bytes (`29+40*101=4,069`; F102 is 4,109). These are canonical-object
fits, not claims about SQLite physical-page residence. The historical campaign
compared K64/F64, K59/F101, and K256/F256 at 100 MiB and retained 512 MiB. It
remains a custody-lost `NO-GO`, not the active closure schedule.

Fixed ordinal grouping is canonical and bounded but a count-changing early
edit can repack the entire suffix. The active fast lane accepts that documented
`O(suffix)` tradeoff under section 12.7's exact analytical bound; it makes no
logarithmic claim. A future deterministic, history-independent
content-defined/prolly mapping would require a separately approved canonical
format specification and is not required by WP4-M.

## 6. Directory-node mapping

### 6.1 Metadata

The directory metadata inner value is:

```text
common_header(tag = 0x04)
mode               u32be
```

### 6.2 Entry pages

Directory entry pages are ordinary Phase 1 Directory objects. Each semantic
child maps to:

```text
name               canonical Phase 1 name, 1..=255 bytes
child_kind          0x01 for a file NodeId; 0x02 for a directory NodeId
child_id            durable child NodeId
```

The semantic `BTreeMap` order is the canonical byte order. Duplicate names are
invalid. `MAX_DIRECTORY_PAGE_BYTES = 262,144`. Pages are greedily
partitioned in that order: add the next whole entry if and only if the resulting
complete Phase 1 Directory object is at most 262,144 bytes and has at most
100,000 direct references; otherwise close the nonempty page and begin the
next. No entry is split and an empty directory has no entry page. A decoder
must authenticate adjacent pages and reject any partition for which the first
entry of the later page would have fit in the earlier page.

A maximum-name entry occupies `4 + 255 + 1 + 32 = 292` bytes. Therefore a
selected page holds
`floor((262,144 - 13)/292) = 897` such entries: 261,937 bytes fit and
262,229 do not. Even 100,000 maximum-size entries therefore require at most
`ceil(100,000/897) = 112` pages. This byte-derived rule, not the file page
capacity, is canonical.

The 16 MiB structural object limit proves that paging is necessary but does
not select a locality ceiling. Greedy-to-16-MiB is therefore retracted as an
unmeasured final-format choice. The selected 262,144-byte ceiling keeps the worst-case
complete directory mapping at
115 objects for 100,000 maximum-name children—112 pages plus metadata, index,
and wrapper, 0.115% of the 100,000 child-object occurrences—while bounding a
same-size one-child replacement to one page plus index and wrapper. It is a
policy fallback, not a proven or measured optimum; section 12.5 freezes its
exact rewrite equation.

### 6.3 Index, wrapper, and identity

The directory index inner value is:

```text
common_header(tag = 0x03)
total_entry_count  u32be                 # 0..=100,000
page_count         u32be                 # 0..=112 under candidate limits
repeated page_count times:
  page_entry_count u32be
  first_name_len   u16be
  first_name       byte[first_name_len]
  page_object_id   [32]byte
```

Descriptors are strictly ordered by `first_name`; each count and first name
must match the authenticated page. The last name of a page is strictly less
than the next page's first name. Counts sum exactly to `total_entry_count` and
the greedy partition is recomputed. A maximum-size descriptor is
`4 + 2 + 255 + 32 = 293` bytes, so the 112-page maximum index is exactly
`13 + 11 + 4 + 4 + 112*293 = 32,848` canonical bytes.

The durable directory node is an ordinary Phase 1 Directory wrapper with
exactly these entries in canonical order:

```text
"m" -> ObjectReference(Bytes, directory_metadata_id)
"t" -> ObjectReference(Bytes, directory_index_id)
```

No other name or entry is permitted. Its canonical length is 89 bytes. The
wrapper `ObjectId` is the directory `NodeId`.

## 7. Root mapping and publication ancestry

A durable `RootId` is exactly the durable directory-node wrapper ID. The root
must have directory role. There is no root envelope and parentage is never
hashed into `RootId`.

Consequently, the same logical root reached from no parent, one parent, or
multiple parents has one `RootId`. Parent/child ancestry belongs to the delta
or publication transition. The current SQLite schema keys `layerfs_roots` only
by `root_id` while storing `parent_root`
(`crates/layerfs-engine/src/lib.rs:737-746,1123-1168`); identical content
reached from different parents can therefore conflict today. WP7 must make a
root record a content handle (`directory_object == root_id`) and keep ancestry
with the transition/delta. This is an integration constraint, not a new WP4
object.

An atomic capture publishes the authenticated strong closure, the visible
child `RootId`, the corresponding durable transition ID, and the exact
section 9.5 validation receipt in one transaction.
The storage key/type remains `DeltaId`, but the decoded durable transition is
an explicit sum:

```text
Genesis { child: RootId }                 # has_parent = 0
Change { delta: Phase3 Delta }            # has_parent = 1
```

Genesis is not a synthetic Phase 3 `Delta` and is never passed to
`Delta::apply`. Change is the only form decoded to a Phase 3 `Delta`. The
replacement corpus required by section 14 must cover both an empty-root
`has_parent=0` publication and a Change that returns to that same empty
`RootId` from a nonempty parent. Republishing identical root content under a
different transition never changes its `RootId`. No root or transition becomes
visible before all strong objects qualify.

## 8. Delta mapping

### 8.1 Semantic order and entries

`Delta::new` admits its Vec order and duplicate paths, and `Delta::apply`
executes entries sequentially (`crates/layerfs-core/src/delta/mod.rs`). The selected format
therefore preserves exact entry order. It never sorts, deduplicates, or merges
operations. A reordering is a different semantic delta and has a different
`DeltaId`; repeated paths are not malformed.

Paths are the existing canonical path bytes with `path_len:u32be`, at most
4,096 bytes and 256 components. The empty path represents the root. Entries
are:

```text
Add (tag 0x01):
  path_len:u32be | path | after_node_id:[32]byte

Remove (tag 0x02):
  path_len:u32be | path | before_node_id:[32]byte

Replace (tag 0x03):
  path_len:u32be | path | before_node_id:[32]byte |
  after_node_id:[32]byte

Metadata (tag 0x04):
  path_len:u32be | path | before_node_id:[32]byte | before_mode:u32be |
  after_node_id:[32]byte | after_mode:u32be
```

The entry tag precedes the shown bytes. Add and Replace refer to the durable ID
of the `TreeNode` embedded by the current Phase 3 value; they do not recursively
embed its bytes. Before/after IDs and modes are checked by sequential replay.

### 8.2 Entry pages

A delta entry page inner value is:

```text
common_header(tag = 0x06)
entry_count        u32be
entries            entry[entry_count]
```

Pages greedily preserve Vec order. Add the next complete entry if and only if
the resulting inner Bytes field is at most 8,388,608 bytes and its direct
strong `NodeId` fields are at most 100,000; otherwise close the nonempty page.
No entry is split, there is no empty page, and noncanonical slack is rejected.

The largest entry is Metadata with a 4,096-byte path:
`1 + 4 + 4,096 + 32 + 4 + 32 + 4 = 4,173` bytes. The 15-byte page
header plus 2,010 such entries is 8,387,745 bytes; 2,011 is 8,391,918 and is
rejected. Thus 100,000 worst-size entries require at most 50 pages. For short
Replace/Metadata entries, the 100,000 direct-reference cap closes a page after
50,000 two-ID entries even if more bytes would fit.

### 8.3 Delta index and identity

The delta index inner value is:

```text
common_header(tag = 0x05)
has_parent         u8                     # exactly 0 or 1
if has_parent == 1:
  parent_root_id   [32]byte
child_root_id      [32]byte
entry_count        u32be                  # 0..=100,000
page_count         u32be                  # 0..=50
page_object_ids    [32]byte * page_count
```

`has_parent == 1` maps a Phase 3 `Delta`, whose parent is mandatory. Empty
entry Vecs are allowed; replay then requires child equals parent. The one
`has_parent == 0` form is a genesis publication and requires zero entries and
zero pages. It is not synthesized as a Phase 3 `Delta`.

The canonical delta-index `ObjectId` is `DeltaId`. A one-page delta still uses
the index plus page shape, so the format does not collapse at a threshold.

### 8.4 Translation to and from provisional Phase 3 identities

Current `DeltaEntry` before/after fields contain provisional in-memory
`TreeNode::identity()` values. Those values are never serialized as durable
NodeIds and durable IDs are never inserted into those in-memory fields.

Encoding a current Phase 3 `Delta` proceeds by sequential replay from its
current parent `RootHandle`:

1. before mapping any object, require the supplied parent handle's provisional
   `RootId` equals `Delta::parent()`; otherwise fail `DeltaParentMismatch`;
2. map/authenticate the parent tree and obtain its durable `RootId`;
3. for Add, map the embedded node and encode its durable NodeId;
4. for Remove/Replace, resolve the path in the current working root, require its
   provisional ID equals the entry's `before`, and encode the resolved node's
   durable ID; Replace also maps/encodes the embedded after node;
5. for Metadata, resolve the current node, check its provisional before ID and
   `before_metadata`, derive `current.with_metadata(after_metadata)`, check the
   entry's provisional after ID, and encode the durable before/after NodeIds and
   exact modes;
6. apply the entry to the working current root using existing Phase 3 rules;
   and
7. after all entries, require the current provisional child matches the
   Phase 3 delta child and encode the independently mapped durable child
   `RootId`.

Decoding `has_parent=1` is the inverse. Starting from the authenticated decoded
parent `RootHandle`, process durable entries sequentially. Resolve each before
state and compare its recomputed durable NodeId to the encoded durable ID; load
Add/Replace after nodes by durable ID; derive Metadata after state and compare
its durable ID. Construct the current in-memory `DeltaEntry` with the resolved
node's **provisional** before/after IDs, apply it to a working `RootHandle`, and
finally call `Delta::new(parent.id(), child.id(), entries)` with current
provisional root IDs. The resulting current Phase 3 delta must pass
`apply(parent)` and its reconstructed child's durable `RootId` must equal the
index child. `has_parent=0` bypasses this translation and yields `Genesis`.

## 9. Strong edges, reconstruction, and limits

### 9.1 Strong edges and order

Strong edges are traversed in this exact order:

| Object role | Ordered strong edges |
|---|---|
| canonical chunk Bytes | none |
| file root | child IDs in descriptor order |
| file branch | child IDs in descriptor order |
| file reference leaf | canonical chunk-object IDs in reference order; raw `ChunkId` is not an edge |
| directory wrapper | `m`, then `t` |
| directory index | page IDs in descriptor order |
| directory page | child NodeIds in canonical name order |
| delta index | parent if present, child, then page IDs in page order |
| delta page | before/after NodeIds in entry order and field order |

### 9.2 Authentication and validation sequence

For each expected strong-edge occurrence, checks run in this exact order:

1. if the expected `(ObjectId, role)` is already on the active ancestor stack,
   fail `MappingCycle` before charging, lookup, or fetch;
2. look up the expected ID; if it is absent, fail `MissingObject(id)`;
3. reject a trustworthy advertised size above the Phase 1 bound, then fetch
   the complete object into one bounded canonical-input buffer; without a
   trustworthy size, stop before accepting more than the bound;
4. authenticate the complete canonical bytes against the expected ID;
5. validate the Phase 1 grammar left to right: magic, kind tag, declared
   lengths and bounds in serialized order, record bodies and their ordering,
   then exact outer EOF;
6. require the expected outer `ObjectKind`;
7. validate the mapping grammar left to right: all eight magic bytes, version,
   record tag, expected mapping role, scalar discriminators/bounds and checked
   arithmetic in serialized order, declared bodies and IDs in serialized
   order, exact inner EOF, then page partition/count/aggregate cross-checks;
8. enqueue strong edges in the table order and continue the iterative DFS; and
9. cross-check raw chunk identities and lengths, every file subtree's height,
   counts and cumulative lengths, root role, and delta replay/parent/child
   results.

Authentication precedes semantic trust and reuse. Every root, branch, leaf,
page, and chunk actually fetched is hashed over its complete canonical bytes
and semantically validated. A publication receipt can authorize skipping
unvisited siblings under section 9.5; it never turns a locator, existence
result, cached ID, or partial remote range into authenticated object bytes. A
whole-object BLAKE3 ID does not authenticate an unverified remote byte range.
An immutable PUT-if-absent plus one atomic visible-head/transition publication
is compatible with Memory, SQLite, and a plausible remote object service. A
presence/HEAD response alone is not authenticated reuse.

### 9.3 Structural bounds

The canonical admission bounds are:

| Bound | selected value and source |
|---|---|
| canonical object | 16,777,216 bytes, existing `MAX_OBJECT_BYTES` |
| Bytes inner field | 8,388,608 bytes, existing `MAX_OBJECT_FIELD_BYTES` |
| per-object direct references/entries | 100,000, existing `MAX_CHILD_REFERENCES` |
| canonical name | 255 bytes, existing `MAX_COMPONENT_BYTES` |
| canonical path | 4,096 bytes and 256 components, existing path limits |
| raw chunk | 32,768 bytes, frozen CDC maximum |
| file reference leaf | selected K64 references; 4,380 canonical bytes |
| file branch | selected F64 children; 2,589 canonical bytes |
| file root | selected F64 children; 2,609 canonical bytes |
| file total count/length | checked `u64`; no lower arbitrary total cap |
| directory page | 262,144 canonical bytes, selected DIR256K ceiling |
| directory pages | 112 maximum, derived above |
| delta pages | 50 maximum, derived above |
| durable logical tree depth | 256 child components |

The old equation `100,000 * 32,768 = 3,276,800,000` bytes (about 3.052 GiB)
described the withdrawn single-index draft, not the scalable mapping. In this
candidate 100,000 is only a direct-reference limit; files use checked `u64`
length/count fields and as many bounded branch levels as those fields require.
The other principal structural limits are 100,000 children in one directory
node or entries in one delta, 256 path components and 4,096 path bytes, a
16 MiB canonical object, and an 8 MiB Bytes field. These are chosen software
protections, not physical limits.

Directly constructed deeper `TreeNode` values fail durable admission with
`MappingDepthExceeded`/`PathLimitExceeded`; no visible head or transition is
published. Authenticated immutable objects acknowledged before the failure may
remain as unreachable residue under the custody rule in section 10.
The existing Phase 1 parser nesting limit 8 is not a graph traversal limit.

K64/F64 capacity with `h` file-branch layers is `64*64^(h+1)` references.
Checked `u64` reference count therefore needs at most nine branch layers; the
last layer needs at most 16 children. A 100-GiB file needs exactly two branch
layers at all three section 12.1 densities. From a delta index, the longest
physical strong-edge path is at most `2 + 3*256 + 11 = 781` edges: delta page
and node, 256 directory wrapper/index/page child steps, then file root, nine
branches, leaf, and chunk. The active stack is therefore at most 782
`(ObjectId, role)` frames; the 100-GiB cases need at most 775. An active repeat
is `MappingCycle`; a completed shared sub-DAG occurrence is valid. Without a
valid snapshot receipt it is re-authenticated on each occurrence; with one,
only actually accessed objects are fetched, and each is still authenticated.
The selected format does not require an unbounded visited set.

### 9.4 Exact decode/allocation and operation admission

Per-object limits do not bound an eager `TreeNode` graph: a tree can have many
bounded objects. This mapping therefore separates peak resident allocation
from cumulative work. `Q` is the only fixed mapping-owned resident-allocation
ceiling. `W`
and `D` are checked `u64` accounting totals, not arbitrary file/workspace
limits and not serialized identity.

At time `t`, define the live allocation charge and its high-water value:

```text
q_live(t) = 256 * live_reconstructed_tree_nodes
  + sum(256 + name_length) for live reconstructed directory entries
  + 96 * live_reconstructed_file_references
  + sum(256 + path_length) for live reconstructed delta entries
  + live_canonical_input_or_builder_bytes
  + live_traversal_spool_window_bytes
  + live_eager_semantic_output_bytes
  + live_stream_output_window_bytes
  + live_receipt_bytes

Q = max over t of q_live(t)
MAX_DURABLE_LIVE_ALLOCATION = 1,073,741,824 bytes (1 GiB)
```

Every live mapping-owned or mapping-requested allocation is assigned to exactly
one Q term. The structural constants conservatively cover IDs, modes,
Vec/map/Arc slots, and allocator-requested storage. Growing an allocation first
increments `q_live` with checked `u64` arithmetic; releasing it decrements
`q_live`, while Q remains the high-water mark. The 1-GiB ceiling is a
pathological durable-admission guard derived from the maximum Phase 3 delta
shape, not a normal allocation plan and not a cumulative payload allowance:

```text
MAX_ENTRY_CHARGE = MAX_PATH_BYTES + 256 = 4,352
BASE = MAX_CHILD_REFERENCES * MAX_ENTRY_CHARGE = 435,200,000
MAX_DURABLE_LIVE_ALLOCATION =
  2 * next_power_of_two(BASE) = 1,073,741,824 bytes
```

The factor two admits one complete maximum-entry vector plus bounded work
windows with explicit margin. An explicit eager compatibility conversion that
would exceed Q fails its preflight with `AllocationBudgetExceeded`. Capture,
closure, full reconstruction, and range APIs stream by default; a 100-GiB file
does not require a 100-GiB resident buffer or an eager `Vec`.

Define independent cumulative telemetry:

```text
W = canonical_bytes_produced_or_fetched_authenticated
  + payload_io_bytes
  + 64 * object_occurrences
  + 256 * tree_node_reconstruction_events
  + sum(256 + name_length) for directory-entry reconstruction events
  + 96 * file-reference reconstruction events
  + sum(256 + path_length) for delta-entry reconstruction events
  + traversal_spool_bytes_written
  + receipt_evidence_bytes_hashed

D = cumulative_streamed_or_spooled_output_bytes
```

`payload_io_bytes` counts raw source bytes consumed by capture or raw bytes
delivered/spooled by reconstruction or a range read. Canonical bytes are
charged once per produced/fetched-and-authenticated occurrence, not once per
hash/parser pass. A producer or receiver increments W and D with checked
`u64` arithmetic before each append/receive/delivery. Re-authenticated shared
DAG occurrences and every exact traversal-spool record are counted again.
Overflow is `LengthOverflow`; there is no fixed W/D ceiling below `u64::MAX`.
A caller may impose a deadline, cancellation, storage quota, or explicit work
budget, but that is a noncanonical operation policy and cannot make a durable
file unrepresentable. Reusing a released stream window raises W/D but not Q.

For retained S1-512 (`536,870,912` raw bytes and `27,162` references), a
streamed reconstruction may deliver all 512 MiB while reusing the same fixed
windows. Its W/D totals exceed 512 MiB as expected; they do not consume Q. An
explicit eager 512-MiB compatibility result would request at least that much
Q and must be preflighted, but is never allocated silently. Streaming is the
WP5 default. Q overflow occurs before the next allocation and returns
`AllocationBudgetExceeded`; no visible head/transition is published, although
already acknowledged immutable objects may remain unreachable under section
10.

The simultaneous in-memory decode components are separately bounded:

| Component | Exact maximum | Rule |
|---|---:|---|
| canonical transport/input buffer | 16,777,216 bytes | one complete Phase 1 object; Bytes inner is a borrowed slice, never a second 8 MiB copy |
| mapping parser heap scratch | 0 bytes | names, paths, entries, and Bytes inner fields borrow from the authenticated input; fixed integer/ID temporaries stay on the call stack |
| active DFS frames | 50,048 bytes | 782 frames charged at an exact maximum of 64 bytes per `(id, role, next-edge)` frame |
| in-memory traversal spool window | 8,388,608 bytes | live capacity charges Q; overflow uses a caller-owned temporary random-access traversal spool and every exact edge-record byte written charges W |
| snapshot receipt | 216 bytes | one complete canonical `ValidatedSnapshotReceiptV1`; no per-object locator transcript |
| eager semantic output, including a compatibility `Vec` range result | at most the remaining Q, never above 1,073,741,824 bytes in aggregate | charged before every requested allocation by the equation above |
| streamed/spooled output window | 8,388,608 bytes | larger reads/full reconstruction reuse successive windows; live capacity charges Q and cumulative delivered bytes charge W and D |

The canonical buffer, spool window, stream window, receipt, and every semantic
allocation are already included in Q. The adapter may not queue an unbounded
number of objects, receipts, pending descriptors, or decoded nodes. An eager
caller-owned result and backend buffers requested on the mapping's behalf are
charged too; ownership cannot evade Q. Process/runtime page cache is outside
this mapping-accounting number and is measured separately as RSS.

The 1 GiB Q limit exists only to give pathological but structurally valid
durable input a finite admission outcome. It is not an allocation plan.
Ordinary capture, closure, reconstruction, and range operations stream through
at most one 16,777,216-byte canonical input/builder, one 8,388,608-byte
traversal-spool window, one 8,388,608-byte output window, the 216-byte receipt,
and the 50,048-byte DFS stack: 33,604,696 bytes before explicitly charged live
semantic results and backend/runtime overhead. They must not preallocate Q,
retain a whole closure, or build an unbounded ID map. WP5 qualification records
actual Q, W, D, mapping-requested peak bytes, and process RSS.

Closure traversal uses an iterative DFS and a bounded random-access edge spool
so it never retains a source-sized pending-ID map or refetches a wide parent
once per child. After authenticating and decoding one parent occurrence once,
the traversal emits every strong edge in section 9.1 order as the exact
noncanonical spool record `child_id[32] || role:u8 || reserved[7]=0`, 40 bytes.
It increments W for `40*edge_count` before writing the run and Q for the live
spool-window capacity, records
`(run_start:u64, edge_count:u32, next_edge:u32)` in the parent's 64-byte frame,
and releases the parent object buffer. A direct-reference maximum run is
4,000,000 bytes and therefore fits the 8 MiB in-memory window.

Parent resumption reads exactly one record at
`run_start + 40*next_edge`, increments `next_edge` before descending, and does
not fetch or decode the parent again. A child's run is appended after its
parent's live run; child completion discards/truncates that child run, and the
parent run remains until its cursor reaches `edge_count`. Overflow beyond the
8 MiB memory window uses a bounded temporary random-access spool; cumulative
edge bytes remain W-charged, and the 782 active frames are the only ancestry
state. Reconstructed raw file bytes are streamed to the caller/spool; they are
not accumulated in `RootHandle`.
Reconstructing the current eager `TreeNode`/`RootHandle` is permitted only while
its live nodes, names, directory entries, and file references fit Q and its
cumulative counters fit checked `u64`. A root with arbitrarily many descendants
therefore does **not** allocate without bound: eager compatibility conversion
fails `AllocationBudgetExceeded`, while the streamed form continues with
bounded live state.
Allocation refusal, spool capacity/no-space, permission, short-I/O, transport,
or cancellation failures use the typed outcomes in section 10 and are never
converted to `Unsupported`. A failure known to occur before publication
dispatch prohibits visibility; a failure after dispatch follows the
reconciliation rules below and may be ambiguous.

### 9.5 Storage-backend compatibility semantics

This section freezes semantics, not a backend trait. Every backend has a
random immutable 16-byte `store_instance_id`; it is process-local for Memory
and persisted for a durable store. A durable backend also has a protected
32-byte `validation_key` and a checked `integrity_epoch:u64`. The epoch changes
before any engine-authorized deletion, replacement, repair, or other mutation
that could invalidate an already validated immutable object. Ordinary
insert-if-absent does not invalidate earlier receipts.

The exact publication receipt inner bytes are:

```text
receipt_magic             [8]byte = "LFS4VAL\0"
receipt_version           u16be   = 1
receipt_kind              u8      = 1       # validated snapshot
store_instance_id         [16]byte
validation_authority_id   [32]byte
integrity_epoch           u64be
head_generation           u64be
child_root_id             [32]byte
transition_id             [32]byte
mapping_profile_id        [32]byte
authenticator             [32]byte
```

The inner length is 203 bytes and its complete canonical Phase 1 Bytes object
is 216 bytes. Exact domains are:

```text
mapping_profile_id = BLAKE3(
  "layerfs/mapping-profile/v1\0"
  || u32be(64)
  || u32be(64)
  || u32be(262144)
  || u32be(8388608))
validation_authority_id = BLAKE3(
  "layerfs/validation-authority/v1\0" || store_instance_id || validation_key)
authenticator = BLAKE3_keyed(validation_key,
  "layerfs/validated-snapshot/v1\0" || all preceding receipt fields)
```

The displayed preimage defines the one selected production profile ID:

```text
b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1
```

Historical WP4-M databases instead derived a private candidate ID exactly as:

```text
BLAKE3("layerfs/mapping-profile/wp4m/v1\0"
  || u32be(K) || u32be(F)
  || u32be(directory_page_ceiling)
  || u32be(delta_page_ceiling))
```

That private ID remains historical evidence only. WP4-P deleted it from live
admission and bound every production receipt, selected-only corpus, and durable
profile check to the production ID above before compatibility promotion.

`ValidatedSnapshotReceiptV1` is a backend-private authenticated attestation,
not a logical object and not part of `RootId` or `DeltaId`. The atomic visible
value is:

```text
VisibleHead {
  generation: u64,
  child: RootId,
  transition: DeltaId,
  validation_receipt: [216]byte,
}
```

The publication idempotency/CAS key is the exact 32-byte value:

```text
BLAKE3("layerfs/publication-idempotency/v1\0" ||
       store_instance_id:[16]byte ||
       prior_tag:u8 ||
       (if prior_tag == 1:
          prior_generation:u64be || prior_child:[32]byte ||
          prior_transition:[32]byte || prior_receipt:[216]byte) ||
       requested_generation:u64be || requested_child:[32]byte ||
       requested_transition:[32]byte || requested_receipt:[216]byte)
```

`prior_tag` is exactly 0 for genesis and exactly 1 for an existing complete
head; all other values are invalid. The key is operation/retry authority, not a
fifth `VisibleHead` field and not content identity. Reconciliation reads the
authoritative complete head and recomputes the requested key from the retained
prior/request tuple; it never infers a key from a partial head.
Generation increments by checked one. Genesis has no prior head; Change
requires the expected generation/root and its decoded parent. Publication
first establishes complete child/transition closure validity, then creates the
receipt, then atomically makes the whole `VisibleHead` visible. Genesis or an
unreceipted capture establishes that fact with the full section 9.2 traversal.
Incremental publication may instead combine a still-valid prior receipt with a
bounded structural diff: authenticate the prior and replacement nodes on every
changed spine; compare their ordered strong-edge IDs; treat an equal child ID
as covered by the prior closure; and fully traverse every new/different child
ID. Induction from the authenticated root proves that the new closure is the
union of prior-covered equal subtrees and fully authenticated new subtrees.
Any missing or corrupt changed-path object fails before publication. This rule
requires no whole-closure replay or unbounded ID map, but it is valid only
under the receipt authority below.

The receipt proves that the closure was complete and authenticated at that
publication under the named store authority, integrity epoch, mapping profile,
generation, child, and transition. It does **not** make fetched bytes trusted,
prove that a skipped sibling is freshly present, or authorize arbitrary
locators/ranges. Every fetched object is still fully hashed and semantically
validated. A missing or corrupted skipped sibling returns its typed failure
when first accessed. Fast reopen means authenticated head/root plus lazy
per-access authentication; it is not full-closure verification. A separate
full-closure scrub ignores the fast-path skip and authenticates every
occurrence.

| Required operation | Memory | SQLite | Plausible remote object service |
|---|---|---|---|
| immutable conditional put/reuse | map insert-if-absent; authenticate and byte-compare an incumbent | `INSERT`-if-absent in the one-writer transaction; authenticate and byte-compare an incumbent BLOB | conditional PUT by `ObjectId`; client authenticates an incumbent/full response unless a separately trusted immutable service attestation exists |
| bounded batch/read/request | one bounded object/batch within Q | one bounded BLOB/batch within Q; the format does not require per-object SQL | one bounded request/response/batch within Q |
| object/range read | authenticate every fetched complete object | authenticate every fetched complete BLOB before using it | client authenticates complete fetched objects; a bare range/HEAD is not object truth |
| capture owner | one exclusive capture guard | one `BEGIN IMMEDIATE`-equivalent writer transaction | one client owns the idempotency key through success, definite failure, conflict, or handed-off ambiguous retry |
| atomic publication/CAS | atomic swap of the complete `VisibleHead` after validation | one head row stores the complete tuple and changes in the same COMMIT after validation | one linearizable conditional write changes the complete tuple after validation |
| receipt authority | same-open process authority only | persisted key/epoch under the engine-controlled database trust model | service-specific protected key/epoch with equivalent mutation coverage |
| fast reopen | current process receipt plus lazy per-access authentication | receipt verification plus head/root and lazy per-access authentication, subject to the trust limitation below | equivalent only when the service proves the exact store authority/epoch |
| full scrub | authenticate the complete closure | authenticate the complete closure | authenticate the complete closure |
| bounded memory/count | one object buffer, 782-frame stack, Q; no unbounded visited set | same plus one bounded BLOB/batch | same plus one bounded response/batch |

For remote operation counts, section 12.6 gives layout-dependent requests;
immutable PUTs may be parallel after IDs are known, but head publication is one
final dependent request. Head reads and CAS are linearizable. A stale head may
be used only when the caller explicitly requests that older pinned snapshot;
it is never compare-and-publish or reconciliation authority.

For SQLite, the receipt authority is sound across normal reopen only under the
explicit trust assumption that all object-table deletion/replacement/repair
goes through one engine transaction that first advances `integrity_epoch`,
performs the mutation, and commits both atomically; no receipt is issued or
accepted against an in-flight epoch. The database file and protected key must
not be copied or rolled back independently, and the OS/filesystem/SQLite
durability boundary must be trusted. A logical head or
epoch counter alone cannot detect out-of-band BLOB corruption, deletion,
replacement, file rollback, or a copied database. WP4 does not invent VFS/page
tracking. Consequently, this selected profile does not authorize adversarial
cross-reopen receipt reuse for the current SQLite engine: proof is pending and
reuse is limited to the same open immutable generation. A normal cross-reopen
row may be reported only under the explicit non-adversarial database trust
assumption above. Otherwise the implementation performs full scrub (or returns
`ValidationAuthorityUnavailable`) instead of claiming fast receipt
equivalence. Full scrub remains mandatory as a separate operation and its
benchmark cannot be replaced by the fast-reopen row unless a later controlling
spec explicitly declares the trust model equivalent.

WP4 freezes no byte-cache or locator-receipt fast path. If later measurements
justify one, WP10 must bind its bounded entry to the exact immutable store,
validation authority, integrity epoch, mapping profile, generation,
authenticated root/transition, object, and locator/row/range. Its count and
byte bounds and deterministic eviction must be explicit. A head cache is never
CAS authority. The selected profile has no per-range receipt,
locator transcript, unbounded receipt cache, or capability for arbitrary
first-time ranges. The one snapshot receipt only authorizes
skipping unvisited siblings under the exact published snapshot; it never
authenticates newly fetched bytes. A present receipt whose bytes or bound tuple
fail verification returns `InvalidValidationReceipt`; it is never relabeled
`ValidationAuthorityUnavailable`. The caller may then start a separate full-
scrub attempt without receipt authority. If that scrub fails, it returns its
own precise first/dominant cause and the earlier invalid-receipt result remains
a separate diagnostic; the two attempts are not folded into ambiguous failure
provenance. A missing receipt or an inability to establish the required
authority may instead run full scrub directly or return
`ValidationAuthorityUnavailable`. The receipt audit has these exact cases:

| Attempt | Required result |
|---|---|
| stale receipt against a newer head | tuple mismatch: `InvalidValidationReceipt` |
| receipt copied to another store ID/key | store/authority/authenticator mismatch: `InvalidValidationReceipt` |
| receipt bytes or mapping profile changed | profile/authenticator mismatch: `InvalidValidationReceipt` |
| engine-authorized deletion/replacement/repair without head change | epoch changes first; old receipt is `InvalidValidationReceipt` |
| out-of-band object deletion/replacement with no epoch change | the logical receipt cannot detect a skipped mutation; adversarial fast reuse is forbidden, and full scrub/access reports the typed object failure |
| authoritative head alone rolled back without external monotonic freshness authority | database-local state cannot prove freshness; a rollback-resistant caller receives `ValidationAuthorityUnavailable` and no fast reuse |
| database/head/key rollback together | no database-local logical field proves freshness; callers requiring rollback resistance receive `ValidationAuthorityUnavailable`, not a fast-reopen claim |

These cases are mandatory independent golden/model tests after final profile
selection. None may silently broaden the trust claim.
Timeout/cancellation before publication dispatch guarantees no visible-head
change; after dispatch section 10 reconciliation applies. Raw SQL remains
backend-private.

## 10. Typed failures and precedence

WP5 must preserve existing Phase 1 errors and add the precise mapping errors
below where the current `CoreError` vocabulary is insufficient:

| Condition | Frozen first cause |
|---|---|
| expected strong-edge object is absent | new `MappingError::MissingObject(ObjectId)`; preserve/translate the existing ID-bearing `EngineError::MissingObject`, because current `CoreError::MissingObject` is a unit variant |
| object or field over an existing bound; one direct count over 100,000; chunk over 32,768 | `ObjectLimitExceeded` |
| checked length/count/offset/counter arithmetic fails | `LengthOverflow` |
| expected ID differs from complete fetched bytes | `IdentityMismatch` |
| truncated fixed field or ID | `UnexpectedEof` |
| bytes remain inside or after a declared record | `TrailingBytes` |
| invalid Phase 1 kind tag | `InvalidObjectKind` |
| valid Phase 1 object used in the wrong mapping role/kind | `WrongLogicalRole` |
| mapping magic or record tag unknown | `InvalidMappingTag` |
| mapping version not 1 | `UnsupportedMappingVersion` |
| `has_parent` is not 0/1 or delta operation tag is not 1/2/3/4 | `InvalidMappingDiscriminator` |
| directory names not strictly ordered within a Phase 1 page | `NonCanonicalOrdering` |
| duplicate directory name across pages | `NameCollision` |
| page/level counts, fullness, first-name routing, or the selected fixed-radix partition is noncanonical | `NonCanonicalPagePartition` |
| raw chunk length or aggregate length differs | `LengthMismatch` / `ChunkLengthMismatch` |
| raw payload hash differs from stored raw `ChunkId` | `ChunkIdentityMismatch` |
| invalid canonical path or path/depth limit | `InvalidPath` / `PathLimitExceeded` / `MappingDepthExceeded` |
| active `(ObjectId, role)` repeats | `MappingCycle` |
| exact Q live-allocation charge would exceed 1,073,741,824 bytes | `AllocationBudgetExceeded` |
| receipt bytes/authenticator/profile/store/generation/epoch/head tuple mismatch | `InvalidValidationReceipt` |
| required validation authority/key/epoch guarantee is unavailable | `ValidationAuthorityUnavailable` |
| requested range does not satisfy `start <= end <= total_raw_length` | existing `InvalidRange` |
| delta parent, child, before/after state, or replay conflicts | existing `DeltaParentMismatch`, `DeltaChildMismatch`, or `DeltaConflict` |
| mapping-owned allocation is refused | `AllocationFailed` |
| spool/backend has insufficient capacity | `CapacityExceeded` |
| spool/backend permission is denied | `PermissionDenied` |
| spool/backend returns an incomplete read or write without a terminal typed cause | `ShortIo` |
| caller/backend cancels the operation | `Cancelled` |
| a bounded backend deadline expires | `TimedOut` |
| transport fails without a more precise class | existing `Io` |
| compare-and-publish observes a different authoritative head | `PublicationConflict` |
| commit outcome cannot be resolved as definitely committed or definitely absent | `AmbiguousDurability` |

For delta encoding, the supplied-parent check in section 8.4 occurs before any
mapping work. For closure/load, precedence is the numbered sequence in section
9.2; within its mapping step, magic, version, tag, expected role, serialized
scalar/discriminator checks, serialized bodies, inner EOF, and aggregate
cross-checks occur in exactly that order. An active cycle is therefore the
first cause before Q exhaustion, absence, or corruption of the repeated
target. Backend/resource failures retain the most precise available class.

Every failed public operation preserves this exact fixed-size provenance on
the stack; it never builds an input-sized error list:

```text
FailureProvenance {
  first: TypedCause,
  cleanup_first: Option<TypedCause>,
  reconciliation: Option<TypedCause>,
  dominant: Option<TypedCause>,
}
```

`first` is the earliest cause under the numbered validation order or, once
validation is complete, event order, and is never overwritten. After `first`,
mandatory cleanup runs in this order: stop further mutable/spool I/O, close the
private temporary traversal/output spool, remove that spool, then rollback or
release the backend-local transaction/publication owner when its state is
known not dispatched. `cleanup_first` preserves the first precise cleanup
failure in that order; later cleanup failures are not retained, allocate no
provenance storage, and cannot replace it. Spool close/removal failure after an
earlier cause never replaces the dominant cause because it cannot change the
visible head. If cleanup itself is the first operation failure, it occupies
`first` and `dominant=Some(first)` and `cleanup_first` is empty.

Before publication dispatch, `dominant = Some(first)`. After dispatch,
authoritative reconciliation sets `reconciliation` to its precise read or
classification failure, if any, and determines the public result exactly:

- complete requested `VisibleHead`, including its byte-identical validation
  receipt, visible and its recomputed key equal to the retained request key:
  success and `dominant = None`; the
  first/cleanup slots remain diagnostic only;
- exact prior head visible: `dominant = Some(first)`;
- different authoritative head: `dominant = Some(PublicationConflict)`; and
- requested/prior/different cannot be established:
  `dominant = Some(AmbiguousDurability)`.

Thus `PublicationConflict` and `AmbiguousDurability` may dominate without
erasing the first or cleanup cause. A cleanup failure never masks a precise
parse, capacity, cancellation, or transport cause, and generic `Io` never
masks a more precise typed cause. The checklist's typed-error claim includes
both `first` and `dominant` under this rule.

Publication has three exact phases. Before atomic publication dispatch, every
typed failure guarantees no visible-head change. It does **not** promise
physical rollback: authenticated content-addressed objects acknowledged before
the failure may remain as unreachable residue. Such immutable residue is in
the store/capture owner's custody, is never a root or transition, and may be
reused only after exact ID authentication; the selected format performs no delete and leaves
garbage collection to a later explicit policy. A private temporary spool stays
in the operation/engine's custody until the ordered cleanup attempt above; a
failed cleanup is reported in `cleanup_first` and must not be described as
successful deletion. After a COMMIT or conditional
compare-and-publish request is dispatched, `ShortIo`, `Io`, `TimedOut`, or
`Cancelled` must reconcile the authoritative visible-head tuple whenever the
backend has not already proved definite absence. If the exact requested
`VisibleHead { generation, child, transition, validation_receipt }` is visible,
including byte-identical receipt, and its recomputed key equals the retained
request key, return success. If the
expected prior complete head is still authoritative, return the original
precise error and guarantee absence. If a different complete head is
authoritative, return `PublicationConflict`. If none of those conclusions can
be established, return `AmbiguousDurability`, make no claim whether
publication occurred, and permit only an idempotent retry of the identical
complete transition key. `AmbiguousDurability` must never be reported as
rollback or downgraded to generic `Io`.

A malformed ID has no separate in-record representation: every encoded ID is
exactly 32 bytes, so a short ID is `UnexpectedEof`; text parsing errors remain
outside this binary grammar. Duplicate/repeated file references, repeated
object IDs in a DAG, and repeated delta paths are valid. Duplicate directory
names are not.

## 11. Future compatibility

The record specifies the one compatibility-promoted K64/F64 + DIR256K topology
and rejects an unknown mapping version before interpreting version-dependent
fields. WP4-P completed selected-only cleanup, goldens, fingerprints, and both
audits. Earlier draft golden IDs have no compatibility authority.

After promotion, an encoder emits exactly one canonical profile. A future page
capacity/topology change uses a new mapping version/profile and intentionally
produces new object/root/delta IDs. A separately authorized future migration
may retain an older decoder, but a live durable admission boundary must not
accept two interchangeable encodings for one logical value. Phase 4 authorizes
only the narrow SQLite schema transition in
`../storage/sqlite/visible-head.md`.

## 12. Performance decision gate

The mapping must leave a credible route to the 100-MiB durable target of
200 MiB/s (500 ms) and stretch target of 300 MiB/s (333.333 ms), but WP4 does
not claim either target. Routine WP4-M closure uses full writes and roundtrips
at deterministic 1-MiB, 10-MiB, and retained 100-MiB sources plus edit arms
only at 100 MiB, with inputs generated outside the timer and one warmup plus
three measured runs per capture arm. The rows separate per-byte
CDC/hash/canonical/payload I/O, per-object/SQL cost, per-file/transaction fixed
cost, index-height transitions, SQLite physical amplification, Q/RSS, and
reopen/range/edit path work. The 100-GiB requirement is analytical capacity and
cost validation, never a local 100-GiB benchmark or naive wall-time
extrapolation. Retained 512-MiB work is occasional scale evidence only, not
routine closure or a condition of WP4-P eligibility.

### 12.1 Exact object counts under the frozen CDC profile

For K64/F64, let `P=ceil(C/64)` and repeatedly let
`Bi=ceil(previous/64)` while the previous level is greater than 64. Then:

```text
O(C) = P + sum(Bi) + 1
M(C) = 68*C + 68*P + 69*sum(Bi) + 49
chunk_object_framing(C) = 13*C
```

All arithmetic is checked `u64`. Exact retained and analytical counts are:

| Case | C chunk refs | Leaves P | Branch nodes by level | Mapping objects O | Mapping bytes M | Chunk framing | Mapping + framing |
|---|---:|---:|---:|---:|---:|---:|---:|
| retained S1-100 | 5,284 | 83 | 2 | 86 | 365,143 | 68,692 | 433,835 |
| retained S1-512 | 27,162 | 425 | 7 | 433 | 1,876,448 | 353,106 | 2,229,554 |
| 100 GiB, every chunk 32 KiB | 3,276,800 | 51,200 | 800, 13 | 52,014 | 226,360,146 | 42,598,400 | 268,958,546 |
| 100 GiB, retained S1 density | 5,410,816 | 84,544 | 1,321, 21 | 85,887 | 373,777,127 | 70,340,608 | 444,117,735 |
| 100 GiB, every chunk 8 KiB | 13,107,200 | 204,800 | 3,200, 50 | 208,051 | 905,440,299 | 170,393,600 | 1,075,833,899 |

The retained-density row is the exact S1-100 count multiplied by 1,024, an
average 19,844.360 bytes/reference; it is a model point, not a promise that a
new 100-GiB CDC stream has that exact count. Metadata plus chunk framing is
0.2505%, 0.4136%, and 1.0019% of 100 GiB respectively, before SQLite row,
B-tree, page, WAL, and free-space amplification. Each case has two branch
levels and physical root-to-chunk path depth four (root, two branches, leaf,
chunk). The root sizes are 569, 889, and 2,049 bytes; a cold one-leaf mapping
path is at most 10,127, 10,447, and 11,607 canonical bytes respectively.

The three pre-release file candidates use the same grammar and differ only in
their fixed leaf/branch capacities. Their exact structural comparison is:

| K/F | max leaf / branch bytes | S1-100 P; branches; O; M | S1-512 P; branches; O; M | 100-GiB retained P; branches; O; M | max cold one-leaf path at 100 GiB |
|---|---:|---:|---:|---:|---:|
| 64/64 | 4,380 / 2,589 | 83; 2; 86; 365,143 | 425; 7; 433; 1,876,448 | 84,544; 1,321,21; 85,887; 373,777,127 | 10,447 bytes |
| 59/101 | 4,040 / 4,069 | 90; none; 91; 365,481 | 461; 5; 467; 1,878,758 | 91,709; 909,9; 92,628; 374,235,091 | 12,587 bytes |
| 256/256 | 17,436 / 10,269 | 21; none; 22; 360,789 | 107; none; 108; 1,854,341 | 21,136; 83; 21,220; 369,378,512 | 31,074 bytes |

Here `P` is the leaf count, `branches` lists successive branch-level object
counts, `O` is all file-mapping objects including the root, and `M` is exact
canonical mapping bytes. K59/F101 is only a complete-canonical-object 4-KiB
comparison, not a claim about one SQLite physical page. The table establishes
capacity, height, and byte/work inputs; it does not select the winner without
the section 12.7 measurements.

There is no arbitrary total-file ceiling: K64/F64's radix topology represents
`64*64^(h+1)` references with `h` branches, and nine branch levels cover every
serialized `u64` count. Operational admission is still bounded mechanically
by checked derived arithmetic. In particular, K64/F64 admits at most
`C=267,036,007,400,295,520` references before canonical `M(C)` itself would
overflow u64; cumulative W/D may reject a particular operation earlier even
though the durable file remains representable for another bounded operation.
Length, reference-count, `P/Bi/O/M`, offset, cumulative-end, and W/D overflow
is typed `LengthOverflow`; object, direct-edge, logical-depth, Q, and backend-
storage limits retain their own typed causes.
Initial capture/full reconstruction is necessarily O(source bytes); at exactly
200/300 MiB/s, merely streaming 100 GiB takes 512/341.333 seconds (about
8.53/5.69 minutes), before fixed and per-object overhead. This is a lower-bound
equation, not a throughput projection.

### 12.2 Encode, copy, and hash passes

Capture needs two semantic chunk hash domains:

1. raw bytes into raw `ChunkId`; and
2. canonical `Object::Bytes(raw)` into canonical chunk-object ID.

They can be fed during one streaming source traversal; they do not require two
source reads or two raw-payload copies. Each mapping page/index is canonically
encoded once and hashed once to obtain its ID. The current SQLite put path then
validates/hashes every submitted canonical object again and authenticates a
stored incumbent before reuse (`crates/layerfs-engine/src/lib.rs:850-885`).

The selected format requires one canonical encode and one identity-hash pass for a
new object, and one complete authentication plus decode when an object is read.
It does not require a second hash/decode of the same trusted buffer, one SQL
statement per object, whole-closure replay after a valid section 9.5 receipt,
or an unbounded dedup/visited map. Those are implementation choices or current
baseline costs: WP5+ may remove them with bounded verified-buffer handoff,
batching, authenticated receipts/caches, and the bounded traversal/spool rules
without changing selected canonical bytes or IDs. Actual pass, SQL, object,
authentication, peak-Q, and RSS counters remain mandatory validation evidence.

Exact native/SQLite copy counts cannot be frozen before WP5/WP7 code exists.
The required counters are canonical bytes materialized, payload copies,
raw/canonical hash bytes, BLOB bytes, and passes. Initial capture must permit a
single streaming source traversal that feeds CDC plus both hash domains; the
format does not require duplicate source reads, duplicate decode/hash of a
trusted buffer, per-object SQL statements, whole-closure replay after a valid
receipt, or an unbounded dedup/visited map. Those are measured implementation
choices, not mapping requirements.

### 12.3 SQL statement equivalents and later batching

The current SQLite path performs one metadata SELECT plus one insert or
incumbent-byte SELECT per submitted object: two marked statements/object. Thus
the selected mapping alone costs `2*O(C)` current statements: 172 for S1-100,
866 for S1-512, and 104,028/171,774/416,102 for the three analytical 100-GiB
rows. Chunk-object and root/delta/publication statements are additional. With
a later bounded batch of B mapping rows, the lower-bound execution count is
`2*ceil(O(C)/B)` plus bounded incumbent-authentication reads and publication;
for B=255 the same rows need 2, 4, and 408/674/1,632 executions. Row work and
authenticated bytes do not disappear. B is an engine operation bound, not a
format field; WP4 adds no batching API.

### 12.4 Capture, reopen, closure, and range authentication

A publication or explicit full scrub authenticates the complete mapping and
every referenced chunk occurrence. A valid section 9.5 snapshot receipt may
instead authorize path traversal while skipping siblings, subject to its
store/epoch trust model. Fast reopen authenticates the receipt, authoritative
head, root wrapper, and objects actually accessed. It does not claim skipped
siblings were freshly checked; full-closure verification remains a separate
row. Any accessed root/branch/leaf/chunk is fetched completely, hashed against
its ObjectId, and semantically validated.

For a one-leaf file range under a valid receipt, selected mapping work is:

```text
A = Root(actual_root_children)
  + sum(Branch(actual_children_on_selected_path))
  + Leaf(actual_references_in_selected_leaf)
```

At the three 100-GiB analytical counts, the cold maximum for one selected full
leaf is exactly 10,127/10,447/11,607 canonical mapping bytes and four mapping
objects, plus the overlapping complete chunk objects. A cross-boundary request
adds each intersected sibling leaf/branch in ordinal order. The full scrub is
`M(C) + source_bytes + 13*C` before directory/root/delta objects. Without a
valid receipt, a path-only operation may not trust skipped cumulative summaries
and must scrub first or return `ValidationAuthorityUnavailable`.

The current SQLite range path hashes and decodes a full BLOB before returning a
slice (`crates/layerfs-engine/src/lib.rs:912-1017`), and public `load_object`
currently performs additional length/validation work
(`crates/layerfs-engine/src/lib.rs:377-392`). Object-sized authentication is
required; duplicate passes are not. WP5+ measures and may remove them through
bounded verified-buffer handoff without changing the format.

### 12.5 COW amplification

For selected fixed radix, a same-count reference edit rewrites one leaf
and its branch/root spine. Exact maximum mapping bytes are 7,098/3 objects at
S1-100, 7,298/3 at S1-512, and 10,447/4 for the retained-density 100-GiB
projection. CDC may change more than one reference around the resynchronization
window; WP5 measures that separately.

An early +1-reference edit is the honest worst case: fixed ordinal boundaries
shift and all later leaves plus affected branches may be replaced. A
conservative whole-file suffix ceiling is `O(C')` objects and `M(C')` bytes.
For the exact `C'=C+1` cases:

| Case | replacement leaves / bytes | branch objects / bytes | root bytes | total mapping objects / bytes | current 2-statement/object | B=255 lower-bound executions |
|---|---:|---:|---:|---:|---:|---:|
| S1-100, C'=5,285 | 83 / 361,704 | 2 / 3,378 | 129 | 86 / 365,211 | 172 | 2 |
| S1-512, C'=27,163 | 425 / 1,858,984 | 7 / 17,203 | 329 | 433 / 1,876,516 | 866 | 4 |
| 100 GiB retained-density, C'=5,410,817 | 84,545 / 370,302,816 | 1,343 / 3,473,627 | 889 | 85,889 / 373,777,332 | 171,778 | 674 |

The table is an invalidated-suffix ceiling, not a claim that every insertion
always rewrites the whole mapping. It also gives the minimum canonical
identity-hash work once; the current path may hash the bytes again on put.
Actual SQLite rows, BLOB/page/WAL bytes, dedup reuse, and physical database
growth must be measured. LayerFS currently has no GC, so unreachable old
chunks/leaves/branches accumulate over edit history even though one snapshot's
canonical overhead is small. Count-changing edits are **not** path-local in
this selected profile; no receipt changes that fact.

Directory COW has a separate variable-width equation. For `E=100,000`
maximum-name entries and a selected complete-page ceiling B:

```text
entries_per_page(B) = floor((B - 13) / 292)
P(B) = ceil(100,000 / entries_per_page(B))
max_index(B) = 32 + 293*P(B)
same_size_replace(B) <= B + max_index(B) + 89
```

The final term is the directory wrapper; unchanged metadata and the replaced
child's own mapping are excluded. The simple candidates are:

| Complete page ceiling B | max pages | max index | max mapping objects | same-size one-child directory rewrite |
|---:|---:|---:|---:|---:|
| 65,536 (64 KiB) | 447 | 131,003 | 450 | 196,628 bytes / 3 objects |
| **262,144 (DIR256K selected fallback)** | **112** | **32,848** | **115** | **295,081 bytes / 3 objects** |
| 1,048,576 (1 MiB) | 28 | 8,236 | 31 | 1,056,901 bytes / 3 objects |

For the selected profile, a worst-case count-changing insertion may repack the entire
suffix. This is the accepted fast-lane policy limitation, not a logarithmic or
path-local operation.
The complete maximum-name directory mapping ceiling is
`100,000*292 + 112*13 + 32,848 + 89 + 28 = 29,234,421` canonical bytes and
115 mapping objects: the terms are entry bodies, one Phase 1 envelope per
page, index, wrapper, and metadata. Child mappings are excluded. Thus 256 KiB
removes 335 maximum mapping objects versus 64 KiB while keeping a same-size
replacement under 300,000 canonical bytes; it is the selected credibility
tradeoff, not a measured optimum.

The original contract required a wide-directory A/B across these three
ceilings. Its promotion-bearing custody is now unavailable, so the prospective
fast lane retains DIR256K through the explicit unavailable-evidence fallback
rather than calling it a measured winner. After promotion, a capacity change
requires a new mapping version. No adaptive or history-dependent tree is added
to the selected format.

### 12.6 Future remote round trips

No remote backend is implemented or used to select K/F. The minimal object
request model is still explicit:

| Operation | Candidate requests/stages |
|---|---|
| fresh file mapping | O(C) immutable PUTs; all known object bytes/IDs may upload concurrently or in bounded batches; visible-head CAS is the final dependent request |
| full scrub | root, all branches/leaves, and every chunk occurrence; level requests may be parallel/batched within Q |
| cold one-leaf range with valid receipt | authoritative head/receipt, then root, h branch GETs, one leaf, and selected chunk GETs; dependent path depth is h+3 after the head |
| warm authenticated root/path prefix | only missing path objects and selected chunks; every returned object still fully hashes/decodes |
| same-count edit | changed/new chunks, one leaf, h branches, root, transition/receipt, then head CAS |
| early count-changing edit | changed suffix leaves/branches/root under section 12.5, then transition/receipt and head CAS |

Branch and leaf IDs are computable locally, so immutable PUT dependency is an
implementation choice; the format requires only that final publication wait
for the validated closure. Range latency depends on RTT, batching/prefetch,
bandwidth, and cache state. The receipt removes no object-authentication work
for bytes actually returned and supplies no adversarial cross-reopen guarantee
beyond section 9.5.

### 12.7 Throughput conclusion

The retained older SQLite experiment reported 459.173 ms for its historical
100-MiB R-SQL row (about 217.8 MiB/s), but it used an older schema and does not
qualify the selected format. It establishes plausibility, not success. K64/F64 and
the 256-KiB directory page are the strongest evidence-backed starting choices,
not global optima.

The earlier 216-row multi-profile campaign is a terminal `NO-GO` under its
original contract, and its deleted approximately 65-GiB evidence root no
longer has promotion-bearing custody. The prospective amendment does not
relabel that campaign. Before the selected profile became final v1
compatibility authority, WP4-M instead ran the compact K64/F64 validation
specified in `../wp4m/fixed-radix-fast-lane-amendment.md`: full-write arms at
deterministic
1-MiB, 10-MiB, and retained 100-MiB plus same-count middle, forced `+1` early,
and forced `+1` middle edit arms only at 100 MiB. One warmup and three measured
invocations across those six arms produce 24 capture invocations, plus one
separately labeled nonmedian full-write complete-roundtrip check per size, for
27 total invocations.

The 24-row schedule measures full-write or edit publication work only.
Post-boundary semantic checks do not enter those medians. The 1/10-MiB arms are
write/roundtrip scaling smokes only: CP-0003 proves its 10-MiB middle workflow
changes 531 references to 530, so no edit-classification claim is made there.
CP-0004 remains the prior workflow baseline. The three
complete-roundtrip checks cover close/fresh reopen, closure authentication,
full streamed reconstruction, fingerprints, and prefix/middle/EOF ranges
outside the medians. The package records wall/CPU/RSS, Q/W/D, objects,
identity-hash operations/bytes, auth bytes, SQL statements/rows,
BLOB/page/WAL/physical DB bytes, created/reused/unreachable bytes, index height,
and remote-equivalent stages. Source generation is outside all timers. The
external package wall spans the first dispatch through the 27th return and must
not exceed 120 seconds; build and fixture preflight occur before that wall.

Profile promotion requires all identity, range, reopen, receipt-adversary,
atomicity, and constant-live-memory gates. The 100-MiB 500.000-ms minimum and
333.333-ms stretch boundaries are reported as internal credibility diagnostics
during WP4-M; they are not a pre-optimization WP4-P blocker. The controlling
200/300-MiB/s product gate applies to the stable promoted profile after the
measured WP10-WP12 optimizations and the WP14 full campaign. A profile cannot
win by omitting work merely because these diagnostics are nonblocking.

K64/F64 is selected for the production profile; the compact evidence validates
that policy input rather than ranking challengers. Every scheduled arm's topology and
rewrite counter must agree exactly with the fixed-radix equations for the
authenticated CDC stream. DIR256K is retained as
`Unavailable(custody_lost)`, not as a measured winner. WP4-P deleted every
losing constant, selector, and fixture, froze the selected-only goldens, and
passed the final audits.

The forced +1-reference 5% ratio is a mandatory local diagnostic, not a
rejection gate. WP4-M reports it, measured 1-to-10-to-100-MiB full-write
scaling, and the exact 100-GiB retained-density projections. The declared
analytical middle bound is at most 2,705,409 rewritten references, 42,273 leaves, 673 branches,
42,947 mapping objects, and 186,891,342 canonical mapping bytes; the equations
must reproduce those values exactly. The separate known early worst case is
5,410,817 references, 84,545 leaves, 1,343 branches, 85,889 mapping objects,
and 373,777,332 bytes. It remains diagnostic and is not substituted for the
declared middle bound. Both are suffix-linear work models, never wall-time or
logarithmic claims.

Initial capture/full reconstruction may scale with source bytes. With a valid
section 9.5 receipt, reopen/range/same-count edit must scale with the bounded
index/change path, not total file bytes. Fixed radix makes no such claim
for count-changing edits. No 100-GiB local run is required: routine scaling
checks the per-byte/per-object model, while section 12.1 remains an analytical
capacity/cost projection with stated uncertainty. A 512-MiB run is optional
occasional scale evidence and cannot replace a routine row.

WP4-C, WP4-M, and WP4-P are complete; WP4 is complete and WP5 is
eligible/pending. Overall Phase 4 is not complete.
The executable dependency order is:

1. `WP4-M` runs the prospective 24-invocation K64/F64 capture schedule plus
   three nonmedian complete-roundtrip checks within 120 seconds and records
   DIR256K comparative evidence as `Unavailable(custody_lost)`.
2. The compact rows remain explicitly non-qualifying for the 200/300-MiB/s
   product target and cannot be relabeled.
3. WP4-P deleted every losing profile and private selector, froze the
   production profile ID, and made K64/F64 + DIR256K the only live format.
4. WP4-P regenerated the independent final goldens and their fingerprints and
   passed both independent read-only audits; WP4 is complete.
5. WP5 is eligible/pending and reruns its frozen-format exit check against the
   single compatibility-promoted profile before later production work.

The selected profile gained compatibility authority only after step 4. WP4-M
remains a disposable measurement lane, not a public feature flag, provider
abstraction, format-negotiation API, or permanent multi-profile production
surface.

## 13. Rejection-by-rejection audit

Categories are exactly those requested:

1. explicitly prohibited by a controlling Phase 1/2/3/4 contract;
2. demonstrably semantically incorrect;
3. demonstrably violates bounded memory, authentication, atomicity, or typed
   error requirements;
4. rejected by retained measurement or a concrete byte/work equation; and
5. only an unproven preference, anticipated remote concern, or complexity
   judgment.

Category 5 is never, by itself, a categorical invalidity claim.

| Alternative or claimed blocker | Verdict | Category and exact basis |
|---|---|---|
| WP4 lacks authority to reject over-limit Phase 3 values | **retract** | The mapping contract expressly assigns maximum object/page/reference/depth/allocation bounds (`...SPEC.md:229-267`; implementation plan `:241-306`). Durable admission is not Phase 3 semantics. |
| flat file manifest is structurally invalid for small files | **retract** | `100,000*68 = 6,800,000 < 8,388,608`; it is a valid bounded small-file encoding when fetched in full. |
| select one flat/giant manifest for scalable files | **uphold rejection** | 3/4: a 100-GiB file needs 3,276,800–13,107,200 refs; one manifest exceeds field/direct-ref bounds and makes range/edit authentication O(C). |
| exact K64/F64 is proven best durably | **retract** | 5: no retained canonical/SQLite K64/F64 versus K59/F101/K256/F256 A/B exists. It is the evidence-backed starting candidate, not a universal optimum. |
| K59/F101 or K256/F256 is invalid | **retract/defer** | 5: both fit structural bounds; section 12.7 measures the smallest useful set before format promotion. |
| a bounded file branch level is unnecessary | **uphold rejection** | 3/4: the retained-density 100-GiB model has 84,544 leaves, above candidate root F64; K64/F64 needs exact branch counts 1,321 and 21. Only the needed levels are emitted. |
| adaptive/history-dependent page sizes | **uphold rejection for the candidate** | 2: without a history-independent normalization, equal logical Vecs can encode differently. A content-defined canonical splitter is reopened as a future alternative but is unmeasured category 5. |
| file's raw `ChunkId`, length, or canonical object ID may be omitted | **uphold** | 1/2: the mapping contract requires all three (`...SPEC.md:217-223,237-238`). Raw ID preserves `LogicalFile` identity without a payload fetch, length routes ranges, canonical ID locates/authenticates the object. |
| reuse file capacity 64 for directory/delta entries | **retract** | 5: file evidence is fixed-width only. The candidate uses a 256-KiB directory byte ceiling and the existing delta Bytes-field/reference caps with greedy canonical packing. |
| directory paging can be omitted for every admitted directory | **uphold** | 3/4: worst case is `13 + 100,000*292 = 29,200,013 > 16,777,216`; the candidate's 262,144-byte page cap admits 897 maximum-name entries and at most 112 pages. |
| greedy-to-16-MiB directory pages are proven best | **retract** | 5: the structural limit does not select COW locality. Section 12.5 compares 64 KiB/256 KiB/1 MiB; the candidate selects 256 KiB from the exact 115-object complete-mapping ceiling and 295,081-byte/3-object same-size-rewrite ceiling, not a measurement claim. |
| delta paging can be omitted for every admitted delta | **uphold** | 3/4: `15 + 100,000*4,173 = 417,300,015 > 8,388,608`; at least 2,010 worst entries fit and at most 50 pages are needed. |
| recursively embed every Add/Replace tree in one delta record | **uphold** | 3/4: a valid 100,000-child tree or 100,000 worst-path entries exceeds the 8/16 MiB object limits. A separately paged recursive format is semantically possible but category 5 complexity; direct durable NodeId edges are the smallest viable composition. |
| sort or deduplicate delta entries/paths | **uphold** | 2: `Delta::new` preserves any Vec order/duplicates and `apply` executes sequentially (`delta/mod.rs:51-103`); Add→Remove and repeated Metadata can be meaningful. |
| derive `RootId` from parent/publication data | **uphold** | 2/4: equal root content reached from different parents would get different identities; current `root_id`-keyed parent storage demonstrably conflicts (`engine/src/lib.rs:737-746,1123-1168`). |
| add a second root envelope/identity | **uphold** | 1/4: Phase 2 says not to add a second root identity (`../../phase-2/handoff.md:145`); the directory wrapper already provides a canonical authenticated ID. |
| add a new `ObjectKind` | **uphold for the candidate** | 1/4: Phase 4 requires existing kinds absent proof (`...SPEC.md:248-252`); the exact mapping fits both existing object limits, so no proof exists. |
| use unauthenticated remote range bytes as object truth | **uphold** | 3: `ObjectId` hashes the complete canonical object; a bare slice cannot establish that hash. Fetch the small whole object or use a separately authenticated immutable receipt/capability. |
| remote latency proves a particular K/F invalid | **retract** | 5: RTT/bandwidth/batching/cache state changes the result. Section 12.6 records the request model without speculative selection. |
| a publication receipt authenticates bytes fetched later or detects arbitrary SQLite-file corruption | **uphold rejection** | 3: ObjectId covers complete bytes, and a logical epoch cannot detect out-of-band deletion/replacement/rollback. Section 9.5 requires per-fetch authentication and full scrub when authority is insufficient. |
| require a fixed canonical global unique-object/edge/byte/work tuple | **retract** | 5: structural limits plus derived depth and live-Q bounds close memory. W/D are checked u64 telemetry; caller work/storage limits are noncanonical policy. |
| use Phase 1 decode nesting limit 8 as graph depth | **uphold rejection** | 1/3: that limit is parser nesting, while WP4 assigns graph depth; the exact candidate graph derives a 781-edge maximum. |
| use 256 as durable logical depth and reject deeper direct trees | **uphold admission rule** | 1/3/4: existing `MAX_PATH_COMPONENTS=256`, bounded range/tree traversal requirements, and `3*256` graph equation. The typed rejection does not redefine admitted values' identity. |
| reject repeated object IDs or repeated delta paths as duplicates | **retract** | 2: shared sub-DAGs, repeated chunks, zero-length chunks, and sequential repeated delta paths are valid. Only directory-name and page-slot uniqueness is required. |
| the current engine root/delta records already freeze the durable mapping | **uphold rejection** | 1/2: current delta payload is opaque and identity ad hoc (`engine/src/lib.rs:190-203`); root parent is stored as intrinsic content despite convergence. |

What changed after the objection:

- flat file manifests were reopened as valid rather than rejected on bounds or
  remote authentication;
- the missing-authority/global-closure stop was removed;
- no global canonical work tuple was invented;
- K64/F64 was confined to files and described as an evidence-backed candidate,
  not a proven durable optimum;
- directory pages now use the explicit 256-KiB candidate ceiling while delta pages
  use the existing Bytes-field/reference caps; and
- the total-file 100,000 and cumulative 8-GiB ceilings were removed; a minimal
  checked-u64 radix and live-memory Q model now analytically cover 100 GiB; and
- a snapshot receipt is limited to publication attestation and lazy path
  validation under an explicit store-integrity authority, never fetched-byte
  authentication.

The original missing-authority stop is retracted. CP-0006 completed WP4-M.
WP4-P selected-only implementation, loser/selector deletion, production profile
ID, goldens, tests, and both independent audits pass. One K64/F64 + DIR256K
profile is compatibility-promoted; no prolly alternative or multi-profile
production surface is authorized.

## 14. Independent golden vectors

### 14.0 Normative selected v1 corpus

WP4-P implementation now has one normative selected K64/F64 + DIR256K corpus:

```text
corpus: crates/layerfs-core/tests/phase4_selected_goldens.rs
manifest: implementation-detail/phase-4/wp4p/selected-goldens-v1.tsv
production profile ID:
  b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1
manifest SHA-256:
  6de8c75299f09148046fe2a17c0162c64a40503e1b20c4b9090ab97e709a7330
golden test SHA-256:
  727fe6683eb1d85860e34d1cf5d709c1d4b323545437f43dfbe75e394c549701
```

The independent packer/hasher covers selected file topology at 1, 64, 65,
4,096, and 4,097 references; DIR256K 1/897/898-entry page/index/wrapper cases;
delta genesis, all four operation forms, and the exact 2,010-entry maximum
delta-page case; the production-profile receipt; and representative exact
malformed results. Two selected-golden tests pass; the third test is an
intentionally ignored manifest printer. Core 44, benchmark 54, parity 14, the
full all-target/all-feature workspace suite, and clippy `-D warnings` pass.

Implementation verification and both independent correctness/performance
audits pass after the 2,010-entry corpus fix. No performance campaign was
rerun: CP-0006 remains controlling, `qualification=false`, and
`promotion=false`. WP4-P is COMPLETE / PASS and the selected v1 profile is now
compatibility-promoted.

The material retained in sections 14.1-14.9 below is a **withdrawn prior-draft
research trace**, not the normative corpus and not input to WP5. It remains only
as historical evidence.

### 14.1 Method and notation

These withdrawn vectors were generated for the preceding single-index draft by a standalone
Python standard-library byte packer that did not call the LayerFS encoder. Its
hex output was hashed by a separate one-shot Rust program linked only to the
already-vendored `blake3` crate and implementing
`BLAKE3("layerfs/object\0" || bytes)`. The packer and hasher agreed with the
existing frozen Phase 1 domain vectors. No repository or production source was
used as a mapping codec.

Every `bytes` value below is the complete Phase 1 canonical object, not only
the inner mapping record. Hex is lowercase without separators. Reconstructed
logical values are stated next to each vector.

### 14.2 Chunk identity domain

```text
raw payload: empty
raw ChunkId:
3eef36118502a4bbe93de43ed0445eeb814b6781bcccf38cd5a9aae36bc58f63
canonical Bytes bytes (13):
4c46534f010000000400000000
canonical Bytes ObjectId:
67382239add877c9e66519ae3d25a6cc1ea45973992ad896f78e3c20498194f3

raw payload: 616263 ("abc")
raw ChunkId:
b8b33f1120d8f8739a1bb786d13aa42a324e280e659c8888868c0de3edd2be0a
canonical Bytes bytes (16):
4c46534f010000000700000003616263
canonical Bytes ObjectId:
43bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b

raw payload: 78797a ("xyz")
raw ChunkId:
07e8f48117c37414091576c9618477ed6ef468e4aab26c33e5216f76e51186ab
canonical Bytes bytes (16):
4c46534f01000000070000000378797a
canonical Bytes ObjectId:
635ca21a3dc20a7bb334abc393a6bf9a5d60c49062c8846ff39e3d6e5f6be7a7
```

These freeze the distinction between raw logical chunk identity and canonical
storage-object identity, including the zero-length case.

### 14.3 Files

Empty file, mode 0, zero length, zero references/pages:

```text
bytes (44):
4c46534f01000000230000001f4c4653344d4150000001010000000000000000000000000000000000000000
NodeId:
35cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb
```

One `abc` reference page:

```text
bytes (96):
4c46534f0100000057000000534c4653344d41500000010200000001b8b33f1120d8f8739a1bb786d13aa42a324e280e659c8888868c0de3edd2be0a0000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b
ObjectId:
8df24aac88f08a5f5091d7bbdd967a1893eec66c8617fd71ddf3c67c0fa16807
```

One-chunk file, mode 0, length 3, referencing that page:

```text
bytes (84):
4c46534f010000004b000000474c4653344d415000000101000000000000000000000003000000010000000100000000000000038df24aac88f08a5f5091d7bbdd967a1893eec66c8617fd71ddf3c67c0fa16807
NodeId:
f096e84478a3bbff5949a163e3862a32cda66406bd5e7f47ac7cf2eeb6cb1a25
```

Multi-reference page in semantic order `abc`, empty, `xyz`. Raw IDs and
canonical object IDs are intentionally distinct; lengths are 3, 0, 3:

```text
bytes (232):
4c46534f01000000df000000db4c4653344d41500000010200000003b8b33f1120d8f8739a1bb786d13aa42a324e280e659c8888868c0de3edd2be0a0000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b3eef36118502a4bbe93de43ed0445eeb814b6781bcccf38cd5a9aae36bc58f630000000067382239add877c9e66519ae3d25a6cc1ea45973992ad896f78e3c20498194f307e8f48117c37414091576c9618477ed6ef468e4aab26c33e5216f76e51186ab00000003635ca21a3dc20a7bb334abc393a6bf9a5d60c49062c8846ff39e3d6e5f6be7a7
ObjectId:
39eb02dee4ae3d7dfd7f2904e44ec9f3f936118cede7c7871afd8fd821529ed2
```

Corresponding mode-0 file, total length 6, three references:

```text
bytes (84):
4c46534f010000004b000000474c4653344d4150000001010000000000000000000000060000000300000001000000000000000639eb02dee4ae3d7dfd7f2904e44ec9f3f936118cede7c7871afd8fd821529ed2
NodeId:
5067e53c1d53b0869a831b06138d2b05918b87101e3fe09465141a2e783be8a0
```

Maximum metadata boundary, empty file with mode `u32::MAX`:

```text
bytes (44):
4c46534f01000000230000001f4c4653344d415000000101ffffffff00000000000000000000000000000000
NodeId:
1c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c21
```

Maximum valid reference page, 64 repetitions of the exact 68-byte `abc`
reference:

```text
prefix (28 bytes):
4c46534f01000011130000110f4c4653344d41500000010200000040
repeat exactly 64 times:
b8b33f1120d8f8739a1bb786d13aa42a324e280e659c8888868c0de3edd2be0a0000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b
expanded length: 4,380
ObjectId:
9d20e9db6efd18ef0113d248593ac7ea95c26bd85955b4339cadaca17568a64d
```

The corresponding 64-reference, length-192 file index is:

```text
bytes (84):
4c46534f010000004b000000474c4653344d4150000001010000000000000000000000c0000000400000000100000000000000c09d20e9db6efd18ef0113d248593ac7ea95c26bd85955b4339cadaca17568a64d
NodeId:
6b5a7e4e46368da556e122bf360b9c7ae438e8bc4fb9f66f622623d94974dd91
```

The prefix plus the exact repeat count is an exact byte grammar, not an
abbreviated logical value; concatenation reproduces the recorded length/hash.

Maximum valid zero-length page, 64 repetitions of the exact 68-byte empty
reference:

```text
prefix (28 bytes):
4c46534f01000011130000110f4c4653344d41500000010200000040
repeat exactly 64 times:
3eef36118502a4bbe93de43ed0445eeb814b6781bcccf38cd5a9aae36bc58f630000000067382239add877c9e66519ae3d25a6cc1ea45973992ad896f78e3c20498194f3
expanded length: 4,380
ObjectId:
e4a59513816235f5d6b67344dbf24608216fbe6922490aa6b23c9d8d24eb73f6
```

The exact equal-end routing file contains two occurrences of that zero page
followed by the one-`abc` page. It has 129 references, length 3, and cumulative
ends `0, 0, 3`:

```text
bytes (164):
4c46534f010000009b000000974c4653344d41500000010100000000000000000000000300000081000000030000000000000000e4a59513816235f5d6b67344dbf24608216fbe6922490aa6b23c9d8d24eb73f60000000000000000e4a59513816235f5d6b67344dbf24608216fbe6922490aa6b23c9d8d24eb73f600000000000000038df24aac88f08a5f5091d7bbdd967a1893eec66c8617fd71ddf3c67c0fa16807
NodeId:
fd3269b6fd29dba8c8b4f8c7d2a326277b66e7c63ae46453980f7a517c5fb30a
```

With a valid receipt, `0..0` and `3..3` authenticate only the index and return
empty; `0..3` uses strict upper-bound routing, skips both equal zero ends,
fetches only the `abc` page/chunk, and returns `616263`. Without a receipt, each
request first authenticates the index, all three page occurrences, and all
chunk-reference occurrences before routing. Full reconstruction likewise loads
both zero-page occurrences and reconstructs exactly 128 zero-length references
followed by `abc`.

One-`xyz` page, used to freeze a nonempty cross-page range:

```text
bytes (96):
4c46534f0100000057000000534c4653344d4150000001020000000107e8f48117c37414091576c9618477ed6ef468e4aab26c33e5216f76e51186ab00000003635ca21a3dc20a7bb334abc393a6bf9a5d60c49062c8846ff39e3d6e5f6be7a7
ObjectId:
142ba15a8b752eb4b53737eee49b6d00dd7c31249d8766a285f2abcf5224f38f
```

The cross-page file has the 64-`abc` page followed by that `xyz` page, 65
references, total length 195, and cumulative ends `192, 195`:

```text
bytes (124):
4c46534f01000000730000006f4c4653344d4150000001010000000000000000000000c3000000410000000200000000000000c09d20e9db6efd18ef0113d248593ac7ea95c26bd85955b4339cadaca17568a64d00000000000000c3142ba15a8b752eb4b53737eee49b6d00dd7c31249d8766a285f2abcf5224f38f
NodeId:
3c892fba2a316bfcd97e0cd31bd545f591a7a851e478c3bb335b3d7df5019694
```

With a valid receipt, range `191..194` authenticates both pages and the `abc`
and `xyz` chunk objects, returning exact bytes `637879` (`"cxy"`). Range
`192..195` upper-bounds directly to the second page and returns `78797a`.
Without a receipt, either request first authenticates both pages and all 65
chunk-reference occurrences before applying the same routing result.

The same three references reordered as `xyz`, empty, `abc` are a distinct
valid logical file, not an alternate encoding of the earlier Vec:

```text
page bytes (232):
4c46534f01000000df000000db4c4653344d4150000001020000000307e8f48117c37414091576c9618477ed6ef468e4aab26c33e5216f76e51186ab00000003635ca21a3dc20a7bb334abc393a6bf9a5d60c49062c8846ff39e3d6e5f6be7a73eef36118502a4bbe93de43ed0445eeb814b6781bcccf38cd5a9aae36bc58f630000000067382239add877c9e66519ae3d25a6cc1ea45973992ad896f78e3c20498194f3b8b33f1120d8f8739a1bb786d13aa42a324e280e659c8888868c0de3edd2be0a0000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b
page ObjectId:
2e2f541ac7e1e0870f9e75c63123889fc607b41f81a3801692deaf94ea7217e1
file bytes (84):
4c46534f010000004b000000474c4653344d415000000101000000000000000000000006000000030000000100000000000000062e2f541ac7e1e0870f9e75c63123889fc607b41f81a3801692deaf94ea7217e1
NodeId:
bb34ff658e3c5df8ed18ef86e0933be9b99f525fd0d975332dc5a2020cd7a8c8
```

### 14.4 Directories, metadata, and roots

Mode-0 directory metadata:

```text
bytes (28):
4c46534f01000000130000000f4c4653344d41500000010400000000
ObjectId:
719fcfe2709f78d26558c2ce25760019981d46eccfe5c82055bdede5e1f936c3
```

Empty directory index:

```text
bytes (32):
4c46534f0100000017000000134c4653344d4150000001030000000000000000
ObjectId:
c6024f7331689b9ac6e22d738b13a0f92d672b87310d877262bf5094bb6d36b6
```

Empty directory wrapper and `RootId`:

```text
bytes (89):
4c46534f020000005000000002000000016d01719fcfe2709f78d26558c2ce25760019981d46eccfe5c82055bdede5e1f936c3000000017401c6024f7331689b9ac6e22d738b13a0f92d672b87310d877262bf5094bb6d36b6
NodeId = RootId:
d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d
```

Mode-`u32::MAX` directory metadata:

```text
bytes (28):
4c46534f01000000130000000f4c4653344d415000000104ffffffff
ObjectId:
342d04626e9e6f7437775ef85d73ecce7d8e666701533d13db0e0f8565501e1b
```

Nested deterministic page with `a` -> maximum-mode empty file and `z` ->
empty directory, proving file/directory child kinds and name order:

```text
bytes (89):
4c46534f0200000050000000020000000161011c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c21000000017a02d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d
ObjectId:
f5a22077c05163281e41c9aede41a72f970f9718515fb5d1d6c942ad976a6543
```

Its directory index:

```text
bytes (71):
4c46534f010000003e0000003a4c4653344d415000000103000000020000000100000002000161f5a22077c05163281e41c9aede41a72f970f9718515fb5d1d6c942ad976a6543
ObjectId:
b768c82a27c46bf6bca2d9f54fa4490317f79aa61bf72300e0e80f873b9a1153
```

Its wrapper/root, using maximum metadata:

```text
bytes (89):
4c46534f020000005000000002000000016d01342d04626e9e6f7437775ef85d73ecce7d8e666701533d13db0e0f8565501e1b000000017401b768c82a27c46bf6bca2d9f54fa4490317f79aa61bf72300e0e80f873b9a1153
NodeId = RootId:
73f7177e91f5ac0945ee2395cd500959645b6239796289972e0d9e6b889aa1e9
```

The empty wrapper's `RootId` remains
`d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d`
whether publication has no parent or any parent. Parent is demonstrated only
in the genesis/non-genesis delta vectors below.

### 14.5 Supporting roots for delta vectors

Empty directory with mode 1:

```text
metadata bytes (28):
4c46534f01000000130000000f4c4653344d41500000010400000001
metadata ObjectId:
3527ccdd9663db65cd7ad8602e5bfdffbd93673f50b7bad28c54c78fa74eaacf
wrapper bytes (89):
4c46534f020000005000000002000000016d013527ccdd9663db65cd7ad8602e5bfdffbd93673f50b7bad28c54c78fa74eaacf000000017401c6024f7331689b9ac6e22d738b13a0f92d672b87310d877262bf5094bb6d36b6
RootId:
bb255d7827828e46faad124eb33893c2f0f6ea306112e433f9bfd0e1d289b901
```

Root containing `a` -> mode-0 empty file:

```text
page bytes (51):
4c46534f020000002a0000000100000001610135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb
page ObjectId:
12c09d9475acb683018bbc3b0975eb1b72d925c0b8ca08a365c02fdd8bbfac96
index bytes (71):
4c46534f010000003e0000003a4c4653344d41500000010300000001000000010000000100016112c09d9475acb683018bbc3b0975eb1b72d925c0b8ca08a365c02fdd8bbfac96
index ObjectId:
094430f91cda3ab16f64e6eb2d947ca340e78fb8a77c3fd99a88d8f2d21205ae
wrapper bytes (89):
4c46534f020000005000000002000000016d01719fcfe2709f78d26558c2ce25760019981d46eccfe5c82055bdede5e1f936c3000000017401094430f91cda3ab16f64e6eb2d947ca340e78fb8a77c3fd99a88d8f2d21205ae
RootId:
f6fe67d1a61497d96923eae5f012b6b717beeb7672fffc9cc2ea49785ac4f55d
```

Root containing `a` -> maximum-mode empty file:

```text
page bytes (51):
4c46534f020000002a000000010000000161011c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c21
page ObjectId:
3e332f04780ae4c5c9627860dd1bcf6935a63b7f1c0a40f0493ace764783c41f
index bytes (71):
4c46534f010000003e0000003a4c4653344d4150000001030000000100000001000000010001613e332f04780ae4c5c9627860dd1bcf6935a63b7f1c0a40f0493ace764783c41f
index ObjectId:
6beb572f470efffb2344e9645f625b69dd36d4a35b76d3980b5edf9532d5743a
wrapper bytes (89):
4c46534f020000005000000002000000016d01719fcfe2709f78d26558c2ce25760019981d46eccfe5c82055bdede5e1f936c30000000174016beb572f470efffb2344e9645f625b69dd36d4a35b76d3980b5edf9532d5743a
RootId:
91a3e921aa2e85287f5b76b31022d7c78127a2ba8a0eb35e1aad366e3eea943a
```

Root containing ordered children `a` -> mode-0 empty file and `b` ->
maximum-mode empty file:

```text
page bytes (89):
4c46534f02000000500000000200000001610135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb0000000162011c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c21
page ObjectId:
d9adba6fe32ad6c53249a40d7173f295d7a64fd128c1d10b8746c24bd0fe3794
index bytes (71):
4c46534f010000003e0000003a4c4653344d415000000103000000020000000100000002000161d9adba6fe32ad6c53249a40d7173f295d7a64fd128c1d10b8746c24bd0fe3794
index ObjectId:
33f099854dad597fc91c0e0ed0655e9241b6b0f95a81ba8430cff39f364361de
wrapper bytes (89):
4c46534f020000005000000002000000016d01719fcfe2709f78d26558c2ce25760019981d46eccfe5c82055bdede5e1f936c300000001740133f099854dad597fc91c0e0ed0655e9241b6b0f95a81ba8430cff39f364361de
RootId:
34827b2df2660eaf9205ce123dfc94039fe2362b7bfb9ce523fd78093a6daff8
```

### 14.6 Genesis and every delta operation

Genesis publication of the empty root, with no parent and no entries/pages:

```text
bytes (65):
4c46534f0100000038000000344c4653344d41500000010500d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d0000000000000000
DeltaId:
e3a9f48972bb20fba39d88552ae7370e0fb0d2651b7a83d5757ace28933e0516
```

Add path `a`, adding the mode-0 empty file; parent empty root, child Add root:

```text
page bytes (66):
4c46534f0100000039000000354c4653344d4150000001060000000101000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb
page ObjectId:
02c4236859d7a1f93147f5955e49d5ab566f7af30f90bab07560af4e1f601e01
index bytes (129):
4c46534f0100000078000000744c4653344d41500000010501d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360df6fe67d1a61497d96923eae5f012b6b717beeb7672fffc9cc2ea49785ac4f55d000000010000000102c4236859d7a1f93147f5955e49d5ab566f7af30f90bab07560af4e1f601e01
DeltaId:
fa51fba627791d803e39027e821c1b8cf1c5ffd6ed687ed1d4e3ec00bffebc19
```

Remove path `a`, requiring the mode-0 empty-file ID; parent Add root, child
empty root:

```text
page bytes (66):
4c46534f0100000039000000354c4653344d4150000001060000000102000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb
page ObjectId:
6ca5b830827124194c47afc8c89100a4a365bb773a2c7cbf3d1302e45b970b41
index bytes (129):
4c46534f0100000078000000744c4653344d41500000010501f6fe67d1a61497d96923eae5f012b6b717beeb7672fffc9cc2ea49785ac4f55dd34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d00000001000000016ca5b830827124194c47afc8c89100a4a365bb773a2c7cbf3d1302e45b970b41
DeltaId:
aa1d788424fa89e7f3db015b1fd872c5d5e601f9232c15bb62fa7721cb440027
```

Replace path `a`, before mode-0 empty file and after maximum-mode empty file;
parent Add root, child Replace root:

```text
page bytes (98):
4c46534f0100000059000000554c4653344d4150000001060000000103000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb1c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c21
page ObjectId:
cb9efdca5fdea042245848eede7a21bd1b8019d28fe10236b3814a3444437045
index bytes (129):
4c46534f0100000078000000744c4653344d41500000010501f6fe67d1a61497d96923eae5f012b6b717beeb7672fffc9cc2ea49785ac4f55d91a3e921aa2e85287f5b76b31022d7c78127a2ba8a0eb35e1aad366e3eea943a0000000100000001cb9efdca5fdea042245848eede7a21bd1b8019d28fe10236b3814a3444437045
DeltaId:
5096dcb1f934e72d67d29027f19764b1e7acfe8aefad035696d7d00ea58b8f77
```

Metadata at the empty root path, before mode 0 and after mode 1; parent empty
root, child mode-1 empty root:

```text
page bytes (105):
4c46534f01000000600000005c4c4653344d415000000106000000010400000000d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d00000000bb255d7827828e46faad124eb33893c2f0f6ea306112e433f9bfd0e1d289b90100000001
page ObjectId:
c17e478c5184e1bfee59de5f8e96db58512ff6b0819430115c5f2d5120430037
index bytes (129):
4c46534f0100000078000000744c4653344d41500000010501d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360dbb255d7827828e46faad124eb33893c2f0f6ea306112e433f9bfd0e1d289b9010000000100000001c17e478c5184e1bfee59de5f8e96db58512ff6b0819430115c5f2d5120430037
DeltaId:
7f193528536ff488dca72ddd3f443bd7f90958642f123a2c408b099de34af036
```

The Metadata page above contains accepted operation tag `04` at canonical byte
offset 28; its successful authenticated page ID is
`c17e478c5184e1bfee59de5f8e96db58512ff6b0819430115c5f2d5120430037`.

Repeated path `a`, first Add mode-0 empty file and then Replace it with the
maximum-mode empty file; parent empty root, child Replace root:

```text
page bytes (136):
4c46534f010000007f0000007b4c4653344d4150000001060000000201000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb03000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb1c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c21
page ObjectId:
b27b79ba70cf7b4a04ef85306e49c062ed28d64caa4eb9d7c999b86632e481ed
index bytes (129):
4c46534f0100000078000000744c4653344d41500000010501d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d91a3e921aa2e85287f5b76b31022d7c78127a2ba8a0eb35e1aad366e3eea943a0000000200000001b27b79ba70cf7b4a04ef85306e49c062ed28d64caa4eb9d7c999b86632e481ed
DeltaId:
82729c56ea61fc4337dbb73926834bbfa0b06edd5008f45b924220da417cf8ce
```

Two Adds in Vec order `a`,`b`, from the empty root to the two-child root:

```text
page bytes (104):
4c46534f010000005f0000005b4c4653344d4150000001060000000201000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb0100000001621c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c21
page ObjectId:
0ed56f8add8783c7a8f38acfc5ef3eec56d0a6e196ebe842d6ed8374241e36c8
index bytes (129):
4c46534f0100000078000000744c4653344d41500000010501d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d34827b2df2660eaf9205ce123dfc94039fe2362b7bfb9ce523fd78093a6daff800000002000000010ed56f8add8783c7a8f38acfc5ef3eec56d0a6e196ebe842d6ed8374241e36c8
DeltaId:
77c6af4b3634757bb31d83f5c7279d195d0db0a1679a8522c6b8028c635c3de5
```

The same two Adds in Vec order `b`,`a`, with the same parent and child:

```text
page bytes (104):
4c46534f010000005f0000005b4c4653344d415000000106000000020100000001621c95b4498167dc05662461ccf3fb9cd5fed05e22137820acd88ecb71972b4c2101000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb
page ObjectId:
254d1fcf0349b3d0bd7bcc9d8bb6bc16821ff949bd2294ce3ff3ca93c0b4f7d0
index bytes (129):
4c46534f0100000078000000744c4653344d41500000010501d34d3820125d73a652ca5abbfb3bb4fad4e59a2aa60d630e2e31de57989e360d34827b2df2660eaf9205ce123dfc94039fe2362b7bfb9ce523fd78093a6daff80000000200000001254d1fcf0349b3d0bd7bcc9d8bb6bc16821ff949bd2294ce3ff3ca93c0b4f7d0
DeltaId:
f5443052110b4096e213edd2000269a0f1871baf59f696bfc402ea5a5a9c3a75
```

Sequential reconstruction of these entry pages produces the recorded child
roots, which independently cross-checks the index parent/child fields.

### 14.7 Reconstructed values and golden strong edges

The successful vectors reconstruct as follows; the bracket order is the exact
strong-edge discovery order and repeated occurrences are retained:

| Vector | Reconstructed semantic value | Ordered strong edges |
|---|---|---|
| empty file | mode 0, length 0, references `[]` | `[]` |
| one-`abc` page | reference `[(raw abc, 3, canonical abc)]` | `[43bf...b94b]` |
| one-chunk file | mode 0, length 3, the one `abc` reference | `[8df2...6807]` |
| three-reference page | ordered `abc`, empty, `xyz` references | `[43bf...b94b, 6738...4f3, 635c...e7a7]` |
| corresponding file | mode 0, length 6, three references including the zero reference | `[39eb...9ed2]` |
| reordered three-reference page/file | ordered `xyz`, empty, `abc`; mode 0, length 6 | page `[635c...e7a7, 6738...4f3, 43bf...b94b]`; file `[2e2f...17e1]` |
| 64-`abc` page | 64 ordered equal `abc` references | `[43bf...b94b]` repeated 64 times |
| 64-reference file | mode 0, length 192, those 64 references | `[9d20...a64d]` |
| 64-zero page | 64 ordered zero-length references | `[6738...4f3]` repeated 64 times |
| equal-end file | mode 0, length 3, 128 zeros then `abc` | `[e4a5...73f6, e4a5...73f6, 8df2...6807]` |
| one-`xyz` page | reference `[(raw xyz, 3, canonical xyz)]` | `[635c...e7a7]` |
| cross-page file | mode 0, length 195, 64 `abc` references then `xyz` | `[9d20...a64d, 142b...f38f]` |
| empty directory wrapper/root | mode 0, children `{}` | `[719f...6c3, c602...36b6]` |
| nested entry page | `a` is the max-mode empty file; `z` is the empty directory | `[1c95...4c21, d34d...360d]` |
| nested directory index | two entries routed by first name `a` | `[f5a2...6543]` |
| nested wrapper/root | mode `u32::MAX`, the nested children above | `[342d...1e1b, b768...1153]` |
| Add-root page/index/wrapper | mode-0 root with `a` -> mode-0 empty file | page `[35cd...32eb]`; index `[12c0...ac96]`; wrapper `[719f...6c3, 0944...05ae]` |
| Replace-root page/index/wrapper | mode-0 root with `a` -> max-mode empty file | page `[1c95...4c21]`; index `[3e33...c41f]`; wrapper `[719f...6c3, 6beb...743a]` |
| two-child page/index/wrapper | mode-0 root with ordered `a` -> mode-0 empty file and `b` -> max-mode empty file | page `[35cd...32eb, 1c95...4c21]`; index `[d9ad...3794]`; wrapper `[719f...6c3, 33f0...61de]` |
| genesis transition | `Genesis { child: empty_root }`; no Phase 3 delta | `[d34d...360d]` |
| Add transition | parent empty, child Add-root, Add `a` | index `[d34d...360d, f6fe...f55d, 02c4...1e01]`; page `[35cd...32eb]` |
| Remove transition | parent Add-root, child empty, Remove `a` | index `[f6fe...f55d, d34d...360d, 6ca5...0b41]`; page `[35cd...32eb]` |
| Replace transition | parent Add-root, child Replace-root, Replace `a` | index `[f6fe...f55d, 91a3...943a, cb9e...7045]`; page `[35cd...32eb, 1c95...4c21]` |
| Metadata transition | parent empty-mode root, child mode-1 root, root mode 0 -> 1 | index `[d34d...360d, bb25...b901, c17e...0037]`; page `[d34d...360d, bb25...b901]` |
| repeated-path Add then Replace | parent empty, child Replace-root; exact Vec targets path `a` twice and replays sequentially | index `[d34d...360d, 91a3...943a, b27b...81ed]`; page `[35cd...32eb, 35cd...32eb, 1c95...4c21]` |
| ordered `a`,`b` Add transition | parent empty, child two-child root; exact Vec order `a`,`b` | index `[d34d...360d, 3482...aff8, 0ed5...36c8]`; page `[35cd...32eb, 1c95...4c21]` |
| reordered `b`,`a` Add transition | same parent and child, exact Vec order `b`,`a`, distinct `DeltaId` | index `[d34d...360d, 3482...aff8, 254d...f7d0]`; page `[1c95...4c21, 35cd...32eb]` |

Chunk objects and metadata Bytes objects have no strong edges. Empty indexes
have none. All abbreviated IDs above are unambiguous prefixes/suffixes of the
complete IDs frozen in sections 14.2-14.6; implementations compare all 32
bytes, never the abbreviation.

### 14.8 Exact failure vectors

Unless stated otherwise the bytes have a valid BLAKE3 object ID, so the listed
mapping/Phase 1 error occurs after identity authentication.

Unless a row states a multi-cause or publication result, its listed typed
cause is `first` and `dominant=Some(first)`, with empty cleanup/reconciliation
slots.
“No visible publication” forbids a visible-head/transition change but permits
authenticated unreachable immutable residue under section 10's custody rule.

The truncation corpus is table-driven but byte-exact. Define
`bytes_object(I) = "LFSO" || 0x01 || u32be(4 + len(I)) || u32be(len(I)) || I`.
For every successful Bytes mapping record in sections 14.3-14.6, form every
proper inner prefix that ends (a) immediately before a field, (b) after each
but the last byte of a fixed-width field, or (c) after each but the last byte
of a declared variable body; wrap it with `bytes_object`, recompute its ID, and
expect `UnexpectedEof`. The exhaustive serialized field lists are:

| Record | Fields after the common `magic[8], version[2], tag[1]` |
|---|---|
| file index | `mode[4], total[8], reference_count[4], page_count[4]`, then every `cumulative_end[8], page_id[32]` |
| file page | `reference_count[4]`, then every `raw_id[32], raw_length[4], canonical_id[32]` |
| directory metadata | `mode[4]` |
| directory index | `entry_count[4], page_count[4]`, then every `page_entry_count[4], first_name_length[2], first_name[length], page_id[32]` |
| delta index | `has_parent[1]`, conditional `parent_id[32]`, `child_id[32], entry_count[4], page_count[4]`, then every `page_id[32]` |
| delta page | `entry_count[4]`, then every `operation[1], path_length[4], path[length]`, followed by that operation's serialized ID/mode fields from section 8.1 |

Apply the same rule separately to each Add, Remove, Replace, and Metadata page,
so every operation-specific field is cut. For a Phase 1 Directory wrapper/page,
the fields are `magic[4], kind[1], payload_length[4], entry_count[4]`, then each
`name_length[4], name[length], child_kind[1], child_id[32]`. Raw proper prefixes
of any complete canonical object retain the original expected ID and therefore
fail `IdentityMismatch` at the store boundary; without an expected ID they fail
`UnexpectedEof`. For authenticated Directory-body truncation tests, retain the
complete nine-byte outer header, set `payload_length` to the retained payload
prefix length, recompute the ID, and expect `UnexpectedEof` from the declared
entry body. These rules cover every structural boundary without storing giant
duplicate hex fixtures.

Mapping decode begins only after complete-object reassembly and authentication,
so a distinct mapping-level fragmented-input grammar is not applicable. Every
transport partition of one exact object must reassemble to the same bytes and
ID. At capture, source fragments `616263`, `61|6263`, `6162|63`, and
`61|62|63` all produce the section 14.2 `abc` chunk and the same section 14.3
one-chunk file; source fragmentation cannot select a different durable mapping.

| Case | Complete bytes or construction | Expected first/dominant result |
|---|---|---|
| truncated one-chunk file | one-chunk file bytes from 14.3 with final byte removed; lookup still expects `f096...1a25` | `IdentityMismatch` at an authenticated store boundary; `UnexpectedEof` when the raw decoder is tested without an expected ID |
| trailing byte after empty file | `4c46534f01000000230000001f4c4653344d415000000101000000000000000000000000000000000000000000` | `TrailingBytes` |
| mapping version 2 | `4c46534f01000000230000001f4c4653344d4150000002010000000000000000000000000000000000000000` | `UnsupportedMappingVersion` |
| unknown mapping tag `ff` | `4c46534f01000000230000001f4c4653344d4150000001ff0000000000000000000000000000000000000000` | `InvalidMappingTag` |
| invalid `has_parent` discriminator | genesis bytes from 14.6 with canonical byte offset 24 changed from `00` to `02`; recomputed ID `22aba1235a3f7cd7102920a59441efde09321d502acfa835f9e139a5ccd68680` | `InvalidMappingDiscriminator` |
| invalid delta operation discriminator `00` | `4c46534f0100000039000000354c4653344d4150000001060000000100000000016135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb`, ID `94186cbbd94f370b8e8004cd3400c3319405f9a75fd062b678cc4a914b66087e` | `InvalidMappingDiscriminator`; tag `04` is accepted by the Metadata success vector |
| invalid delta operation discriminator | Add-page bytes from 14.6 with canonical byte offset 28 changed from `01` to `ff`; recomputed ID `bd1d9042714d7fbbc2af2657f8f08237e0f4c90e83550c5273cbd31902cc87d0` | `InvalidMappingDiscriminator` |
| file count 100,001 | `4c46534f01000000230000001f4c4653344d415000000101000000000000000000000000000186a100000000` | `ObjectLimitExceeded` |
| Bytes field length 8,388,609 | `4c46534f010080000500800001`, ID `a558c567455c7ff54a5431dd5be9e76a0d7908fbb40466550c395da6bdb559ba` | `ObjectLimitExceeded` before the missing body is considered |
| chunk length 32,769 | `4c46534f0100000057000000534c4653344d41500000010200000001b8b33f1120d8f8739a1bb786d13aa42a324e280e659c8888868c0de3edd2be0a0000800143bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b` | `ObjectLimitExceeded` |
| short canonical chunk ID | `4c46534f0100000056000000524c4653344d41500000010200000001b8b33f1120d8f8739a1bb786d13aa42a324e280e659c8888868c0de3edd2be0a0000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb9` | `UnexpectedEof` |
| reordered wrapper (`t`,`m`) | `4c46534f020000005000000002000000017401c6024f7331689b9ac6e22d738b13a0f92d672b87310d877262bf5094bb6d36b6000000016d01719fcfe2709f78d26558c2ce25760019981d46eccfe5c82055bdede5e1f936c3` | `NonCanonicalOrdering` |
| duplicate name `a` in one page | `4c46534f02000000500000000200000001610135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb00000001610135cd27f3e57aa16e27633fcaa1f3587526cb33ee6f57a26eeac97fc5563532eb` | `NonCanonicalOrdering` |
| duplicate name across two individually ordered pages | two authenticated pages whose boundary names are equal | `NameCollision` |
| 63-reference non-final file page | a two-page file index whose first authenticated page has 63 references | `NonCanonicalPagePartition` |
| non-greedy directory/delta page | next complete entry fits the prior page under both byte/reference caps | `NonCanonicalPagePartition` |
| wrong role | canonical file page `8df2...6807` decoded as a file index | `WrongLogicalRole` |
| absent strong page | file-index bytes `4c46534f010000004b000000474c4653344d41500000010100000000000000000000000300000001000000010000000000000003aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`, ID `3dec87389de00a0db9dee4f111158c0d11cb5d1aaa2aff2769f034c317b71c93`, with no `aa...aa` object in the store | `MissingObject(aa...aa)` |
| raw/canonical mismatch | one-reference page whose raw ID is the `xyz` raw ID but canonical ID loads `abc` | `ChunkIdentityMismatch` |
| aggregate mismatch | file index total length 4 over the one `abc` reference page | `LengthMismatch { expected: 4, actual: 3 }` |
| duplicate/repeated delta path | exact Add-then-Replace page/index from 14.6, targeting `a` twice | valid sequential delta, `DeltaId 82729c...cf8ce`; no duplicate error |
| reordered delta entries | exact `a,b` and `b,a` page/index pairs from 14.6 | both valid, same parent/child, distinct `DeltaId` values `77c6af...c3de5` and `f54430...c3a75`; neither is an alternate encoding of the other Vec |
| reordered file references | exact `xyz`, empty, `abc` page/file from 14.3 | valid distinct NodeId `bb34ff658e3c5df8ed18ef86e0933be9b99f525fd0d975332dc5a2020cd7a8c8` |
| invalid range, reversed | request `3..2` from the one-chunk file | `InvalidRange` after authenticating its index |
| invalid range, past EOF | request `0..4` from the one-chunk file | `InvalidRange` after authenticating its index |
| missing snapshot receipt or unavailable validation authority | omit the receipt, or make the required key/epoch guarantee unavailable, for one-chunk-file request `0..3` | run complete closure validation and return `616263`, or return `ValidationAuthorityUnavailable`; never authorize path-only routing |
| malformed or mismatched snapshot receipt | change any one of store ID, authority, epoch, generation, child, transition, mapping profile, or authenticator for one-chunk-file request `0..3` | return `InvalidValidationReceipt`; a caller may subsequently start a separate full scrub, whose precise result is independent and never relabels the known-bad receipt as unavailable authority |
| authoritative-head-only rollback | present a receipt/head pair older than an externally required monotonic generation while keeping the database-local receipt fields internally consistent | without external monotonic freshness authority, return `ValidationAuthorityUnavailable` for rollback-resistant use; do not claim fast reuse |
| receipt length over fixed form | append one byte to the exact 216-byte receipt | `InvalidValidationReceipt`; no variable evidence or locator transcript is accepted |
| delta supplied-parent mismatch | encode the empty-entry delta `Delta::new(P, P, [])` using a supplied handle with provisional ID `Q != P` | `DeltaParentMismatch` before any mapping object is produced |
| tree depth 257 | directly constructed root with a 257-component child chain | `MappingDepthExceeded`; no visible publication; earlier immutable residue is permitted |
| delta path depth 257 | authenticated delta entry with a 257-component path | `PathLimitExceeded` |
| active graph repeat | authenticated closure resolves an `(ObjectId, role)` already on its active stack | `MappingCycle` |
| checked arithmetic overflow | test-only checked accumulator state `u64::MAX` followed by an exact charge of 1, or K64/F64 `C=267,036,007,400,295,521` whose derived `M(C)=u64::MAX+26` | `LengthOverflow` before allocation or publication |
| live allocation budget exhausted | exact prior `q_live=1,073,741,824` followed by a one-byte allocation request | `AllocationBudgetExceeded` before allocation; streamed cumulative W/D work is unaffected |
| streamed-delivery counter overflow | test-only D `u64::MAX` followed by one output byte | `LengthOverflow` before delivering the byte; Q is unchanged |
| retained S1-512 streamed reconstruction | exact section 9.4 shape with reusable bounded input/spool/output windows | cumulative D reaches `536,870,912` while live Q stays bounded by the declared windows plus live semantic results; an eager compatibility Vec is separately charged and not promised admission |
| mapping allocation refusal | after its exact Q charge, the mapping allocator rejects a requested one-byte append | `AllocationFailed`; no visible publication |
| spool capacity failure | after its exact Q charge, the random-access traversal spool returns no-space for one 40-byte edge record | `CapacityExceeded`; no visible publication |
| spool permission failure | after its exact Q charge, the traversal spool denies a one-byte read/write before publication dispatch | `PermissionDenied`; no visible publication |
| spool short write | after its exact Q charge, a 40-byte edge-record write returns 39 bytes without a typed cause | `ShortIo`; no visible publication |
| backend short read | an authenticated-object fetch declares `N <= 16,777,216`, returns exactly `N-1` bytes, then EOF without a typed cause | `ShortIo`; no visible publication |
| transport failure before dispatch | a bounded backend object request fails without a more precise class | `Io`; no visible publication |
| deadline before dispatch | the bounded deadline expires before atomic publication dispatch | `TimedOut`; no visible publication |
| cancelled operation before dispatch | cancellation is observed before the next fetch/append/publication dispatch | `Cancelled`; no visible publication |
| first cause plus spool-cleanup failure | Q is exhausted before dispatch; ordered removal of the existing private spool then returns permission denied | `first=AllocationBudgetExceeded`, `cleanup_first=PermissionDenied`, `reconciliation=None`, `dominant=Some(AllocationBudgetExceeded)`; spool remains in engine custody; no visible publication |
| short publication acknowledgment | after compare-and-publish dispatch, an `N`-byte acknowledgment returns exactly `N-1` bytes then EOF without proving absence | classify `ShortIo`, then perform the same authoritative reconciliation matrix below |
| lost acknowledgment, requested head visible | after dispatch first returns `ShortIo`; authoritative reconciliation reads the exact requested `VisibleHead { generation, child, transition, validation_receipt }`, including byte-identical receipt, and recomputes the retained request's idempotency key | success; publication occurred; `first=ShortIo` remains diagnostic and `dominant=None` |
| lost acknowledgment, prior head remains | after dispatch first returns `TimedOut`, authoritative reconciliation reads the exact expected prior head | `first=TimedOut`, `dominant=Some(TimedOut)`; publication definitely absent |
| lost acknowledgment, different head | after dispatch first returns `Io`, authoritative reconciliation reads a different authoritative head | `first=Io`, `dominant=Some(PublicationConflict)`; requested transition is not claimed visible |
| reconciliation unavailable after cleanup failure | after dispatch first returns `TimedOut`, private-spool removal returns `PermissionDenied`, and authoritative reconciliation fails with `Io` without establishing requested/prior/different | `first=TimedOut`, `cleanup_first=PermissionDenied`, `reconciliation=Io`, `dominant=Some(AmbiguousDurability)`; visibility unknown; only the identical idempotency key may retry |

Authenticated IDs for standalone malformed vectors are included for
reproducibility:

```text
trailing byte:   9b3f2ef7656c43b77be4974adec637b2b798644a0b46763c4b145d02db6895fc
version 2:       11f3971004078a5e8c09a3cbe3e90635fe424ef1c5971ebcdeb6060728780bf2
unknown tag:     903aa3d0dfe5cd772d78ad340455ab69d63bd305b98c8397155703b1eb64eced
count 100001:    3f7874896c08d8d5c564ac92fe2f761bab520f1b2b2a7e43f3b065745d985532
reordered wrap:  a5e8f767c2fedaa8384ea485ff62ae059ac28232ecd6bdad6eadffd436689645
duplicate name:  2de486c9ab955a5812df231ebe2874362288d187595bee961544804fb171ee50
chunk len 32769:9fe513370f132bfacbb3513c1f05789340416df8307a7acef09a405c0b4fe613
short chunk ID: 21c245a720bf8b2182c15769dffaffd19b7f578e4e10d5b32f7ed80b11b584ed
bad has_parent:  22aba1235a3f7cd7102920a59441efde09321d502acfa835f9e139a5ccd68680
bad operation 00:94186cbbd94f370b8e8004cd3400c3319405f9a75fd062b678cc4a914b66087e
bad operation:   bd1d9042714d7fbbc2af2657f8f08237e0f4c90e83550c5273cbd31902cc87d0
oversize field:  a558c567455c7ff54a5431dd5be9e76a0d7908fbb40466550c395da6bdb559ba
missing-page idx:3dec87389de00a0db9dee4f111158c0d11cb5d1aaa2aff2769f034c317b71c93
```

WP5 must **not** consume these withdrawn IDs. The promoted replacement corpus
asserts encode-decode-encode byte identity and instantiates
table-driven structural cuts, receipt-adversary cases, K/F boundaries,
897-entry, 2,010-entry, direct-100,000-reference, u64-height, and 256/257-depth
cases without checking giant blobs into the repository.

### 14.9 Withdrawn prior-draft fingerprint

The following **non-authoritative** prior-draft fingerprint was SHA-256 over the exact UTF-8 byte slice that
begins with the first `#` of the heading `### 14.2 Chunk identity domain` and
ends immediately before the first `#` of this heading. It therefore covers
every complete/constructed success vector, identity, reconstructed value,
ordered strong edge, failure vector, and expected first/dominant result above,
while avoiding a self-referential digest.

```text
fa62b4ac5f88bdf929ea2da4fe16415c3da5f7d1f928d1eda0564f8397bb5325
```

## 15. WP4 acceptance checklist

- [x] Caller/type inventory and Phase 1/2/3 semantic reconciliation are frozen
  in sections 2-3.
- [x] Exact selected outer composition, inner magic/version/tags, integer
  widths, ordering, and EOF are specified and the one live compatibility
  profile is promoted by WP4-P.
- [x] Raw `ChunkId`, raw length, canonical Bytes `ObjectId`, zero-length
  behavior, and cross-domain verification are frozen in sections 5 and 9.
- [x] Selected K64/F64 partitioning, the DIR256K ceiling, greedy
  delta paging, descriptor linkage, scalable u64 counts, and canonical
  rejection rules are specified; K/F and directory-ceiling promotion passed
  WP4-P.
- [x] Tree composition, durable Node/Root identity, transition-only parentage,
  genesis, ordered Delta semantics, and provisional/durable translation are
  frozen in sections 6-8.
- [x] Selected independent topology, directory, delta, receipt, and malformed
  goldens are frozen in section 14.0; range/COW boundaries pass the verified
  owner/workspace suites.
- [x] Object/field/direct-reference/name/path/chunk/depth bounds, physical edge
  depth, active-cycle behavior, Q/W/D separation, exact edge-spool
  cursor/resumption, receipt trust boundary, transport/parser/stack/spool/output
  allocation, and eager/streamed admission are specified in section 9.
- [x] Missing/malformed/version/role/order/partition/identity/range/replay/
  resource/I/O/cancellation/ambiguous-durability errors, bounded first/dominant
  provenance, cleanup ordering, and immutable-residue custody are frozen in
  sections 9.2 and 10.
- [x] Memory, SQLite, and plausible remote immutable-object plus atomic
  publication requirements are frozen without implementing a third backend in
  sections 7, 9, and 12.6.
- [x] The 200/300 MiB/s credibility diagnostics, exact routine 1/10/100-MiB
  rows, 100-GiB analytical equations, file/directory COW, declared middle-edit
  bound, and prospective compact K64/F64 lane are specified in section 12 and
  the fast-lane amendment; CP-0006 passed the 27-invocation package.
- [x] Selected exact success/failure bytes and IDs are frozen in the normative
  TSV/test corpus; section 14's prior-draft vectors remain withdrawn.
- [x] The dependency order, private provisional measurement authority, loser
  deletion point, and absence of pre-promotion compatibility/product authority
  are frozen in section 12.7.
- [x] Every categorical rejection is tied to contract, semantic, boundedness,
  or measured/equation evidence; category-5 preferences are reopened in
  section 13.
- [x] Final post-freeze read-only audits pass, the terminal fingerprints are
  recorded by the WP4-P promotion ledger, and WP4 is complete.

## 16. WP4 completion boundary

WP4-C/WP4-M froze the research, selected grammar, evidence, and policy input.
WP4-P deleted alternatives, froze the production profile ID and selected
goldens, and passed the implementation, test, deletion, fingerprint, and both
independent audit gates. WP4-P and WP4 are complete; the one K64/F64 + DIR256K
profile is compatibility-promoted. This document itself does not:

- implement this codec or change current provisional in-memory IDs;
- change Memory or SQLite engine behavior/schema;
- add benchmark code or publish a 200/300 MiB/s result;
- implement batching, receipts, caches, or remote transport; or
- authorize a public or compatibility-bearing multi-profile production path.

The active step is WP5, which is eligible/pending against the single promoted
profile. CP-0006 remains nonqualifying historical WP4-M evidence and was not
relabelled. The WP7 integration must resolve root-parent convergence exactly
as specified in section 7 rather than perpetuating parent as immutable root
content. Overall Phase 4 remains incomplete until its later work packages pass.
