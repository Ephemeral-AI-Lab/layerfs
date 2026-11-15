# branch family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `branch` calls: list and read a project's Branches and their saved history, fork, merge, publish to `main`, and compare two Branches, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/branch`](../../../../api/crates/layerfs-api/src/branch), [`layerfs-store/src/calls/branch.rs`](../../../../control/crates/layerfs-store/src/calls/branch.rs), [`layerfs-store-service/src/methods/branch`](../../../../engine/crates/layerfs-store-service/src/methods/branch), [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |
| Runs by default | where the Store's calls run: the caller's process for PostgreSQL and for sqlite on a locally executing machine (`native`, except on Windows); else the Store machine's worker (`store.call`). No daemon serves a Store call |
| Design | [Store calls](../../engine/store-calls/README.md), [reads](../../engine/store-calls/reads.md), [History](../../core/history/README.md), [merge flow](../../flows/merge.md) |

## Calls

```text
                 main (the project's line of Layers)
                   |
   list / get      | fork <name> main         fork <name> <branch> [commit]
   commits         v                                   |
   get_commit    agent  ---- file.* writes,            v
                   |         workspace.commit -->  review
                   +------ diff <from> <to> -----------+
                   +------ merge <new> <a> <b> --------+--> joined
                   main <---------- publish <branch> -------+
                   (next Layer; overwritten Layers reported)
```

