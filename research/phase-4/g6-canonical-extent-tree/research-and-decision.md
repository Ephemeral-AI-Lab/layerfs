# G6 canonical extent-tree research and decision

Disposition: **`G6_SPEC_READY_PENDING_G5_BASELINE`**

This report is a read-only reconciliation of LayerFS source, retained Phase-4
evidence, prior research, and primary external sources. It does not implement
G6, change a canonical profile, run a measured campaign, or make G6 eligible.

## 1. Executive decision

LayerFS should continue toward **one canonical authenticated extent mapping and
one portable resolver**, but it should not implement the thesis under the
current unconditional complexity wording.

The current mapping already is an authenticated extent map: every leaf stores
ordered `(raw_length, canonical ObjectId)` occurrences, while internal nodes
store byte measures and child identities. Its problem is fixed ordinal
partitioning, not the absence of extents.

The best compatible candidate is a bounded content-defined measured sequence
tree. It preserves one ordered canonical occurrence sequence/one mapping root
and usually reuses the suffix after a nearby stable boundary. The stronger
one-raw-byte-content/one-root property also depends on the frozen FastCDC
construction/edit witness; the tree alone cannot canonicalize two different
valid chunk segmentations of the same bytes. Its hard worst case remains
suffix-linear. Therefore:

```text
Supported claim:
  ordinary/expected raw mutation
    = O(k*log E + unique CDC/tree replay + streamed replacement)

Hard claim:
  mapping-only worst case = Theta(E_suffix)
  successful raw fallback
    = Theta(replacement remainder + raw suffix + extent suffix)
  with bounded nodes and bounded resident memory

Unsupported claim:
  all count-changing edits are hard O(log E + local work)
```

If hard logarithmic updates and raw-byte one-content/one-root are both
non-negotiable,
the current research is insufficient and the architecture question remains
open. A normal balanced rope/B+ tree cannot silently substitute because equal
logical content reached through different histories can have different roots.

G6 now explicitly owns the broad variable-size solution: raw insert/delete,
shorter/longer replacement, bounded atomic multi-splice, variable FastCDC
occurrence counts, dual-coordinate diff/coalescing, virtual count-changing
visibility, and native tail/shift/reflink/fallback routes. G5 is narrowed to the
terminal trusted/position-preserving/fallback baseline and is not authority for
these G6 paths.

## 2. Evidence authority

### 2.1 Repository/source state

Observed repository state at the research snapshot:

```text
branch  codex/empty-worktree
HEAD    d58c5a1307253dfc221fe50de996c183deb9458a
```

The benchmark and materialization binaries were dirty from active G5 work.
No G5 file was edited, built, or measured by this research. The reusable
`layerfs-engine` still uses the older `layerfs_*` schema and a simpler visible
root, while the active private benchmark uses `wp4m_*`, complete visible-head
receipts, reconciliation, and G5-only projection code. G5-2 itself records this
promotion gap in
[`PROMOTION-READINESS-v3.md`](../../../implementation-detail/phase-4/experiments/g5-warm-projection/v3/PROMOTION-READINESS-v3.md).
During the read-only audit, the projection source advanced from its provisional
`47578f...` binding through `286e7b...` to the timestamped final research
snapshot
`2b9f197d1dc816f40f02fc10cdeefa0ee12fea3ba6d926aa66a70052120debbb`
at `2026-08-23T08:05:56Z`; the v3 package remains
`PREMEASUREMENT_REVISE`, confirming that no settled G5 projection source is
available as a G6 control.

### 2.2 Evidence hierarchy

| Evidence | Status for G6 research |
|---|---|
| CP-0008 raw/analysis | Sealed historical scale evidence for fixed-radix suffix work; canonical-v1 widths |
| CP-0009 raw/analysis | Sealed historical workflow control; not the current G6 control |
| Canonical-v2 accepted baseline | Current format/identity and 100-MiB mapping-width authority |
| G3-v13 | Sealed benchmark-private same-size native projection evidence |
| G4 stage baseline | Valid observed protected-operation anchor; v12 measured terminal remains `REVISE` under its frozen percentage rule, while a separate stage PASS uses the user-approved sub-1-ms materiality rule; benchmark-private |
| G5-0 H11-v9 | Sealed narrow long-history/Q evidence |
| G5-1 v24 | Mechanism evidence only; promotion authority revised |
| G5-1 v26 | Prospective premeasurement authority rerun; zero measured rows at snapshot |
| G5-2 v1/v2 diagnostics | Diagnostic only; superseded/revised |
| G5-2 v3 | `PREMEASUREMENT_REVISE`; zero product rows |
| This report and prior plans | Hypothesis/design, never performance evidence |

