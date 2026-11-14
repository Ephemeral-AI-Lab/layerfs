# commit family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `commit` calls: read a side Branch's Commits and compare two saved versions, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/commit`](../../../../api/crates/layerfs-api/src/commit), [`layerfs-store/src/calls/commit.rs`](../../../../control/crates/layerfs-store/src/calls/commit.rs), [`methods/branch`](../../../../engine/crates/layerfs-store-service/src/methods/branch), [`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| Runs by default | where the Store's calls run (the caller's process, or the Store machine's worker); all three only read and never take the lane |
| Design | [Store calls](../../engine/store-calls/README.md), [reads](../../engine/store-calls/reads.md) |

## Calls

```text
 Branch head
    |
    v
  C3 --parent--> C2 --parent--> C1 --> (base Layer)
  ^ list: newest first, paged      ^
  | get <id or 8+ prefix>          |
  +---------- diff <C1> <C3> ------+   (any two saved Commits or Layers)
```

| Call | Store method |
| --- | --- |
| [`commit.list`](#commitlist) | `branch.list_commits` |
| [`commit.get`](#commitget) | `branch.list_commits`, paged by control |
| [`commit.diff`](#commitdiff) | `branch.diff_commits` |

A folder scope refuses every `commit` call (`out of scope: history`). For
typed saved references, numbered pages and `main`'s publications use
[`branch.commits`](branch.md#branchcommits) and
[`branch.get_commit`](branch.md#branchget_commit).

### commit.list

A Branch's Commits, newest first, one page at a time.

```text
JS      await layerfs.commit.list(project, branch, { page_size, page_pos })
Python  layerfs.commit.list(project, branch, *, page_size=None, page_pos=None)
CLI     layerfs commit list <project> <branch> [--page-size <page-size>] [--page-pos <page-pos>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `branch` | Name | a side Branch; `main` has no Commits and is `NoSuchBranch` |
| `page_size` option | Int | 1..=1000, default 50 |
| `page_pos` option | Int | the last page's `next_page_pos` |

```text
{ items: [ commit, ... ], has_more, next_page_pos | null }

commit = { id, branch, parent | null, second_parent | null, base, root,
           via, message | null, merged_from | null, time,
           logical, new_canonical, new_stored, new_metadata,
           displaced | null, lww: [..], lww_truncated, units? }
```

| Field | Meaning |
| --- | --- |
| `parent`, `second_parent` | the first parent; the merged side of a merge |
| `base`, `root` | its base Layer and tree root |
| `via` | how it was made (Workspace Commit, `file` write, merge) |
| `logical`, `new_*` | logical bytes and what this Commit added to the Store |
| `displaced` | the head it overwrote |
| `lww`, `lww_truncated` | last-writer-wins facts, cut at 4 KiB |
| `units` | a merge's unit decisions, when given |

The value is `commit_value`
([`sdk_values.rs`](../../../../engine/crates/layerfs-bridge/src/sdk_values.rs)).
Each item costs its message plus 1 KiB against the 48 KiB reply budget; a
page ends early when it is spent
([`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs)).
It is the read to make after an uncertain `workspace.commit` or `file` write.

### commit.get

One Commit of a Branch, by id or by an id prefix of at least eight
characters.

```text
JS      await layerfs.commit.get(project, branch, commit)
Python  layerfs.commit.get(project, branch, commit)
CLI     layerfs commit get <project> <branch> <commit>
```

Returns one `commit` value. Control pages `branch.list_commits` 1000 at a
time from the head, each page one Store call with its own lease claim, and
answers the first (newest) match; the walk grows with the Branch's history
([`calls/commit.rs`](../../../../control/crates/layerfs-store/src/calls/commit.rs) `get`).
Errors: `NoSuchCommit` after the last page; `Failed` "a list made no progress"
when a page position does not move on
([`calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs) `page_of`).

### commit.diff

Files that differ between two saved versions of a project.

```text
JS      await layerfs.commit.diff(project, from_commit, to_commit, { paths_only, page_size, page_pos, cursor })
Python  layerfs.commit.diff(project, from_commit, to_commit, *, paths_only=None, page_size=None, page_pos=None, cursor=None)
CLI     layerfs commit diff <project> <from-commit> <to-commit> [--paths-only] [--page-size <page-size>] [--page-pos <page-pos>] [--cursor <cursor>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `from_commit`, `to_commit` | Name | full saved Commit or Layer ids of the project, on any Branches (a Branch name is `InvalidInput`); Commit↔Commit, Commit↔Layer and Layer↔Layer all work |
| `paths_only` option | Bool | sent as `hunks = !paths_only` |
| `page_size`, `page_pos` options | Int | 1..=1000 (default 50); the last page's `next_page_pos` |
| `cursor` option | Text | the last page's `next_cursor`, a path |

```text
{ kind: "ok", from: { commit, root }, to: { commit, root },
  items: [ { path, kind, type, before, after, content, identity,
             metadata, hunks, hunks_omitted } ],
  has_more, next_page_pos, next_cursor }
```

Items are as in [`branch.diff`](branch.md#branchdiff); each side names the
supplied id as `commit` and its immutable `root`
([`methods/branch/diff_commits.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/diff_commits.rs)).

## Family errors

`NoSuchBranch`, `NoSuchCommit`
([`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs)),
and the Store refusals shared by every Store call: `NoSuchStore`,
`NoSuchProject`, `Busy`, `Unresolved`, `Unavailable`. `Unknown` (a worker's
answer was lost) leaves no Store change: these calls only read and may be
called again.

## Related

- [branch family](branch.md), [file family](file.md).
- [Store calls: reads](../../engine/store-calls/reads.md).
- [Errors and events](../calls/errors-and-events.md).