| Call | Store method | Kind |
| --- | --- | --- |
| [`branch.list`](#branchlist) | `project.list_branches` | read |
| [`branch.get`](#branchget) | `project.list_branches` with `name` | read |
| [`branch.commits`](#branchcommits) | `branch.commits` | saved read |
| [`branch.get_commit`](#branchget_commit) | `branch.get_commit` | saved read |
| [`branch.fork`](#branchfork) | `branch.fork` | write |
| [`branch.merge`](#branchmerge) | `branch.merge` | write |
| [`branch.publish`](#branchpublish) | `branch.publish` | write |
| [`branch.diff`](#branchdiff) | `branch.diff_branches` | saved read |

Each entry forwards a typed request to `layerfs_store::calls::branch`, which
renames the table's snake_case fields to the Store method's
([`calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs)) and
selects one method of the engine's SDK facade
([`methods/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs)).
`main` is the project's line of publication Layers: its saved versions are
Layers, a side Branch's are Commits. Reads use the read-only session and
never take the publication lane; a write takes it only for its final
publish step.

Under a folder scope, Branch names are composed as `<namespace>.<name>`,
a scoped `branch.list` and `branch.get` answer namespace-relative names, and
`branch.commits`, `branch.get_commit` and `branch.diff` are refused
(`out of scope: history`)
([`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs)).

### branch.list

A project's Branches, `main` first, one page at a time.

```text
JS      await layerfs.branch.list(project, { page_size, page_pos, page })
Python  layerfs.branch.list(project, *, page_size=None, page_pos=None, page=None)
CLI     layerfs branch list <project> [--page-size <page-size>] [--page-pos <page-pos>] [--page <page>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `page_size` option | Int | 1..=1000, default 50 |
| `page_pos` option | Int | legacy offset: the last page's `next_page_pos` |
| `page` option | Int | one-based numbered page; not combined with `page_pos` |

```text
{ items: [
    { name: "main", id: null, base: <head Layer>, head: null, saved_head },
    { name, id, base, head | null, forked_from | null, saved_head }, ...
  ],
  has_more, next_page_pos | null, consistency: "live" }
```

Items after `main` are in byte-name order; `saved_head` is a typed
reference `{ id, kind, branch, root }` (a Layer for `main` and for a side
Branch with no Commit). The inventory is live: Branches added between pages
can appear. A page also ends early at the 48 KiB reply budget. A scoped list
keeps only `<ns>.<x>` Branches, shown as `<x>`, reading 1000 at a time
(`branches_in`); a page that makes no progress is `Failed` "a list made no
progress" ([`calls/branch.rs`](../../../../control/crates/layerfs-store/src/calls/branch.rs)).

### branch.get

One Branch, or `main`: its base Layer, head Commit and typed saved head.

```text
JS      await layerfs.branch.get(project, branch)
Python  layerfs.branch.get(project, branch)
CLI     layerfs branch get <project> <branch>
```

Returns one `branch.list` item. The Store looks the name up in its name
index; control answers the item whose `name` or `id` matches, else
`NoSuchBranch`. It is the read to make after an uncertain fork, merge or
publish.

### branch.commits

Saved side-Branch Commits, or `main`'s publications, newest first, from an
immutable start.

```text
JS      await layerfs.branch.commits(project, branch, { page, page_size, at_commit, include_storage })
Python  layerfs.branch.commits(project, branch, *, page=None, page_size=None, at_commit=None, include_storage=None)
CLI     layerfs branch commits <project> <branch> [--page <page>] [--page-size <page-size>] [--at-commit <at-commit>] [--include-storage]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `page`, `page_size` options | Int | numbered pages ([saved queries](#saved-queries)) |
| `at_commit` option | Text | a full saved id to start from; absent, the Branch head is resolved once |
| `include_storage` option | Bool | adds each row's `new_canonical`, `new_stored`, `new_metadata` (null for a Layer, which performs no Save) |

```text
{ branch, at_commit: { id, kind, branch, root }, items, page, page_size,
  has_more, scan: { complete, examined, reason } }
```

Each item has a full `ref: { id, kind, branch, root }` and compact metadata
(`id`, `parent`, `second_parent`, `base`, `source_commit`, `source_branch`,
`time`, `source_time`, `time_status`, `via`, `logical` where it applies), a
message cut to 128 bytes with `message_truncated`, and `details_omitted`
pointing to `branch.get_commit`. A side Branch follows its first-parent
Commit chain (merge second parents are metadata, not rows); an empty side
Branch has no rows and pins its base Layer. `main` follows publication Layer
parents, Init included. A Layer stores no publication time: `time` is null
with `time_status: "unavailable"`, and `source_time` is the source Commit's
own time ([`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs)).

### branch.get_commit

Full details of one saved Commit or `main` publication Layer admitted by the
Branch.

```text
JS      await layerfs.branch.get_commit(project, branch, commit)
Python  layerfs.branch.get_commit(project, branch, commit)
CLI     layerfs branch get-commit <project> <branch> <commit>
```

`commit` is the complete id (no prefix matching, unlike `commit.get`). The
answer keeps the recorded metadata and adds typed `ref`, `parent_ref`,
`second_parent_ref`, `base_layer`, `source_commit_ref`, `source_time` and
`time_status`; a relationship that does not apply is null. For a
publication, `parent` is the previous Layer and `source_commit` what was
published. `main` admits its Layer lineage; a side Branch admits its
first-parent and base lineage and its own recorded Commits, displaced ones
included. The context lookup visits at most 10,000 records, and the full
answer, merge-unit reports included, must fit 48 KiB; either limit is
`Capacity`.

### branch.fork

A new Branch at another Branch's head, at `main`, or at a given Commit or
Layer. Metadata only; no file is copied.

```text
JS      await layerfs.branch.fork(project, name, from_branch, from_commit?)
Python  layerfs.branch.fork(project, name, from_branch, from_commit=None)
CLI     layerfs branch fork <project> <name> <from-branch> [<from-commit>]
```

| `from` (`from_commit` if given, else `from_branch`) | The new Branch starts at |
| --- | --- |
| `main` | `main`'s head Layer, as its base, with no head |
| a Branch name | its head Commit, or its base Layer when it has none |
| a Layer id | that Layer |
| a Commit id | that Commit, on the Branch its metadata names (none: `InvalidInput`) |

Returns `{ kind: "Forked", branch, base, head | null }`; the Branch id
derives from the project and name. One short write through the Store's
single writer; no tree is read. Errors: `ReservedName` (`name` is `main`),
`NameTaken`, `NoSuchBranch`, `NoSuchCommit`, `NotFound`; after `Unknown`,
`branch.get` of `name` tells whether it published (a second fork is
`NameTaken`).

### branch.merge

Merges two Branches (either may be `main`) into a new Branch; both sources
stay unchanged.

```text
JS      await layerfs.branch.merge(project, name, a, b, { message, units })
Python  layerfs.branch.merge(project, name, a, b, *, message=None, units=None)
CLI     layerfs branch merge <project> <name> <a> <b> [--message <message>] [--units <units>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | Name | the new Branch; not `main`, not taken |
| `a`, `b` | Name | left ("ours", sent as `branch`) and right ("theirs", sent as `with`); different |
| `message` option | Text | the merge Commit's message |
| `units` option | Json | a list of glob patterns or `{ name, patterns }`; a unit both sides changed comes whole from the later write (`a` on a tie); overlapping units belong to the first listed ([`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs)) |

```text
{ kind: "Merged", fast_forward, branch, head, root,
  sources: [ { ref, node }, { ref, node } ], merge_base,
  conflicts: [], conflicts_truncated, lww: [ { path, rule, ... } ],
  lww_truncated, units: [ decisions ], units_truncated }
```

`fast_forward` means one side was an ancestor of the other: the new Branch
points at the newer side and nothing is saved. `lww` lists paths settled by
`/.lww` last-writer-wins rules (taken from `a`). The merge Commit's base is
`main`'s when either side is `main`, else `a`'s. All merge work happens
before the lane; only the final `merge_branch` publish holds it.

| Error | When |
| --- | --- |
| `NoMergeBase` | the sides share no history |
| `Conflict` | conflicting paths; nothing created; `detail` has `sources`, `merge_base`, `conflicts`, `lww`, `units` and the truncation flags |
| `HeadMoved` | the catalog moved under the publish |
| `Failed` | a Store without merge scratch (`BackendUnavailable`), other failures |

### branch.publish

Makes a Branch's head, or one of its Commits, `main`'s next Layer. It
overwrites: Layers added to `main` since the Branch's base are reported, not
merged.

```text
JS      await layerfs.branch.publish(project, branch, commit?)
Python  layerfs.branch.publish(project, branch, commit=None)
CLI     layerfs branch publish <project> <branch> [<commit>]
```

`branch` must be a side Branch (`main` is `NoSuchBranch`); `commit` is a
66-hex Commit id (a Layer id is `InvalidInput`), default the head.

```text
Added:     { kind: "Added", source: { branch, commit },
             published_layer: { ...Layer, overwrote_count }, based_on,
             overwrote_count, overwrote: [ <= 50 Layers ], overwrote_has_more }
UpToDate:  { kind: "UpToDate", layer }      (this Commit is already published)
NoChanges: { kind: "NoChanges", head_layer } (its tree equals main's head)
```

The add is a compare-and-set on `main`'s head and the Branch's base:
`HeadMoved` when either moved, never retried. Other errors: `NoCommit` (no
saved Commit), `NotInHistory` (the Commit is not on the Branch). After
`Unknown`, publishing the same Commit again answers `UpToDate` if it landed.

### branch.diff

Files that differ between two Branches' trees (either may be `main`), or
between pinned saved endpoints.

```text
JS      await layerfs.branch.diff(project, from_branch, to_branch, { paths_only, page_size, page_pos, cursor, page, from_commit, to_commit, path_prefix, include_attribution, continuation })
Python  layerfs.branch.diff(project, from_branch, to_branch, *, paths_only=None, page_size=None, page_pos=None, cursor=None, page=None, from_commit=None, to_commit=None, path_prefix=None, include_attribution=None, continuation=None)
CLI     layerfs branch diff <project> <from-branch> <to-branch> [--paths-only] [--page-size <page-size>] [--page-pos <page-pos>] [--cursor <cursor>] [--page <page>] [--from-commit <from-commit>] [--to-commit <to-commit>] [--path-prefix <path-prefix>] [--include-attribution] [--continuation <continuation>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `from_branch`, `to_branch` | Name | a Branch or `main` each (a Commit id is `InvalidInput`; give it as `from_commit`/`to_commit` or use `commit.diff`) |
| `paths_only` option | Bool | sent as `hunks = !paths_only` |
| `page_pos`, `cursor` options | Int, Text | legacy paging: the last page's `next_page_pos` or `next_cursor` (a path) |
| `page`, `page_size` options | Int | numbered matching-result pages |
| `from_commit`, `to_commit` options | Text | full Commit or Layer ids admitted by each Branch |
| `path_prefix` option | Text | matches whole path components |
| `include_attribution` option | Bool | also find equal-content files whose unchanged lines have different known owners |
| `continuation` option | Text | resume token ([saved queries](#saved-queries)) |

```text
{ kind: "ok", from: { ref, label, root }, to: { ref, label, root },
  items: [ { path, kind: Added|Removed|Modified, type,
             before | null, after | null, content, identity, metadata,
             hunks | null, hunks_omitted } ],
  has_more, next_page_pos, next_cursor }
```

`content`, `identity` and `metadata` say what changed (bytes, inode serial,
attributes); a directory whose only change is in its children is not listed.
Any of `page`, the endpoint, `path_prefix` or `include_attribution` options
selects the extended mode: `from` and `to` become full typed references,
items add `attribution`, `attribution_coverage` and `changed_owner_lines`,
and the page carries `coverage.complete` and `scan.complete`. Unknown
ownership stays null and is listed as an unresolved candidate. Hunks that do
not fit are omitted explicitly (`file.diff` gives the bounded view); content
comparison and attribution are each bounded to 64 MiB per request
([`read/branch_compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/branch_compare.rs),
[`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs)).

## Saved queries

Shared by `branch.commits`, `branch.get_commit`, `branch.diff` and the
`file` saved reads ([`file.revision`](file.md#filerevision),
[`file.diff`](file.md#filediff), [`file.blame`](file.md#fileblame)).

| Rule | Detail | Source |
| --- | --- | --- |
| Typed references | `{ id, kind: "Commit" \| "Layer", branch, root }` with full ids; display prefixes are never query identities. An id outside the project's Branch context is refused | [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |
| Pin once | the first page resolves its anchor (`at_commit`, `from`/`to`); reuse those ids for later pages and file reads. A refreshed head is a new query | same |
| Numbered pages | `page` one-based (default 1), `page_size` 1..=1000 (default 50); they count matching results, not candidates; not combined with `page_pos` or `cursor` | [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| Budgets | at most 10,000 candidate visits per request, seeking included (`MAXIMUM_QUERY_WORK`), and a 48 KiB reply; a page that cannot complete is `Capacity`, never a short successful page | [`read/query.rs`](../../../../engine/crates/layerfs-store-service/src/read/query.rs) |
| `has_more` | false only on observed exhaustion; no total or page count is promised | same |
| Boundary cache | at most 128 page boundaries per Store, keyed by pin, filters, traversal and page size; eviction changes cost, never results | [`read/paged_boundaries.rs`](../../../../engine/crates/layerfs-store-service/src/read/paged_boundaries.rs), [`read/comparison_pages.rs`](../../../../engine/crates/layerfs-store-service/src/read/comparison_pages.rs) |
| Continuation | `branch.diff` and `file.revision` answer an opaque `lfq1` token while more pages exist (null on the last). A new process repeats the same pins, filters and `page_size` and passes it; it is authenticated by the Store's cursor capability, and a mismatch or tampering is `InvalidInput` | [`read/continuation.rs`](../../../../engine/crates/layerfs-store-service/src/read/continuation.rs) |
| Saved roots only | blame and diff run at immutable roots, never at a live Workspace overlay | [`read/blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument, mixed paging styles, a bad continuation; out of scope | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs), [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject` | the project's Store or project is not recorded | [`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | a Branch, Commit or Layer is missing | [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Capacity` | a numbered page cannot fit its work or reply budget | [`read/query.rs`](../../../../engine/crates/layerfs-store-service/src/read/query.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer; a PostgreSQL server is unreachable. Nothing starts a stopped machine | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `Unknown` | a worker's answer was lost, or the Store reported an unknown outcome. A read may be called again; after a fork, merge or publish, read `branch.get` first | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Related

- [commit family](commit.md), [file family](file.md).
- [Store calls](../../engine/store-calls/README.md) and [reads](../../engine/store-calls/reads.md).
- [Merge flow](../../flows/merge.md), [compare and merge](../../core/compare-and-merge/README.md).
- [Errors and events](../calls/errors-and-events.md).
