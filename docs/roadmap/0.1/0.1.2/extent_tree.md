# Extent trees: edit the references, keep the bytes

> **Status:** Research; informative and not a product contract.
> Instructional companion to the v0.1.2 architecture record, grounded in the
> shipped implementation and its pre-v0.1 history.

**The key shift: changing a file's logical layout no longer requires moving
its unchanged payload or regrouping every reference in its suffix.**

| Capability | Architectural enabler |
| --- | --- |
| Small edits without file-sized payload work | Reuse immutable payload slices and unaffected subtrees |
| Arbitrary valid edit position | Split/join a byte-measured sequence |
| File-length changes | Derive logical positions from subtree summaries |
| Canonical extent-count changes | Rebuild affected structure without fixed-ordinal suffix regrouping |

This foundation combines **extent trees + immutable slices + operational
identity**. The Workspace piece tree exposes it to uncommitted edits.
“File-size agnostic” means no mandatory processing of untouched file bytes,
not identical timings or constant-time Commit. Diagrams below are ASCII.

## 1. Before → after

| Representation | What a middle insertion changes | Main cost driver |
| --- | --- | --- |
| Contiguous file reconstructed through a temporary copy | Copies the prefix and suffix around new bytes | File bytes |
| Historical fixed-ordinal mapping | Can regroup references throughout the suffix; payload may still be shared | Suffix references |
| Byte-measured extent tree | Rebuilds boundary/path metadata and references the retained prefix/suffix | Replacement bytes + affected tree work |

These are **three different mechanisms**. The historical mapping did not
necessarily copy payload; the obsolete temp-copy benchmark did.

### Before: fixed groups turn one insertion into suffix work

Each letter below is a payload reference, not a byte. Four entries per group
is an illustration, not LayerFS's actual fanout.

```text
Original: [ A B C D ] [ E F G H ] [ I J K L ]
Insert X: [ A X B C ] [ D E F G ] [ H I J K ] [ L ]
             ^            ^            ^       ^
          edited       regrouped    regrouped  new

Unchanged payload can survive; unchanged mapping groups cannot all survive.
```

### After: keep the suffix subtree, change its logical position

```text
OLD ROOT                         NEW ROOT
   |                                |
   +--> prefix subtree P            +--> same P       [REUSED]
   +--> boundary region B           +--> new boundary [CHANGED]
   +--> suffix subtree S            |       |
                                    |       +--> new payload
                                    +--> same S       [REUSED]

P and S identify the same immutable subtrees in both versions.
New boundary nodes can also retain slices of existing payload.
```

**Same labels mean reused object references.** Boundary nodes may split,
merge or rebalance; the diagram does not promise reuse of every metadata node.

### Why a larger file need not mean a proportionally slower edit

```text
FULL TEMP-COPY INSERTION
  [copy unchanged prefix] + [NEW 4 KiB] + [copy unchanged suffix]
                 work follows the original file size

EXTENT-TREE INSERTION
  [reference prefix]      + [NEW 4 KiB] + [reference suffix]
                 work follows replacement + tree metadata
```

| File size | Full reconstruction: output bytes | Extent splice: replacement plus metadata work |
| --- | ---: | ---: |
| 1 MiB | Approximately 1 MiB + 4 KiB | 4 KiB + tree work |
| 10 MiB | Approximately 10 MiB + 4 KiB | 4 KiB + tree work |
| 100 MiB | Approximately 100 MiB + 4 KiB | 4 KiB + tree work |
| 500 MiB | Approximately 500 MiB + 4 KiB | 4 KiB + tree work |

The reconstruction column describes output size, not total device traffic:
reading the original adds traffic too. Tree work is not necessarily identical
across sizes. This is an algorithm illustration, not a benchmark measurement.

## 2. What an extent actually stores

An extent is a **slice of an immutable payload object**, not a copy of that slice.

| Field | Meaning | What it is not |
| --- | --- | --- |
| `payload_object_id` | Identity of the immutable payload object | A mutable file path |
| `source_offset` | Start inside that payload object | Absolute position in the logical file |
| `logical_length` | Number of bytes exposed by this slice | Size of the entire file |

### Tiny example: insert `xyz` into `ABCDEFGHIJKL`

Toy payload sizes are for teaching; they do not depict real CDC boundaries.

```text
Existing object O: ABCDEFGHIJKL        New object X: xyz

BEFORE
  (O, offset 0, length 12)                 -> ABCDEFGHIJKL

AFTER: insert at logical position 4
  (O, offset 0, length 4)                  -> ABCD
  (X, offset 0, length 3)                  -> xyz
  (O, offset 4, length 8)                  -> EFGHIJKL
                                             |
                                             v
  Logical file: ABCDxyzEFGHIJKL

O is unchanged. Its suffix starts at logical position 7, but source offset 4.
```

## 3. How the tree finds bytes without global offsets

