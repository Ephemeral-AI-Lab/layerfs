# Reads

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How the read calls of [Store calls](README.md) answer from one saved, immutable snapshot within bounded windows and pages.

| | |
| --- | --- |
| Source | [`engine/crates/layerfs-store-service/src/read`](../../../../engine/crates/layerfs-store-service/src/read), [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs), [`history/pages.rs`](../../../../engine/crates/layerfs-store-service/src/history/pages.rs) |

Nothing in `read/` reads a live Workspace. A read fixes its head once, when
the snapshot resolves, and then reads only immutable objects under that
root, so a publish during the read cannot mix two heads.

## Snapshots

```text
name: "main" | Branch name | 66-hex Commit or Layer id
  |  snapshot::resolve           (published lookups via session 0)
  +-- "main"      -> Layer(stack.head_layer)          Kind::Main
  +-- 66 hex      -> Commit | Layer                   Kind::Commit/Layer
  +-- Branch name -> head Commit, else base Layer     Kind::Branch
  v
Snap { kind, label, node, root, tree }  --> every later step reads by root
```

| Rule | Detail | Source |
| --- | --- | --- |
| Header | Every answer starts with `kind`, `snapshot` (label such as `feat@<short id>`), `root`, `path`. | [`snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| Saved anchor | Every read also returns `at`: the full id, kind, originating Branch and root of the node it read. | [`paged_reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/paged_reads.rs), [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |
| Wrong-length id | A hex name of 8 or more characters that is no Branch and not 66 long is `InvalidInput`, not `NoSuchBranch`. | [`snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| Branch-qualified pin | `branch` plus `commit` resolves through `resolve_context`: the Commit must be on that Branch's first-parent history (or a publication Layer of `main`) within 10,000 steps, and in the same project. | [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |
| File size | From Store metadata (`file_lengths`), never from payload. | [`snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |

## Text windows

| Call | Answer | Bound | Source |
| --- | --- | --- | --- |
| `fs.read` | Lines `startLine`..`endLine` as one `text`; `nextLine` when cut. | 2000 lines (`LINES`), 48 KiB (`BYTES`), header space reserved first. | [`reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs), [`read_reply.rs`](../../../../engine/crates/layerfs-store-service/src/read/read_reply.rs) |
| `fs.stat` | `type`, `inode`, `size`, `mode`. | One entry. | [`reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |

| Window rule | Value | Source |
| --- | --- | --- |
| Binary check window | 16 KiB; NUL or invalid UTF-8 ends the text there and fails `Binary` | [`window.rs`](../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| Decode block | 256 KiB (16 windows), one decode pass per file | same |
| Long line | Clipped, flagged `lineTruncated`; buffers stay bounded for any line length | same |

## Walk and search

```text
walk_within(snap, path)          depth-first, name order, one memo per walk
  directory windows: 64 names / 8 KiB
  glob: enter only folders matching walk::leading(pattern)
  |
grep: files gathered in windows of ROOTS = 64
  Cached::roots   lengths first, then roots per demand (<= 64 files, 4 MiB)
  search::matches literal substring over whole text parts
  search::Reuse   content already scanned this call answered from kept lines
```

| Call or part | Rule | Source |
| --- | --- | --- |
| `fs.list` | One page of a directory: `name`, `path`, `type`, `inode`. | [`reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |
| `fs.glob` | Paths matching a `/.lww`-syntax glob; only folders on the pattern's literal lead are entered. | [`walk.rs`](../../../../engine/crates/layerfs-store-service/src/read/walk.rs) |
| `fs.grep` | Lines holding a nonempty literal; a pattern with LF anywhere but last matches nothing; binary files skipped and counted. | [`search.rs`](../../../../engine/crates/layerfs-store-service/src/read/search.rs) |
| Per-call cache | `Cached` keeps pack bodies, decoded groups and the decode arena for the call's later demands; dropped with the call. | [`cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |

## Numbered pages and continuations

```text
args.page present?  no  -> offset paging: pagePos skipped by walking
                    yes -> query::result_page { limit, skip, number }
                             (page cannot combine with pagePos or cursor)
  boundary for (node, mode, path, pattern, size, page) kept? -> seek there
  else walk from the start, counting matches; Work::step per candidate
  candidates > MAXIMUM_QUERY_WORK (10,000) -> Capacity, whole page refused
  answer > 48 KiB                           -> Capacity "reduce page_size"
  keep boundary for page + 1  (at most 128 kept; eviction changes cost only)
```

| Part | Rule | Source |
| --- | --- | --- |
| Result page | One-based `page`, `pageSize` 1..=1000 (default 50); a page counts matching results, never silently filtered candidates. | [`query.rs`](../../../../engine/crates/layerfs-store-service/src/read/query.rs) |
| Paged `fs.list`, `fs.grep`, `fs.glob` | Opt-in with `page`; resume from indexed name boundaries, not by re-walking earlier names. `project.list_branches` takes `page` too, over catalog offsets. | [`paged_reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/paged_reads.rs), [`paged_boundaries.rs`](../../../../engine/crates/layerfs-store-service/src/read/paged_boundaries.rs) |
| `fs.revision` | Pages of revisions where a literal path changed; `traversal` `provenance` (persisted first parents) or `branch` (Commit parent, else base Layer). | [`revision.rs`](../../../../engine/crates/layerfs-store-service/src/read/revision.rs), [`revision_pages.rs`](../../../../engine/crates/layerfs-store-service/src/read/revision_pages.rs) |
| `branch.commits` boundaries | At most 128 kept per process, keyed by Store, start, Branch, size and page. | [`history/pages.rs`](../../../../engine/crates/layerfs-store-service/src/history/pages.rs) |
| Continuation token | `lfq1:<payload>:<tag>`: query and position encoded with the bridge Answer codec, authenticated by the history catalog's `query_authenticator`. Decode checks the tag (constant-time), then that every query field matches. Payload at most 4 KiB; nothing is stored. | [`continuation.rs`](../../../../engine/crates/layerfs-store-service/src/read/continuation.rs) |

Boundary caches hold immutable positions keyed by immutable ids, so
eviction changes only how far a page walks, never what it answers.

## Blame and attribution

```text
fs.blame(path, fromLine, lineCount <= 2000)
  tip = regular file at snapshot (else NotFile)
  loop over persisted first parents, while lines lack an owner:
    versions budget (10,000) or bytes budget (64 MiB) spent -> stop, limited
    older file absent / not regular -> remaining lines owned here
    same content root               -> step back
    both texts complete             -> bounded line LCS
    else                            -> equal byte prefix/suffix only (coarse)
  answer: lines[{n, text, commit, message}], versions, comparisonCoarse,
          nextLine   (within 48 KiB)
```

| Part | Rule | Source |
| --- | --- | --- |
| First parents | A Commit's parent, or its base Layer; a Layer's source Commit, else its parent Layer. | [`blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs), [`operator/files.rs`](../../../../engine/crates/layerfs-store-service/src/operator/files.rs) |
| Coarse comparison | A middle too large for the bounded LCS becomes one removed/added block and is disclosed (`comparisonCoarse`). | [`blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs), [`line_alignment.rs`](../../../../engine/crates/layerfs-store-service/src/read/line_alignment.rs) |
| Attribution | `includeAttribution` on a comparison runs blame per changed file under one shared budget (10,000 steps, 64 MiB); byte equality is never taken as an owner. Exhaustion is reported as incomplete coverage or `Capacity`. | [`attribution.rs`](../../../../engine/crates/layerfs-store-service/src/read/attribution.rs) |

## Compare

```text
fs.diff_files {left, right}          branch.diff_commits / diff_branches
  each side resolved, same project     from, to resolved (from first)
  both text <= 1 MiB -> line_diff      same node -> empty page
  else Equal | Binary                  plan = plans::find(roots) or make
  hunks in 48 KiB, else hunksOmitted   walk changes after pagePos / cursor
                                       <= pageSize, <= 48 KiB
```

| Call | Sides | Answer | Source |
| --- | --- | --- | --- |
| `fs.diff_files` | Two `{path, commit?}` selectors (default the call's `branch`, else `main`); `allowMissing`, `includeAttribution`, `page` select the extended view with aligned rows. | `kind` `Equal`, `Text` or `Binary`; `contentEqual`, `sameInode`, `hunks`, `hunksOmitted`. | [`file_compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/file_compare.rs), [`compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `branch.diff_commits` | Two Commit or Layer ids. | One page of path-level changes. | [`compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `branch.diff_branches` | Two Branch names or `main`; with `fromCommit`, `toCommit`, `pathPrefix`, `includeAttribution`, `page` or `continuation` it becomes a pinned, numbered comparison. | Path-level changes, optional attribution. | [`branch_compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/branch_compare.rs), [`comparison_pages.rs`](../../../../engine/crates/layerfs-store-service/src/read/comparison_pages.rs) |

| Rule | Value | Source |
| --- | --- | --- |
| Whole-text bound for a line diff | `MAXIMUM_TEXT_BYTES` 1 MiB | [`compare/text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| Kept plans | `PLANS` = 4, least recently used out; a difference over `MAXIMUM_CHANGED` (65,536) or without recorded bindings takes the full walk, kept as `None` | [`plans.rs`](../../../../engine/crates/layerfs-store-service/src/read/plans.rs), [`compare/changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| Byte-equality payload per query | `CONTENT_QUERY_BYTES` 64 MiB; beyond it the answer is `Capacity`, never a guessed change | [`file_facts.rs`](../../../../engine/crates/layerfs-store-service/src/read/file_facts.rs) |
| Listing | A directory whose only change is its children is not listed; equal bytes under different roots count as unchanged; `hunks: false` lists paths only | [`compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| Cursor | The last listed path; a later page enters only directories on its path | same |

## Tree statistics

`project.stats` resolves a Branch-qualified pin once, then walks its tree
counting files, directories and symlinks (root excluded, links never
followed). The walk is bounded by `MAXIMUM_QUERY_WORK`; when it runs out the
answer says `complete: false` with a reason, while `logical` stays the
pin's recorded total
([`tree_stats.rs`](../../../../engine/crates/layerfs-store-service/src/read/tree_stats.rs)).

## Caches

| Cache | Lifetime | Holds | Source |
| --- | --- | --- | --- |
| `Memo` | One call or page | Upper pages of inode and directory B-trees | [`reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |
| `Cached` | One call | Pack bodies, decoded groups, decode arena | [`cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |
| `search::Reuse` | One grep call | Scan results by content identity, up to 48 KiB | [`search.rs`](../../../../engine/crates/layerfs-store-service/src/read/search.rs) |
| Difference plans | Process | Up to 4 by root pair | [`plans.rs`](../../../../engine/crates/layerfs-store-service/src/read/plans.rs) |
| Page boundaries | Process | Up to 128 per kind, held by a weak Store reference | [`paged_boundaries.rs`](../../../../engine/crates/layerfs-store-service/src/read/paged_boundaries.rs), [`comparison_pages.rs`](../../../../engine/crates/layerfs-store-service/src/read/comparison_pages.rs) |

Every cache holds immutable objects or positions keyed by identity, so none
can show a stale head.

## Related

- [Store calls](README.md): the service, read sessions and method table.
- [Core: Compare and merge](../../core/compare-and-merge/README.md): plans, change walks, line difference.
- [Core: Physical storage](../../core/physical-storage/README.md): object reads and file lengths.
- [API: SDK](../../api/sdk/README.md): the public read calls.
