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
Snap { kind, label, node, root, tree, serial }  --> every later step reads by root
```

The resolve reads the node's Commit or Layer row once: the ownership check
and the root come from the same record. A read call (`stat`, `read`, `list`,
`glob`, `grep`, `blame`, through `snapshot_full`) also keeps that record and
reads the node's `history_serial` together with the project's
`project.remove_file` markers in one statement (`node_path_removals`) where
the serial alone was read (#818 R2). The record builds the answer's `at`
without a second read; the markers reach the visibility filter at the place
the marker read used to be, so its failure still refuses there. A node with
no row, or a provider without the combined read, reads the markers alone.

A read naming `main`, a valid Branch name or a full Commit or Layer identity
reads its history context with one catalog call, `read_context` (#818 R5):
the project's stack, the Branch, the node's record and, on PostgreSQL, the
node's serial, the project's markers and a Commit's recorded Branch, all in
one fenced statement (`read_context_named`, `read_context_node`). The store
service decodes it in the order the reads ran one by one: `NoSuchProject`,
then the Branch (with the identity-shaped rule), `NoSuchCommit`/`NotFound`,
another project's node, the root's tree, then the path. The markers (with
the serial) and the recorded Branch are each decoded into their own result,
so a row that does not decode refuses where the reads one by one meet it:
the markers at the marker step after the path, the recorded Branch where
the answer's `at` reads it (read again there). A failure of the statement
itself refuses before the path, as the project read's did. SQLite's provider
answers with the default, `read_context_serial`: the stack, Branch and node
reads one after another, with the markers and the recorded Branch read later
where they always were, so its statements do not change. A `commit` prefix,
an invalid name or an identity that does not parse keeps the reads one by
one.

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
| Per-call cache | `Cached` keeps pack bodies, decoded groups and the decode arena for the call's later demands; dropped with the call. The five reads build it before the snapshot resolves, so the root's tree opens through it, and file sizes and grep's lengths read through it too (`lengths_with`, #818 R4). | [`cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |
| One path walk | `fs.stat`, `fs.read`, `fs.list`, `fs.glob` and `fs.grep` build the call's `Cached` and one `Memo` first and walk the path once, strictly, through them (#818 R3): a symlink component or a read failure refuses there, a hidden entry refuses `Removed`, and the entry found is what the read, listing or walk starts from. An absent path is refused `NoSuchPath` by the read itself, after the arguments it reads first. The marker lookups of `removed_count` go through the same memo; the paged forms keep their own walk. | [`visibility.rs`](../../../../engine/crates/layerfs-store-service/src/read/visibility.rs), [`reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |

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
| Rows read once | A blame, revision or `fs.list_revisions` walk reads its rows through one per-call `Rows` (#818 H1): the last 8 Commit/Layer rows are held, so a node read as a parent is not read again as the next node or for a `ref`, `comparedTo` or record; each Branch name is read once per call on the control reader (it was read on the writer's session), at most 1,024 names kept; a name met once the memo is full is read each time. Where the Store batches reads (PostgreSQL), blame's and `fs.revision`'s walks read their parent Commits with each one's serial in windows of 16, 32, 64 then 128, never more than the walk's remaining work bound (`first_parent_window`, #818 H4); a window's row that failed to read refuses only when the walk reaches it, and a node outside the window is read alone as before. SQLite reads no window. Blame takes the resolve's row and recorded Branch, keeps each owner's `ref` from the row held when the owner was assigned, and carries the older text of one comparison to the next. | [`history/rows.rs`](../../../../engine/crates/layerfs-store-service/src/history/rows.rs) |
| One walk per node | `fs.revision` and `fs.list_revisions` read a node's path entry (and, in `fs.revision`, its serial when the entry exists) once: a node's parent side is the next node's own and is carried to it (#818 H2). A hidden parent ends the walk, so nothing is carried past it. | [`revision.rs`](../../../../engine/crates/layerfs-store-service/src/read/revision.rs), [`operator/files.rs`](../../../../engine/crates/layerfs-store-service/src/operator/files.rs) |
| One cache across versions | `fs.blame`, `fs.revision`, `fs.diff_files`, `fs.list_revisions` and `fs.get_storage` read every version's objects through one call `Cached` (roots, inode and directory pages, file lengths, text windows, byte ranges and comparisons; 3 MiB bound) and bind path names through its directory memo, `(directory root, name) → serial`, at most 4,096 names: a directory root's contents never change, so a name bound once is bound in every version holding that root (#818 H3). `fs.diff_files` keeps its strict lookup (`raw`) instead of walking again, and its attribution's blame pages read through the same cache; a branch comparison's owners share one cache per path. | [`cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |
| Version windows | Where the Store pays a round trip per demand and its reader serves per-object reads at once (`reads_each_at_once`), blame's and `fs.revision`'s walks look their path up in windows of the versions their row window holds: 16, 32, 64 then 128 versions resolved level by level together (`PathAcross`: one roots demand, one per inode level, one per directory page level of the memo's misses; #818 H5). A window reads ahead through per-object reads (`read_canonical_each`): an object that is missing, mismatched or undecodable is recorded by the call's caches and never read again, and the window's versions are then walked alone as the walk reaches them, so the failure refuses only at a version the walk reads, as before; a failure of a demand itself refuses at once. A strict-walk refusal (intermediate symlink, unbound component) is reproduced by walking that version alone. SQLite, and readers without per-object demands, walk each version alone. | [`versions.rs`](../../../../engine/crates/layerfs-store-service/src/read/versions.rs), [`cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs) |
| Reads planned ahead | Under the same condition (#818 H6), a version window also reads ahead what the walk reads at its versions, through per-object demands: its regular files' lengths in one demand (`lengths_each_with`); for blame, the roots of the older texts it compares, in walk order while each comparison's `(newer + older) * 3` bytes fit the remaining budget, at most 64 roots and 4 MiB of root bytes in one demand; for `fs.revision`, its entries' attribute-tree roots. `fs.diff_files` locates both sides, then reads both sides' lengths and text roots (within the text bound) together; the left side's text is still read, and the right side's refusal still met, in that order. The call's caches keep each length or file's own failure, and hold the roots read ahead (at most 4 MiB, released by the next plan); a demand naming only kept lengths or held roots is answered from them, and a kept failure refuses where the call first asks for that file. Each comparison's `equal_edges` stays per pair, and the content-comparison budget is charged in the same order. | [`versions.rs`](../../../../engine/crates/layerfs-store-service/src/read/versions.rs), [`cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs), [`file_compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/file_compare.rs) |

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
