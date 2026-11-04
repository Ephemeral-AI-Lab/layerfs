# Merge

[Architecture](../../README.md) › [Core](../README.md) › [Compare and merge](README.md) › Merge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Merging two lines of history into one tree.

| | |
| --- | --- |
| Component | [Compare and merge](README.md) |
| Source | [`core/crates/layerfs-content/src/merge`](../../../../core/crates/layerfs-content/src/merge) |

## Merge decisions

`merge_roots` merges `ours` and `theirs` against their common `base`
([`entry.rs`](../../../../core/crates/layerfs-content/src/merge/entry.rs)). Trivial cases return at once:

| Roots | Result |
| --- | --- |
| `ours == base` or `ours == theirs` | `theirs`, nothing built |
| `theirs == base` | `ours`, nothing built |
| ours and theirs in different inode scopes | `InvalidRecord("merge scope")` |

Otherwise the merge works **by serial** ([`serial.rs`](../../../../core/crates/layerfs-content/src/merge/serial.rs)): it
walks only the `from` side's changes from the base (`inode_changes(base,
from)`) and reads each changed serial's value on the `into` side.
`MergeInput::decide_from` picks `from`; the other side is `into`, the tree
the result is built on.

```text
 inode_changes(base, from), 256 serials per window
        |
        v  for serial s:  b = base(s), f = from(s), i = into(s)
   i == f ------------------------------> nothing to do
   f is a directory --------------------> merge its names (name points)
   f, i both present, i != b -----------> decide(b, f, i) at s's path
   i == b (only from changed) ----------> take f
   b, f present, i absent --------------> defer (into deleted it)
   b, i present, f absent, i != b ------> deleted_and_changed
```

When both sides changed one entry, `Decider::decide` picks the result. An
entry is its kind plus, for a file or symlink, its content; two directories
are the same entry and their children decide
([`decide.rs`](../../../../core/crates/layerfs-content/src/merge/decide.rs)). Rules apply in this order:

| # | Case (b = base, o = ours, t = theirs) | Result | Conflict |
| --- | --- | --- | --- |
| 1 | `o` same as `t`, or `o` same as `b` | theirs | — |
| 2 | `t` same as `b` | ours (or absent) | — |
| 3 | a `/.lww` rule matches the path | side with newer mtime, ours on a tie | — (`LwwDecision`) |
| 4 | `o`, `t` regular files; base, ours, theirs all bounded text | `merge3` text | `content` (base present) or `add/add` (no base) when a hunk remains |
| 5 | `o`, `t` regular files; base, ours or theirs not bounded text | ours | `binary` |
| 6 | one side a non-directory, the other absent | the kept side, with deletion markers if text | `modify/delete` (ours kept) or `delete/modify` |
| 7 | anything else (kinds disagree) | ours if present, else theirs | `type` |

"Same" for regular files means the same bytes (`same_content`), not the same
root. In a bounded merge (the publication lane) bytes are compared only up
to `MAXIMUM_TEXT_BYTES`; larger files compare by root, so the lane's hold
does not grow with file size.

## Last writer wins

`/.lww` holds gitignore-style patterns ([`lww.rs`](../../../../core/crates/layerfs-content/src/merge/lww.rs)). The rules
come from one side: `RulesFrom::Ours` for `branch.merge`,
`RulesFrom::Theirs` (the Branch head) for merge-on-commit.

| Syntax | Meaning |
| --- | --- |
| `#` | comment |
| `!pattern` | negates: clears an earlier match |
| trailing `/` | ignored |
| pattern containing `/` | anchored at the root |
| `*`, `?` | within one name |
| `**` | spans names; `**/` at a name boundary is zero or more directories |
| `%include <file>` | reads another file of the same tree in place, at most 16 files |

The last matching rule decides. A path both sides changed that a rule matches
takes the side whose entry has the newer modification time, ours on a tie. A
deleted path takes the time of its nearest surviving parent directory.

## Places and names

A move follows its serial: an edit inside a directory one side moved lands
at the new path. Two record-driven parts handle structure.

