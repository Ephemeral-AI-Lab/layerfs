# commit family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › commit

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `commit` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/commit`](../../../../../api/crates/layerfs-api/src/commit), [`store/calls.rs`](../../../../../control/crates/layerfs-control/src/store/calls.rs), [`engine/crates/layerfs-store-service/src/methods/branch`](../../../../../engine/crates/layerfs-store-service/src/methods/branch) |

## Calls

```text
 Branch head
    |
    v
  C3 --parent--> C2 --parent--> C1 --> (base Layer)
  ^ list: newest first, paged      ^
  | get <id or 8+ prefix>          |
  +---------- diff <C1> <C3> ------+   (any two saved Commits)
```

Three read-only calls, in help order
([`commit/mod.rs`](../../../../../api/crates/layerfs-api/src/commit/mod.rs)).
Each runs `layerfs_control::store::calls::run`.

| Call | Arguments | Options | Returns | Store method |
| --- | --- | --- | --- | --- |
| [`list`](list.md) | `project` (Project), `branch` (Name) | `page_size`, `page_pos` (Int) | `{ items: [commit], has_more, next_page_pos }`, newest first | `branch.list_commits` |
| [`get`](get.md) | `project`, `branch` (Name), `commit` (Name) | — | one `commit` | `branch.list_commits`, paged |
| [`diff`](diff.md) | `project`, `from_commit`, `to_commit` (Name) | `paths_only` (Bool), `page_size`, `page_pos` (Int), `cursor` (Text) | `{ kind: "ok", from, to, items, has_more, next_page_pos, next_cursor }` | `branch.diff_commits` |

A `commit` value is `commit_value`
([`sdk_values.rs`](../../../../../engine/crates/layerfs-bridge/src/sdk_values.rs)):
`id`, `branch`, `parent`, `second_parent`, `base`, `root`, `via`,
`message`, `merged_from`, `time`, `logical` and further statistics. A
`diff` item is one path's change: `path`, `kind`, `type`, `before`,
`after`, `content`, `identity`, `metadata`, `hunks`, `hunks_omitted`
([`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs)).

## Arguments and options

```text
 commit.get(project, branch, commit)
   pos = 0
   loop: branch.list_commits { branch, pageSize: 1000, pagePos: pos }
         match item.id == commit, or a prefix of 8+ characters
         found ------------------------------> answer the item
         next_page_pos > pos ----------------> pos = next, again
         next_page_pos <= pos ---------------> Failed (no progress)
         no next page -----------------------> NoSuchCommit
```

| Table name | Store field | Rule |
| --- | --- | --- |
| `page_size`, `page_pos` | `pageSize`, `pagePos` | the `PAGE` set; `pageSize` 1..=1000, default 50 ([`operator/args.rs`](../../../../../engine/crates/layerfs-store-service/src/operator/args.rs)) |
| `from_commit`, `to_commit` | `from`, `to` | each resolved by `snapshot::resolve` |
| `paths_only` | `hunks` = not `paths_only` | part of the `DIFF` set |
| `cursor` | `cursor` | a path or null, from the last page's `next_cursor` |

`commit.list` also stops a page early when the replies would pass the
read budget `REPLY_BUDGET` (48 KiB) and reports `has_more`
([`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs),
[`read/window.rs`](../../../../../engine/crates/layerfs-store-service/src/read/window.rs)).

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument or page; every `commit` call under a folder scope (`out of scope: history`) | [`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is missing | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs) |
| `NoSuchBranch` | the Branch is missing | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchCommit` | `get` read every page and found no match | [`store/calls.rs`](../../../../../control/crates/layerfs-control/src/store/calls.rs) |
| `Failed` | a page position that does not move on | [`store/calls.rs`](../../../../../control/crates/layerfs-control/src/store/calls.rs) |
| `Busy`, `Unresolved` | the Store's record holds another operation's `op` | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer | [`common/place.rs`](../../../../../control/crates/layerfs-control/src/common/place.rs) |
| `Unknown` | **uncertain:** a worker's answer was lost | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

These calls only read, so an `Unknown` here leaves no Store change behind.
LayerFS does not resend it. See
[error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

Like every Store call: `store_call` claims the Store's lease, then runs the
Store method in the caller's process for a PostgreSQL Store or a sqlite
Store whose machine executes locally (`native`, except on Windows), else
sends `store.call` to that machine's worker
([`store/call.rs`](../../../../../control/crates/layerfs-control/src/store/call.rs),
[`common/place.rs`](../../../../../control/crates/layerfs-control/src/common/place.rs)).
`commit.get` sends one `branch.list_commits` per page, each on that route.
No daemon serves these calls. See the
[routing table](../../../control/routing/routing-table.md).

## Example

Not run; written from the call table.

```js
const page = await layerfs.commit.list("shop/api", "agent", { page_size: 10 });
const diff = await layerfs.commit.diff("shop/api", page.items[1].id, page.items[0].id, { paths_only: true });
```

```python
page = layerfs.commit.list("shop/api", "agent", page_size=10)
diff = layerfs.commit.diff("shop/api", page["items"][1]["id"], page["items"][0]["id"], paths_only=True)
```

```console
$ layerfs commit list shop/api agent --page-size 10
$ layerfs commit get shop/api agent 71e889e6
```

## Related

- [Table](../../call-table/table.md) and [Errors](../../call-table/errors.md).
- [branch family](../branch/README.md) and [file family](../file/README.md).
- [Routing table](../../../control/routing/routing-table.md).
- [Engine: Store calls](../../../engine/store-calls/README.md), [reads](../../../engine/store-calls/reads.md).

---

← [branch.diff](../branch/diff.md) · [Up: SDK](../README.md) · [commit.list](list.md) →
