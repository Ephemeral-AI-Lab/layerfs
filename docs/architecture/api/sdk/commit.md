# commit family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `commit` calls: list a Branch's Commits, read one Commit and compare two Commits or Branches, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/commit`](../../../../api/crates/layerfs-api/src/commit) (one file per call), [`layerfs-store/src/calls/commit.rs`](../../../../control/crates/layerfs-store/src/calls/commit.rs), [`layerfs-home/src/cursor.rs`](../../../../control/crates/layerfs-home/src/cursor.rs), [`methods/branch`](../../../../engine/crates/layerfs-store-service/src/methods/branch), [`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| Runs by default | where the Store's calls run (the caller's process, or the Store machine's worker); all three only read and never take the publication lane |
| Design | [Store calls](../../engine/store-calls/README.md), [reads](../../engine/store-calls/reads.md), [History](../../core/history/README.md) |

## Calls

```text
 Branch head
    |
    v
  C3 --parent--> C2 --parent--> C1 --base--> (the Commit it was forked at)
  ^ list: newest first, by cursor   ^
  | get <id or 8+ prefix> [--branch B]
  +------- diff <C3> --from <C1> ---+   (two Commits or Branches)
           diff <branch>                (since its fork point)
```

| Call | Store operation (`StoreOp`) | MCP |
| --- | --- | --- |
| [`commit.list`](#commitlist) | `ListCommits` (one page from a pinned start) | `work` |
| [`commit.get`](#commitget) | `GetCommit` (the id or prefix resolved in the Store) | `admin` |
| [`commit.diff`](#commitdiff) | `Diff` (one page of the paths that differ) | `work` |

Each call file calls one function of
[`layerfs_store::calls::commit`](../../../../control/crates/layerfs-store/src/calls/commit.rs),
which sends one `StoreOp` on the project's Store and maps the answer to the
call's long form (a Commit's time becomes `created_at`, UTC text). A Commit
argument is a full id (66 hex characters) or a prefix of 8 or more
characters: a prefix more than one Commit starts with is refused
`InvalidInput` with `matches` (a count), one no Commit starts with is
`NotFound`. A folder scope refuses every `commit` call
(`out of scope: history`).

The two paged calls carry a cursor made by
[`cursor.rs`](../../../../control/crates/layerfs-home/src/cursor.rs): an
opaque text holding the call it belongs to, the place the page ended, the
filters and the Commits the first page pinned. A cursor given with other
filters or another pin, or to another call, is refused `InvalidInput`; one
whose pinned Commit is gone is `NotFound`. `limit` is 1 to 1000 (100 by
default); a page also ends early at 64 KiB of rows, with a cursor.

### commit.list

A Branch's Commits, newest first.

```text
CLI     layerfs commit list <project> <branch> [--at C] [--limit 100] [--cursor C]
Node    await layerfs.commit.list(project, branch, { at, limit, cursor })
Python  layerfs.commit.list(project, branch, *, at=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name; on `main`, its publications |
| `at` option | name | start at this Commit instead of the Branch's head: a full id, or a prefix of 8 or more characters |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the pinned Commit and the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor` |
| `items[]` short | `commit`; when set: `message`, `message_truncated`, `source_branch` |
| `items[]` long | `commit, message, message_truncated, source_branch, branch, parent, second_parent, base, root, via, created_at, merged_from, source_commit, size, new_canonical_bytes, new_stored_bytes, new_metadata_bytes` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `work` |

Starts at the Branch's head, or at `--at` (`NotFound` when it is not in the
Branch's history), and follows first parents; a merge's second parent is a
field, not a row. On `main` the rows are its publications, each with the
`source_branch` it came from. A message is cut at 128 characters in a list
(`message_truncated`); `commit get` answers it whole. `via` is how the
Commit was made: `workspace`, `file` or `merge`, and on `main` `publish` or
`init`. The long row always carries the byte counts the Commit's Save added
(`new_canonical_bytes`, `new_stored_bytes`, `new_metadata_bytes`). The
`next_cursor` carries the Commit the first page started at, so a later page
lists the same history while the Branch moves on.

Run on macOS native with a debug build (ids are full, cursor shortened here to `…`):

```console
$ layerfs commit list shop/api agent --limit 2
{"items":[{"commit":"12d12fbd01e3990284c374af5ed021fa2551db346d2a178cf4f1228794fc6f7f80","message":"Apply filesystem changes"},{"commit":"1244862d14dfce305c52f94fea57d0a31815c22d7d7cb8d03f1568433a06200a65","message":"Apply filesystem changes"}],"next_cursor":"…"}
$ layerfs commit list shop/api main
{"items":[{"commit":"32220c54f9cc0f24b9f32bbb17fd8a1748130284aa07cc1c39348ec5888118c916","source_branch":"agent"},{"commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"}]}
```

### commit.get

One Commit, by its id or a prefix of 8 or more characters.

```text
CLI     layerfs commit get <project> <commit> [--branch B]
Node    await layerfs.commit.get(project, commit, { branch })
Python  layerfs.commit.get(project, commit, *, branch=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `commit` | name | a Commit id, or a prefix of 8 or more characters |
| `branch` option | name | refuse unless the Commit is in this Branch's history |

| Answer | Keys |
| --- | --- |
| short | each when set: `branch`, `parent`, `message` |
| long | `branch, parent, message, commit, second_parent, base, root, via, created_at, merged_from, displaced, overwritten, overwritten_count, units, source_branch, source_commit, size, new_canonical_bytes, new_stored_bytes, new_metadata_bytes` |
| call | writes one call-log line; MCP `admin` |

Reads one Commit of the project in one Store call, without naming its
Branch; `branch` in the answer is the Branch the Commit was made on.
`--branch B` refuses `NotFound` unless the Commit is in `B`'s history. On
`main` a publication's id is accepted, and it answers `source_branch` and
`source_commit`. The long form has the list row's keys (no
`message_truncated`) and `displaced` (the head a forced Commit replaced),
`overwritten` (the paths a merge took from the side with the later write,
the first 20), `overwritten_count` (only when that list was cut, by the 20
or by the Store's reply window) and `units`.

### commit.diff

The paths that differ between two Commits or Branches.

```text
CLI     layerfs commit diff <project> <ref> [--from R] [--path P] [--limit 100] [--cursor C] [--with patch]
Node    await layerfs.commit.diff(project, ref, { from, path, limit, cursor, with })
Python  layerfs.commit.diff(project, ref, *, from_=None, path=None, limit=None, cursor=None, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `ref` | name | a Branch or a Commit: the side compared to |
| `from` option | name | the side compared from: a Branch name, else a Commit id or prefix; the fork point of a Branch or the parent of a Commit when left off |
| `path` option | text | only this file, or this folder and what is under it |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries both Commits and the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor, from, to, from_root, to_root` |
| `items[]` short | `path`, `change` |
| `items[]` long | `path, change, type, content, metadata, identity` |
| `--with patch` | adds `hunks`, `hunks_omitted` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `work` |

Compares `from` to `ref`. Each is read as a Branch name first and, when no
Branch has that name, as a Commit. Without `--from` the side compared from
is a Branch's fork point, a Commit's parent, the base of a Branch's first
Commit, and for `main`'s first publication the empty tree (every path
`added`); `main`, which has no fork, is compared as its head Commit is.
`--path P` keeps only `P` and what is under it. A row's short form is
`{ path, change }` (`added`, `removed`, `modified`); its long form adds
`type`, `content`, `metadata` and `identity` (which of them differ). A
folder whose only change is in its children is not listed.

`--with patch` adds each changed text file's lines as `hunks` (rows
`{op, text}`, `op` one of `' '`, `-`, `+`; an added file's lines all `+`, a
removed file's all `-`; every `\n` ends a line, so a blank line's `text` is
empty) and `hunks_omitted` when they did not fit the page. The answer's
long form adds `from`, `to`, `from_root` and `to_root` (`from` and
`from_root` null for the empty tree). The `next_cursor` carries both
Commits the first page compared and the filters, so a later page compares
the same two Commits.

Run on macOS native with a debug build:

```console
$ layerfs commit diff shop/api agent
{"items":[{"path":"/README.md","change":"modified"},{"path":"/docs/a.txt","change":"modified"},{"path":"/x.txt","change":"added"},{"path":"/y","change":"added"}]}
$ layerfs commit diff shop/api agent --with patch --path /docs/a.txt
{"items":[{"path":"/docs/a.txt","change":"modified","hunks":[{"op":" ","text":"one"},{"op":"-","text":"two"},{"op":"+","text":"three"},{"op":" ","text":""}]}]}
```

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument; an ambiguous prefix (`matches`); a cursor of other filters, another pin or another call; out of scope (`history`) | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs), [`cursor.rs`](../../../../control/crates/layerfs-home/src/cursor.rs) |
| `NotFound` | the Store, project, Branch or Commit is missing; `--at` or the Commit is not in the Branch's history; a cursor's Commit is gone | [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Busy`, `Unresolved` | a move or remove holds the Store | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer; nothing starts a stopped machine | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `Unknown` | a worker's answer was lost; a read may be called again | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Related

- [branch family](branch.md), [file family](file.md) (`file history` and `file blame` read one path's history).
- [Store calls: reads](../../engine/store-calls/reads.md).
- [Errors and events](../calls/errors-and-events.md).
