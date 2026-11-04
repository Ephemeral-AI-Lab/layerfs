# Edits

[Architecture](../../README.md) › [Core](../README.md) › [Files](README.md) › Edits

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Changing part of a file without rewriting all of it.

| | |
| --- | --- |
| Component | [Files](README.md) |
| Source | [`core/crates/layerfs-content/src/file/edit`](../../../../core/crates/layerfs-content/src/file/edit) |

## Edit model

A known edit is an immutable base root plus an ordered list of replacements.
Each `Edit` replaces `start..end` with `replacement_len` bytes, in
**current-result coordinates**: each edit addresses the result of every earlier
edit in the same list.

```text
 base (len 100)    0          30   40                      100
                   |-----------|####|-----------------------|
 edit 0            Edit::new(30, 40, 5)     replace 10 bytes by 5
 after edit 0      0          30  35                       95
                   |-----------|++|-------------------------|
 edit 1            Edit::insert(60, 8)      60 is in the 95-byte result
 after edit 1      0          30  35        60      68      103
                   |-----------|++|---------|+++++++|-------|
```

| Type or trait | Role | Source |
| --- | --- | --- |
| `Edit` | one replacement; `new`, `overwrite`, `insert`, `delete` | [`input.rs`](../../../../core/crates/layerfs-content/src/file/edit/input.rs) |
| `EditSequence` | replayable ordered list: `base_len`, `final_len`, `len`, `edit_at(i)` | same |
| `EditSource` | replacement bytes by index and offset; `read_run_at` may return `FileRun::Zero` | same |
| `IndexedEditSource` | the same, with fallible length lookup | same |
| `EditRequest` / `IndexedEditRequest` | base root (or an open `FileView`), edits and source | [`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |
| `ConstructedFile` | result root, logical length and `EditCounters` | [`bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs) |

Rules of the list:

| Rule | Enforced by |
| --- | --- |
| Edits are never reordered or merged; their boundaries decide the chunk layout of the result. | [`input.rs`](../../../../core/crates/layerfs-content/src/file/edit/input.rs) |
| An edit may not start inside bytes an earlier edit in the same list introduced. | `Plan::advance` refuses it as `InvalidEdit { what: "coordinate before current result" }`; the `EditSequence` implementation is expected to validate the list first |
| An edit may not reach past the base. | `Plan::advance`: `InvalidEdit { what: "range beyond base length" }` |
| The base length must match `EditSequence::base_len`. | `apply_view`: `InvalidEdit { what: "declared base length" }` |
| A replacement must deliver exactly its declared length. | [`runs.rs`](../../../../core/crates/layerfs-content/src/file/edit/runs.rs): `InvalidEdit { what: "replacement length" }` |

**What the result equals.** A whole-file or empty result has the root that
complete construction of the same bytes gives. A chunked result keeps the
untouched extents of the base and chunks only the replacement, so its partition
and root depend on the edit boundaries. They are not, in general, the root a
fresh construction of the same bytes would give
([`edit_model.rs`](../../../../core/crates/layerfs-content/tests/edit_model.rs),
[`edit_reference.rs`](../../../../core/crates/layerfs-content/tests/edit_reference.rs)).

## Edit plans

`apply_edits`, `apply_edits_backed` and `apply_indexed_edits_view_backed` all
run one driver, `apply_view`
([`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs)).
The result's length, not the base's, picks the route.

```text
 open base view (one root read)
        |
        v
 no edits? ---------------------------------------------> return base root
        |
        v
 compare pass: every replacement equal to its base range? -> return base root
        |  (64 KiB windows, stops at first difference)
        v
 representation of final_len
        |
   +----+--------------------+--------------------------+
   | Empty                   | WholeFile                | Chunked
   v                         v                          v
 emit empty leaf      assemble kept ranges +     base chunked?
 + FileState          replacements into one       |            |
                      pre-sized object; hint     yes           no
                      base as OriginalBase        |            |
                                                  v            v
                                          stored-node     rescan whole result
                                          split/concat    (PlanRuns -> chunker)
```

`Plan` walks the edit list lazily and yields `Segment::Retain { base }` and
`Segment::Replace { index, base, len }` in order. It holds no copy of the list,
and its base cursor only moves forward. A replaced base range is never read.

### Stored-node split and concatenate

For a chunked base and chunked result, each edit is four operations on
summaries ([`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs)):

```text
 base extents      | e1 | e2 | e3 | e4 | e5 | e6 | e7 |
 edit                        |<-- replaced -->|
                          split at start   split at end

 1. split(start)   left  = | e1 | e2 |e3a|
 2. split(removed) gone  =               |e3b| e4 |e5a|    discarded, unread
                   right =                           |e5b| e6 | e7 |
 3. scan replacement       | n1 | n2 |                    new chunks
 4. concat         | e1 | e2 |e3a| n1 | n2 |e5b| e6 | e7 |

 e1, e2, e6, e7 keep their IDs; e3a and e5b are slices of the stored
 chunks e3 and e5 (only offset and length change); n1, n2 are new.
```

```text
 page view of the same edit

 BEFORE   FileState --> branch B
                        /    |    \
                  leaf L1  leaf L2  leaf L3
                           (e3..e5)

 AFTER    FileState' --> branch B'
                        /    |     \
                  leaf L1  leaf L2'  leaf L3      L1 and L3: same IDs,
                                                  never read
```