```text
  directory d (serial 12) changed on from
    base listing   from listing   into listing
    a -> 20        a -> 20        a -> 20
    b -> 21        (gone)         b -> 21      into kept b: take from
    c -> 22        c -> 30        c -> 31      both rebound c: decide
                   n -> 40        n -> 41      both added n: decide
                                               (two new dirs: absorb)
```

| Part | Rule | Source |
| --- | --- | --- |
| Name points | At each name `from`'s listing changed: `into` already binds what `from` binds, nothing; `into` binds what the base bound, take `from`'s binding; both rebound, decide by the entries. Two new directories at one name merge by name into `into`'s. | [`names.rs`](../../../../core/crates/layerfs-content/src/merge/names.rs) |
| Survive | A serial `from` deleted that the merge keeps keeps its binding, and each parent `from` also deleted survives with only what it keeps. | [`places.rs`](../../../../core/crates/layerfs-content/src/merge/places.rs) |
| Revive | A serial `into` lacks that the merge keeps is bound where `from` holds it; missing parents are revived from `from`. | [`places.rs`](../../../../core/crates/layerfs-content/src/merge/places.rs) |
| Resurrect | A serial `from` bound that `into` deleted comes back with its whole subtree as `from` holds it. | [`names.rs`](../../../../core/crates/layerfs-content/src/merge/names.rs) |
| Moved directory | A directory both sides moved to different places, or moved inside itself, is a `type` conflict and `from`'s move is cancelled. | [`places.rs`](../../../../core/crates/layerfs-content/src/merge/places.rs) `moved` |