Branch descriptors store **cumulative byte ends within their parent**, plus
cumulative extent counts and child identities.

```text
                  [ PARENT: 15 logical bytes ]
                       /       |       \
                      v        v        v
Child:                A        X        B
Length:               4        3        8
Cumulative end:       4        7       15
Logical interval:   [0,4)    [4,7)    [7,15)
Bytes:               ABCD     xyz     EFGHIJKL
```

| Find logical byte 9, zero-based | Calculation |
| --- | --- |
| Choose containing child | `7 ≤ 9 < 15`: child B |
| Convert to child-local position | `9 − 7 = 2` |
| Resolve the illustrated payload slice | Object O, byte `4 + 2 = 6`: `G` |

Changing a parent's summaries does **not** rewrite offsets inside every
descendant. Real nodes have up to 128 entries and at most 8 KiB encoded size;
the three-child diagram is schematic.

## 4. One splice recipe, many operations

```text
                    split at p      split after d
                         |                |
                         v                v
Original:       [ PREFIX ][ REMOVED RANGE ][ SUFFIX ]

Result:         [ PREFIX ][ REPLACEMENT X ][ SUFFIX ]
                    ^                         ^
                    |                         |
                reuse references          reuse references

  Chunk replacement bytes -> join retained sides with replacement
                          -> persist changed structure and new root
```

Conceptual parameters, **not executable SDK syntax**:

| Operation | Start | Delete length | Replacement |
| --- | ---: | ---: | --- |
| Prepend | `0` | `0` | New bytes |
| Append | `N` | `0` | New bytes |
| Middle insert | `p` | `0` | New bytes |
| Equal-length overwrite | `p` | `d` | `d` new bytes |
| Grow / shrink replacement | `p` | `d` | More / fewer than `d` bytes |
| Delete range | `p` | `d` | Empty |
| Truncate | `p` | `N − p` | Empty |

All ranges must be valid. Same mechanism does not mean identical elapsed time:
boundary position, replacement volume and rebalancing still affect work.

```text
PREPEND        []                + [NEW] + [ENTIRE OLD FILE]
APPEND         [ENTIRE OLD FILE]  + [NEW] + []
MIDDLE INSERT  [PREFIX]           + [NEW] + [SUFFIX]   remove nothing
OVERWRITE      [PREFIX]           + [NEW] + [SUFFIX]   replace same length
GROW/SHRINK    [PREFIX]           + [NEW] + [SUFFIX]   replace other length
DELETE RANGE   [PREFIX]           + []    + [SUFFIX]
TRUNCATE       [PREFIX]           + []    + []
```

### Why all three benchmark families benefit

| Family | What changes | Why the suffix remains reusable |
| --- | --- | --- |
| `edit_length_preserving` | Bytes change; file length stays fixed | Replace the affected range and retain surrounding references |
| `edit_length_changing` | Logical file length changes | Derive new positions from byte summaries, not global offsets |
| `edit_canonical_chunk_count` | Extent count can increase/decrease even at equal byte length | Split/join local structure and update count summaries |

Canonical extent count is **not unique payload-object count**. The engine does
not need a separate algorithm selected by family or append/prepend position.

### Equal byte length, different extent count

```text
BEFORE
  [shared prefix P] [ b1 | b2 | b3      ] [shared suffix S]
                    <---- 64 KiB ---->

AFTER
  [same prefix P]   [ x1 | x2 | x3 | x4 ] [same suffix S]
                    <---- 64 KiB ---->

Changed:   3 extents -> 4 extents in the illustrated replacement region
           boundary/path metadata and affected count summaries
Unchanged: byte length; reusable prefix/suffix subtrees and payload
```

Illustrative partitions, not exact benchmark chunk boundaries. The axes
overlap: equal-length edits can change extent count; a length-changing edit
alone does not prove extent count changed. Separate qualified controls test it.

## 5. The identity decision that makes locality useful

```text
Payload bytes                    --> Payload ObjectId
Authenticated extent structure   --> Operational FileStateRoot
Complete logical byte stream     --> Separate semantic ContentDigest

History A --> tree A --> root A --+
                                 +--> identical bytes are possible
History B --> tree B --> root B --+
```

| Choice | Consequence |
| --- | --- |
| Authenticate each object and reference | Shared old data remains verifiable |
| Permit history-shaped operational roots | Same logical bytes can have different valid structural roots |
| Separate whole-file semantic equality | A local edit need not hash every unchanged byte |

**Canonical node encoding does not imply one unique tree shape for every
possible edit history.** Computing a fresh whole-file digest still costs `Θ(N)`.

## 6. Time complexity: what improved

| Symbol | Meaning |
| --- | --- |
| `N` | Original file bytes |
| `a` | Supplied replacement bytes |
| `S` | Mapping references in the affected suffix |
| `E`, `H`, `b` | Extent count, tree height, node fanout (`b ≤ 128`) |
| `W_tree` | Actual metadata work: visit, decode, split/join, rebalance, hash and prune |