| Step | Source |
| --- | --- |
| Split cuts one stored extent at a boundary; the payload ID stays, offset and length move. | [`split.rs`](../../../../core/crates/layerfs-content/src/file/edit/split.rs), [`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs) |
| The removed middle is discarded before it is encoded. | `apply.rs` (`tree::discard`) |
| The replacement is chunked by the same scanner and builder complete construction uses; each new chunk gets an advisory predecessor from the `Aligner`. | [`chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs), [Chunking](chunking.md#predecessor-based-chunk-deltas) |
| Concatenation rebuilds only the pages along the two join boundaries; contiguous slices of one payload are coalesced. | `tree.rs`, [`concat.rs`](../../../../core/crates/layerfs-content/src/file/edit/concat.rs) |
| After each edit, drafts the result no longer reaches are released (`settle`). | [`objects.rs`](../../../../core/crates/layerfs-content/src/file/edit/objects.rs) |
| At the end, only pages the final root reaches are emitted, children first, then the file state. | [`finish.rs`](../../../../core/crates/layerfs-content/src/file/edit/finish.rs) |

## Runs and batches

**Runs.** A replacement source may return `FileRun::Zero(n)` from
`read_run_at`, so a replacement can contain a hole without its bytes. The
default `read_run_at` wraps `read_at`, so byte-only sources keep working
([`input.rs`](../../../../core/crates/layerfs-content/src/file/edit/input.rs)).
Zero runs then go through the same repeated-chunk path as complete
construction ([Chunking](chunking.md#zero-runs)). The comparison pass checks
zero runs against stored ranges without reading payloads where it can
([`zero.rs`](../../../../core/crates/layerfs-content/src/file/edit/zero.rs)).

**Batches.** One request applies any number of edits in one pass. The
unfinished frontier lives in one of two state profiles
([`state.rs`](../../../../core/crates/layerfs-content/src/file/edit/state.rs)):

| Profile | Entry point | Where drafts live | Bound | Source |
| --- | --- | --- | --- | --- |
| Memory | `apply_edits` | decoded drafts in process memory | charge of at most `EDIT_DEFERRED_LIMIT` (8 MiB - 1), refused as `BoundedCapacityExceeded { what: "edit.deferred_nodes" }` | [`objects.rs`](../../../../core/crates/layerfs-content/src/file/edit/objects.rs), [`engine.rs`](../../../../core/crates/layerfs-content/src/file/edit/engine.rs) |
| Backed | `apply_edits_backed`, `apply_indexed_edits_view_backed` | raw records in a caller-owned `IndexedEditBacking` | each record at most 64 KiB; `get_many` asks for at most 64 keys in one 64 KiB envelope | [`backing.rs`](../../../../core/crates/layerfs-content/src/file/edit/backing.rs), `state.rs` |

The backing trait is plain keyed records with guarded batches
(`EditRecordChange` with an expected pre-state, `EditRecordApply::Applied` or
`NotApplied`). Content defines no database type; the engine's Commit supplies
the backing ([`captured/owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs)).
A failure is returned once and the editor becomes terminal; nothing retries.

## Cost and scaling

| Work | Grows with | Source |
| --- | --- | --- |
| Mapping pages read | the paths from the root to each edit boundary | [`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs) |
| Payloads read | the replaced base ranges of same-length replacements, read by the compare pass up to the first difference; a split moves offsets and reads no payload | [`compare.rs`](../../../../core/crates/layerfs-content/src/file/edit/compare.rs), [`split.rs`](../../../../core/crates/layerfs-content/src/file/edit/split.rs) |
| Payloads written | the replacement bytes | [`chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs) |
| Pages emitted | pages on the join paths that changed | [`finish.rs`](../../../../core/crates/layerfs-content/src/file/edit/finish.rs) |
| Untouched subtrees | nothing: kept by ID, never read | `tree.rs` |
| A page two passes need | one read for the whole operation (shared `PageCache`) | [`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |

So a localized edit to a chunked file costs about the same on a large file as on
a small one. [`edit_localized.rs`](../../../../core/crates/layerfs-content/tests/edit_localized.rs)
records the object counts for one overwrite of three whole extents: 3 pages
read on an 8 MB file with 5 mapping pages, 4 pages read on a 24 MB file with
12, and the same 3 payloads read in both.

Routes whose cost follows the result size, not the edit:

| Case | Cost | Why |
| --- | --- | --- |
| Whole-file result | reads every kept byte (at most the cutoff) | the result is one object |
| Whole-file base, chunked result | rescans the whole result | there is no stored mapping to keep |
| Compare pass | reads base and replacement bytes in 64 KiB windows up to the first difference | it is how a no-op returns the base root |

## Related

- [Construction](construction.md): the builder and representations edits reuse.
- [Chunking](chunking.md): scanning the replacement and choosing hints.
- [Reads and views](reads-and-views.md): `FileView`, `RangeCursor` and
  `PageCache`, which edits read through.
- [Engine: Commit](../../engine/commit/README.md): the caller that turns
  captured Workspace changes into edits.
- [Glossary](../../glossary.md).

---

← [Files: Construction](construction.md) · [Up: Files](README.md) · [Files: Reads and views](reads-and-views.md) →