Paths are named only where a rule needs one, from `BindingHints` when given,
else by breadth-first `locate` ([Compare](compare.md#named-and-sorted-comparison)).

## Metadata

A serial both sides changed merges mode and mtime field by field
([`metadata.rs`](../../../../core/crates/layerfs-content/src/merge/metadata.rs)):

| Field changed by | Result |
| --- | --- |
| neither / only `into` | `into`'s value |
| only `from` | `from`'s value |
| both | the decided entry's value |

Modes changed differently on both sides of a non-directory are a `mode`
conflict. Metadata is compared decoded, so root and inline forms of equal
metadata are no change. A stored form that already holds the merged fields
is reused; otherwise a new form is written in the decided entry's form.

## Conflict markers

`merge3` is a three-way line merge ([`text.rs`](../../../../core/crates/layerfs-content/src/merge/text.rs)). Base lines
matched on both sides (`match_lines`) split the texts into chunks.

```text
 chunk changed on one side      -> that side's lines
 chunk changed the same on both -> those lines
 chunk changed differently      -> marker block, reported as a Hunk:

   line N   <<<<<<< ours-label        Hunk.line = N (1-based)
            ...our lines...           Hunk.ours
            =======
            ...their lines...         Hunk.theirs
            >>>>>>> theirs-label

 deletion (ours kept)           deletion (theirs kept)
   <<<<<<< ours-label             <<<<<<< ours-label (deleted)
   ...text...                     =======
   =======                        ...text...
   >>>>>>> theirs-label (deleted) >>>>>>> theirs-label
```

| Item | Value | Source |
| --- | --- | --- |
| Labels | `branch.merge`: `<left>@<commit>` and `<right>@<commit>`; merge-on-commit: `workspace` and `<branch>@<head>` | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`store/commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| `has_markers` | true when a text holds an opening line, a later `=======` line and a later closing line | [`text.rs`](../../../../core/crates/layerfs-content/src/merge/text.rs) |
| `paths_with_markers` | named paths that are bounded text files with a complete block; merge-on-commit refuses a Commit while a conflicted path still has one | [`markers.rs`](../../../../core/crates/layerfs-content/src/merge/markers.rs) |

A merged or marker file takes the kind and metadata of the side it came
from (`like`) and is constructed with `construct_bytes` into the merge's
consumer.

## Building the result

```text
 merge.decide  pass 1: decide each changed serial, record decisions
      |
 merge.places  moved directories, deferred serials, survivors
      |
 merge.plan    pass 2: write directory rows and values from records
      |
 merge.update  update_filesystem_streamed_backed(into, plan rows)
      v
   MergeOutcome { root, conflicts, lww, decided, work, hinted, naming }
```

| Part | Rule | Source |
| --- | --- | --- |
| Plan | Values, fresh serials and changed names as point records keyed by serial and name. | [`build.rs`](../../../../core/crates/layerfs-content/src/merge/build.rs), [`rows.rs`](../../../../core/crates/layerfs-content/src/merge/rows.rs) |
| Fresh serials | From the caller's `fresh` callback; a file value keeps its source serial only when that cannot give one serial two values. | [`build.rs`](../../../../core/crates/layerfs-content/src/merge/build.rs) |
| Spill | Runs of at most 60 KiB chunks; up to 8 MiB held in memory, then the caller's `IndexedConstructionBacking` records. Without records a `Resident` store is used. | [`spill.rs`](../../../../core/crates/layerfs-content/src/merge/spill.rs) |
| Result | One backed update of `into`, the same canonical producer and route as a Workspace Commit. No plan rows: `into`'s root unchanged. | [`build.rs`](../../../../core/crates/layerfs-content/src/merge/build.rs) |
| Outcome | Conflicts and `.lww` decisions sorted by path; `clean()` is true when there are no conflicts. | [`entry.rs`](../../../../core/crates/layerfs-content/src/merge/entry.rs), [`types.rs`](../../../../core/crates/layerfs-content/src/merge/types.rs) |

Callers in the engine:

| Caller | `decide_from` | `rules_from` | `bound` | records |
| --- | --- | --- | --- | --- |
| `branch.merge` | `Ours` | `Ours` | none | merge records |
| Commit merge before the lane (`merge_candidate`) | `Ours` | `Theirs` | none | merge records |
| Commit merge in the publication lane (`merge_in_lane`) | `Theirs` | `Theirs` | `LANE_MERGE_PATHS` (1,024) | none |

## Revert

Merge units let `branch.merge` take a group of paths whole from one side
([`units.rs`](../../../../core/crates/layerfs-content/src/merge/units.rs), [`revert.rs`](../../../../core/crates/layerfs-content/src/merge/revert.rs)).

```text
 Units::new(patterns)        reject empty names, duplicates, comments,
        |                    "//", control characters, all-negated units
 unit_changes(base, ours)    per unit: changed paths, latest mtime
 unit_changes(base, theirs)
        |
 decide_units                unit both sides changed: later side wins,
        |                    ours on a tie
 revert_units(loser)         loser's paths in the unit bound as in base
        |
 merge_roots(base, ours', theirs')   no text merge or conflict inside
                                     a decided unit
```

| Rule | Source |
| --- | --- |
| A unit covers every path one of its patterns matches and everything below; the first listed unit wins; the root is in none. | [`units.rs`](../../../../core/crates/layerfs-content/src/merge/units.rs) `unit_of` |
| A path is changed when added, deleted, or its kind, bytes or metadata differ; a new serial with the same kind, bytes and metadata is not a change. | [`units.rs`](../../../../core/crates/layerfs-content/src/merge/units.rs) `changed` |
| The revert binds, at every path of a lost unit, what the base binds there. A base directory the side moved elsewhere is rebuilt under a fresh serial; a restored file the side also binds elsewhere keeps one serial and counts as a cross link. | [`revert.rs`](../../../../core/crates/layerfs-content/src/merge/revert.rs) |
| State per unit and side is a count and a time, never a path list. | [`units.rs`](../../../../core/crates/layerfs-content/src/merge/units.rs) |

The engine wires units in [`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs).

## Related

- [Compare](compare.md): the comparisons a merge is built on.
- [Filesystem trees: Updates](../filesystem-trees/updates.md): the backed
  update that builds the merged root.
- [Files: Construction](../files/construction.md): `construct_bytes` for
  merged and marker files.
- [Engine: Commit, publish and merge](../../engine/commit/publish-and-merge.md)
  and [Engine: Store calls, Branches](../../engine/store-calls/branches.md):
  the callers.
- [Fork and merge](../../flows/merge.md): the flow across layers.
- [Glossary](../../glossary.md)

---

← [Compare and merge: Compare](compare.md) · [Up: Compare and merge](README.md) · [Core: Physical storage](../physical-storage/README.md) →
