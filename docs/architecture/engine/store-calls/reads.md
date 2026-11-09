# Reads

[Architecture](../../README.md) › [Engine](../README.md) › [Store calls](README.md) › Reads

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Reads, windows, search, blame and compare.

| | |
| --- | --- |
| Component | [Store calls](README.md) |
| Source | [`engine/crates/layerfs-store-service/src/read`](../../../../engine/crates/layerfs-store-service/src/read) |

Every read names a saved snapshot and reads immutable objects. Nothing in
`read/` reads a live Workspace
([`read/snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs)).

## Reads and windows

```text
name ("main" | Branch name | 66-hex Commit/Layer id)
  |
  v  snapshot::resolve                     published lookups via session 0
  +-- "main"      -> NodeId::Layer(stack.head_layer)        Kind::Main
  +-- 66 hex      -> NodeId::Commit | NodeId::Layer          Kind::Commit/Layer
  +-- Branch name -> head Commit, else base Layer            Kind::Branch
  |
  v  root(node) -> TreeTable::open(reader, root)
Snap { kind, label, node, root, tree }
  |
  v  every later step reads by immutable root only
fs.read / fs.stat / fs.list / fs.grep / fs.glob / fs.blame
```

| Call | What it answers | Bound | Source |
| --- | --- | --- | --- |
| `fs.read` | Lines `startLine`..`endLine` of one file, as one `text`; `nextLine` when cut. | At most `window::LINES` (2000) lines and `REPLY_BUDGET` (48 KiB). | [`read/reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |
| `fs.stat` | `type`, `inode`, `size`, `mode` of one path. | One entry. | same |
| `fs.list` | One page of a directory: `name`, `path`, `type`, `inode`. | `pageSize` and 48 KiB; entries read 64 at a time. | same |
| `fs.grep` | Lines holding a literal pattern under a path. | `pageSize` and 48 KiB; binary files skipped and counted (`skipped`). | same |
| `fs.glob` | Paths matching a `/.lww`-syntax glob. | `pageSize` and 48 KiB. | same |

Each answer starts with the same header: `kind`, `snapshot` (label such as
`feat@<short id>`), `root` and `path` (`snapshot::header`).

| Window rule | Value | Source |
| --- | --- | --- |
| Reply budget (`BYTES`) | 48 KiB | [`read/window.rs`](../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| Lines per window (`LINES`) | 2000 | same |
| Binary check window (`WINDOW`) | 16 KiB; a NUL or invalid UTF-8 ends the text at that window's start and fails `Binary` | same |
| Chunked-file hand-on block (`BLOCK`) | 256 KiB (16 windows), one decode pass per file | same |
| A file's size | From Store metadata (`file_lengths`), never from its payload | [`read/snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| Whole-text bound for compare | `MAXIMUM_TEXT_BYTES` = 1 MiB | [`compare/text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |

A long line is clipped, not dropped: `lineTruncated` says so, and the scan
keeps bounded buffers for any line length.

## Walk and search

```text
walk_within(cached, snap, path, within)        depth-first, name order
  Frame { path, entry, after, pending, exhausted }
    each directory read in windows of 64 names / 8 KiB
    glob: only folders that agree with walk::leading(pattern)
          e.g. "a/b/**/*.js" -> enter only a/, a/b/
  |
  v
grep: files collected in windows of ROOTS = 64
  Cached::roots(ids)   lengths first, then roots in demands of
                       <= 64 files and <= 4 MiB of root bytes
  search::matches      substring search over whole text parts
  search::Reuse        a content already scanned in this call is
                       answered from its kept lines (<= 48 KiB kept)
```

| Part | Rule | Source |
| --- | --- | --- |
| Walk | One memo per walk; directory windows of 64 entries and 8192 bytes; bounded by the window, not by tree size. | [`read/walk.rs`](../../../../engine/crates/layerfs-store-service/src/read/walk.rs) |
| Glob pruning | `walk::leading` gives the literal leading folders of an anchored pattern; other folders are not entered. | same |
| Grep pattern | A nonempty literal; a pattern with LF anywhere but last matches nothing. | [`read/search.rs`](../../../../engine/crates/layerfs-store-service/src/read/search.rs), [`methods/file/grep.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/grep.rs) |
| Grep root batches | `ROOTS` = 64 files, `ROOT_BYTES` = 4 MiB per demand; one larger file goes alone. | [`read/cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |
| Per-call caches | `Cached` keeps pack bodies, decoded groups and the decode arena for the call's later demands; dropped with the call. | same |

Paging is by offset (`pagePos`): earlier items are skipped by walking, so
skipping is linear in the offset
([`operator/args.rs`](../../../../engine/crates/layerfs-store-service/src/operator/args.rs)).

## Blame

```text
fs.blame(path, fromLine)
  tip = file at snapshot           (NotFile unless a regular file)
  tracks = lines from fromLine, <= 2000 lines, <= 48 KiB - 8 KiB
  loop over first parents (Commit -> parent, or Layer -> source/parent):
    older file absent or not regular  -> remaining lines owned here; stop
    same content root                 -> step back, nothing to decide
    both texts complete (<= window)   -> match_lines (line LCS)
    else (coarse)                     -> equal byte prefix/suffix only;
                                         middle lines owned here
  answer: lines[{n, text, commit, message}], versions, comparisonCoarse,
          nextLine
```

| Field | Meaning | Source |
| --- | --- | --- |
| `versions` | First-parent revisions visited. | [`read/blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |
| `comparisonCoarse` | True when some step used the byte prefix/suffix rule instead of a line match. | same |
| `nextLine` | First line not in this answer. | same |

The walk follows persisted first parents only (`operator::files::revision`):
a Commit's parent, or its base Layer when it has none; a Layer's source Commit,
else its parent Layer. It stops when every tracked line has an owner. The walk
has no fixed depth limit; Not established from source whether a caller bounds
it elsewhere.

## Compare

```text
fs.diff_files({left, right})        branch.diff_commits / diff_branches
  left resolved, then right            from, to resolved (from first)
  both text <= 1 MiB -> line_diff      same root -> empty page
  else -> same_content -> Equal|Binary plan = plans::find((left, right))
  hunks in 48 KiB, else hunksOmitted          else Plan::make(memo, hints)
                                              keep (LRU, PLANS = 4)
                                        ChangeWalk::planned | ::full
                                        emit: after pagePos or cursor,
                                              <= pageSize, <= 48 KiB
                                        -> items, nextPagePos, nextCursor
```

| Call | Sides accepted | Answer | Source |
| --- | --- | --- | --- |
| `fs.diff_files` | Two `{path, commit?}` selectors; `commit` defaults to the call's `branch`, else `main`. | `kind` `Equal`, `Text` or `Binary`; `contentEqual`, `sameInode`, `hunks`, `hunksOmitted`. | [`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `branch.diff_commits` | Two Commit identities; anything else is `InvalidInput`. | One page of path-level changes. | same |
| `branch.diff_branches` | Two Branch names or `main`. | One page of path-level changes. | same |

A directory whose only change is its children is not listed. Two regular
files with different roots but the same bytes count as unchanged. A page may
resume by offset or by `cursor` (the last path listed); a cursor skips pairs
at or before it and enters only directories on its path, so a later page does
not re-walk earlier ones. `hunks: false` lists paths without line hunks.

A plan (the changed entries of two roots) depends only on those immutable
roots, so up to `PLANS` = 4 are kept, least recently used first out. A
difference with more than `MAXIMUM_CHANGED` (65,536) changes, or without
recorded bindings, takes the full walk; that answer is kept as `None` too
([`read/plans.rs`](../../../../engine/crates/layerfs-store-service/src/read/plans.rs),
[`compare/changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs)).

## Snapshots and caching

| Cache | Lifetime | Holds | Source |
| --- | --- | --- | --- |
| `Memo` | One call (or one page) | Upper pages of inode and directory B-trees | [`read/reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |
| `Cached` (`ReadCache`) | One call | Pack bodies, decoded groups, decode arena | [`read/cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |
| `search::Reuse` | One grep call | Scan results by content identity, up to 48 KiB | [`read/search.rs`](../../../../engine/crates/layerfs-store-service/src/read/search.rs) |
| `plans::RECENT` | Process | Up to 4 difference plans by root pair | [`read/plans.rs`](../../../../engine/crates/layerfs-store-service/src/read/plans.rs) |
| `Store` canonical cache | Store lifetime | Shared immutable objects (64 KiB for a `StoreService`) | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |

All caches hold immutable, authenticated objects keyed by identity, so none can
show a stale head: the head is fixed once, when the snapshot is resolved.
Snapshot name lookups go through the control reader (session 0), which keeps
no cache of its own ([Service](service.md#read-handles)).

## Related

- [Service](service.md) — the read sessions these calls use.
- [SDK facade](sdk-facade.md) — the argument order for each read.
- [Core: Compare and merge](../../core/compare-and-merge/README.md) — `Plan`, `ChangeWalk`, line difference.
- [Store: Metadata and objects](../store/metadata-and-objects.md) — object reads and file lengths.

---

← [Store calls: Service](service.md) · [Up: Store calls](README.md) · [Store calls: Writes](writes.md) →

## Stateless saved-query continuation

`read/continuation.rs` encodes query and next-position values through the
existing Bridge Answer codec in an `lfq1` token. HistoryProvider authenticates
it with the existing authority capability, CatalogId and incarnation; the
engine verifies the tag before decoding, then compares every query field.
No key is exposed and no continuation state is stored. Payload admission is
4 KiB, leaving space for it alongside a result page.

Revisions bind saved start id, Branch, literal path, traversal and page size;
they retain the next immutable node and candidate index. Pinned comparisons
bind both ids, Branch contexts, path prefix, attribution/hunks and page size,
and retain the last delivered matching path. Resuming uses the existing
indexed path/name seeker or immutable parent traversal from that position.
It does not replay previous pages, while normal Capacity semantics remain.
Commit differences accept full Commit or Layer endpoints and keep the
existing changed-entry plan.