No current G5 diagnostic value is a sealed G6 baseline. Final G6 percentage,
absolute, RSS, storage, and process-shape gates must be frozen against the
eventual G5 terminal package.

## 3. Current algorithm audit

### 3.1 CDC and payload identity

The frozen CDC profile remains 8 KiB minimum, 16 KiB target, 32 KiB maximum,
normalization 2, and seed zero. Full create necessarily scans `Theta(B)` input
bytes. Bounded exact rejoin can make a small edit proportional to the changed
CDC window, but fallback remains complete when rejoin is not established.

Canonical-v2 payload objects retain the existing Phase-1 canonical `Bytes`
framing and `ObjectId`. G6 does not change payload chunking, payload object
bytes, CAS identity, compression, or hash domains in its first variable.

### 3.2 Exact canonical-v2 mapping

Observed codec facts:

- [`canonical_v2.rs:15`](../../../crates/layerfs-core/src/canonical_v2.rs#L15)
  freezes mapping version 2 and a 36-byte occurrence;
- [`canonical_v2.rs:72`](../../../crates/layerfs-core/src/canonical_v2.rs#L72)
  encodes `u32be(raw_length) || ObjectId[32]`;
- [`content/persistence.rs:68`](../../../crates/layerfs-core/src/content/persistence.rs#L68)
  defines a 40-byte child descriptor with cumulative raw end and child ID;
- [`content/persistence.rs:280`](../../../crates/layerfs-core/src/content/persistence.rs#L280)
  requires every nonfinal leaf to be exactly K=64;
- [`content/persistence.rs:301`](../../../crates/layerfs-core/src/content/persistence.rs#L301)
  applies the same F=64 canonical partition rule to children;
- [`content/persistence.rs:321`](../../../crates/layerfs-core/src/content/persistence.rs#L321)
  derives minimal height from occurrence count.

This produces a deterministic left-packed tree. A file with equal ordered
occurrences has one tree and one root independent of edit history.

### 3.3 Construction

The current `FileBuilder` owns one leaf and one bounded frontier per active
level at
[`phase4_create_edit_benchmark.rs:5505`](../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs#L5505).
It flushes a leaf at K and a branch at F, then emits the minimal root. Full
construction is:

```text
time       Theta(B + E)
resident   O(K + F*H + bounded CDC/CAS/SQL windows)
metadata   Theta(E)
```

Here `B` is payload bytes, `E` is extent/occurrence count, and `H` is mapping
height.

### 3.4 Same-count edit

The same-count path at
[`phase4_create_edit_benchmark.rs:7743`](../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs#L7743)
rewrites the selected leaf and only its ancestor spine. For one changed leaf:

```text
O(CDC window + K + F*H)
```

The accepted G4 anchor is 8.043 ms for same-open 100-MiB same-count edit. The
new structure must protect this already-good path.

### 3.5 Count-changing edit

The count-changing code at
[`phase4_create_edit_benchmark.rs:8183`](../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs#L8183)
reuses complete prefix subtrees, inserts into the selected leaf, then walks
and feeds every later reference into a fresh fixed-radix builder. Exact suffix
counters are finalized at lines 8443–8461.

For insertion ordinal `p` in `E` references:

```text
Z = E - p
mapping work = O(Z)
worst case   = Theta(E)
memory       = bounded
```

The unchanged suffix payload objects are not rewritten. Their references and
mapping containers are repacked.

This path is narrower than its historical name suggests. Its construction
proof admits one inserted occurrence and requires `new_E=old_E+1`; the product
path creates a standalone one-byte occurrence rather than rerunning frozen
FastCDC over a raw one-byte insertion. CP-0008 therefore proves fixed-radix
suffix scaling, not general raw-byte mutation semantics.

The reusable semantic `replace_range` and private rejoin path each accept one
contiguous replacement and retain source-sized/owned occurrence state. They do
not yet provide bounded arbitrary insert/delete, shorter/longer replacement, or
atomic multi-island mutation. G6 must separate:

```text
DeltaB = final raw-byte-length delta
DeltaE = new FastCDC occurrence count - old occurrence count
```

Any raw `+/-1 B`, `+/-4 KiB`, `+/-64 KiB`, or `+/-1 MiB` mutation may yield
`DeltaE<0`, `=0`, or `>0`; only a fresh frozen-FastCDC oracle decides.

### 3.6 Range read

The root and recursive range paths at lines 9157–9348 use child byte summaries
to prune unrelated subtrees, authenticate intersecting leaves and payload
objects, and emit only requested slices. The present exact form is:

```text
O(F*visited branches + K*visited leaves + intersecting chunks + returned bytes)
```

For one leaf and fixed bounds this is logarithmic routing plus returned bytes.
The current segmented helper still owns all requested segments. G6 should
provide a bounded callback/cursor resolver so SDK, VFS, and materializer share
the traversal without requiring a complete output vector.

### 3.7 Publication and durability

The private current Store uses `BEGIN IMMEDIATE`, one transaction, one visible
head, one `COMMIT`, and fresh read-only requested/prior/different/ambiguous
reconciliation. The profile remains rollback-journal `DELETE`,
`synchronous=FULL`, `temp_store=FILE`, `mmap_size=0`, and `cache_spill=2000` in
the active benchmark.

G6 changes immutable mapping nodes only. It may not change this publication
shape, hide work after canonical ACK, or treat virtual/native projection as
part of the canonical COMMIT.

### 3.8 Current operation complexity summary

`B` is logical payload bytes, `E` occurrence count, `Z` the affected suffix,
and `H` current mapping height. These are source-derived classes; cache warmth
and current wall medians do not change them.

| Current operation | Algorithmic work |
|---|---|
| Full create / mapping construction | `Theta(B + E)` input/CDC plus `Theta(E)` mapping, bounded streaming memory |
| Same-open same-count edit | `O(local CDC window + K + F*H)` after exact rejoin |
| Structural occurrence-count edit | `O(Z)` mapping; worst `Theta(E)`; unchanged payload CAS objects reused; not a general raw FastCDC splice |
| Direct range read | `O(F*visited branches + K*visited leaves + intersecting chunks + returned bytes)` |
| Reopen/head lookup alone | Bounded head/receipt lookup and decode; it does not establish a fresh full Verified closure by itself |
| First Verified edit after reopen | `Theta(B + E)` complete closure authentication before the edit, plus the chosen edit class |
| First TrustedLocalDev edit after reopen | Intended to remove only the eager closure term, but current G5-1 promotion evidence is unsealed and cannot be a G6 control |
| Warm or fresh full logical reconstruction | `Theta(B + E)`; cache class changes constants only |
| Cold/full native materialization | `Theta(B + E)` authenticated traversal and `Omega(B)` destination writes |
| Same-size incremental native projection | Bounded changed-range work with an exact protected seed; otherwise `Theta(B)` full fallback |
| Count-changing native projection today | Full fallback in current G5-2 representation, `Theta(B + E)`; G5-2 remains premeasurement |

## 4. Evidence-backed bottleneck

### 4.1 CP-0008 observed curve

CP-0008 has raw SHA-256
`599a2dc8e62ace12876c14342435d4794ae349556fd87eeb3d6fa21e5fdd1804`
and analysis SHA-256
`d477fe0a8e75bbf3fa6b63dcdf557ce288ec3e8ce63c468966a7a5c479d60a2c`.

| Size | Early `+1` | Middle `+1` | Suffix refs early/middle | Mapping bytes early/middle |
|---:|---:|---:|---:|---:|
| 1 MiB | 0.958 ms | 1.081 ms | 53 / 27 | 4,073 / 4,073 |
| 10 MiB | 1.739 ms | 1.394 ms | 531 / 266 | 37,121 / 19,601 |
| 100 MiB | 7.403 ms | 5.715 ms | 5,284 / 2,642 | 365,495 / 185,915 |
| 500 MiB | 27.141 ms | 15.102 ms | 26,533 / 13,267 | 1,833,348 / 918,921 |

From 100 to 500 MiB, suffix references grew 5.021x/5.022x and mapping bytes
grew 5.016x/4.943x. This directly confirms `O(suffix)` mapping work. CP-0008
used canonical-v1 widths, so its reference counts and slopes transfer; its
absolute metadata bytes and 500-MiB wall are not canonical-v2 measurements.
Its `+1` is one structural occurrence insertion, not a raw-byte FastCDC oracle;
no new magnitude gate may subtract or relabel those walls.

Any paired raw A arm is therefore a new prospectively frozen G6 reference: the
same raw mutation frontend and independent full FastCDC occurrence manifest,
grouped with current canonical-v2 K64/F64 and published through the matched
durability endpoint. It is not a historical G5/CP subtraction. A and C logical
bytes/digest match, while profile-specific roots match their own oracles.

### 4.2 Current canonical-v2 constants

The accepted G4 reconciliation corrects the 100-MiB v2 file mapping to 196,055
bytes in 86 mapping objects. Current v2 operation counters are:

| Operation | File mapping bytes | Total mapping-rewrite counter |
|---|---:|---:|
| same-count middle | 5,050 | 5,334 |
| early `+1` | 196,091 | 196,375 |
| middle `+1` | 100,479 | 100,763 |

Canonical-v2 improved full-create constants and halved occurrence width, but
did not change the suffix asymptotic class.

### 4.3 Payload, mapping, and projection separation

```text
Canonical count-change
  payload CDC/CAS reuse       already local after exact rejoin
  mapping regrouping          suffix-sensitive problem targeted by G6
  SQLite durable publication fixed one-transaction/one-COMMIT boundary

Native projection
  same length                 G3/G4 clone + sparse patch fast path
  changed length              current G5-2 complete fallback
  contiguous APFS inode       still needs suffix shift or full reconstruction
```

A canonical tree can make virtual visibility and logical mapping local. It
cannot make a conventional contiguous APFS inode insertion hard logarithmic.

## 5. H05/H05A/H09 reconciliation

### H05

H05 replaced a complete-source private digest with a 190,224-byte ordered
canonical occurrence commitment. It won 3/3 pairs with a 16.655343% paired
median but failed its frozen allocation-equality rule and remains terminal
NO-GO. It changed full-create constants, not mapping topology.

### Canonical-v2 / H05A

Canonical-v2 removed the redundant raw `ChunkId` from each occurrence:

```text
68 bytes -> 36 bytes
365,262 -> 196,174 mapping bytes at 100 MiB
```

Its accepted full-create A/B improved 23.281%. K64/F64 remained unchanged;
therefore count-changing work remained suffix-linear.

### H09

H09 was intended to make grouping content-determined so a count change could
resynchronize and reuse later canonical nodes. No accepted H09 simulator or
implementation exists. Retained research found:

| Model | 100-MiB live mapping | Objects | Finding |
|---|---:|---:|---|
| K64/F64 v2 | 196,055 exact | 86 | Current control |
| exact Xet 3–9 | 235,363 minimum; about 271,002 ordinary | 663 minimum; about 1,185 ordinary | Exceeds local metadata/object gates |
| CD32–64 | 196,735–203,351 derived envelope | 86–173 | Plausible shadow, expected-local only |

The CD32–64 local-path model of roughly 5.5–14.0 KiB ordinary count-change
metadata is promising but unmeasured.

## 6. Alternatives matrix

| Structure | One ordered occurrence sequence / one root | Count-change / range | Memory bounds | History/storage | Portability | Migration risk | G6 decision |
|---|---|---|---|---|---|---|---|
| Current fixed K64/F64 | Yes | `Theta(suffix)` / good logarithmic path | Bounded streaming | Suffix metadata per count edit | Already portable | None as control | Control/fallback |
| Persistent B+ tree with subtree lengths | No by default | Hard `O(log E)` path copy / excellent | Hard bounded nodes/path | Good sharing; topology follows update history | Portable codec | High: new profile/roots plus identity-policy change | Reject as canonical |
| Deterministic split/merge B+ tree | Still history-dependent | Hard `O(log E)` / excellent | Hard bounded nodes/path | Different histories retain different legal splits | Portable codec | High and fails canonical identity | Reject as canonical |
| Rope/RRB/finger tree | No by default | `O(log E)` split/splice / good | Balance-dependent path | Fragmentation/shape follows edits | Portable in memory | High as authority; low as transient cache | Transient/projection only |
| Piece table | No | Local descriptor edit / indexed traversal | Needs descriptor/compaction cap | Piece count grows with history | Portable in memory | High: second authority and compaction semantics | Derived VFS cache only |
| B-treap/implicit treap | Only with distinct stable keys | Expected logarithmic / expected logarithmic | Probabilistic height | Duplicate occurrences/offset shifts break transfer | Portable algorithm | High and unproved for this sequence | Reject for current sequence |
| Prolly/content-defined measured tree | Yes | Expected local, hard `Theta(E)` / good | Hard min/max nodes and path | Strong ordinary sharing; suffix worst case | Portable codec/resolver | High but explicit versioned profile/conversion | Selected shadow candidate |
| Edit log/overlay | No | Cheap append / replay amplification | Requires replay/compaction caps | History chain grows | Portable log | High: second authority and crash-safe compaction | Reject as canonical |
| Carrier/pack/delta chain | Physical only | Does not solve grouping / index dependent | Policy-dependent | Repack/GC/recovery cost | Layout/backend dependent | Very high storage/recovery change | Reject for G6 variable |
| Filesystem extent tree | Physical identity only | Backend-local path change / excellent | Platform-specific | Good physical snapshots | Not universal | Unsuitable as canonical migration | Inspiration/accelerator only |

## 7. Why the hard target conflicts with canonical identity

### Conventional balanced tree

Path-copying B+ trees and ropes can update a small number of nodes. But equal
sequences built by bulk load, left-to-right insertion, and mixed insertion can
have different valid split/rotation histories. Hashing those node bytes gives
different roots. A deterministic tie-breaker per operation does not guarantee
history-independent topology.

Canonicalizing the entire sequence after each change restores one root but can
reintroduce broad rewriting—the problem G6 is meant to remove.

### Content-defined tree

A prolly tree determines boundaries from entry content, so fresh build and
incremental construction converge. A nearby unchanged boundary normally lets
the new stream rejoin the old suffix.

However, a public deterministic boundary predicate admits streams where every
entry qualifies, no entry qualifies, or repeated values share one marker
class. A hard maximum bounds node size but then becomes an ordinal forced cut;
an insertion can shift all later forced groups. Correctness and memory remain
bounded, but mapping work can reach EOF.

Therefore the correct target is expected locality plus an explicit hard worst
case, not an unconditional logarithmic claim.

## 8. Selected revised candidate

### 8.1 Leaf occurrence

Reuse canonical-v2 exactly:

```text
ExtentOccurrenceV3 {
    raw_length: u32,
    object_id: ObjectId[32],
}
```

The occurrence always covers the complete decoded canonical Bytes payload:
source offset zero and source length equal to `raw_length`. Arbitrary stored
CAS slices are forbidden because they create alternate representations. The
resolver may emit partial slices only at the requested range boundaries.
`raw_length` remains legal over `0..=32,768`: zero-length occurrences stay in
the canonical sequence, cut identity, and subtree extent counts, while
nonempty range intersection skips them. A nonempty all-zero occurrence
sequence is distinct from the unique empty occurrence sequence.

### 8.2 Internal child

Replace cumulative parent-relative offsets with local subtree measures:

```text
ChildDescriptorV3 {
    subtree_raw_length: u64,
    subtree_extent_count: u64,
    child_id: ObjectId[32],
}
```

Local lengths keep an unchanged sibling descriptor byte-identical when a prior
sibling changes length. A resolver accumulates lengths while scanning the
bounded node or builds a non-authoritative prefix table in its bounded decoded
cache.

### 8.3 Grouping

The first shadow reuses the existing CD32–64 research variable with a
byte-exact predicate:

```text
preimage = b"layerfs/g6/cd32-64/cut/v1\0"
           || role_u8
           || output_level_u8
           || u32be(entry_bytes.len)
           || entry_bytes
marker = (BLAKE3(preimage)[0] & 0x1f) == 0

role 0x01 = leaf occurrence; role 0x02 = internal child descriptor
leaf output level = 0; internal output level = child level + 1
```

Positions are one-indexed. Positions 1–31 are ineligible; the first marker at
32–63 closes inclusively; 64 closes inclusively regardless of marker. Only the
final tail may contain fewer than 32. If a complete prospective top descriptor
stream has at most 64 entries, the root embeds it directly regardless of
markers; otherwise ordinary grouping applies. Validation rejects a root that
can flatten one internal level into at most 64 descriptors. Levels count from
leaves, so root growth never relabels lower nodes.

The exact predicate/threshold is not frozen for product code until the shadow
shows its distribution and adversarial behavior. It must be profile bytes,
not a runtime tuning option.

Under an iid digest estimate, occupancy is 51.7763 entries and the forced-64
rate is 36.2055%; neither is an observation or guarantee. The shadow reports
both by level and does not tune the predicate after results.

### 8.4 Update

```text
establish Verified closure authority or explicit TrustedLocalDev Store scope
  -> authenticate and pin parent root
  -> normalize <=64 old-coordinate mutation islands before target writes
  -> derive every new coordinate from prior signed deltas
  -> accept only streamed, digest-bound replacement sources
  -> generate internal FastCDC restart/rejoin proof per influence cluster
  -> coalesce CDC-overlapping islands deterministically
  -> reconstruct only the unique affected reference streams/union paths
  -> emit deterministic leaf nodes
  -> stop when exact old boundary + node identity + source cursor realign
  -> repeat convergence at each parent level
  -> reuse all later subtrees by ObjectId
  -> fail bounded or fallback once from earliest unresolved cluster
  -> establish inductive equality to a fresh FastCDC/tree build from the
     exact-root base witness, normalized coverage, restart/rejoin boundaries,
     cursor alignment, occurrence order, and output root
  -> publish one immutable target through one expected-head COMMIT
```

The product edit never runs a fresh full-file oracle. Focused tests, the
metadata shadow, and frozen benchmark fixtures compare with an actual
independent fresh build outside the product ACK path and its timers.

### 8.5 Resolver

The smallest reusable engine surface is:

```text
resolve_byte_range(snapshot_read_scope, root, start, end, emit_slice)
stream_extents(snapshot_read_scope, root, emit_extent)
splice(edit_base_scope, root, CanonicalReplacement, expected_head)
diff_splices(parent_scope, parent, target_scope, target, emit_splice)
```

Occurrence-index lookup is internal unless a real caller requires it. No
backend registry or generalized adapter trait is needed before a second real
consumer exists.

Verified selective traversal requires a complete-closure receipt for the
exact root or a fresh full scrub before trusting unopened subtree measures.
TrustedLocalDev may use only its explicit Store-lifetime assumption and never
manufactures Verified authority. Every fetched mapping/payload object is still
identity-authenticated. `DiffSplice` carries explicit old start/length and new
start/length plus a bounded/streamed replacement plan; one coordinate system
cannot express insertion or deletion.

The input `CanonicalReplacement` and output `DiffSplice` list are deliberately
different. Projection recomputes its dual-coordinate plan from authenticated
parent/target roots because CDC influence can expand/merge ranges and latest
coalescing may skip multiple committed revisions.

For `k` normalized islands, streamed replacement `R`, unique CDC bytes `C`,
leaf replay `D_l`, and per-level descriptor replay `D_j`, expected ordinary
work is `O(kH + C + D_l + sum(D_j))`, ordinarily
`O(R + local resynchronization + kH)`. Successful unresolved fallback is
`Theta(R_remaining + raw suffix + extent suffix)`. This is file-size
insensitive for a fixed mutation shape, never mutation-size independent.

Editable roots also carry a profile/root-bound
`CanonicalSegmentationWitness` in the same visible transition/receipt SQLite
transaction and COMMIT. It is publication metadata, not a mapping object or
Verified closure authority. Reopen and fresh reconciliation authenticate the
head/profile/transition/witness tuple. Its exact codec/schema waits for the
sealed G5 engine boundary and is a product Stage-B blocker, not a metadata
shadow blocker.

## 9. Projection and portability

| Layer | Portable authority | Capability-specific behavior |
|---|---|---|
| Canonical tree/resolver | Node bytes, identities, validation, range/splice/diff | None |
| Direct SDK | Exact root/range calls into resolver | Consumer buffer/stream |
| Linux FUSE | Root pin and resolver semantics | Direct/cached/write-through/writeback behavior |
| macOS FSKit | Root pin and resolver semantics | Extension lifecycle and cache coherency |
| Windows ProjFS | Root-bound read hydration | Placeholder/full-file cache semantics |
| Native materializer | Authenticated plan and target root | APFS clone, Linux reflink, sequential fallback |

Primary platform facts:

- Apple documents whole-file `clonefile`, `clonefileat`, and `fclonefileat`,
  but no public arbitrary-range clone in that API surface:
  [Apple APFS Tools and APIs](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/ToolsandAPIs/ToolsandAPIs.html).
  Apple also describes same-volume clones as initially sharing storage in
  [About Apple File System](https://developer.apple.com/documentation/foundation/about-apple-file-system).
  The lack of a documented public range-clone API is an inference from this
  public surface, not a claim about undisclosed internals.
- Linux `FICLONERANGE` can COW-share a selected aligned range on a supporting
  same filesystem:
  [`ioctl_ficlonerange(2)`](https://man7.org/linux/man-pages/man2/ioctl_ficlonerange.2.html).
- Linux insert/collapse range is filesystem- and alignment-dependent:
  [`fallocate(2)`](https://man7.org/linux/man-pages/man2/fallocate.2.html).
- FUSE has direct, write-through, and writeback-cache modes with materially
  different acknowledgement/cache behavior. In writeback mode, `write()`
  acknowledgement is not LayerFS durability; `fsync` must map to canonical
  durability, and root change requires cache invalidation or versioned inodes:
  [Linux FUSE I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html).
- Apple FSKit implements a userspace filesystem extension. Its current
  Handler/DataCache/KernelOffloadedIO surfaces are beta; the initial adapter
  should use
  [FSVolume.ReadWriteHandler](https://developer.apple.com/documentation/fskit/fsvolume/readwritehandler).
  Kernel-offloaded extents are physical disk extents, not LayerFS CAS logical
  extents, so they cannot replace the resolver:
  [KernelOffloadedIOHandler](https://developer.apple.com/documentation/fskit/fsvolume/kerneloffloadediohandler),
  [DataCacheHandler](https://developer.apple.com/documentation/fskit/fsvolume/datacachehandler).
- ProjFS asks a userspace provider for placeholder metadata and file data and
  caches hydrated content. Once locally modified, a placeholder becomes a
  full local file rather than merely a provider cache. Initial scope is thus
  read/snapshot projection; writable LayerFS requires a separately designed
  close-modified ingestion, expected-head conflict, and canonical COMMIT path:
  [Microsoft ProjFS provider overview](https://learn.microsoft.com/en-us/windows/win32/projfs/provider-overview),
  [ProjFS cache states](https://learn.microsoft.com/en-us/windows/win32/projfs/cache-state).

These mechanisms consume the portable resolver; they never alter canonical
split rules, node bytes, or roots.

### Native routes

| Condition | Route | Honest complexity/claim |
|---|---|---|
| virtual/SDK read | Resolver only | No native file bytes |
| same length + exact seed + bounded ranges | Clone + sparse patch | Changed ranges plus durability |
| tail append/truncate | Whole clone + append/truncate | Changed tail or metadata; shifted suffix zero |
| APFS count change | Clone + bounded suffix shift | `Theta(native suffix bytes)` |
| Linux aligned reflink | Reflink prefix/suffix + patch edges | Extent metadata plus boundary bytes |
| Linux aligned insert/collapse | Clone + range operation + patch | Capability-dependent |
| unsupported/invalid authority | Sequential authenticated fallback | `Theta(file bytes)` |
| cold standalone export | Sequential authenticated materialization | `Theta(file bytes)` lower bound |

Capability selection is frozen before rows. Linux reports exactly one of
`InsertCollapsePatch` or `RangeReflinkSplice`; the other is NotSelected, not a
combined slash label. Accelerator failure discards the private temp before a
fresh `FullFallback`; uncertain visibility reconciles before any further
publication. Candidate-only route cells establish semantics/direct work and
absolute timing, not a percentage speedup.

## 10. Long history and concurrency

Immutable roots naturally pin reader snapshots. A reader resolves one exact
root for the complete operation; a newer COMMIT does not mutate that graph.
New/latest opens may choose the newer head after publication.

For projection coalescing, retain one in-flight request and one pending target
root. Do not concatenate edge-relative ranges across count-changing edits.
When dequeued, compute one authenticated diff from the active root to the final
pending target. Exact-root requests are never replaced.

History remains append-only until a separately authorized GC. G6 must count
new nodes and bytes per revision and prove no complete mapping duplication on
ordinary local edits. It must not claim a population-wide storage result from
H11's deterministic 1-MiB workload.

## 11. Migration decision

The candidate changes canonical mapping bytes and roots. It requires a new
mapping profile and version; it cannot reinterpret v2 objects as v3.

The first implementation strategy, if later authorized, should be:

1. isolated benchmark-private v3 store/profile;
2. no existing-store conversion in the first candidate;
3. v2 remains readable through the exact v2 profile and identities;
4. fresh v3 payload canonical objects are byte-identical and reusable under
   explicit store authority;
5. full reconstruction/range parity and the exact-root
   `CanonicalSegmentationWitness` prove the fresh v3 path;
6. original v2 store remains readable and is the rollback artifact;
7. no v2-parent/v3-child transition is emitted unless a separately specified
   migration envelope can represent both profiles.

The later supported route to an editable v3 head is complete authenticated raw
reconstruction followed by a fresh frozen-FastCDC v3 build. That deliberately
normalizes legacy alternate segmentation and zero-length occurrences and
creates new profile/root identities under an explicit transition. If lossless
occurrence preservation is ever required, it belongs to a distinct read-only
legacy-preserved profile; ordinary v3 splice rejects it until full
normalization. Conversion never widens the public splice boundary to arbitrary
segmentation.

Do not add dual write, background migration, or a generic format registry to
the first experiment.

## 12. External design evidence

- Ropes show persistent sequence split/concatenate benefits, but not canonical
  history-independent topology:
  [Boehm, Atkinson, and Plass, “Ropes: An Alternative to Strings”](https://onlinelibrary.wiley.com/doi/10.1002/spe.4380251203).
- Noms describes prolly trees as content-defined B-tree-like structures whose
  chunks remain stable around local changes:
  [Noms prolly-tree introduction](https://github.com/attic-labs/noms/blob/master/doc/intro.md).
- Noms' own design discussion identifies deterministic entry hashing and the
  adversarial block-growth problem:
  [Noms issue 3878](https://github.com/attic-labs/noms/issues/3878).
- Xet's pinned aggregation source uses content-derived 3–9 grouping (with
  one/two-entry final tails), providing the exact negative/reference arm rather
  than a name-only imitation:
  [xet-core `aggregated_hashes.rs` at `af1a3ff`](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/aggregated_hashes.rs).
- Finger trees and B-treaps show useful persistent-sequence/unique-expected-tree
  bounds, but their assumptions do not supply canonical topology for shifting,
  duplicate LayerFS occurrences:
  [Hinze and Paterson, “Finger Trees”](https://www.cs.tufts.edu/comp/150FP/archive/ralf-hinze/finger-trees.pdf),
  [Golovin, “B-Treaps”](https://www.cs.cmu.edu/~dgolovin/papers/btreap.pdf).
- Btrfs demonstrates COW B-trees and file extents, including splitting a large
  physical extent around a middle overwrite; it is physical-layout precedent,
  not a canonical content-root proof:
  [Btrfs design](https://btrfs.readthedocs.io/en/stable/dev/dev-btrfs-design.html).
- BLAKE3/Bao show authenticated range/verified-streaming tree techniques; they
  do not by themselves solve canonical insertion locality for the LayerFS CDC
  occurrence sequence:
  [BLAKE3 specification](https://raw.githubusercontent.com/BLAKE3-team/BLAKE3-specs/master/blake3.pdf),
  [Bao](https://github.com/oconnor663/bao).
- Git pack files define a physical delta/index representation below logical
  object identity and introduce delta dependencies/repacking concerns:
  [Git pack format](https://git-scm.com/docs/pack-format).

## 13. Required target revision

The provisional targets are revised as follows:

| Area | Original thesis | Evidence-backed G6 research target |
|---|---|---|
| Raw mutation Big-O | Hard `O(log E + local)` independent of magnitude | Expected `O(kH + unique CDC/tree replay)`, ordinarily `O(R + local resync + kH)`; raw fallback includes raw and extent suffixes |
| Byte versus occurrence delta | Treated as one count-change class | `DeltaB` and oracle-derived `DeltaE` are independent; CP-0008 remains structural occurrence evidence only |
| Canonical identity | One raw content/one root | Tree proves one ordered occurrence sequence/one mapping root; ordinary publication additionally requires the frozen FastCDC witness; optional occurrence-preserving legacy conversion is a separate read-only profile |
| Suffix payload | Zero fetch/rewrite | Retain zero unchanged suffix payload fetch/write on fast route |
| Suffix mapping | Zero rewrite | Required on qualifying ordinary rows after exact rejoin; fallback reported separately |
| Virtual projection | No full reconstruction | Required for resolver visibility |
| Native tail | Unspecified | TailAppend/TailTruncate with shifted suffix zero |
| Native APFS projection | Implicitly local | Explicit `Theta(native suffix + new span)` clone-shift or full fallback |
| Native Linux projection | Generic reflink | Exactly one preflight-selected aligned insert/collapse or range-reflink route; otherwise FullFallback/NotApplicable |
| Metadata | Below 1% | Required; stretch below 0.30% on ordinary K64 files |
| Memory | Low single-digit MiB | Resolver additional Q <=4 MiB provisional, terminal zero |

## 14. Final recommendation

**`G6_SPEC_READY_PENDING_G5_BASELINE`** is the supportable current
disposition. The earlier `REVISE_EXTENT_TREE_THESIS` checkpoint is superseded:
the content-defined extent-tree design is retained, while its complexity claim
has been narrowed and its variable-size/multi-splice/native contracts are now
specified.

Evidence supports the bottleneck, the universal-resolver separation, and one
specific shadow candidate. Evidence does not support a hard logarithmic edit
guarantee for a history-independent public-hash grouping, and no H09 shadow has
yet proved the candidate's identities, distribution, adversarial behavior, or
resource equations.

Smallest next step after terminal G5 PASS:

```text
metadata-only A/B/C shadow
  A current K64/F64
  B exact Xet 3–9 negative/reference
  C bounded CD32–64 measured sequence
  raw rows consume independently frozen full-FastCDC occurrence manifests
  structural occurrence +/-1 remains separately labeled

<20 seconds total
no payload persistence
no SQLite
no native projection
no G6 product source
```

After a passing shadow, implement G6 sequentially: canonical tree plus atomic
multi-island resolver, then virtual count-change, then native tail/shift/Linux/
fallback routes, then one combined protected screen/gate. Until G5 is sealed,
all G6 work remains research/specification only.