| Work | Before | Extent-tree approach |
| --- | --- | --- |
| Temp-copy reconstruction | `Θ(N + a)` byte movement | No required copy of untouched payload; process replacement bytes |
| Historical early/middle count-changing mapping | Could require `Θ(S)` reference regrouping and corresponding mapping-byte work | Reuse unaffected subtrees; perform boundary/path work |
| Locate a byte | Representation-dependent | One root-to-leaf descent; at most `O(bH)` entry work for a simple per-node scan |
| Known-range canonical splice | File/suffix-wide work in the mechanisms above | `O(a + W_tree)` content-engine work; Store costs are separate |
| Fresh digest / read all bytes / materialize file | `Θ(N)` | Still `Θ(N)` |

```text
OLD COST DRIVER                         NEW COST DRIVER

unchanged file / suffix                  changed bytes + tree metadata
########################  + edit        edit + boundary/path work
          N or S                                     a + W_tree
```

| Intuition | Honest implementation claim |
| --- | --- |
| A balanced tree has logarithmic height in `E` | Finding a position does not visit every extent |
| A textbook local splice suggests replacement work plus logarithmic paths | **Do not assert a proved `O(a + log E)` worst-case bound here**: current split/join can concatenate during unwinding and uses deferred maps/sets |
| A 500 MiB file need not copy 500 MiB for a small edit | Metadata height, cache/Store behavior and publication can still vary with size |

**The improvement is removal of mandatory untouched-payload linear work,
not a universal `O(1)` Edit + Commit guarantee.**

## 7. Memory: references replace file-sized working buffers

| Scope | What must remain in memory |
| --- | --- |
| Unchanged payload | No file-sized edit buffer is required; payload remains in immutable objects |
| Canonical edit | Streaming replacement buffers and affected/deferred tree metadata |
| Durable result | New payload and changed metadata still consume storage |
| Full temp-copy alternative | Can also stream with bounded RAM, but still moves file-sized data |
| Process / container peak | Includes allocator, caches, database and lifecycle state—not just the tree |

Extent trees remove the **need** to materialize the unchanged file for a local
edit. They do not, alone, prove a constant RSS ceiling for the whole process.

### Judge the design by speed, storage and memory

| Priority | Benefit | Cost to watch |
| --- | --- | --- |
| Fast edits | No mandatory untouched-payload copy/scan | Fragmentation, metadata and Store latency |
| Reasonable storage efficiency | Shared payload and unchanged subtrees across versions | Changed metadata, retained versions, small live slices retaining larger objects |
| Low memory | Reference-based state and streamed changed content | Retained Inline bytes, deferred metadata, database/cache memory |

The goal is fast edits with reasonable storage and low memory, not minimal
internal complexity. No claim here establishes “fastest among all designs.”

## 8. Where this sits in v0.1.2

```text
One SDK range edit
        |
        v
Workspace piece tree      record uncommitted references
        |
        v
Commit                    lower final changed runs
        |
        v
Canonical extent tree     splice authenticated committed structure
        |
        v
Store                     admit objects and publish
```

| Layer | Contribution | Limit |
| --- | --- | --- |
| Workspace piece tree | Avoid eager canonical rebuilding on every edit | Work depends on piece height, input and retained state |
| Canonical extent tree | Preserve unchanged payload and subtrees at Commit | Metadata work remains |
| Targeted FUSE refresh | Avoid mount teardown/recreation after owner edits | Driver-specific lifecycle improvement, not a new splice algorithm |
| Store | Admit objects and publish new state | Database and namespace costs remain |

The extent foundation predates v0.1.0; the content crate is unchanged between
v0.1.1 and v0.1.2. Current measured SDK improvements therefore **do not isolate
the extent-tree transition**. See the [SDK timing tables](../../../../release-notes/0.1.2/sdk-edit-benchmark-results.md)
for measured 1/10/100/500 MiB results, and the
[full architecture analysis](architecture_shift.md#8-implementation-grounded-time-and-space-analysis)
for complete Workspace/Commit formulas and memory qualifications.

## Source map

| Read | Explains |
| --- | --- |
| [Extent types and invariants](../../../../crates/layerfs-content/src/file/extent.rs) | Slice fields, summaries, fanout and node bounds |
| [Canonical splice implementation](../../../../crates/layerfs-content/src/file/rope/edit.rs) | Replacement scan, split, concatenate and persistence |
| [Workspace pieces](../../../../crates/layerfs-workspace/src/file_edit.rs) | Uncommitted reference representation |
| [Commit lowering](../../../../crates/layerfs-workspace/src/changes.rs) | Final pieces → canonical edits |
| [Architecture history](architecture_shift.md) | Commit-pinned before/after evidence and identity decisions |

**Remember: change the reference sequence; keep the unchanged bytes.**
