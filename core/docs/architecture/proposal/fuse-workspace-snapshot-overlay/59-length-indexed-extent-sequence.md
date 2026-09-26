# Length-indexed extent sequence for one file

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Dated 2026-09-25. Describes the implementation landed on branch
> `codex/issue245-range-cow-plan` for issue #245 packages A–C, written against
> the working tree whose production LOC is 56,035. It supersedes the
> absolute-offset piece-page description in
> [02-overlay-snapshot.md](02-overlay-snapshot.md) for the file index only; the
> namespace, binding and custody rules there are unchanged. No performance,
> capacity or release claim follows from this document.
> [§7](#7-level-preserving-rebuild-and-balanced-packing-2026-09-26-248-phase-2)
> corrects the level and packing rules §2 and §3 describe below: the source
> those sections were pinned to got both wrong above two branch levels.

## 1. Why the format changed

The earlier file index keyed every extent record by its **absolute logical
start** in a fixed two-level page tree. A structural insertion moves every
suffix extent, so those keys had to be rewritten for every suffix page, and each
callback rebuilt the whole page set from a materialized `Vec<Piece>` capped at
1,024 extents.

The current index derives an extent's logical start from its **position in the
sequence**. A leaf packs extent records; a branch packs child references, each
carrying the logical byte length of its child subtree. Inserting bytes rewrites
those lengths on one ancestor path and the leaves the replaced interval touches;
every other page is shared with the previous root.

## 2. Page format

`PageData` keeps its 4,096-byte frame, `LFSWMTA1` magic, incarnation stamp, page
identity, level byte and trailing SHA-256 seal. Two header fields that were
reserved and zero now declare what the body is:

| Field | Offset | Value |
| --- | --- | --- |
| format | 49 | `0` keyed cells, `1` length-indexed pieces |
| kind | 54 | `0` cells page, `1` pieces page |

A reader refuses a page whose declared format does not match the reader it is
asked for, so an extent page written under the previous absolute-offset format
is **refused**, never reinterpreted. There is no in-place migration: a retained
private root from the older format must be migrated or refused, and this
implementation refuses it.

A **pieces leaf** (`level == 0`) holds packed 32-byte records:

| Bytes | Meaning |
| --- | --- |
| 0 | kind: `0` base, `1` local, `2` zero |
| 1..4 | length, 24-bit big-endian, `1 ..= MAX_EXTENT` |
| 4..12 | source byte offset inside the extent's own origin |
| 12..20 | private payload identity (`Local` only) |
| 20..28 | payload custody page (`Local` only) |
| 28..32 | zero |

A **pieces branch** (`level > 0`) holds 16-byte child references: an 8-byte page
identity and an 8-byte subtree logical length. Levels are **relative**: a branch
directly above the leaves declares `1`, and every branch declares exactly one
below its parent, so the root declares the tree's height. A reader accepts a
root at any declared level `1 ..= 7` and requires each deeper branch to declare
one below its parent. A branch holds at least one child. Ordinary packing gives
every branch two or more children, so the tree is canonical and its height is
exactly what the extent count requires; a fold that replaces a whole node
leaves it with one, which is lifted rather than refused ([§8](#8-collapsed-node-lifting-2026-09-26-248-phase-2)).

`MAX_EXTENT` is `2^24 - 1`. A larger extent is stored as several adjacent parts,
which is a page boundary and never a new logical byte. The declared level
ceiling is 7, and every arithmetic step is checked; a file that cannot be
represented inside those bounds fails with `Capacity`.

## 3. Path-copied replacement

`metadata_pieces::replace` performs one accepted edit:

1. Walk the old tree in order, accumulating the logical start of each subtree
   from its own child lengths (and its canonical read offset from the extents).
2. Share every subtree the replaced interval does not touch, by reference. A
   subtree that ends exactly where the interval begins is carried the same way;
   the replacement is merged at the boundary above it. A shared subtree's
   declared length is charged to the result, but its extents and replacement
   bytes stay unread, and the open leaf closes before the shared page so the
   parent places every page in order.
3. In the leaves the interval touches, retain the bytes before it, drop the
   overlap, and retain the bytes after it. An extent that straddles a boundary is
   split; its retained part keeps reading its own origin from one byte past what
   the interval consumed.
4. Merge adjacent compatible extents while the new leaves are written.
5. Pack the folded leaves and every branch level above them; the result is a new
   root that shares all untouched pages with the old one.

A **NULL old root** folds into the sequence the version already is. The length
of the sequence being replaced is derived from the splice arithmetic: a version
that is exactly one base read of its selected content (a never-edited file, or a
version a capture just converted) folds the interval into that implicit base —
the retained prefix and tail are base extents exactly as a stored leaf would
hold them; an empty sequence (a fresh file, or a version truncated to nothing)
takes the replacement as its whole result. Any other NULL-root length cannot
exist and is refused.

`Fold` charges each published extent before the page that names it is written,
validating length, shape, canonical offsets inside the recorded base length and
the running edit count. The count includes the sequence's trailing deviation
from its base — a final run of replacement bytes, or a tail that ends short of
it (a truncation, which lowering later derives as one deletion edit).

Two figures are **recorded** on the inode for the Commit walk:

- `edits`: the maximal replacement runs the splice itself counted, or `u16::MAX`
  when a shared subtree made the exact figure unavailable. Lowering always
  derives the exact count from the sequence and cross-checks it, except for the
  `u16::MAX` marker, which lowering resolves and `save` never reuses a base for.
- `replacement`: the sequence's **exact** replacement bytes, carried by
  arithmetic from the figure the replaced sequence recorded — minus what the
  interval dropped, plus what this splice inserted. A shared subtree's bytes
  are untouched by the interval, so they carry over without being read; when
  nothing was shared the fold's own emitted total must agree with the carried
  figure exactly, or the splice is refused.

A refused splice publishes no root and leaves the previous sequence exactly as
it was.

## 4. Bounded cursor

`metadata_pieces::Cursor` holds one page path and one leaf's records. A seek
subtracts subtree lengths on the way down; `next` steps through the leaf and
walks to the following leaf through the same path. The cursor checks the
operation deadline between page reads and never materializes a file's whole
extent list.

Each frame of the path remembers the logical start and the length of the child
it selected ([§9](#9-monotone-cursor-advance-2026-09-26-248-phase-2)). Advancing
to the following leaf therefore subtracts remembered lengths instead of
re-reading the path from the root: one page read for each frame the step passes
through, plus one for each level the following descent enters. Summed over a
whole walk the page visits are `O(H + L)` for `H` the height and `L` the leaves
the walk reaches — each page of the tree is entered about once — rather than
`O(H)` per leaf.

## 5. Complexity

| Path | Work per accepted callback |
| --- | --- |
| Local write index | `O(H + K)` extents folded plus the retained bytes; only touched leaves and their ancestors are written |
| One read at an offset | `O(H)` index pages plus the leaf it lands in |
| Commit walk of one file | one ordered pass, `O(H + L)` private page visits for `L` leaves, bounded edit buffer and bounded cursor |

`H ≈ log_F P` for `P` extents, fanout `F` (a branch page holds 248 children, a
leaf 124 records) and `K` extents touched by one callback. The logical file
ceiling remains 4 GiB.

## 6. What this does not yet do

- Commit lowering streams the sequence through the cursor and packs the derived
  edit list as the descriptor prefix of the operation's body stream; the
  replacement bytes follow through the existing bounded `Source`. A version's
  replacement total is bounded by the file ceiling, and its recorded edit count
  by the explicit per-operation edit budget (4,096) the transport declares —
  the retired 256-edit and 8 MiB replay caps were removed together with that
  streaming transport (#245 package D), not by moving a ceiling elsewhere. The
  remaining bound is the one the file itself sets: the replacement extents of
  one call are still materialized before the splice, so a complete-file
  construction is bounded by the file, not by a frame.
- No continuously writable generation proof (package E) and no frozen
  performance selection (package F) exists.

## 7. Level-preserving rebuild and balanced packing (2026-09-26, #248 phase 2)

Source pin: `f74dbe77da12fa533587be8a578375bce3f19373` plus the change committed
with this note, which is the phase-2 extent-tree slice of
[#248](https://github.com/EasyAI-Lab/layerfs/issues/248). Two rules §2 and §3
state as landed were not true of the source they describe, and both broke a
file whose extent count needs more than one branch level.

**Balanced packing.** `pack` filled each branch greedily - 248 children, then
whatever is left - so a sequence of 249 leaf pages was packed as `[248, 1]`. The
page encoder refuses a branch with fewer than two children, so the splice
returned `Capacity` for a perfectly representable file: any leaf count congruent
to one modulo the fanout (249, 497, 745, …) could not be written at all.
`pack_level` now splits each level into its fewest chunks of nearly equal size
(249 into 125 and 124), which keeps every non-root branch at two children or
more at every level.

**One level per rebuilt frontier.** `descend` answered with its collected
children, one level below the node it replaced. For a root one level above the
leaves that is harmless: the collected pages are leaves and `pack` rebuilt the
level above them. For a taller tree it mixed the levels - an untouched sibling
subtree came back as a declared level-1 page while a folded leaf came back as a
declared level-0 page - and the rebuild relabelled everything from level 1. The
result was a tree whose declared levels did not match its depth, which the
cursor refuses (`Io`, "levels are relative"): a file of roughly 30,876 extents or
more became unreadable after one narrow write. `descend` now returns a `Level` -
pages that all declare the replaced node's own level - and a branch packs its
rebuilt children up to that level before answering its parent. The root adopts a
single returned page at its own level, or adds one level above several.

**The exact file bound.** `PieceRecord::parse` accepted an extent whose
`offset + length` reached `2^32 - 1` while the declared file maximum is exactly
`2^32 = 4 GiB`. The last record of a full 4 GiB base read therefore failed to
parse, so the whole sequence was unreadable at the maximum the format claims to
represent. The bound is now `MAX_FILE`. One base read of exactly 4 GiB
round-trips as 257 records and reads back byte-exact.

**Left open in this slice.** A fold that replaces an entire sibling subtree
with a single page left that child below its own level and was **refused** with
`Capacity` rather than publishing a tree whose children sit at two depths. [§8](#8-collapsed-node-lifting-2026-09-26-248-phase-2)
closes it by lifting the collapsed node instead.

**Counted evidence.** Through the page-format-identical in-memory store the
external tests use: 4,097 separated one-byte runs read back exactly and one
narrow splice reads 2 and writes 2 pages; 65,536 separated runs make 529 leaf
pages and one narrow splice reads 3 and writes 3 pages; 249 leaf pages pack into
branches every one of which holds at least two children. These are work counts
from the same traversal the mounted path uses, not a latency or capacity claim.

## 8. Collapsed-node lifting (2026-09-26, #248 phase 2)

Written against the source this section is committed with, on top of the
[§7](#7-level-preserving-rebuild-and-balanced-packing-2026-09-26-248-phase-2)
rebuild. It closes the `Capacity` refusal §7 records as still open. No
canonical Store format, file format version or public API changes.

**The refusal.** §7 makes `descend` answer at the level of the node it replaced,
which is what keeps every leaf at one depth. A node whose rebuilt children
number one or zero cannot answer at its own level: `pack_level` answers with the
single page it was given, one level below, and the parent refused the splice
with `Capacity` rather than hang a shallower page beside its siblings. That
happens when one fold replaces the whole content of a subtree — a large
truncation, a range rewrite covering a branch, or a coarse rewrite of a
fragmented file. It is reachable through ordinary POSIX calls and it refused a
file the format can represent.

**The repair.** `lift` answers one rebuilt inner node at its own declared level
(§2 now states the rule this needs):

| Rebuilt children | Answer |
| --- | --- |
| two or more | `pack_level` writes the branch pages above them, as before. |
| exactly one | that child is lifted inside one branch page of its own level: a branch with one child. |
| none | an empty level, so the parent drops the node. |

The lifted page replaces the node one for one, so a collapse neither adds a page
to the tree nor changes its height, and a repeated collapse over the same range
rewrites the same single page instead of stacking another level. The root is the
one node that does not lift: `replace` passes it down as the sequence's own
root, and a root that collapses to a single page simply adopts it and loses a
level.

Rebalancing the collapsed child against a sibling — borrowing one of the
sibling's children, or merging both into balanced nodes — also restores a
two-child shape, at the cost of reading and rewriting that sibling's subtree.
It cannot restore it at all when the tree holds three pages in total: two
children cannot be spread over three pages without leaving one node with one
child. Preserving the level is what the cursor, the encoder and every later
splice actually require, so the smaller repair is the one that holds for every
count.

**Counted evidence.** Through the page-format-identical in-memory store: a
two-level tree of 249 leaf pages folds its whole first level-1 child (125 leaf
pages, 15,500 bytes) into one page, which is lifted, and the tree stays
height-uniform with 125 leaves and the same 30,876 bytes readable in order; a
second fold over the same child keeps exactly those 125 leaves and one lifted
page rather than adding a level; deleting that whole child instead answers with
no page, drops the node and lets the root lose a level, leaving the untouched
124 leaf pages readable. Every case checks that each branch's children declare
exactly one level below their parent and that the earlier generation still reads
what it published. These are structural and work counts from the same traversal
the mounted path uses, not a capacity or latency claim.

## 9. Monotone cursor advance (2026-09-26, #248 phase 2)

Written against the source this section is committed with. It changes no page
format, no public API and no canonical Store representation: the same pages are
read in the same order, and the sequence a walk reports is identical.

**The cost this removes.** §4 promised `O(H + L)` page visits for a walk. The
cursor's `step_in` re-read every ancestor from the root each time it moved to
the following leaf, so a walk of `L` leaves paid `O(H)` reads per leaf — `O(H*L)`
over the tree — and a Commit lowering pass or a replacement transfer inherited
that factor. It is exactly the "reopened cursors reread ancestors" term the
joint study lists for the frozen file lowering.

**The rule.** A frame keeps what its parent told it: which child it selected,
where that child's subtree starts and how long it is. Steps follow from
remembered arithmetic:

| Event | Page reads |
| --- | --- |
| next leaf under the same branch | re-read that branch page, then read the leaf it enters |
| next child of a higher branch | re-read that one branch page, then one read per level the descent below it enters |
| leaf reached, no frame has another child | none: the walk is over |

Every page a walk enters is therefore read about once, plus one re-read for each
step inside it, which is bounded by the tree's own page count. A walk of the
65,536-extent shape (529 leaves, 533 pages, height 2) reads 1,061 pages; resuming
at its midpoint and walking the remaining 32,768 extents reads 532. Reading the
path from the root per leaf costs about three reads per leaf on the same shape
(~1,587), which the committed test refuses.

**Counted evidence.** Through the page-format-identical in-memory store the
external tests use: `one_walk_advances_without_rereading_the_path_above_each_leaf`
asserts the full walk stays within twice the tree's page count plus the first
descent, that the same bound holds for a walk resumed at the midpoint, and that
both return the sequence exactly, in order, with every byte accounted for. These
are page-visit counts from the same traversal the mounted path uses, not a
latency or throughput claim.
